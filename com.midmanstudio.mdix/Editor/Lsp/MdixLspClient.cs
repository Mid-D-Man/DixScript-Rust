using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using UnityEditor;
using UnityEngine;
using Debug = UnityEngine.Debug;

namespace MidManStudio.Mdix.Unity.Editor.Lsp
{
    /// <summary>
    /// Spawns the real `mdix-lsp` binary and speaks LSP (JSON-RPC 2.0 over
    /// stdio, Content-Length framed) directly from Editor code.
    ///
    /// Confirmed against the actual mdix-lsp source before writing this:
    ///   - Transport is plain stdio (mdix-lsp/src/lib.rs::run_with_extensions:
    ///     tokio::io::stdin()/stdout() wired straight into tower_lsp::Server).
    ///     All logging goes to stderr only — stdout is never anything but the
    ///     LSP channel, so it's safe to parse byte-for-byte with no ambiguity.
    ///   - textDocument_sync.change is TextDocumentSyncKind::FULL
    ///     (mdix-lsp/src/capabilities.rs), so didChange always sends the
    ///     complete current text, never incremental ranges — no diff/patch
    ///     computation needed here.
    ///   - completion_provider.resolve_provider is false — completion items
    ///     come back fully resolved; no completionItem/resolve round-trip.
    ///   - completion trigger characters: @ . < ~ { ( [
    ///   - hover_provider is a plain boolean (HoverProviderCapability::Simple),
    ///     no special hover options to declare client-side.
    ///
    /// NOT yet compiled or run against a live mdix-lsp process — Unity Editor
    /// isn't available in this environment. First real test is opening this
    /// in the Editor with an actual mdix-lsp binary on hand.
    /// </summary>
    internal sealed class MdixLspClient : IDisposable
    {
        // ── Public surface ────────────────────────────────────────────────────

        /// <summary>
        /// Fired when the server pushes textDocument/publishDiagnostics.
        /// Always invoked on the main thread (drained from a thread-safe queue
        /// on EditorApplication.update), safe to touch VisualElements from.
        /// The int is the document version the server analysed (the one this
        /// client last sent), or -1 if the server didn't say.
        /// </summary>
        public event Action<string /*uri*/, MdixJsonValue /*Diagnostic[]*/, int /*version*/>? DiagnosticsReceived;

        /// <summary>Fired for window/logMessage and window/showMessage. Main-thread.</summary>
        public event Action<string>? ServerMessage;

        /// <summary>Fired if the server process exits unexpectedly (crash, killed, etc). Main-thread.</summary>
        public event Action<int /*exitCode*/>? ProcessExited;

        public bool IsRunning => _process != null && !_hasExited;

        // ── Binary resolution ─────────────────────────────────────────────────

        private const string PrefKeyServerPath = "MdixStudio_LspServerPath";

        /// <summary>
        /// Resolution order mirrors mdix-vscode/src/extension.ts::resolveServerPath
        /// exactly, adapted for the fact that a Unity package has no fixed
        /// relative path back to a DixScript-Rust checkout the way the VSCode
        /// extension (which lives inside that same monorepo) does:
        ///   1. EditorPrefs user override (mirrors dixscript.server.path)
        ///   2. MDIX_LSP_PATH env var (same var name as the VSCode extension,
        ///      so one env var covers both editors in a dev setup)
        ///   3. The binary build-upm.yml bundles into this package at
        ///      Editor/Bin/{platform}/mdix-lsp[.exe], resolved via
        ///      PackageInfo — see BundledBinaryPath() below for why a raw
        ///      "Packages/..." filesystem guess doesn't work here.
        ///   4. System PATH (`where`/`which`)
        /// Returns null if nothing is found — caller is responsible for
        /// surfacing that to the user (see MdixLspClient.Start's return value).
        /// </summary>
        public static string? ResolveServerPath()
        {
            var exeName = Application.platform == RuntimePlatform.WindowsEditor
                ? "mdix-lsp.exe"
                : "mdix-lsp";

            var userPath = EditorPrefs.GetString(PrefKeyServerPath, string.Empty).Trim();
            if (!string.IsNullOrEmpty(userPath) && File.Exists(userPath))
                return userPath;

            var envPath = Environment.GetEnvironmentVariable("MDIX_LSP_PATH");
            if (!string.IsNullOrEmpty(envPath) && File.Exists(envPath))
                return envPath;

            var platformDir = PlatformDir();
            if (platformDir != null)
            {
                var bundled = BundledBinaryPath(platformDir, exeName);
                if (bundled != null)
                    return bundled;
            }

            return Which(exeName);
        }

