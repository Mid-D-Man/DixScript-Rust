// ============================================================================
// Replaces MdixLanguageClientProvider.cs (delete that file when adding this
// one — see Issues.cs for the full original error dump, and
// MdixLanguageClient.cs.experimental's own header comment for how this was
// tracked down).
//
// ROOT CAUSE, confirmed: MdixLanguageClientProvider.cs implemented against
// `MonoDevelop.Ide.Lsp.LspLanguageClientProvider` — a base class that never
// existed in any real MonoDevelop.Ide or VS4Mac SDK release. The `[Extension]`
// / `protected override` shape it used doesn't correspond to anything real.
//
// The REAL mechanism, confirmed via two independent sources:
//   1. Matt Ward's (mrward, the actual VS-for-Mac LSP integration author)
//      own walkthrough, "Language Server Protocol support in Visual Studio
//      for Mac 7.4" (lastexitcode.com, 2018) — states outright: "The API
//      provided by the Language Server Client extension follows the API
//      defined by the Language Server Protocol extension used by Visual
//      Studio on Windows as closely as possible." I.e. VS4Mac's
//      `ILanguageClient` is not a parallel, VS4Mac-specific interface — it's
//      registered via ordinary MEF ([Export], System.ComponentModel.
//      Composition), exactly like the Windows one.
//   2. Your own project's build cache (.vs/MdixAddin/xs/project-cache/
//      MdixAddin-Debug.json) lists
//      /Applications/Visual Studio.app/Contents/MonoBundle/
//      Microsoft.VisualStudio.LanguageServer.Client.dll as an ACTUAL
//      resolved reference on your machine — this closes the exact
//      uncertainty the .experimental file's header raised ("I don't have a
//      verified source for that assembly... it may not exist in a working
//      form at all"). It does; the 2022 Cocoa/.NET7 rewrite bundles it
//      directly rather than requiring the old separate mrward .mpack.
//
// I can't compile this myself — no .NET toolchain, and even with one, no
// copy of the actual VS4Mac SDK/app bundle to build against. Treat the next
// `vstool build`/`dotnet build` run on your machine as the real test.
// ============================================================================
//
// ----------------------------------------------------------------------------
// 2026-08-02 update — clearing the 14 build warnings from Issues.cs.
// (CS8603 fix on OnServerInitializeFailedAsync, dead-overload removal,
// assembly-level [SupportedOSPlatform("macos")] for the CA1416 warnings.
// Full reasoning in earlier revisions of this file / conversation history.)
// ----------------------------------------------------------------------------
//
// ----------------------------------------------------------------------------
// 2026-08-06 — diagnostic logging added throughout. Reason: package installs
// clean, no warnings, mdix-lsp confirmed present in the .mpack (unzip -l) —
// and yet nothing happens when opening a .mdix file. No colors, no
// completions, but ALSO no crash and no error notification.
//
// Per Matt Ward's own walkthrough (lastexitcode.com/blog/2018/03/18/
// LanguageServerSupportInVisualStudioMac7-4/), the real activation chain is:
//   MEF discovers [Export(typeof(ILanguageClient))] and constructs it
//     -> host calls OnLoadedAsync()
//     -> OnLoadedAsync fires the StartAsync event
//     -> the HOST's own base LSP-client infrastructure (not our code) is
//        what actually calls ActivateAsync() in response to that event
//
// OnLoadedAsync here already does the right thing (StartAsync?.InvokeAsync
// (...)), so the C# logic itself looks correct against the documented
// pattern -- which means total silence most likely means MEF never
// instantiated this class in the first place, and everything downstream
// never got a chance to run OR fail. Rather than guess further at why
// composition might not be finding it, logging a breadcrumb at every stage
// turns "nothing happens" into "here's exactly how far it got":
//   constructed -> OnLoadedAsync entered -> StartAsync had a subscriber
//   -> ActivateAsync entered -> process launched -> initialized/failed
//
// Writes to /tmp/mdix-debug.log (not Desktop -- avoids any sandbox/
// permission ambiguity). Wrapped in try/catch purely so a logging failure
// itself can never mask or alter the real behavior being diagnosed.
// ----------------------------------------------------------------------------

using Microsoft.VisualStudio.LanguageServer.Client;
using Microsoft.VisualStudio.Threading;
using Microsoft.VisualStudio.Utilities;
using StreamJsonRpc;
using System;
using System.Collections.Generic;
using System.ComponentModel.Composition;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Runtime.Versioning;
using System.Threading;
using System.Threading.Tasks;

// This addin only ever runs hosted inside Visual Studio for Mac, i.e. on
// macOS — tells the CA1416 platform-compatibility analyzer that, so it stops
// flagging Process/ProcessStartInfo usage below as if this were a
// cross-platform library that might also run on Windows/Linux/browser.
[assembly: SupportedOSPlatform("macos")]

namespace MidManStudio.Mdix
{
    // Shared by everything below -- one log file, one place to look.
    internal static class MdixDebugLog
    {
        const string LogPath = "/tmp/mdix-debug.log";

        public static void Write(string message)
        {
            try
            {
                File.AppendAllText(LogPath, $"{DateTime.Now:HH:mm:ss.fff} {message}\n");
            }
            catch
            {
                // Never let logging itself be the reason something breaks.
            }
        }
    }

