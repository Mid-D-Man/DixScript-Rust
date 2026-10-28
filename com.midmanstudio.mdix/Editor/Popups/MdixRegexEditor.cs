using System;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.UIElements;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// The content of the regex popup: a pattern box, a test-text box, a status line and the list of matches
    /// with their groups. The pattern is shown as the regex itself, with no extra escaping: the file's
    /// backslashes are written for the person when the edit is applied.
    ///
    /// After a short pause in typing, the pattern and text are run through the tester the window supplies,
    /// which asks the language server and falls back to .NET (labelled approximate). Answers that arrive after
    /// the text has changed again are dropped. A pattern the server calls invalid blocks Apply; an approximate
    /// verdict never does, because the real engine compiles the edit before it is accepted.
    /// </summary>
    internal sealed class MdixRegexEditor
    {
        private const long DebounceMs   = 250;
        private const int  MaxRowsShown = 50;

        private readonly Func<string, string, Task<MdixRegexTestResult>> _tester;
        private readonly VisualElement _root;
        private readonly TextField     _pattern;
        private readonly TextField     _testText;
        private readonly Label         _status;
        private readonly Label         _note;
        private readonly VisualElement _list;

        private long _tick;
        private bool _closed;

        public VisualElement Root        { get { return _root; } }
        public VisualElement FocusTarget { get { return _pattern; } }

        public string Pattern  { get { return _pattern.value ?? string.Empty; } }
        public string TestText { get { return _testText.value ?? string.Empty; } }

        /// <summary>False once the server has said the pattern is not valid.</summary>
        public bool CanApply { get; private set; }

        /// <summary>The server's complaint about the pattern, or null.</summary>
        public string Error { get; private set; }

        /// <summary>Raised each time a result has been shown, so the popup can update Apply and its error line.</summary>
        public event Action Changed;

        public MdixRegexEditor(string initialPattern, string initialTestText, Func<string, string, Task<MdixRegexTestResult>> tester)
        {
            _tester   = tester;
            CanApply  = true;
            initialPattern = initialPattern ?? string.Empty;

            _root = new VisualElement();
            _root.AddToClassList("mdix-rx");

            _root.Add(Caption("Pattern"));
            _pattern = new TextField
            {
                value     = initialPattern,
                multiline = initialPattern.IndexOf('\n') >= 0 || initialPattern.IndexOf('\r') >= 0,
                tooltip   = "The regular expression itself. Backslashes are written for you.",
            };
            _pattern.RegisterValueChangedCallback(_ => Schedule());
            _root.Add(_pattern);

            var hint = new Label("Written as the regex sees it, with no extra escaping. The file's backslashes are added for you.");
            hint.AddToClassList("mdix-pop__hint");
            _root.Add(hint);

            _root.Add(Caption("Test text"));
            _testText = new TextField { value = initialTestText ?? string.Empty, multiline = true };
            _testText.AddToClassList("mdix-rx__test");
            _testText.RegisterValueChangedCallback(_ => Schedule());
            _root.Add(_testText);

            _status = new Label();
            _status.AddToClassList("mdix-rx__status");
            _root.Add(_status);

            _note = new Label();
            _note.AddToClassList("mdix-rx__note");
            _note.style.display = DisplayStyle.None;
            _root.Add(_note);

            _list = new ScrollView(ScrollViewMode.Vertical);
            _list.AddToClassList("mdix-rx__list");
            _root.Add(_list);

            Schedule();                  // judge the pattern that is already there
        }

        /// <summary>Stops late answers from touching the popup after it has closed.</summary>
        public void Close()
        {
            _closed = true;
        }

        // ── Running the test ──────────────────────────────────────────────────

        private void Schedule()
        {
            var tick = ++_tick;
            _status.text = "Checking…";

            _root.schedule.Execute(() =>
            {
                if (!_closed && tick == _tick) Run(tick);
            }).ExecuteLater(DebounceMs);
        }

        private async void Run(long tick)
        {
            var pattern = Pattern;
            var text    = TestText;

            MdixRegexTestResult result;
            try
            {
                result = await _tester(pattern, text);
            }
            catch (Exception ex)
            {
                Debug.LogException(ex);
                return;
            }

            if (_closed || tick != _tick || result == null) return;      // the text changed again, or the popup is gone
            Show(result);
        }

        // ── Showing a result ──────────────────────────────────────────────────

        private void Show(MdixRegexTestResult result)
        {
            if (result.Valid)
            {
                Error        = null;
                CanApply     = true;
                _status.text = MdixRegexTest.Summary(result);
            }
            else if (result.Approximate)
            {
                // .NET disliked it, but .NET is not DixScript's engine: say so, and let the real compile decide.
                Error        = null;
                CanApply     = true;
                _status.text = ".NET does not accept this pattern (" + result.Error + "). DixScript's engine may differ; Apply checks it.";
            }
            else
            {
                Error        = result.Error;
                CanApply     = false;
                _status.text = string.Empty;
            }

            if (result.Approximate)
            {
                _note.text = "Approximate: the language server could not be reached, so this check uses .NET's regex engine. " +
                             "DixScript's engine differs (it has no look-around or backreferences). Applying still compiles the " +
                             "pattern with the real engine first.";
                _note.style.display = DisplayStyle.Flex;
            }
            else
            {
                _note.style.display = DisplayStyle.None;
            }

            FillList(result);

            var handler = Changed;
            if (handler != null) handler();
        }

        private void FillList(MdixRegexTestResult result)
        {
            _list.Clear();

            var shown = Math.Min(result.Hits.Count, MaxRowsShown);
            for (int i = 0; i < shown; i++)
            {
                var hit = result.Hits[i];
                _list.Add(Row(MdixRegexTest.DescribeHit(i + 1, hit), "mdix-rx__hit"));

                for (int g = 0; g < hit.Groups.Count; g++)
                    _list.Add(Row(MdixRegexTest.DescribeGroup(g + 1, hit.Groups[g]), "mdix-rx__group"));
            }

            if (result.Hits.Count > shown)
                _list.Add(Row("… " + (result.Hits.Count - shown) + " more matches not shown.", "mdix-rx__group"));
        }

        private static Label Row(string text, string className)
        {
            var label = new Label(text);
            label.AddToClassList(className);
            return label;
        }

        private static Label Caption(string text)
        {
            var label = new Label(text);
            label.AddToClassList("mdix-rx__caption");
            return label;
        }
    }
}