        /// <summary>
        /// Resolves Editor/Bin/{platformDir}/{exeName} to a real filesystem
        /// path using the package's actual on-disk location.
        ///
        /// "Packages/com.midmanstudio.mdix/..." is only a literal filesystem
        /// path for an embedded package (physically under
        /// &lt;ProjectRoot&gt;/Packages/) or a local `file:` package. For a
        /// git-URL or registry install — i.e. the actual point of shipping
        /// this as a package rather than dropping source into every
        /// consumer's Assets folder — Unity caches the real content under
        /// &lt;ProjectRoot&gt;/Library/PackageCache/com.midmanstudio.mdix@&lt;hash&gt;/
        /// instead. "Packages/&lt;name&gt;/..." still works as a virtual path
        /// through AssetDatabase/PackageManager APIs, but a raw
        /// System.IO.Path.GetFullPath("Packages/...") call (which resolves
        /// against the process's working directory, i.e. the project root)
        /// bypasses that virtualization entirely and simply won't find
        /// anything there for those install methods — the earlier version of
        /// this method had exactly that bug.
        ///
        /// PackageInfo.FindForAssetPath + .resolvedPath is Unity's own
        /// supported way to get the real physical root for a package
        /// regardless of how it was installed (confirmed against the 2022.3
        /// scripting reference — resolvedPath is documented as exactly this:
        /// "the local path of the package on disk").
        /// </summary>
        private static string? BundledBinaryPath(string platformDir, string exeName)
        {
            var packageInfo = UnityEditor.PackageManager.PackageInfo.FindForAssetPath(
                "Packages/com.midmanstudio.mdix/package.json");

            if (packageInfo != null)
            {
                var resolved = Path.Combine(
                    packageInfo.resolvedPath, "Editor", "Bin", platformDir, exeName);
                if (File.Exists(resolved))
                    return resolved;
            }

            // Last-resort fallback for the (normally unreachable) case where
            // PackageInfo can't find the package via the asset database yet
            // — e.g. mid-import. Only ever correct for embedded/local
            // installs, same caveat as before, kept only as a safety net
            // rather than the primary path.
            var rawGuess = Path.GetFullPath(Path.Combine(
                "Packages/com.midmanstudio.mdix/Editor/Bin", platformDir, exeName));
            return File.Exists(rawGuess) ? rawGuess : null;
        }

        public static void SetServerPathOverride(string path) =>
            EditorPrefs.SetString(PrefKeyServerPath, path ?? string.Empty);

        private static string? PlatformDir()
        {
            return Application.platform switch
            {
                RuntimePlatform.WindowsEditor => "win32-x64",
                RuntimePlatform.OSXEditor      =>
                    RuntimeInformation_IsArm() ? "darwin-arm64" : "darwin-x64",
                RuntimePlatform.LinuxEditor    => "linux-x64",
                _                              => null,
            };
        }

        // SystemInfo.processorType string-sniffing avoided on purpose — this
        // uses the same RuntimeInformation the .NET BCL exposes, consistent
        // with how a normal .NET tool would detect Apple Silicon vs Intel.
        private static bool RuntimeInformation_IsArm() =>
            System.Runtime.InteropServices.RuntimeInformation.ProcessArchitecture
                == System.Runtime.InteropServices.Architecture.Arm64;

        private static string? Which(string exeName)
        {
            try
            {
                var isWindows = Application.platform == RuntimePlatform.WindowsEditor;
                var psi = new ProcessStartInfo
                {
                    FileName               = isWindows ? "where" : "which",
                    Arguments              = exeName,
                    RedirectStandardOutput = true,
                    UseShellExecute        = false,
                    CreateNoWindow         = true,
                };
                using var proc = Process.Start(psi);
                if (proc == null) return null;
                var output = proc.StandardOutput.ReadToEnd();
                proc.WaitForExit(2000);
                var firstLine = output.Split(
                    new[] { '\r', '\n' }, StringSplitOptions.RemoveEmptyEntries);
                var candidate = firstLine.Length > 0 ? firstLine[0].Trim() : null;
                return !string.IsNullOrEmpty(candidate) && File.Exists(candidate)
                    ? candidate
                    : null;
            }
            catch
            {
                return null;
            }
        }

