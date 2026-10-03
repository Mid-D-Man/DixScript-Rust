using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using UnityEditor;
using UnityEngine;
using UnityEngine.UIElements;
using MidManStudio.Mdix.Unity.Editor.Highlight;
using MidManStudio.Mdix.Unity.Editor.Lsp;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>Editor-wide MDIX Studio preferences (EditorPrefs-backed).</summary>
    internal static class MdixStudioPrefs
    {
        private const string KeyHighlight = "MdixStudio_SyntaxHighlighting";
        private const string KeyVerbose   = "MdixStudio_LspVerboseLog";

        public static bool SyntaxHighlighting
        {
            get => EditorPrefs.GetBool(KeyHighlight, true);
            set => EditorPrefs.SetBool(KeyHighlight, value);
        }

        /// <summary>Echo mdix-lsp's routine INFO/DEBUG log lines to the Unity console.</summary>
        public static bool VerboseServerLog
        {
            get => EditorPrefs.GetBool(KeyVerbose, false);
            set => EditorPrefs.SetBool(KeyVerbose, value);
        }
    }

    /// <summary>
    /// Language-server features and syntax colouring for MDIX Studio's Editor
    /// tab. Lives in its own file (MdixEditorWindow is `partial`) so the
    /// Explorer / Templates / save logic in MdixEditorWindow.cs stays untouched.
    ///
    /// Hooked into the main file at four small points: InitializeLspIntegration
    /// (end of BindElements), NotifyLspTextChanged (the code field's change
    /// callback), NotifyLspDocumentOpened (end of LoadAsset) and
    /// ShutdownLspIntegration (OnDisable).
    ///
    /// Behaviour worth knowing, each of which was a real bug in the first release:
    ///  * Every document the window shows is opened with the server — an
    ///    unsaved/scratch document gets a synthetic "untitled:" URI (mdix-lsp
    ///    explicitly supports non-file URIs). Previously nothing was ever sent
    ///    for a document without a path, so completion fell back to the generic
    ///    section list and diagnostics never arrived.
    ///  * Pending edits are flushed to the server before every completion or
    ///    hover request, so the server answers for the text you see.
    ///  * Completion items carrying snippet syntax are expanded (MdixSnippet),
    ///    with Tab / Shift+Tab moving between the placeholders.
    ///  * Keys are handled in the TRICKLE-DOWN phase. The text editor stops
    ///    propagation of nearly every key it handles, so a normal (bubble-phase)
    ///    listener on the field never saw arrows, Enter, Tab or Ctrl+Space.
    ///    Escape is also swallowed: Unity's TextField otherwise reverts the whole
    ///    text to its value at focus time.
    ///  * The header reflects the real state instead of a hard-coded
    ///    "not started".
    ///
    /// Unity Editor isn't available where this was written. The pure logic
    /// (tokenizer, rich text, snippets, completion model, positions, markdown)
    /// is unit-tested; the UI Toolkit glue is built only from members verified
    /// against the real 2022.3 sources, but its on-screen behaviour needs a
    /// first run in the Editor.
    /// </summary>
    public sealed partial class MdixEditorWindow
    {
        // ── Language-server state ─────────────────────────────────────────────

        private const string UntitledUri = "untitled:MDIX-Studio-Scratch.mdix";

        private const double LspSyncDebounceSeconds     = 0.35;
        private const double StartFailureBackoffSeconds = 15;
        private const double CrashRestartBackoffSeconds = 3;

        private MdixLspClient? _lspClient;
        private bool   _lspStarting;
        private bool   _lspInitialized;
        private double _lspRetryAt;
        private string? _lspOpenUri;

        private string _lspStatus = "starting…";
        private bool   _lspStatusIsError;

        private bool   _lspSyncPending;
        private bool   _lspFlushInFlight;
        private double _lastEditTime;
        private double _nextFlushAllowed;

        private int _diagnosticCount = -1; // -1: nothing received for this document yet

        // ── Syntax highlighting ───────────────────────────────────────────────

        private MdixCodeOverlay? _overlay;
        private readonly List<MdixMark> _marks = new List<MdixMark>();

        // ── Problems panel ────────────────────────────────────────────────────

        private VisualElement? _problemsPanel;
        private ScrollView?    _problemsList;
        private Label?         _problemsHeader;

        // ── Completion ────────────────────────────────────────────────────────

        private static readonly HashSet<char> CompletionTriggerChars =
            new HashSet<char> { '@', '.', '<', '~', '{', '(', '[' };

        private VisualElement? _completionPopup;
        private ScrollView?    _completionList;
        private readonly List<Label> _completionRows = new List<Label>();

        private List<MdixCompletionEntry> _completionView = new List<MdixCompletionEntry>();
        private bool   _completionVisible;
        private int    _completionSelected;
        private int    _completionAnchor = -1;
        private string _completionRequestText = string.Empty;
        private int    _completionRequestId;

        // The text editor delivers Enter/Tab as a key event AND (on some platforms) a
        // separate character event. After consuming one we must swallow the other.
        private double _swallowCharUntil;

        // ── Snippet tab stops ─────────────────────────────────────────────────

        private sealed class LiveStop
        {
            public int Start;
            public int Length;
        }

        private List<LiveStop>? _snippetStops;
        private int _snippetCurrent;

        // ── Hover ─────────────────────────────────────────────────────────────

        private const double HoverIdleSeconds = 0.7;

        private VisualElement? _hoverPopup;
        private Label?         _hoverLabel;
        private bool   _hoverVisible;
        private bool   _hoverInFlight;
        private int    _lastCaret = -1;
        private int    _hoverDoneCaret = -2;
        private double _caretIdleSince;

        // ═════════════════════════════════════════════════════════════════════
        //  Hooks called from MdixEditorWindow.cs
        // ═════════════════════════════════════════════════════════════════════

        /// <summary>Call once from BindElements(), after _codeField is created, populated and added.</summary>
        private void InitializeLspIntegration()
        {
            if (_codeField == null) return;

            BuildProblemsPanel();
            BuildCompletionPopup();
            BuildHoverPopup();

            // TrickleDown: see the class remarks — a bubble-phase handler never fires for these keys.
            _codeField.RegisterCallback<KeyDownEvent>(OnCodeFieldKeyDown, TrickleDown.TrickleDown);
            _codeField.RegisterCallback<FocusOutEvent>(_ => HideAllPopups());

            _overlay = new MdixCodeOverlay(_codeField);
            ApplyHighlightPreference();

            EditorApplication.update += OnLspEditorUpdate;

            // Start the server now rather than on the first keystroke, so the header is
            // truthful from the moment the window opens and the first request is fast.
            _ = EnsureDocumentOpenAsync();
        }

        /// <summary>Call from OnDisable().</summary>
        private void ShutdownLspIntegration()
        {
            EditorApplication.update -= OnLspEditorUpdate;

            HideAllPopups();
            EndSnippet();

            _overlay?.Detach();
            _overlay = null;

            var client  = _lspClient;
            var openUri = _lspOpenUri;

            _lspClient      = null;
            _lspInitialized = false;
            _lspOpenUri     = null;

            if (client == null) return;

            client.DiagnosticsReceived -= OnLspDiagnostics;
            client.ServerMessage       -= OnLspServerMessage;
            client.ProcessExited       -= OnLspProcessExited;

            try
            {
                if (openUri != null) client.DidClose(openUri);
            }
            catch
            {
                // The process may already be gone; we're about to kill it regardless.
            }

            client.Stop();
        }

        /// <summary>Call from the tail of the code field's value-changed callback (user edits).</summary>
        private void NotifyLspTextChanged(string newValue, string previousValue)
        {
            AfterTextEdited();
            AdjustSnippetForEdit(newValue, previousValue);
            MaybeTriggerCompletion(newValue, previousValue);
        }

        /// <summary>Call from LoadAsset(), after _currentPath / _sourceText are set and the field is populated.</summary>
        private void NotifyLspDocumentOpened()
        {
            HideAllPopups();
            EndSnippet();

            _marks.Clear();
            _diagnosticCount = -1;
            ClearProblemsList();
            RefreshProblemsHeader();

            _overlay?.Refresh();

            // The same asset can be (re)loaded with different text; make sure the
            // server hears about it even if its URI didn't change.
            _lspSyncPending = true;
            _lastEditTime   = EditorApplication.timeSinceStartup;

            _ = EnsureDocumentOpenAsync();
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Server lifecycle
        // ═════════════════════════════════════════════════════════════════════

        private static string ProjectRootPath() => Directory.GetParent(Application.dataPath)!.FullName;

        private string CurrentDocUri =>
            string.IsNullOrEmpty(_currentPath) ? UntitledUri : FileUri(_currentPath);

        private static string FileUri(string projectRelativePath)
        {
            var full = Path.GetFullPath(Path.Combine(ProjectRootPath(), projectRelativePath));
            return new Uri(full).AbsoluteUri;
        }

        private static string NormalizeUri(string? uri)
        {
            if (string.IsNullOrEmpty(uri)) return string.Empty;
            return Uri.TryCreate(uri, UriKind.Absolute, out var parsed) ? parsed.AbsoluteUri : uri!;
        }

        private MdixLspClient CreateLspClient()
        {
            var client = new MdixLspClient();
            client.DiagnosticsReceived += OnLspDiagnostics;
            client.ServerMessage       += OnLspServerMessage;
            client.ProcessExited       += OnLspProcessExited;
            return client;
        }

        private void SetLspStatus(string status, bool error)
        {
            _lspStatus        = status;
            _lspStatusIsError = error;
            RefreshProblemsHeader();
        }

        private void FailLspStart(string message, double backoffSeconds)
        {
            _lspInitialized = false;
            _lspOpenUri     = null;
            _lspRetryAt     = EditorApplication.timeSinceStartup + backoffSeconds;
            SetLspStatus(message, error: true);
        }

        private async Task<bool> EnsureLspClientReadyAsync()
        {
            if (_lspInitialized && _lspClient != null && _lspClient.IsRunning)
                return true;

            if (_lspStarting)
            {
                // Someone else is bringing it up; wait for that rather than spawning a second process.
                for (var i = 0; i < 100 && _lspStarting; i++)
                    await Task.Delay(100);
                return _lspInitialized;
            }

            if (EditorApplication.timeSinceStartup < _lspRetryAt)
                return false;

            _lspStarting = true;
            SetLspStatus("starting…", error: false);

            try
            {
                var client = _lspClient ??= CreateLspClient();

                if (!client.IsRunning)
                {
                    _lspOpenUri = null; // a fresh process knows no documents

                    if (!client.Start())
                    {
                        FailLspStart(
                            "not found — build it with `cargo build -p mdix-lsp --release`, then use " +
                            "MidManStudio > MDIX Language Server > Set Server Path (or put it on PATH).",
                            StartFailureBackoffSeconds);
                        return false;
                    }
                }

                var rootUri = new Uri(ProjectRootPath()).AbsoluteUri;
                var ok      = await client.InitializeAsync(rootUri);

                // The window may have closed (or restarted the server) while we were waiting.
                if (!ReferenceEquals(_lspClient, client)) return false;

                if (!ok)
                {
                    client.Stop();
                    FailLspStart("failed to initialize.", StartFailureBackoffSeconds);
                    return false;
                }

                _lspInitialized = true;
                SetLspStatus("ready", error: false);
                return true;
            }
            catch (Exception ex)
            {
                Debug.LogException(ex);
                FailLspStart("error: " + ex.Message, StartFailureBackoffSeconds);
                return false;
            }
            finally
            {
                _lspStarting = false;
            }
        }

        private void OnLspProcessExited(int exitCode)
        {
            var client = _lspClient;
            if (client == null) return;

            client.Stop(); // release handles; also unhooks its main-thread pump

            _lspInitialized = false;
            _lspOpenUri     = null;
            _lspRetryAt     = EditorApplication.timeSinceStartup + CrashRestartBackoffSeconds;

            SetLspStatus($"stopped (exit code {exitCode}); it restarts on your next edit.", error: true);
        }

        private void OnLspServerMessage(string line)
        {
            if (string.IsNullOrEmpty(line)) return;

            // mdix-lsp logs routine INFO lines to stderr; only surface real problems unless asked.
            if (line.IndexOf(" ERROR ", StringComparison.Ordinal) >= 0)
                Debug.LogError("[mdix-lsp] " + line);
            else if (line.IndexOf(" WARN ", StringComparison.Ordinal) >= 0)
                Debug.LogWarning("[mdix-lsp] " + line);
            else if (MdixStudioPrefs.VerboseServerLog)
                Debug.Log("[mdix-lsp] " + line);
        }

        /// <summary>Opens the current document with the server (starting it if needed). Idempotent.</summary>
        private async Task<bool> EnsureDocumentOpenAsync()
        {
            try
            {
                if (!await EnsureLspClientReadyAsync()) return false;

                var client = _lspClient;
                if (client == null) return false;

                var uri = CurrentDocUri;
                if (_lspOpenUri == uri) return true;

                if (_lspOpenUri != null) client.DidClose(_lspOpenUri);

                client.DidOpen(uri, _sourceText);
                _lspOpenUri     = uri;
                _lspSyncPending = false; // didOpen carried the current text

                return true;
            }
            catch (Exception ex)
            {
                Debug.LogException(ex);
                return false;
            }
        }

        /// <summary>Makes sure the server has the document AND its latest text.</summary>
        private async Task<bool> FlushLspSyncAsync()
        {
            if (!await EnsureDocumentOpenAsync()) return false;

            var client = _lspClient;
            if (client == null) return false;

            if (_lspSyncPending)
            {
                _lspSyncPending = false;
                client.DidChange(CurrentDocUri, _sourceText);
            }

            return true;
        }

        private async void RunDebouncedFlush()
        {
            _lspFlushInFlight = true;
            try
            {
                if (!await FlushLspSyncAsync())
                    _nextFlushAllowed = EditorApplication.timeSinceStartup + 1.0; // server unavailable: don't spin
            }
            finally
            {
                _lspFlushInFlight = false;
            }
        }

        internal void RestartLanguageServer()
        {
            var client = _lspClient;
            _lspClient      = null;
            _lspInitialized = false;
            _lspOpenUri     = null;
            _lspRetryAt     = 0;

            if (client != null)
            {
                client.DiagnosticsReceived -= OnLspDiagnostics;
                client.ServerMessage       -= OnLspServerMessage;
                client.ProcessExited       -= OnLspProcessExited;
                client.Stop();
            }

            _diagnosticCount = -1;
            _marks.Clear();
            ClearProblemsList();
            _overlay?.Refresh();
            SetLspStatus("restarting…", error: false);

            _lspSyncPending = true;
            _ = EnsureDocumentOpenAsync();
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Per-frame work
        // ═════════════════════════════════════════════════════════════════════

        private void OnLspEditorUpdate()
        {
            var now = EditorApplication.timeSinceStartup;

            if (_lspSyncPending && !_lspFlushInFlight && now >= _nextFlushAllowed &&
                now - _lastEditTime >= LspSyncDebounceSeconds)
            {
                RunDebouncedFlush();
            }

            if (_activeTab != 1)
            {
                HideAllPopups();
                return;
            }

            PollCompletionState();
            UpdateHoverIdleCheck(now);
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Text / caret helpers
        // ═════════════════════════════════════════════════════════════════════

        private int SafeCaret()
        {
            if (_codeField == null) return -1;
            try { return _codeField.cursorIndex; }
            catch { return -1; }
        }

        /// <summary>Selects [anchor, caret) — pass equal values to just place the caret.</summary>
        private void SetSelection(int caret, int anchor)
        {
            if (_codeField == null) return;

            _codeField.SelectRange(caret, anchor);

            // Unity can re-apply its own selection while it processes the text change that
            // just happened; set it once more on the next scheduler tick.
            var field = _codeField;
            field.schedule.Execute(() => field.SelectRange(caret, anchor));
        }

        /// <summary>
        /// Bookkeeping shared by every edit, typed or programmatic: tell the server
        /// there is new text, drop underlines (their offsets are now stale), and
        /// repaint the colour layer immediately so a typed character never lags.
        /// </summary>
        private void AfterTextEdited()
        {
            _lspSyncPending = true;
            _lastEditTime   = EditorApplication.timeSinceStartup;

            HideHover();
            if (_marks.Count > 0) _marks.Clear();

            _overlay?.Refresh();
        }

        /// <summary>
        /// Replaces [start, end) with <paramref name="insert"/> programmatically.
        /// Uses SetValueWithoutNotify and does the change-callback's bookkeeping
        /// itself: events sent from inside another event handler are queued by the
        /// dispatcher and would arrive after any "suppress side effects" flag had
        /// already been reset.
        /// </summary>
        private int ApplyProgrammaticEdit(int start, int end, string insert)
        {
            var text = _sourceText;
            start = Mathf.Clamp(start, 0, text.Length);
            end   = Mathf.Clamp(end, start, text.Length);

            var updated = text.Substring(0, start) + insert + text.Substring(end);

            _sourceText = updated;
            _isDirty    = true;
            _codeField!.SetValueWithoutNotify(updated);
            UpdateStatusBar(parsed: false, entryCount: 0, flatCount: 0, tableCount: 0);

            AfterTextEdited();
            return start + insert.Length;
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Syntax highlighting
        // ═════════════════════════════════════════════════════════════════════

        private string BuildRichText(string plain) =>
            MdixRichText.Build(plain, MdixTokenizer.Tokenize(plain), _marks);

        /// <summary>Attach or detach the colour layer according to the user's preference.</summary>
        internal void ApplyHighlightPreference()
        {
            if (_overlay == null) return;

            if (MdixStudioPrefs.SyntaxHighlighting)
            {
                if (!_overlay.IsAttached && !_overlay.Attach(BuildRichText))
                {
                    Debug.LogWarning(
                        "[MDIX Studio] syntax highlighting is off: " + _overlay.FailureReason +
                        ". Editing is unaffected.");
                }
            }
            else if (_overlay.IsAttached)
            {
                _overlay.Detach();
            }
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Problems panel
        // ═════════════════════════════════════════════════════════════════════

        private struct Problem
        {
            public int    Severity;
            public string Message;
            public int    Line;
            public int    Character;
        }

        private void BuildProblemsPanel()
        {
            if (_panelEditor == null) return;

            _problemsPanel = new VisualElement
            {
                style =
                {
                    height          = 140,
                    borderTopWidth  = 1,
                    borderTopColor  = new Color(0.15f, 0.18f, 0.24f),
                    backgroundColor = new Color(0.024f, 0.035f, 0.059f),
                },
            };

            _problemsHeader = new Label(string.Empty)
            {
                enableRichText = false,
                style =
                {
                    paddingTop    = 4,
                    paddingLeft   = 8,
                    paddingBottom = 4,
                    color         = new Color(0.55f, 0.62f, 0.74f),
                },
            };

            _problemsList = new ScrollView(ScrollViewMode.Vertical) { style = { flexGrow = 1 } };

            _problemsPanel.Add(_problemsHeader);
            _problemsPanel.Add(_problemsList);
            _panelEditor.Add(_problemsPanel);

            RefreshProblemsHeader();
        }

        private void RefreshProblemsHeader()
        {
            if (_problemsHeader == null) return;

            string text;
            Color  colour;

            if (!_lspInitialized)
            {
                text   = "mdix-lsp: " + _lspStatus;
                colour = _lspStatusIsError ? new Color(0.92f, 0.45f, 0.45f) : new Color(0.55f, 0.62f, 0.74f);
            }
            else if (_diagnosticCount < 0)
            {
                text   = "mdix-lsp: ready";
                colour = new Color(0.55f, 0.62f, 0.74f);
            }
            else if (_diagnosticCount == 0)
            {
                text   = "mdix-lsp: no problems";
                colour = new Color(0.44f, 0.75f, 0.45f);
            }
            else
            {
                text   = "mdix-lsp: " + _diagnosticCount + (_diagnosticCount == 1 ? " problem" : " problems");
                colour = new Color(0.92f, 0.45f, 0.45f);
            }

            _problemsHeader.text        = text;
            _problemsHeader.style.color = colour;
        }

        private void ClearProblemsList() => _problemsList?.Clear();

        private void OnLspDiagnostics(string uri, MdixJsonValue diagnostics, int version)
        {
            // Diagnostics for a document we no longer have open (e.g. just switched assets).
            if (_lspOpenUri == null || NormalizeUri(uri) != NormalizeUri(_lspOpenUri))
                return;

            var client = _lspClient;
            var latest = client != null ? client.GetDocumentVersion(_lspOpenUri) : -1;

            // The server stamps each result with the version it analysed. If that's older
            // than what we've sent since, or we have unsent edits, the positions no longer
            // match the text on screen — still list the problems, but don't underline.
            var positionsTrustworthy = !_lspSyncPending && (version < 0 || latest < 0 || version >= latest);

            var text     = _sourceText;
            var problems = new List<Problem>();
            var marks    = new List<MdixMark>();

            foreach (var d in diagnostics.AsArray())
            {
                var range = d.TryGet("range", out var r) ? r : MdixJsonValue.Null;

                int startLine = 0, startChar = 0, endLine = 0, endChar = 0;
                if (range.TryGet("start", out var a))
                {
                    startLine = a["line"].AsInt();
                    startChar = a["character"].AsInt();
                }
                if (range.TryGet("end", out var b))
                {
                    endLine = b["line"].AsInt();
                    endChar = b["character"].AsInt();
                }

                var severity = d.TryGet("severity", out var sv) ? sv.AsInt(1) : 1;

                problems.Add(new Problem
                {
                    Severity  = severity,
                    Message   = d.TryGet("message", out var m) ? m.AsString() : "(no message)",
                    Line      = startLine,
                    Character = startChar,
                });

                if (!positionsTrustworthy || marks.Count >= 200) continue;

                var from = MdixTextPositions.PositionToOffset(text, startLine, startChar);
                var to   = MdixTextPositions.PositionToOffset(text, endLine, endChar);

                if (to <= from)
                {
                    // A zero-width range would be invisible; widen it by one character.
                    if (from < text.Length && text[from] != '\n') to = from + 1;
                    else if (from > 0) { to = from; from--; }
                }

                if (to > from) marks.Add(new MdixMark(from, to - from, severity));
            }

            _diagnosticCount = problems.Count;
            RefreshProblemsHeader();
            RenderProblems(problems);

            if (positionsTrustworthy)
            {
                _marks.Clear();
                _marks.AddRange(marks);
                _overlay?.Refresh();
            }
        }

        private void RenderProblems(List<Problem> problems)
        {
            if (_problemsList == null) return;

            _problemsList.Clear();

            var shown = Math.Min(problems.Count, 100);
            for (var i = 0; i < shown; i++)
            {
                var p = problems[i];

                string icon;
                Color  colour;
                switch (p.Severity)
                {
                    case 1:  icon = "x"; colour = new Color(0.92f, 0.45f, 0.45f); break;
                    case 2:  icon = "!"; colour = new Color(0.90f, 0.75f, 0.35f); break;
                    default: icon = "i"; colour = new Color(0.55f, 0.70f, 0.90f); break;
                }

                var row = new Label($"[{icon}]  Line {p.Line + 1}: {p.Message}")
                {
                    enableRichText = false,
                    style =
                    {
                        color         = colour,
                        paddingTop    = 2,
                        paddingLeft   = 8,
                        paddingBottom = 2,
                        whiteSpace    = WhiteSpace.Normal,
                    },
                };

                var line      = p.Line;
                var character = p.Character;
                row.RegisterCallback<ClickEvent>(_ => JumpTo(line, character));

                _problemsList.Add(row);
            }

            if (problems.Count > shown)
            {
                _problemsList.Add(new Label($"… and {problems.Count - shown} more")
                {
                    enableRichText = false,
                    style = { paddingLeft = 8, color = new Color(0.55f, 0.62f, 0.74f) },
                });
            }
        }

        private void JumpTo(int line, int character)
        {
            if (_codeField == null) return;

            var offset = MdixTextPositions.PositionToOffset(_sourceText, line, character);
            _codeField.Focus();
            SetSelection(offset, offset);
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Popups (shared)
        // ═════════════════════════════════════════════════════════════════════

        private VisualElement CreatePopup(float minWidth, float maxWidth)
        {
            var popup = new VisualElement
            {
                style =
                {
                    position        = Position.Absolute,
                    display         = DisplayStyle.None,
                    minWidth        = minWidth,
                    maxWidth        = maxWidth,
                    backgroundColor = new Color(0.078f, 0.094f, 0.137f),
                    borderTopWidth  = 1,
                    borderTopColor  = new Color(0.24f, 0.49f, 0.97f),
                },
            };

            // Pressing the mouse on a popup must not pull keyboard focus out of the code
            // field: that would fire FocusOut and close the popup before the click lands.
            popup.RegisterCallback<PointerDownEvent>(e => e.PreventDefault(), TrickleDown.TrickleDown);
            popup.RegisterCallback<MouseDownEvent>(e => e.PreventDefault(), TrickleDown.TrickleDown);

            rootVisualElement.Add(popup);
            return popup;
        }

        /// <summary>Positions <paramref name="popup"/> just below the caret (above it if there's no room).</summary>
        private bool PlacePopup(VisualElement popup, float estimatedHeight)
        {
            if (_overlay == null || !_overlay.TryGetCaretPosition(rootVisualElement, out var caret))
                return false;

            var bounds = rootVisualElement.layout;
            var left   = Mathf.Max(4f, Mathf.Min(caret.x, bounds.width - 320f));
            var top    = caret.y + 4f;

            if (top + estimatedHeight > bounds.height - 4f)
                top = Mathf.Max(4f, caret.y - 22f - estimatedHeight);

            popup.style.left = left;
            popup.style.top  = top;
            return true;
        }

        private void HideAllPopups()
        {
            HideCompletion();
            HideHover();
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Completion
        // ═════════════════════════════════════════════════════════════════════

        private bool CompletionOpen => _completionVisible && _completionView.Count > 0;

        private void BuildCompletionPopup()
        {
            _completionPopup = CreatePopup(minWidth: 260, maxWidth: 560);
            _completionList  = new ScrollView(ScrollViewMode.Vertical) { style = { maxHeight = 200 } };
            _completionPopup.Add(_completionList);
        }

        private void MaybeTriggerCompletion(string newValue, string previousValue)
        {
            var delta = newValue.Length - previousValue.Length;
            var caret = SafeCaret();

            if (caret <= 0 || caret > newValue.Length)
            {
                HideCompletion();
                return;
            }

            if (delta < 0)
            {
                // Deleting: keep an open popup in step with the shorter word; never open a new one.
                if (CompletionOpen) RequestCompletionAtCaret(triggerKind: 1, triggerChar: null);
                return;
            }

            // Anything but a single typed character (paste, auto-indent, programmatic edit) closes it.
            if (delta != 1)
            {
                HideCompletion();
                return;
            }

            var typed = newValue[caret - 1];
            if (CompletionTriggerChars.Contains(typed))
                RequestCompletionAtCaret(triggerKind: 2, triggerChar: typed.ToString());
            else if (MdixTextPositions.IsWordChar(typed))
                RequestCompletionAtCaret(triggerKind: 1, triggerChar: null);
            else
                HideCompletion();
        }

        private async void RequestCompletionAtCaret(int triggerKind, string? triggerChar)
        {
            try
            {
                if (_codeField == null) return;

                var requestId = ++_completionRequestId;

                // The server must be looking at the text on screen, not at what it had 0.35s ago.
                if (!await FlushLspSyncAsync()) return;
                if (requestId != _completionRequestId) return; // superseded while flushing

                var text  = _sourceText;
                var caret = SafeCaret();
                var client = _lspClient;
                if (caret < 0 || client == null) return;
                caret = Math.Min(caret, text.Length);

                var (line, character) = MdixTextPositions.OffsetToPosition(text, caret);

                var result = await client.RequestCompletionAsync(
                    CurrentDocUri, line, character, triggerKind, triggerChar);

                // Typed or moved on while waiting: a newer request (or nothing) is the right answer.
                if (requestId != _completionRequestId) return;
                if (_sourceText != text || SafeCaret() != caret) return;

                ShowCompletion(MdixCompletionModel.Parse(result), text, caret);
            }
            catch (Exception ex)
            {
                Debug.LogException(ex);
            }
        }

        private void ShowCompletion(List<MdixCompletionEntry> all, string text, int caret)
        {
            if (_completionPopup == null) return;

            _completionRequestText = text;
            _completionAnchor      = MdixTextPositions.FindWordStart(text, caret);

            var typed = text.Substring(_completionAnchor, caret - _completionAnchor);
            _completionView     = MdixCompletionModel.FilterAndSort(all, typed);
            _completionSelected = 0;

            if (_completionView.Count == 0)
            {
                HideCompletion();
                return;
            }

            // One plain candidate identical to what's already typed is just noise.
            if (_completionView.Count == 1 && !_completionView[0].IsSnippet &&
                string.Equals(_completionView[0].Label, typed, StringComparison.Ordinal))
            {
                HideCompletion();
                return;
            }

            RenderCompletionList();

            if (!PlacePopup(_completionPopup, Math.Min(_completionView.Count, 8) * 22f + 6f))
            {
                HideCompletion();
                return;
            }

            _completionPopup.style.display = DisplayStyle.Flex;
            _completionVisible = true;
        }

        private void RenderCompletionList()
        {
            if (_completionList == null) return;

            _completionList.Clear();
            _completionRows.Clear();

            for (var i = 0; i < _completionView.Count; i++)
            {
                var entry    = _completionView[i];
                var detail   = !string.IsNullOrEmpty(entry.Detail) ? entry.Detail : MdixCompletionModel.KindTag(entry.Kind);
                var selected = i == _completionSelected;

                var markup = MdixRichText.Escape(entry.Label);
                if (detail.Length > 0)
                    markup += "   <color=#7A8599>" + MdixRichText.Escape(detail) + "</color>";

                var row = new Label(markup)
                {
                    enableRichText       = true,
                    parseEscapeSequences = false,
                    style =
                    {
                        paddingTop      = 3,
                        paddingBottom   = 3,
                        paddingLeft     = 8,
                        paddingRight    = 8,
                        whiteSpace      = WhiteSpace.NoWrap,
                        color           = selected ? new Color(1f, 1f, 1f) : new Color(0.82f, 0.85f, 0.90f),
                        backgroundColor = selected ? new Color(0.24f, 0.49f, 0.97f, 0.35f) : new Color(0f, 0f, 0f, 0f),
                    },
                };

                // Accept on mouse DOWN, not click: by the time a click completes, the press
                // may already have shifted focus and closed the popup.
                var index = i;
                row.RegisterCallback<PointerDownEvent>(e =>
                {
                    e.PreventDefault();
                    e.StopPropagation();
                    AcceptCompletion(index);
                });

                _completionList.Add(row);
                _completionRows.Add(row);
            }
        }

        private void MoveCompletionSelection(int delta)
        {
            if (_completionView.Count == 0) return;

            _completionSelected = Mathf.Clamp(_completionSelected + delta, 0, _completionView.Count - 1);
            RenderCompletionList();

            if (_completionList != null && _completionSelected < _completionRows.Count)
                _completionList.ScrollTo(_completionRows[_completionSelected]);
        }

        private void HideCompletion()
        {
            _completionVisible = false;
            _completionRequestId++; // invalidate any request still in flight

            if (_completionPopup != null)
                _completionPopup.style.display = DisplayStyle.None;
        }

        /// <summary>Closes the popup once the caret leaves the word it was opened for.</summary>
        private void PollCompletionState()
        {
            if (!_completionVisible) return;

            var caret = SafeCaret();
            if (caret < 0 || caret < _completionAnchor ||
                !MdixTextPositions.IsAllWordChars(_sourceText, _completionAnchor, caret))
            {
                HideCompletion();
            }
        }

        private void AcceptCompletion(int index)
        {
            if (_codeField == null || index < 0 || index >= _completionView.Count) return;

            var entry = _completionView[index];
            var text  = _sourceText;
            var caret = SafeCaret();
            if (caret < 0) return;
            caret = Math.Min(caret, text.Length);

            // What to replace. The server's explicit range is only trustworthy while the text
            // is exactly what it answered for; once the user has typed more, replace the word
            // being typed instead.
            int start, end;
            if (entry.HasEditRange && text == _completionRequestText)
            {
                start = MdixTextPositions.PositionToOffset(text, entry.StartLine, entry.StartCharacter);
                end   = MdixTextPositions.PositionToOffset(text, entry.EndLine, entry.EndCharacter);
                if (end < caret)   end   = caret;
                if (start > caret) start = caret;
            }
            else
            {
                start = MdixTextPositions.FindWordStart(text, caret);
                end   = caret;
            }

            string insert;
            List<MdixSnippetStop>? stops = null;

            if (entry.IsSnippet)
            {
                var expanded = MdixSnippet.Expand(entry.InsertText, MdixTextPositions.LineIndent(text, start));
                insert = expanded.Text;
                stops  = expanded.Stops;
            }
            else
            {
                insert = entry.InsertText;
            }

            EndSnippet(); // an edit made here invalidates any placeholder session in progress
            HideCompletion();

            var after = ApplyProgrammaticEdit(start, end, insert);
            _codeField.Focus();

            if (stops != null) BeginSnippet(start, stops);
            else               SetSelection(after, after);
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Snippet tab stops
        // ═════════════════════════════════════════════════════════════════════

        private void BeginSnippet(int insertStart, List<MdixSnippetStop> stops)
        {
            var live = new List<LiveStop>(stops.Count);
            foreach (var stop in stops)
                live.Add(new LiveStop { Start = insertStart + stop.Start, Length = stop.Length });

            // The list always ends with the final ($0) stop. With nothing before it there
            // is nothing to tab through: just put the caret there.
            if (live.Count <= 1)
            {
                var only = live[0];
                SetSelection(only.Start, only.Start);
                _snippetStops = null;
                return;
            }

            _snippetStops   = live;
            _snippetCurrent = 0;
            SelectStop(live[0]);
        }

        private void SelectStop(LiveStop stop) => SetSelection(stop.Start + stop.Length, stop.Start);

        private void AdvanceSnippet(int direction)
        {
            if (_snippetStops == null) return;

            var last = _snippetStops.Count - 1; // the final stop
            var next = Math.Max(0, _snippetCurrent + direction);

            if (next >= last)
            {
                var final = _snippetStops[last];
                EndSnippet();
                SetSelection(final.Start, final.Start);
                return;
            }

            _snippetCurrent = next;
            SelectStop(_snippetStops[next]);
        }

        private void EndSnippet() => _snippetStops = null;

        /// <summary>
        /// Keeps the remaining tab stops pointing at the right text while the user types
        /// inside the active placeholder, and ends the session if they wander off.
        /// </summary>
        private void AdjustSnippetForEdit(string newValue, string previousValue)
        {
            if (_snippetStops == null) return;

            var delta   = newValue.Length - previousValue.Length;
            var current = _snippetStops[_snippetCurrent];
            var caret   = SafeCaret();

            var newLength = Math.Max(0, current.Length + delta);
            if (caret < current.Start || caret > current.Start + newLength)
            {
                EndSnippet();
                return;
            }

            foreach (var stop in _snippetStops)
            {
                if (!ReferenceEquals(stop, current) && stop.Start > current.Start)
                    stop.Start += delta;
            }

            current.Length = newLength;
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Keyboard
        // ═════════════════════════════════════════════════════════════════════

        private static void Consume(KeyDownEvent evt)
        {
            evt.StopPropagation();
            evt.PreventDefault();
        }

        private void OnCodeFieldKeyDown(KeyDownEvent evt)
        {
            var now = EditorApplication.timeSinceStartup;
            var key = evt.keyCode;
            var ch  = evt.character;

            // The character half of an Enter/Tab press we already handled.
            if (key == KeyCode.None && now < _swallowCharUntil && (ch == '\n' || ch == '\r' || ch == '\t'))
            {
                Consume(evt);
                return;
            }

            // Trigger completion by hand: Ctrl+Space, or Cmd/Ctrl+I (VSCode's macOS binding,
            // since the OS often owns Ctrl+Space).
            if ((evt.ctrlKey && key == KeyCode.Space) || (evt.actionKey && key == KeyCode.I))
            {
                Consume(evt);
                RequestCompletionAtCaret(triggerKind: 1, triggerChar: null);
                return;
            }

            // Never let Escape reach the TextField: it would revert ALL edits since focus.
            if (key == KeyCode.Escape)
            {
                if (CompletionOpen)               HideCompletion();
                else if (_hoverVisible)           HideHover();
                else if (_snippetStops != null)   EndSnippet();

                Consume(evt);
                return;
            }

            if (CompletionOpen)
            {
                switch (key)
                {
                    case KeyCode.DownArrow: MoveCompletionSelection(+1); Consume(evt); return;
                    case KeyCode.UpArrow:   MoveCompletionSelection(-1); Consume(evt); return;
                    case KeyCode.PageDown:  MoveCompletionSelection(+8); Consume(evt); return;
                    case KeyCode.PageUp:    MoveCompletionSelection(-8); Consume(evt); return;

                    case KeyCode.Return:
                    case KeyCode.KeypadEnter:
                    case KeyCode.Tab:
                        _swallowCharUntil = now + 0.15;
                        Consume(evt);
                        AcceptCompletion(_completionSelected);
                        return;
                }

                return;
            }

            if (_snippetStops != null && key == KeyCode.Tab)
            {
                _swallowCharUntil = now + 0.15;
                Consume(evt);
                AdvanceSnippet(evt.shiftKey ? -1 : +1);
            }
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Hover
        // ═════════════════════════════════════════════════════════════════════

        private void BuildHoverPopup()
        {
            _hoverPopup = CreatePopup(minWidth: 120, maxWidth: 520);
            _hoverPopup.style.paddingTop    = 6;
            _hoverPopup.style.paddingLeft   = 8;
            _hoverPopup.style.paddingRight  = 8;
            _hoverPopup.style.paddingBottom = 6;
            _hoverPopup.style.borderTopColor = new Color(0.30f, 0.34f, 0.42f);

            _hoverLabel = new Label(string.Empty)
            {
                enableRichText       = true,
                parseEscapeSequences = false,
                style =
                {
                    whiteSpace = WhiteSpace.Normal,
                    color      = new Color(0.85f, 0.88f, 0.92f),
                },
            };

            _hoverPopup.Add(_hoverLabel);
        }

        private void UpdateHoverIdleCheck(double now)
        {
            if (_codeField == null || _hoverInFlight || CompletionOpen) return;
            if (EditorWindow.focusedWindow != this) return;

            var caret = SafeCaret();
            if (caret != _lastCaret)
            {
                _lastCaret      = caret;
                _caretIdleSince = now;
                HideHover();
                return;
            }

            // Ask once per caret position: dismissing the popup shouldn't make it reappear.
            if (caret >= 0 && !_hoverVisible && caret != _hoverDoneCaret &&
                now - _caretIdleSince >= HoverIdleSeconds)
            {
                RequestHoverAtCaret(caret);
            }
        }

        private async void RequestHoverAtCaret(int caret)
        {
            _hoverInFlight = true;
            try
            {
                if (!await FlushLspSyncAsync())
                {
                    _hoverDoneCaret = caret;
                    return;
                }

                if (SafeCaret() != caret) return;

                var text   = _sourceText;
                var client = _lspClient;
                if (client == null) return;

                var (line, character) = MdixTextPositions.OffsetToPosition(text, Math.Min(caret, text.Length));
                var result = await client.RequestHoverAsync(CurrentDocUri, line, character);

                _hoverDoneCaret = caret;

                if (SafeCaret() != caret || _sourceText != text) return;
                ShowHover(result);
            }
            catch (Exception ex)
            {
                Debug.LogException(ex);
            }
            finally
            {
                _hoverInFlight = false;
            }
        }

        private void ShowHover(MdixJsonValue? hover)
        {
            if (_hoverPopup == null || _hoverLabel == null || hover == null || hover.IsNull)
            {
                HideHover();
                return;
            }

            // contents: MarkupContent {kind, value} | string | MarkedString | an array of those.
            var markdown = string.Empty;
            if (hover.TryGet("contents", out var contents))
            {
                switch (contents.Kind)
                {
                    case MdixJsonKind.String:
                        markdown = contents.AsString();
                        break;

                    case MdixJsonKind.Object:
                        markdown = contents.TryGet("value", out var v) ? v.AsString() : string.Empty;
                        break;

                    case MdixJsonKind.Array:
                        var parts = new List<string>();
                        foreach (var item in contents.AsArray())
                        {
                            if (item.Kind == MdixJsonKind.String) parts.Add(item.AsString());
                            else if (item.TryGet("value", out var iv)) parts.Add(iv.AsString());
                        }
                        markdown = string.Join("\n\n", parts);
                        break;
                }
            }

            var rich = MdixMarkdown.ToRichText(markdown);
            if (rich.Length == 0)
            {
                HideHover();
                return;
            }

            _hoverLabel.text = rich;

            if (!PlacePopup(_hoverPopup, 90f))
            {
                HideHover();
                return;
            }

            _hoverPopup.style.display = DisplayStyle.Flex;
            _hoverVisible = true;
        }

        private void HideHover()
        {
            _hoverVisible = false;
            if (_hoverPopup != null)
                _hoverPopup.style.display = DisplayStyle.None;
        }

        // ═════════════════════════════════════════════════════════════════════
        //  Menu items
        // ═════════════════════════════════════════════════════════════════════

        private const string MenuRoot      = "MidManStudio/MDIX Language Server/";
        private const string MenuHighlight = MenuRoot + "Syntax Highlighting";
        private const string MenuVerbose   = MenuRoot + "Verbose Server Log";

        [MenuItem(MenuHighlight)]
        private static void ToggleSyntaxHighlighting()
        {
            MdixStudioPrefs.SyntaxHighlighting = !MdixStudioPrefs.SyntaxHighlighting;
            foreach (var window in Resources.FindObjectsOfTypeAll<MdixEditorWindow>())
                window.ApplyHighlightPreference();
        }

        [MenuItem(MenuHighlight, true)]
        private static bool ToggleSyntaxHighlightingValidate()
        {
            Menu.SetChecked(MenuHighlight, MdixStudioPrefs.SyntaxHighlighting);
            return true;
        }

        [MenuItem(MenuVerbose)]
        private static void ToggleVerboseServerLog() =>
            MdixStudioPrefs.VerboseServerLog = !MdixStudioPrefs.VerboseServerLog;

        [MenuItem(MenuVerbose, true)]
        private static bool ToggleVerboseServerLogValidate()
        {
            Menu.SetChecked(MenuVerbose, MdixStudioPrefs.VerboseServerLog);
            return true;
        }

        [MenuItem(MenuRoot + "Restart")]
        private static void RestartLanguageServerMenu()
        {
            foreach (var window in Resources.FindObjectsOfTypeAll<MdixEditorWindow>())
                window.RestartLanguageServer();
        }

        [MenuItem(MenuRoot + "Set Server Path...")]
        private static void SetServerPathMenu()
        {
            var extension = Application.platform == RuntimePlatform.WindowsEditor ? "exe" : string.Empty;
            var chosen    = EditorUtility.OpenFilePanel("Select the mdix-lsp executable", string.Empty, extension);
            if (string.IsNullOrEmpty(chosen)) return;

            MdixLspClient.SetServerPathOverride(chosen);
            foreach (var window in Resources.FindObjectsOfTypeAll<MdixEditorWindow>())
                window.RestartLanguageServer();
        }
    }
}
