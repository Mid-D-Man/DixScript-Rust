using System;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.UIElements;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Popup editors for Explorer values that a one-line field does not suit: dates and timestamps (a month
    /// grid and time boxes), regexes (live matching) and long or raw text (a text area).
    ///
    /// A popup changes the value through the same path as the inline field, <see cref="ApplyExplorerEdit"/>:
    /// the new literal is compiled first and refused unless the value reads back as entered. A refusal is
    /// shown in the popup, which stays open. Nothing is written to disk until Save.
    /// </summary>
    public sealed partial class MdixEditorWindow
    {
        /// <summary>Text longer than this opens the text area instead of the one-line field.</summary>
        private const int InlineTextLimit = 100;

        private MdixPopup _valuePopup;

        // The last text a regex was tried on, so the next regex popup starts with it. Kept for this window only.
        private string _lastRegexTestText = string.Empty;

        /// <summary>Whether a value of this kind (and current text) is edited in a popup rather than the one-line field.</summary>
        private static bool UsesValuePopup(MdixLiteralKind kind, string text)
        {
            switch (kind)
            {
                case MdixLiteralKind.Date:
                case MdixLiteralKind.Timestamp:
                case MdixLiteralKind.Regex:
                case MdixLiteralKind.RawString:
                    return true;

                case MdixLiteralKind.String:
                    return text.Length > InlineTextLimit || text.IndexOf('\n') >= 0 || text.IndexOf('\r') >= 0;

                default:
                    return false;
            }
        }

        /// <summary>Closes the open popup, if any. Called whenever the Explorer is rebuilt, so a popup never outlives the text it was opened on.</summary>
        private void CloseValuePopup()
        {
            if (_valuePopup != null) _valuePopup.Close();
            _valuePopup = null;
        }

        private void OpenValuePopup(MdixLiteralKind kind, string path, string initial, string oldLiteral)
        {
            EndExplorerEdit();
            CloseValuePopup();

            MdixPopup popup = null;
            VisualElement body;
            VisualElement focus;
            Func<string> apply;
            Action onClosed = null;
            string title;
            var enterApplies = true;

            if (kind == MdixLiteralKind.Date || kind == MdixLiteralKind.Timestamp)
            {
                var isTimestamp = kind == MdixLiteralKind.Timestamp;
                var picker      = new MdixDateTimePicker(isTimestamp, initial);

                picker.Changed += () =>
                {
                    if (popup == null) return;
                    popup.SetError(picker.Error);
                    popup.SetApplyEnabled(picker.Error == null);
                };

                title = isTimestamp ? "Edit timestamp" : "Edit date";
                body  = picker.Root;
                focus = picker.FocusTarget;
                apply = () => picker.Error ?? ApplyExplorerEdit(path, picker.Text, oldLiteral, () => popup.Close());
            }
            else if (kind == MdixLiteralKind.Regex)
            {
                var editor = new MdixRegexEditor(initial, _lastRegexTestText, TestRegexAsync);

                editor.Changed += () =>
                {
                    _lastRegexTestText = editor.TestText;
                    if (popup == null) return;
                    popup.SetError(editor.Error);
                    popup.SetApplyEnabled(editor.CanApply);
                };

                title = "Edit regex";
                body  = editor.Root;
                focus = editor.FocusTarget;
                onClosed = editor.Close;                   // late answers are dropped once the popup is gone
                apply = () =>
                {
                    _lastRegexTestText = editor.TestText;
                    return ApplyExplorerEdit(path, editor.Pattern, oldLiteral, () => popup.Close());
                };
            }
            else
            {
                var area = BuildTextArea(kind, initial, out var hint);

                title        = kind == MdixLiteralKind.RawString ? "Edit raw string" : "Edit text";
                body         = hint;
                focus        = area;
                enterApplies = false;                      // Enter is a line break here; Ctrl or Cmd + Enter applies
                apply        = () => ApplyExplorerEdit(path, area.value, oldLiteral, () => popup.Close());
            }

            popup = MdixPopup.Open(rootVisualElement, title, path, body, apply, "Apply", focus, enterApplies, onClosed);
            _valuePopup = popup;
        }

        /// <summary>
        /// Tries a pattern on some text. The language server answers when it can (it runs the same regex crate the
        /// compiler uses, and is started here if it is not running yet); otherwise .NET's engine gives an answer that is
        /// labelled approximate. Never throws.
        /// </summary>
        private async Task<MdixRegexTestResult> TestRegexAsync(string pattern, string testText)
        {
            try
            {
                if (await EnsureLspClientReadyAsync())
                {
                    var client = _lspClient;
                    if (client != null && client.IsRunning)
                    {
                        var json       = await client.RequestTestRegexAsync(pattern, testText);
                        var fromServer = MdixRegexTest.FromServer(json, testText);
                        if (fromServer != null) return fromServer;
                    }
                }
            }
            catch (Exception ex)
            {
                Debug.LogException(ex);
            }

            return MdixRegexTest.FromDotNet(pattern, testText);
        }

        /// <summary>A multi-line text box with a line under it that counts the text and says how it is written back.</summary>
        private static TextField BuildTextArea(MdixLiteralKind kind, string initial, out VisualElement content)
        {
            var area = new TextField { value = initial, multiline = true };
            area.AddToClassList("mdix-pop__textarea");
            area.style.width     = 520;
            area.style.height    = 260;
            area.style.flexShrink = 1;
            EnableCodeFieldScroller(area);

            var info = new Label();
            info.AddToClassList("mdix-pop__hint");

            var note = kind == MdixLiteralKind.RawString
                ? "Raw string: written exactly as typed, so it cannot contain a backtick."
                : "Special characters (quotes, backslashes, line breaks) are escaped for you.";

            Action refresh = () =>
            {
                var text  = area.value ?? string.Empty;
                var lines = 1;
                foreach (var c in text) if (c == '\n') lines++;
                info.text = text.Length + " characters, " + lines + (lines == 1 ? " line.  " : " lines.  ") + note +
                            "  Ctrl or Cmd + Enter applies.";
            };
            area.RegisterValueChangedCallback(_ => refresh());
            refresh();

            var box = new VisualElement();
            box.Add(area);
            box.Add(info);
            content = box;
            return area;
        }
    }
}