        // ── Process lifecycle ─────────────────────────────────────────────────

        private Process?  _process;
        private Thread?   _readerThread;
        private volatile bool _running;
        private volatile bool _hasExited;
        private readonly object _writeLock = new();

        private int _nextRequestId = 1;
        private readonly Dictionary<int, TaskCompletionSource<MdixJsonValue>> _pending = new();
        private readonly object _pendingLock = new();

        private readonly Dictionary<string, int> _docVersions = new();

        private volatile bool _stopping;

        // Server events arrive on the reader threads, but listeners touch Unity
        // objects, so they are marshalled through this queue and drained on the
        // main thread. EditorApplication.delayCall is NOT used for this: it is a
        // plain delegate field rather than an event, so `+=` from a background
        // thread races with the editor swapping it out on the main thread and
        // callbacks can be silently lost (for diagnostics: an empty Problems
        // panel with no error anywhere).
        private readonly ConcurrentQueue<Action> _mainThreadQueue = new ConcurrentQueue<Action>();
        private bool _pumpHooked;

        private void PostToMainThread(Action action) => _mainThreadQueue.Enqueue(action);

        private void PumpMainThreadQueue()
        {
            // Bounded per tick so a flood of log lines can't stall the editor.
            for (var budget = 256; budget > 0 && _mainThreadQueue.TryDequeue(out var action); budget--)
            {
                try { action(); }
                catch (Exception ex) { Debug.LogException(ex); }
            }
        }

        /// <summary>The document version this client last sent for <paramref name="uri"/>, or -1.</summary>
        public int GetDocumentVersion(string uri) =>
            _docVersions.TryGetValue(uri, out var v) ? v : -1;

        /// <summary>
        /// Starts the mdix-lsp process. Does NOT perform the LSP initialize
        /// handshake — call InitializeAsync() next. Returns false (with no
        /// process left running) if the binary couldn't be found or failed
        /// to start.
        /// </summary>
        public bool Start(string? explicitServerPath = null)
        {
            var serverPath = explicitServerPath ?? ResolveServerPath();
            if (string.IsNullOrEmpty(serverPath))
            {
                Debug.LogWarning(
                    "MdixLspClient: mdix-lsp binary not found. Build it with " +
                    "`cargo build -p mdix-lsp --release`, then either set the path via " +
                    "MdixLspClient.SetServerPathOverride(...), set the MDIX_LSP_PATH " +
                    "env var, or put it on your system PATH.");
                return false;
            }

            var psi = new ProcessStartInfo
            {
                FileName               = serverPath,
                UseShellExecute        = false,
                CreateNoWindow         = true,
                RedirectStandardInput  = true,
                RedirectStandardOutput = true,
                RedirectStandardError  = true,
                StandardOutputEncoding = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false),
                StandardErrorEncoding  = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false),
            };

            try
            {
                _process = Process.Start(psi);
            }
            catch (Exception firstError)
            {
                // A bundled binary can lose its executable bit when a package is
                // copied or extracted. Try to restore it once, then retry.
                if (!TryRestoreExecutableBit(serverPath))
                {
                    Debug.LogError($"MdixLspClient: failed to start '{serverPath}': {firstError.Message}");
                    _process = null;
                    return false;
                }

                try
                {
                    _process = Process.Start(psi);
                }
                catch (Exception retryError)
                {
                    Debug.LogError($"MdixLspClient: failed to start '{serverPath}': {retryError.Message}");
                    _process = null;
                    return false;
                }
            }

            if (_process == null) return false;

            _hasExited = false;
            _running   = true;
            _stopping  = false;

            var started = _process;
            started.EnableRaisingEvents = true;
            started.Exited += (_, _) =>
            {
                _hasExited = true;

                // An intentional Stop() kills the process too — that is not a crash.
                if (_stopping) return;

                var code = SafeExitCode(started);
                PostToMainThread(() => ProcessExited?.Invoke(code));
            };

            if (!_pumpHooked)
            {
                EditorApplication.update += PumpMainThreadQueue;
                _pumpHooked = true;
            }

            _readerThread = new Thread(ReaderLoop) { IsBackground = true, Name = "MdixLspReader" };
            _readerThread.Start();

