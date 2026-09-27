using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using UnityEditor;
using UnityEngine;
using UnityEngine.UIElements;
using MidManStudio.Mdix.Unity.Editor.Lsp;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// LSP-backed features for MDIX Studio's Editor tab: real diagnostics,
    /// completion, and hover, wired to the real mdix-lsp binary via
    /// MdixLspClient. Kept in its own file (MdixEditorWindow is now
    /// `partial`) rather than woven through the existing 683-line file, so
    /// the existing Explorer/Templates/save logic is untouched.
    ///
    /// Hookup into the rest of the class is five small, additive call-outs
    /// from the real file (see the accompanying patch notes) — nothing here
    /// replaces existing behavior.
    ///
    /// Two things are flagged rather than asserted as fact, because Unity
    /// Editor isn't available to actually run this against:
    ///   1. TextField's cursorIndex / cursorPosition / SelectRange, accessed
    ///      here through the ITextSelection interface confirmed to exist in
    ///      the 2022.3 scripting reference (TextInputBaseField&lt;T&gt;
    ///      implements it). The exact accessor path is the single highest-risk
    ///      point in this file — SafeCursorIndex()/SafeCursorPosition() below
    ///      wrap it in a try/catch specifically so a wrong guess here disables
    ///      completion/hover gracefully instead of breaking the whole tab.
    ///   2. Popup pixel placement (PositionPopupAtCursor) — built from
    ///      VisualElement.worldBound plus the local cursor position, which is
    ///      the standard technique, but exact offsets are the kind of thing
    ///      you tune by eye once it's actually on screen.
    /// Diagnostics (the Problems panel) don't depend on either of these —
    /// that part has no positioning/hit-testing involved at all.
    /// </summary>
    public sealed partial class MdixEditorWindow
    {
        // ── State ─────────────────────────────────────────────────────────────

        private MdixLspClient? _lspClient;
        private bool           _lspStarting;
        private bool           _lspInitialized;

        private bool   _lspSyncPending;
        private double _lastEditTime;
        private const double LspSyncDebounceSeconds = 0.5;

        // Guards against the completion-trigger check re-firing when we
        // programmatically set _codeField.value ourselves (e.g. on accepting
        // a completion item), as opposed to the user actually typing.
        private bool _suppressLspChangeSideEffects;

        private static readonly HashSet<char> CompletionTriggerChars =
            new() { '@', '.', '<', '~', '{', '(', '[' };

        // Problems panel
        private VisualElement? _problemsPanel;
        private ScrollView?    _problemsList;
        private Label?         _problemsHeader;

        // Completion popup
        private VisualElement?      _completionPopup;
        private ScrollView?         _completionList;
        private List<MdixJsonValue> _completionItems = new();
        private int                 _completionSelectedIndex;

        // Hover popup
        private VisualElement? _hoverPopup;
        private Label?         _hoverLabel;
        private bool           _hoverPopupVisible;
        private bool           _hoverRequestInFlight;
        private int            _lastHoverCursorIndex = -1;
        private double         _cursorIdleSince;
        private const double   HoverIdleSeconds = 0.6;

        // ── Wiring (called from the five small hookup points in MdixEditorWindow.cs) ──

        /// <summary>Call once from BindElements(), after _codeField is created and added.</summary>
        private void InitializeLspIntegration()
        {
            BuildProblemsPanel();
            BuildCompletionPopup();
            BuildHoverPopup();

            _codeField?.RegisterCallback<KeyDownEvent>(OnCodeFieldKeyDown);
            EditorApplication.update += OnLspEditorUpdate;
        }

        /// <summary>Call from the end of OnDisable().</summary>
        private void ShutdownLspIntegration()
        {
            EditorApplication.update -= OnLspEditorUpdate;

            if (_lspClient != null)
            {
                if (!string.IsNullOrEmpty(_currentPath))
                    _lspClient.DidClose(GetDocumentUri());

                // Fire-and-forget: the window is closing, we don't need to
                // block on a clean shutdown handshake completing.
                _ = _lspClient.ShutdownAsync();
                _lspClient = null;
                _lspInitialized = false;
            }
        }

        /// <summary>Call from the tail of the existing _codeField value-changed callback.</summary>
        private void NotifyLspTextChanged(string newValue, string previousValue)
        {
            _lspSyncPending = true;
            _lastEditTime   = EditorApplication.timeSinceStartup;

            if (_suppressLspChangeSideEffects) return;

            MaybeTriggerCompletion(newValue, previousValue);
        }

        /// <summary>Call from LoadAsset(), after _currentPath/_sourceText are set.</summary>
        private async void NotifyLspDocumentOpened()
        {
            if (string.IsNullOrEmpty(_currentPath)) return;

            if (!await EnsureLspClientReadyAsync())
                return;

            _lspClient!.DidOpen(GetDocumentUri(), _sourceText);
        }

        // ── Client lifecycle ──────────────────────────────────────────────────

        private async Task<bool> EnsureLspClientReadyAsync()
        {
            if (_lspInitialized) return true;
            if (_lspStarting)
            {
                // Another call is already bringing the client up — poll briefly
                // rather than starting a second process.
                for (var i = 0; i < 50 && _lspStarting; i++)
                    await Task.Delay(100);
                return _lspInitialized;
            }

            _lspStarting = true;
            try
            {
                _lspClient ??= new MdixLspClient();

                if (!_lspClient.IsRunning)
                {
                    if (!_lspClient.Start())
                    {
                        SetProblemsHeader(
                            "mdix-lsp not found — diagnostics/completion/hover disabled. " +
                            "Build it with `cargo build -p mdix-lsp --release`, or set its path.",
                            error: true);
                        return false;
                    }

                    _lspClient.DiagnosticsReceived += OnLspDiagnostics;
                    _lspClient.ServerMessage       += msg => Debug.Log($"[mdix-lsp] {msg}");
                    _lspClient.ProcessExited       += code =>
                    {
                        _lspInitialized = false;
                        SetProblemsHeader($"mdix-lsp exited (code {code}).", error: true);
                    };
                }

                var projectRoot = Directory.GetParent(Application.dataPath)!.FullName;
                var rootUri     = new Uri(projectRoot).AbsoluteUri;

                _lspInitialized = await _lspClient.InitializeAsync(rootUri);

                if (!_lspInitialized)
                    SetProblemsHeader("mdix-lsp failed to initialize.", error: true);

                return _lspInitialized;
            }
            finally
            {
                _lspStarting = false;
            }
        }

        private void OnLspEditorUpdate()
        {
            if (_lspClient == null || !_lspInitialized) return;

            if (_lspSyncPending &&
                EditorApplication.timeSinceStartup - _lastEditTime >= LspSyncDebounceSeconds)
            {
                _lspSyncPending = false;
                if (!string.IsNullOrEmpty(_currentPath))
                    _lspClient.DidChange(GetDocumentUri(), _sourceText);
            }

            UpdateHoverIdleCheck();
        }

        // ── Document URI / offset<->position helpers ─────────────────────────

        private string GetDocumentUri()
        {
            var projectRoot = Directory.GetParent(Application.dataPath)!.FullName;
            var fullPath    = Path.GetFullPath(Path.Combine(projectRoot, _currentPath));
            return new Uri(fullPath).AbsoluteUri;
        }

        private static (int line, int character) OffsetToPosition(string text, int offset)
        {
            var line      = 0;
            var lineStart = 0;
            var end       = Math.Min(offset, text.Length);

            for (var i = 0; i < end; i++)
            {
                if (text[i] == '\n')
                {
                    line++;
                    lineStart = i + 1;
                }
            }

            return (line, end - lineStart);
        }

        private static int PositionToOffset(string text, int line, int character)
        {
            var currentLine = 0;
            var i           = 0;

            while (currentLine < line && i < text.Length)
            {
                if (text[i] == '\n') currentLine++;
                i++;
            }

            return Math.Min(i + character, text.Length);
        }

        private static int FindWordStart(string text, int cursorIndex)
        {
            var i = Math.Min(cursorIndex, text.Length);
            while (i > 0 && (char.IsLetterOrDigit(text[i - 1]) || text[i - 1] == '_'))
                i--;
            return i;
        }

        // Wraps the one part of this file whose exact API shape isn't
        // confirmed by actually running it — see the class doc comment.
        private static int SafeCursorIndex(TextField field)
        {
            try { return field.cursorIndex; }
            catch { return -1; }
        }

        private static Vector2 SafeCursorPosition(TextField field)
        {
            try { return field.cursorPosition; }
            catch { return default; }
        }

        // ── Problems panel ────────────────────────────────────────────────────

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

            _problemsHeader = new Label("mdix-lsp: not started")
            {
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
        }

        private void SetProblemsHeader(string text, bool error)
        {
            if (_problemsHeader == null) return;
            _problemsHeader.text  = text;
            _problemsHeader.style.color = error
                ? new Color(0.92f, 0.45f, 0.45f)
                : new Color(0.55f, 0.62f, 0.74f);
        }

        private void OnLspDiagnostics(string uri, MdixJsonValue diagnostics)
        {
            if (_problemsList == null || _problemsHeader == null) return;

            _problemsList.Clear();
            var count = diagnostics.Count;

            SetProblemsHeader(
                count == 0 ? "mdix-lsp: 0 problems" : $"mdix-lsp: {count} problem(s)",
                error: count > 0);

            foreach (var diag in diagnostics.AsArray())
            {
                var message  = diag.TryGet("message", out var m) ? m.AsString() : "(no message)";
                var severity = diag.TryGet("severity", out var s) ? s.AsInt(1) : 1;
                var range    = diag.TryGet("range", out var r) ? r : MdixJsonValue.Null;
                var startLine = range.TryGet("start", out var start) && start.TryGet("line", out var l)
                    ? l.AsInt() : 0;

                // DiagnosticSeverity: 1=Error, 2=Warning, 3=Information, 4=Hint.
                var icon = severity switch
                {
                    1 => "✗",
                    2 => "⚠",
                    _ => "ℹ",
                };

                var color = severity switch
                {
                    1 => new Color(0.92f, 0.45f, 0.45f),
                    2 => new Color(0.90f, 0.75f, 0.35f),
                    _ => new Color(0.55f, 0.70f, 0.90f),
                };

                var row = new Label($"{icon}  Line {startLine + 1}: {message}")
                {
                    style =
                    {
                        color       = color,
                        paddingTop  = 2,
                        paddingLeft = 8,
                        paddingBottom = 2,
                        whiteSpace  = WhiteSpace.Normal,
                    },
                };

                var capturedLine = startLine;
                row.RegisterCallback<ClickEvent>(_ => JumpToLine(capturedLine));

                _problemsList.Add(row);
            }
        }

        private void JumpToLine(int line)
        {
            if (_codeField == null) return;
            var offset = PositionToOffset(_sourceText, line, 0);
            _codeField.SelectRange(offset, offset);
        }

        // ── Completion ────────────────────────────────────────────────────────

        private void BuildCompletionPopup()
        {
            _completionPopup = new VisualElement
            {
                style =
                {
                    position        = Position.Absolute,
                    display         = DisplayStyle.None,
                    minWidth        = 220,
                    maxHeight       = 220,
                    backgroundColor = new Color(0.078f, 0.094f, 0.137f),
                    borderTopWidth  = 1,
                    borderTopColor  = new Color(0.24f, 0.49f, 0.97f),
                },
            };

            _completionList = new ScrollView(ScrollViewMode.Vertical) { style = { maxHeight = 220 } };
            _completionPopup.Add(_completionList);

            rootVisualElement.Add(_completionPopup);
        }

        private static readonly TimeSpan CompletionRequestTimeout = TimeSpan.FromSeconds(3);

        private void MaybeTriggerCompletion(string newValue, string previousValue)
        {
            if (newValue.Length <= previousValue.Length) { HideCompletionPopup(); return; }
            if (_codeField == null) return;

            var idx = SafeCursorIndex(_codeField);
            if (idx <= 0 || idx > newValue.Length) return;

            var typedChar = newValue[idx - 1];
            var isIdentifierChar = char.IsLetterOrDigit(typedChar) || typedChar == '_';

            if (CompletionTriggerChars.Contains(typedChar) || isIdentifierChar)
                RequestCompletionAtCursor();
            else
                HideCompletionPopup();
        }

        private async void RequestCompletionAtCursor()
        {
            if (_codeField == null) return;
            if (!await EnsureLspClientReadyAsync()) return;

            var cursorIdx = SafeCursorIndex(_codeField);
            if (cursorIdx < 0) return;

            var (line, character) = OffsetToPosition(_sourceText, cursorIdx);
            var result = await _lspClient!.RequestCompletionAsync(
                GetDocumentUri(), line, character, CompletionRequestTimeout);

            if (result == null || result.IsNull) { HideCompletionPopup(); return; }

            // CompletionList { items: [...] } or a bare CompletionItem[] — handle both.
            var items = result.TryGet("items", out var itemsVal) ? itemsVal : result;
            ShowCompletionPopup(items);
        }

        /// <summary>Fresh completion results from the server — resets the selection to the top item.</summary>
        private void ShowCompletionPopup(MdixJsonValue items)
        {
            _completionItems = new List<MdixJsonValue>(items.AsArray());
            _completionSelectedIndex = 0;

            if (_completionItems.Count == 0) { HideCompletionPopup(); return; }

            RenderCompletionList();
            PositionPopupAtCursor(_completionPopup!);
            _completionPopup!.style.display = DisplayStyle.Flex;
        }

        /// <summary>Re-renders the already-fetched items (e.g. after arrow-key navigation) without touching the selection.</summary>
        private void RenderCompletionList()
        {
            if (_completionPopup == null || _completionList == null || _codeField == null) return;

            _completionList.Clear();

            for (var i = 0; i < _completionItems.Count; i++)
            {
                var item  = _completionItems[i];
                var label = item.TryGet("label", out var l) ? l.AsString() : "(unnamed)";
                var kind  = item.TryGet("detail", out var d) ? d.AsString() : string.Empty;

                var row = new Label(string.IsNullOrEmpty(kind) ? label : $"{label}  —  {kind}")
                {
                    style =
                    {
                        paddingTop    = 3,
                        paddingLeft   = 8,
                        paddingBottom = 3,
                        color         = i == _completionSelectedIndex
                            ? new Color(1f, 1f, 1f)
                            : new Color(0.82f, 0.85f, 0.90f),
                        backgroundColor = i == _completionSelectedIndex
                            ? new Color(0.24f, 0.49f, 0.97f, 0.35f)
                            : new Color(0, 0, 0, 0),
                    },
                };

                var capturedIndex = i;
                row.RegisterCallback<ClickEvent>(_ => AcceptCompletionItem(capturedIndex));

                _completionList.Add(row);
            }
        }

        private void HideCompletionPopup()
        {
            if (_completionPopup == null) return;
            _completionPopup.style.display = DisplayStyle.None;
            _completionItems.Clear();
        }

        private bool CompletionPopupOpen =>
            _completionPopup != null && _completionItems.Count > 0;

        private void AcceptCompletionItem(int index)
        {
            if (_codeField == null || index < 0 || index >= _completionItems.Count) return;

            var item = _completionItems[index];
            var insertText =
                item.TryGet("insertText", out var it) && !it.IsNull && it.AsString().Length > 0
                    ? it.AsString()
                    : item.TryGet("label", out var lbl) ? lbl.AsString() : string.Empty;

            if (string.IsNullOrEmpty(insertText)) { HideCompletionPopup(); return; }

            var cursorIdx = SafeCursorIndex(_codeField);
            if (cursorIdx < 0) { HideCompletionPopup(); return; }

            var wordStart = FindWordStart(_sourceText, cursorIdx);
            var newText   = _sourceText.Substring(0, wordStart) + insertText + _sourceText.Substring(cursorIdx);
            var newCursor = wordStart + insertText.Length;

            _suppressLspChangeSideEffects = true;
            try
            {
                _sourceText      = newText;
                _codeField.value = newText;
                _codeField.SelectRange(newCursor, newCursor);
            }
            finally
            {
                _suppressLspChangeSideEffects = false;
            }

            HideCompletionPopup();
            _lspSyncPending = true;
            _lastEditTime   = EditorApplication.timeSinceStartup;
        }

        // ── Hover ─────────────────────────────────────────────────────────────

        private void BuildHoverPopup()
        {
            _hoverPopup = new VisualElement
            {
                style =
                {
                    position        = Position.Absolute,
                    display         = DisplayStyle.None,
                    maxWidth        = 420,
                    backgroundColor = new Color(0.078f, 0.094f, 0.137f),
                    borderTopWidth  = 1,
                    borderTopColor  = new Color(0.30f, 0.34f, 0.42f),
                    paddingTop      = 6,
                    paddingLeft     = 8,
                    paddingRight    = 8,
                    paddingBottom   = 6,
                },
            };

            _hoverLabel = new Label(string.Empty) { style = { whiteSpace = WhiteSpace.Normal, color = new Color(0.85f, 0.88f, 0.92f) } };
            _hoverPopup.Add(_hoverLabel);
            rootVisualElement.Add(_hoverPopup);
        }

        private void UpdateHoverIdleCheck()
        {
            if (_activeTab != 1 || _codeField == null || _hoverRequestInFlight || CompletionPopupOpen)
                return;

            var idx = SafeCursorIndex(_codeField);

            if (idx != _lastHoverCursorIndex)
            {
                _lastHoverCursorIndex = idx;
                _cursorIdleSince      = EditorApplication.timeSinceStartup;
                HideHoverPopup();
                return;
            }

            if (idx >= 0 &&
                !_hoverPopupVisible &&
                EditorApplication.timeSinceStartup - _cursorIdleSince >= HoverIdleSeconds)
            {
                RequestHoverAtCursor();
            }
        }

        private static readonly TimeSpan HoverRequestTimeout = TimeSpan.FromSeconds(3);

        private async void RequestHoverAtCursor()
        {
            if (_codeField == null) return;
            _hoverRequestInFlight = true;

            try
            {
                if (!await EnsureLspClientReadyAsync()) return;

                var cursorIdx = SafeCursorIndex(_codeField);
                if (cursorIdx < 0) return;

                var (line, character) = OffsetToPosition(_sourceText, cursorIdx);
                var result = await _lspClient!.RequestHoverAsync(
                    GetDocumentUri(), line, character, HoverRequestTimeout);

                if (result == null || result.IsNull) { HideHoverPopup(); return; }
                ShowHoverPopup(result);
            }
            finally
            {
                _hoverRequestInFlight = false;
            }
        }

        private void ShowHoverPopup(MdixJsonValue hover)
        {
            if (_hoverPopup == null || _hoverLabel == null) return;

            var text = string.Empty;

            if (hover.TryGet("contents", out var contents))
            {
                text = contents.Kind switch
                {
                    MdixJsonKind.String => contents.AsString(),
                    MdixJsonKind.Object => contents.TryGet("value", out var v) ? v.AsString() : string.Empty,
                    MdixJsonKind.Array when contents.Count > 0 =>
                        contents[0].Kind == MdixJsonKind.Object && contents[0].TryGet("value", out var v2)
                            ? v2.AsString()
                            : contents[0].AsString(),
                    _ => string.Empty,
                };
            }

            if (string.IsNullOrWhiteSpace(text)) { HideHoverPopup(); return; }

            _hoverLabel.text = text;
            PositionPopupAtCursor(_hoverPopup);
            _hoverPopup.style.display = DisplayStyle.Flex;
            _hoverPopupVisible = true;
        }

        private void HideHoverPopup()
        {
            if (_hoverPopup == null) return;
            _hoverPopup.style.display = DisplayStyle.None;
            _hoverPopupVisible = false;
        }

        // ── Shared popup placement ────────────────────────────────────────────

        // See the class doc comment — this is the other spot to eyeball once
        // it's actually running: worldBound gives the TextField's bounds in
        // window space, cursorPosition gives the caret's position within the
        // field's own content. Added together they should land the popup at
        // the caret; a fixed +18px vertical nudge clears the current text line.
        private void PositionPopupAtCursor(VisualElement popup)
        {
            if (_codeField == null) return;

            var fieldBounds = _codeField.worldBound;
            var localCursor = SafeCursorPosition(_codeField);

            popup.style.left = fieldBounds.x + localCursor.x;
            popup.style.top  = fieldBounds.y + localCursor.y + 18;
        }

        // ── Keyboard navigation while a completion popup is open ─────────────

        private void OnCodeFieldKeyDown(KeyDownEvent evt)
        {
            if ((evt.ctrlKey || evt.commandKey) && evt.keyCode == UnityEngine.Event_KeyCode.Space)
            {
                RequestCompletionAtCursor();
                evt.StopPropagation();
                evt.PreventDefault();
                return;
            }

            if (!CompletionPopupOpen) return;

            switch (evt.keyCode)
            {
                case UnityEngine.Event_KeyCode.DownArrow:
                    _completionSelectedIndex =
                        Math.Min(_completionSelectedIndex + 1, _completionItems.Count - 1);
                    RenderCompletionList();
                    evt.StopPropagation();
                    evt.PreventDefault();
                    break;

                case UnityEngine.Event_KeyCode.UpArrow:
                    _completionSelectedIndex = Math.Max(_completionSelectedIndex - 1, 0);
                    RenderCompletionList();
                    evt.StopPropagation();
                    evt.PreventDefault();
                    break;

                case UnityEngine.Event_KeyCode.Return:
                    AcceptCompletionItem(_completionSelectedIndex);
                    evt.StopPropagation();
                    evt.PreventDefault();
                    break;

                case UnityEngine.Event_KeyCode.Escape:
                    HideCompletionPopup();
                    evt.StopPropagation();
                    evt.PreventDefault();
                    break;
            }
        }

    }
}