    // Maps the .mdix file extension to a content type, so the IDE knows this
    // language client applies to DixScript files. Required companion piece
    // alongside the client itself -- without it ActivateAsync never fires.
    public static class MdixContentDefinition
    {
#pragma warning disable CS0649 // assigned by MEF at composition time, not by us
        [Export]
        [Name("mdix")]
        [BaseDefinition(CodeRemoteContentDefinition.CodeRemoteContentTypeName)]
        internal static ContentTypeDefinition? MdixContentTypeDefinition;

        [Export]
        [FileExtension(".mdix")]
        [ContentType("mdix")]
        internal static FileExtensionToContentTypeDefinition? MdixFileExtensionDefinition;
#pragma warning restore CS0649

        // Static constructors run once, the first time ANYTHING in this
        // class is touched -- including MEF just inspecting the exports
        // above during composition. If this line never shows up in the log,
        // MEF isn't even looking at this assembly's content-type exports.
        static MdixContentDefinition()
        {
            MdixDebugLog.Write("MdixContentDefinition static ctor ran (MEF touched this class)");
        }
    }

    [ContentType("mdix")]
    [Export(typeof(ILanguageClient))]
    public class MdixLanguageClient : ILanguageClient
    {
        public event AsyncEventHandler<EventArgs>? StartAsync;

#pragma warning disable CS0067 // required by ILanguageClient, unused here
        public event AsyncEventHandler<EventArgs>? StopAsync;
#pragma warning restore CS0067

        public string Name => "DixScript Language Client";

        public IEnumerable<string> ConfigurationSections
        {
            get { yield return "mdix"; }
        }

        public object? InitializationOptions => null;
        public IEnumerable<string>? FilesToWatch => null;
        public object? MiddleLayer => null;
        public object? CustomMessageTarget => null;

        // If the server fails to initialize, surface it to the user rather
        // than failing silently -- for a fresh install where MDIX_LSP_PATH
        // isn't set and the bundled binary isn't there yet, this is the only
        // signal they'd otherwise get.
        public bool ShowNotificationOnInitializeFailed => true;

        public MdixLanguageClient()
        {
            // If this line never appears in /tmp/mdix-debug.log after
            // opening a .mdix file, MEF composition never constructed this
            // class at all -- the problem is entirely on the addin-loading /
            // composition side, not anything below this point.
            MdixDebugLog.Write("MdixLanguageClient constructed (MEF instantiated the export)");
        }

        public Task<Connection?> ActivateAsync(CancellationToken token)
        {
            MdixDebugLog.Write("ActivateAsync entered");

            var serverPath = ResolveServerPath();
            MdixDebugLog.Write($"ActivateAsync: resolved server path = {serverPath}");

            var info = new ProcessStartInfo
            {
                FileName               = serverPath,
                UseShellExecute        = false,
                RedirectStandardInput  = true,
                RedirectStandardOutput = true,
                CreateNoWindow         = true,
            };

            var process = new Process { StartInfo = info };

            Connection? connection = null;
            try
            {
                if (process.Start())
                {
                    MdixDebugLog.Write($"ActivateAsync: process.Start() succeeded, pid={process.Id}");
                    connection = new Connection(
                        process.StandardOutput.BaseStream,
                        process.StandardInput.BaseStream);
                }
                else
                {
                    MdixDebugLog.Write("ActivateAsync: process.Start() returned false");
                }
            }
            catch (Exception ex)
            {
                // process.Start() throws (rather than returning false) when
                // the file genuinely can't be found/executed -- e.g. wrong
                // path, missing +x bit, bad architecture. Logging the real
                // exception here rather than letting it propagate silently.
                MdixDebugLog.Write($"ActivateAsync: process.Start() THREW: {ex.GetType().Name}: {ex.Message}");
            }

            return Task.FromResult(connection);
        }

        public async Task OnLoadedAsync()
        {
            MdixDebugLog.Write($"OnLoadedAsync entered, StartAsync has subscriber = {StartAsync != null}");

            if (StartAsync != null)
                await StartAsync.InvokeAsync(this, EventArgs.Empty);
        }

        public Task OnServerInitializedAsync()
        {
            MdixDebugLog.Write("OnServerInitializedAsync -- server initialized successfully");
            return Task.CompletedTask;
        }

        public Task OnServerInitializeFailedAsync(Exception e)
        {
            MdixDebugLog.Write($"OnServerInitializeFailedAsync(Exception) -- {e.GetType().Name}: {e.Message}");
            return Task.CompletedTask;
        }

        // Same resolution order as the VS Code / mdix-lsp wrappers: env
        // override, then bundled binary next to this assembly, then PATH.
        static string ResolveServerPath()
        {
            var env = Environment.GetEnvironmentVariable("MDIX_LSP_PATH");
            if (!string.IsNullOrEmpty(env) && File.Exists(env))
                return env;

            var addinDir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location)!;
            var bundled  = Path.Combine(addinDir, "lsp", "mdix-lsp");
            if (File.Exists(bundled))
                return bundled;

            return "mdix-lsp"; // fall back to PATH resolution by the shell
        }

        // This IS the real interface member (return type is
        // Task<InitializationFailureContext?>, not plain Task).
        Task<InitializationFailureContext?> ILanguageClient.OnServerInitializeFailedAsync(ILanguageClientInitializationInfo initializationState)
        {
            MdixDebugLog.Write("ILanguageClient.OnServerInitializeFailedAsync(ILanguageClientInitializationInfo) called");
            return Task.FromResult<InitializationFailureContext?>(null);
        }
    }
}