            // stderr is log output only (see mdix-lsp's own doc comment on this) —
            // drain it on its own thread so a chatty server can't block the
            // process by filling the OS pipe buffer, same class of bug the
            // `debug_mode: verbose` deadlock (fixed elsewhere in this repo) was.
            var stderrThread = new Thread(() => DrainStderr(_process)) { IsBackground = true, Name = "MdixLspStderr" };
            stderrThread.Start();

            return true;
        }

        /// <summary>Runs `chmod +x` on the server binary (macOS/Linux only). True if it succeeded.</summary>
        private static bool TryRestoreExecutableBit(string path)
        {
            if (Application.platform == RuntimePlatform.WindowsEditor) return false;

            try
            {
                var psi = new ProcessStartInfo
                {
                    FileName        = "chmod",
                    Arguments       = "+x \"" + path + "\"",
                    UseShellExecute = false,
                    CreateNoWindow  = true,
                };

                using var chmod = Process.Start(psi);
                if (chmod == null) return false;
                chmod.WaitForExit(3000);
                return chmod.HasExited && chmod.ExitCode == 0;
            }
            catch
            {
                return false;
            }
        }

        private static int SafeExitCode(Process? p)
        {
            try { return p?.ExitCode ?? -1; }
            catch { return -1; }
        }

        private void DrainStderr(Process process)
        {
            try
            {
                string? line;
                while ((line = process.StandardError.ReadLine()) != null)
                {
                    var captured = line;
                    PostToMainThread(() => ServerMessage?.Invoke(captured));
                }
            }
            catch
            {
                // Process gone / stream closed — nothing to do.
            }
        }

        public void Stop()
        {
            _stopping = true;
            _running  = false;

            try
            {
                if (_process != null && !_hasExited)
                {
                    _process.Kill();
                }
            }
            catch
            {
                // Already exited or inaccessible — fine, that's the goal anyway.
            }

            lock (_pendingLock)
            {
                foreach (var kv in _pending)
                    kv.Value.TrySetCanceled();
                _pending.Clear();
            }

            _process?.Dispose();
            _process = null;

            // Anything still queued belongs to a server that no longer exists.
            while (_mainThreadQueue.TryDequeue(out _)) { }

            if (_pumpHooked)
            {
                EditorApplication.update -= PumpMainThreadQueue;
                _pumpHooked = false;
            }
        }

        public void Dispose() => Stop();

        // ── LSP lifecycle methods ─────────────────────────────────────────────

        public async Task<bool> InitializeAsync(string workspaceRootUri, TimeSpan? timeout = null)
        {
            var initParams = MdixJsonValue.Object();
            initParams["processId"] = MdixJsonValue.Number(Process.GetCurrentProcess().Id);
            initParams["rootUri"]   = MdixJsonValue.String(workspaceRootUri);

            var capabilities = MdixJsonValue.Object();
            var textDocument = MdixJsonValue.Object();

            var completionCap = MdixJsonValue.Object();
            completionCap["dynamicRegistration"] = MdixJsonValue.Bool(false);

            // MDIX Studio expands snippets (placeholders + tab stops) itself — see
            // MdixSnippet — so say so; a server may otherwise fall back to plain text.
            var completionItemCap = MdixJsonValue.Object();
            completionItemCap["snippetSupport"] = MdixJsonValue.Bool(true);
            var docFormats = MdixJsonValue.Array();
            docFormats.Add(MdixJsonValue.String("markdown"));
            docFormats.Add(MdixJsonValue.String("plaintext"));
            completionItemCap["documentationFormat"] = docFormats;
            completionCap["completionItem"] = completionItemCap;

            textDocument["completion"] = completionCap;

            var hoverCap = MdixJsonValue.Object();
            hoverCap["dynamicRegistration"] = MdixJsonValue.Bool(false);
            var hoverFormats = MdixJsonValue.Array();
            hoverFormats.Add(MdixJsonValue.String("plaintext"));
            hoverFormats.Add(MdixJsonValue.String("markdown"));
            hoverCap["contentFormat"] = hoverFormats;
            textDocument["hover"] = hoverCap;

            var syncCap = MdixJsonValue.Object();
            syncCap["dynamicRegistration"] = MdixJsonValue.Bool(false);
            syncCap["didSave"]             = MdixJsonValue.Bool(true);
            textDocument["synchronization"] = syncCap;

            var publishDiagCap = MdixJsonValue.Object();
            publishDiagCap["relatedInformation"] = MdixJsonValue.Bool(false);
            textDocument["publishDiagnostics"] = publishDiagCap;

            capabilities["textDocument"] = textDocument;
            capabilities["workspace"]    = MdixJsonValue.Object();

            initParams["capabilities"] = capabilities;
            initParams["clientInfo"]   = MakeClientInfo();

            var result = await SendRequestAsync(
                "initialize", initParams, timeout ?? TimeSpan.FromSeconds(10));

            if (result == null) return false;

            SendNotification("initialized", MdixJsonValue.Object());
            return true;
        }

        private static MdixJsonValue MakeClientInfo()
        {
            var info = MdixJsonValue.Object();
            info["name"]    = MdixJsonValue.String("MDIX Studio (Unity)");
            info["version"] = MdixJsonValue.String("0.1.0");
            return info;
        }

        public void DidOpen(string uri, string text)
        {
            _docVersions[uri] = 1;

            var textDocument = MdixJsonValue.Object();
            textDocument["uri"]        = MdixJsonValue.String(uri);
            textDocument["languageId"] = MdixJsonValue.String("mdix");
            textDocument["version"]    = MdixJsonValue.Number(1);
            textDocument["text"]       = MdixJsonValue.String(text);

            var p = MdixJsonValue.Object();
            p["textDocument"] = textDocument;

            SendNotification("textDocument/didOpen", p);
        }

        /// <summary>
        /// Sends the FULL current text — mdix-lsp declares
        /// TextDocumentSyncKind::FULL, so there's no incremental-range form
        /// to compute here; this always resends everything.
        /// </summary>
        public void DidChange(string uri, string text)
        {
            var version = _docVersions.TryGetValue(uri, out var v) ? v + 1 : 1;
            _docVersions[uri] = version;

            var textDocument = MdixJsonValue.Object();
            textDocument["uri"]     = MdixJsonValue.String(uri);
            textDocument["version"] = MdixJsonValue.Number(version);

            var change = MdixJsonValue.Object();
            change["text"] = MdixJsonValue.String(text);

            var changes = MdixJsonValue.Array();
            changes.Add(change);

            var p = MdixJsonValue.Object();
            p["textDocument"]    = textDocument;
            p["contentChanges"]  = changes;

            SendNotification("textDocument/didChange", p);
        }

        public void DidClose(string uri)
        {
            _docVersions.Remove(uri);

            var textDocument = MdixJsonValue.Object();
            textDocument["uri"] = MdixJsonValue.String(uri);

            var p = MdixJsonValue.Object();
            p["textDocument"] = textDocument;

            SendNotification("textDocument/didClose", p);
        }

        public async Task ShutdownAsync()
        {
            try
            {
                await SendRequestAsync("shutdown", MdixJsonValue.Null, TimeSpan.FromSeconds(2));
                SendNotification("exit", MdixJsonValue.Null);
                // Give the process a brief moment to exit on its own before Stop() kills it.
                await Task.Delay(200);
            }
            catch
            {
                // Falling through to Stop() below either way.
            }
            finally
            {
                Stop();
            }
        }

        // ── Feature requests ──────────────────────────────────────────────────

        /// <param name="triggerKind">
        /// LSP CompletionTriggerKind: 1 = invoked (typing an identifier, or the
        /// explicit shortcut), 2 = a trigger character was typed, 3 = re-trigger
        /// of an incomplete list.
        /// </param>
        public async Task<MdixJsonValue?> RequestCompletionAsync(
            string uri, int line, int character,
            int triggerKind = 1, string? triggerCharacter = null,
            TimeSpan? timeout = null)
        {
            var context = MdixJsonValue.Object();
            context["triggerKind"] = MdixJsonValue.Number(triggerKind);
            if (!string.IsNullOrEmpty(triggerCharacter))
                context["triggerCharacter"] = MdixJsonValue.String(triggerCharacter);

            var p = MdixJsonValue.Object();
            p["textDocument"] = MakeTextDocumentIdentifier(uri);
            p["position"]     = MakePosition(line, character);
            p["context"]      = context;

            return await SendRequestAsync(
                "textDocument/completion", p, timeout ?? TimeSpan.FromSeconds(5));
        }

        public async Task<MdixJsonValue?> RequestHoverAsync(
            string uri, int line, int character, TimeSpan? timeout = null)
        {
            var p = MdixJsonValue.Object();
            p["textDocument"] = MakeTextDocumentIdentifier(uri);
            p["position"]     = MakePosition(line, character);

            return await SendRequestAsync(
                "textDocument/hover", p, timeout ?? TimeSpan.FromSeconds(5));
        }

        private static MdixJsonValue MakeTextDocumentIdentifier(string uri)
        {
            var td = MdixJsonValue.Object();
            td["uri"] = MdixJsonValue.String(uri);
            return td;
        }

        private static MdixJsonValue MakePosition(int line, int character)
        {
            var pos = MdixJsonValue.Object();
            pos["line"]      = MdixJsonValue.Number(line);
            pos["character"] = MdixJsonValue.Number(character);
            return pos;
        }

        // ── Transport: writing ────────────────────────────────────────────────

        private Task<MdixJsonValue?> SendRequestAsync(
            string method, MdixJsonValue @params, TimeSpan timeout)
        {
            if (!IsRunning)
                return Task.FromResult<MdixJsonValue?>(null);

            int id;
            var tcs = new TaskCompletionSource<MdixJsonValue>(
                TaskCreationOptions.RunContinuationsAsynchronously);

            lock (_pendingLock)
            {
                id = _nextRequestId++;
                _pending[id] = tcs;
            }

            var msg = MdixJsonValue.Object();
            msg["jsonrpc"] = MdixJsonValue.String("2.0");
            msg["id"]      = MdixJsonValue.Number(id);
            msg["method"]  = MdixJsonValue.String(method);
            msg["params"]  = @params;

            WriteMessage(msg);

            return WaitWithTimeout(tcs, id, timeout);
        }

        private async Task<MdixJsonValue?> WaitWithTimeout(
            TaskCompletionSource<MdixJsonValue> tcs, int id, TimeSpan timeout)
        {
            var completed = await Task.WhenAny(tcs.Task, Task.Delay(timeout));
            if (completed != tcs.Task)
            {
                lock (_pendingLock) _pending.Remove(id);
                return null; // Timed out — caller treats this the same as "no answer".
            }

            try
            {
                return await tcs.Task;
            }
            catch
            {
                return null; // Server returned a JSON-RPC error — swallow to a null result here.
            }
        }

        private void SendNotification(string method, MdixJsonValue @params)
        {
            if (!IsRunning) return;

            var msg = MdixJsonValue.Object();
            msg["jsonrpc"] = MdixJsonValue.String("2.0");
            msg["method"]  = MdixJsonValue.String(method);
            msg["params"]  = @params;

            WriteMessage(msg);
        }

        private void WriteMessage(MdixJsonValue msg)
        {
            if (_process == null) return;

            var json    = msg.ToString();
            var body    = Encoding.UTF8.GetBytes(json);
            var header  = Encoding.ASCII.GetBytes($"Content-Length: {body.Length}\r\n\r\n");

            lock (_writeLock)
            {
                try
                {
                    var stream = _process.StandardInput.BaseStream;
                    stream.Write(header, 0, header.Length);
                    stream.Write(body, 0, body.Length);
                    stream.Flush();
                }
                catch (Exception ex)
                {
                    Debug.LogWarning($"MdixLspClient: write failed (server likely exited): {ex.Message}");
                }
            }
        }

        // ── Transport: reading ────────────────────────────────────────────────

        private void ReaderLoop()
        {
            if (_process == null) return;
            var stream = _process.StandardOutput.BaseStream;

            try
            {
                while (_running)
                {
                    var contentLength = ReadHeaders(stream);
                    if (contentLength < 0) break; // stream closed / EOF

                    var body = ReadExact(stream, contentLength);
                    if (body == null) break;

                    var json = Encoding.UTF8.GetString(body);

                    MdixJsonValue message;
                    try
                    {
                        message = MdixJsonValue.Parse(json);
                    }
                    catch (Exception ex)
                    {
                        Debug.LogWarning($"MdixLspClient: failed to parse server message: {ex.Message}");
                        continue;
                    }

                    Dispatch(message);
                }
            }
            catch
            {
                // Stream torn down (process killed) — normal on Stop(), nothing to report.
            }
        }

        /// <summary>Reads header lines up to the blank line, returns Content-Length, or -1 on EOF.</summary>
        private static int ReadHeaders(Stream stream)
        {
            var contentLength = -1;
            var line = new StringBuilder();

            while (true)
            {
                var b = stream.ReadByte();
                if (b < 0) return -1; // EOF

                if (b == '\r')
                {
                    var next = stream.ReadByte();
                    if (next != '\n')
                    {
                        // Tolerate a bare \r — treat it as part of the line and keep going.
                        if (next >= 0) line.Append((char)b).Append((char)next);
                        continue;
                    }

                    if (line.Length == 0)
                        return contentLength; // blank line -> headers done

                    var headerLine = line.ToString();
                    var colonIdx   = headerLine.IndexOf(':');
                    if (colonIdx > 0)
                    {
                        var name  = headerLine.Substring(0, colonIdx).Trim();
                        var value = headerLine.Substring(colonIdx + 1).Trim();
                        if (string.Equals(name, "Content-Length", StringComparison.OrdinalIgnoreCase))
                            int.TryParse(value, out contentLength);
                    }

                    line.Clear();
                    continue;
                }

                line.Append((char)b);
            }
        }

        private static byte[]? ReadExact(Stream stream, int count)
        {
            if (count <= 0) return System.Array.Empty<byte>();

            var buffer = new byte[count];
            var offset = 0;

            while (offset < count)
            {
                var read = stream.Read(buffer, offset, count - offset);
                if (read <= 0) return null; // EOF mid-message
                offset += read;
            }

            return buffer;
        }

        // ── Dispatch ──────────────────────────────────────────────────────────

        private void Dispatch(MdixJsonValue message)
        {
            if (message.TryGet("method", out var methodVal))
            {
                var method = methodVal.AsString();
                var hasId  = message.TryGet("id", out var idVal);

                HandleServerMethod(method, message);

                // Any message with both "method" and "id" is a request FROM the
                // server that expects a reply — respond with a generic empty
                // result for anything we don't explicitly implement above, so
                // a strict server implementation never sits there waiting.
                if (hasId)
                    RespondEmpty(idVal);

                return;
            }

            if (message.TryGet("id", out var responseId))
            {
                TaskCompletionSource<MdixJsonValue>? tcs;
                lock (_pendingLock)
                {
                    var id = responseId.AsInt();
                    if (_pending.TryGetValue(id, out tcs))
                        _pending.Remove(id);
                }

                if (tcs == null) return;

                if (message.TryGet("error", out var error))
                {
                    var errMsg = error.TryGet("message", out var m) ? m.AsString() : "unknown LSP error";
                    tcs.TrySetException(new Exception(errMsg));
                }
                else
                {
                    tcs.TrySetResult(message.TryGet("result", out var result) ? result : MdixJsonValue.Null);
                }
            }
        }

        private void HandleServerMethod(string method, MdixJsonValue message)
        {
            switch (method)
            {
                case "textDocument/publishDiagnostics":
                {
                    if (!message.TryGet("params", out var p)) return;
                    var uri         = p.TryGet("uri", out var u) ? u.AsString() : string.Empty;
                    var diagnostics = p.TryGet("diagnostics", out var d) ? d : MdixJsonValue.Array();
                    var version     = p.TryGet("version", out var ver) && ver.Kind == MdixJsonKind.Number ? ver.AsInt() : -1;
                    PostToMainThread(() => DiagnosticsReceived?.Invoke(uri, diagnostics, version));
                    break;
                }

                case "window/logMessage":
                case "window/showMessage":
                {
                    if (!message.TryGet("params", out var p)) return;
                    var text = p.TryGet("message", out var m) ? m.AsString() : string.Empty;
                    PostToMainThread(() => ServerMessage?.Invoke(text));
                    break;
                }

                // window/workDoneProgress/create, client/registerCapability, and
                // workspace/configuration all just get the generic empty-result
                // reply from Dispatch() above — none of them need special
                // handling for this client's feature set (no progress UI, no
                // dynamic capability registration, no per-section config).
            }
        }

        private void RespondEmpty(MdixJsonValue id)
        {
            var msg = MdixJsonValue.Object();
            msg["jsonrpc"] = MdixJsonValue.String("2.0");
            msg["id"]      = id;
            msg["result"]  = MdixJsonValue.Null;
            WriteMessage(msg);
        }
    }
}
