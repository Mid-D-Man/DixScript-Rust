using System;
using System.Globalization;
using UnityEngine.UIElements;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// The content of the date and timestamp popup: a text box with the whole value, a month grid, and for a
    /// timestamp the time boxes and the zone. Everything edits one <see cref="MdixDateTimeModel"/>, so the
    /// parts always agree with each other.
    ///
    /// An input that does not parse is refused and marked; the value stays as it was and <see cref="Error"/>
    /// says what is wrong, so the popup can block Apply. The next input that does parse clears it.
    /// </summary>
    internal sealed class MdixDateTimePicker
    {
        private static readonly string[]   Weekdays = { "Mo", "Tu", "We", "Th", "Fr", "Sa", "Su" };
        private static readonly CultureInfo Inv     = CultureInfo.InvariantCulture;

        private readonly MdixDateTimeModel _model;
        private readonly VisualElement     _root;
        private readonly TextField         _text;
        private readonly Label             _month;
        private readonly Label[]           _days = new Label[MdixDateTimeModel.GridCells];

        // Timestamps only.
        private TextField _hour, _minute, _second, _fraction, _offset;
        private Button    _zoneNone, _zoneUtc, _zoneOffset;

        private MdixCalendarCell[] _cells = new MdixCalendarCell[MdixDateTimeModel.GridCells];
        private VisualElement      _errorSource;
        private bool               _syncing;

        public VisualElement Root        { get { return _root; } }
        public VisualElement FocusTarget { get { return _text; } }

        /// <summary>The literal text the picker would write.</summary>
        public string Text  { get { return _model.Text; } }

        /// <summary>What is wrong with the last input, or null when every input is fine.</summary>
        public string Error { get; private set; }

        /// <summary>Raised after every input, so the popup can show <see cref="Error"/>.</summary>
        public event Action Changed;

        public MdixDateTimePicker(bool includeTime, string initialText)
        {
            _model = new MdixDateTimeModel(includeTime, initialText, DateTime.Now);

            _root = new VisualElement();
            _root.AddToClassList("mdix-cal");

            _text = new TextField { value = _model.Text, tooltip = "The whole value. Type it here or use the grid." };
            _text.RegisterValueChangedCallback(e => OnTextEdited(e.newValue));
            _root.Add(_text);

            // Month navigation.
            var nav = Row("mdix-cal__nav");
            nav.Add(NavButton("«", "Previous year",  -12));
            nav.Add(NavButton("‹", "Previous month",  -1));
            _month = new Label();
            _month.AddToClassList("mdix-cal__month");
            nav.Add(_month);
            nav.Add(NavButton("›", "Next month",       1));
            nav.Add(NavButton("»", "Next year",       12));
            _root.Add(nav);

            // Weekday header and the six weeks.
            var head = Row("mdix-cal__row");
            foreach (var name in Weekdays) head.Add(Cell(name, "mdix-cal__dow"));
            _root.Add(head);

            for (int week = 0; week < 6; week++)
            {
                var row = Row("mdix-cal__row");
                for (int col = 0; col < 7; col++)
                {
                    var index = week * 7 + col;
                    var cell  = Cell(string.Empty, "mdix-cal__day");
                    cell.RegisterCallback<ClickEvent>(_ => OnDayClicked(index));
                    _days[index] = cell;
                    row.Add(cell);
                }
                _root.Add(row);
            }

            if (includeTime) BuildTimeRows();

            var now = new Button(OnNowClicked)
            {
                text    = includeTime ? "Now" : "Today",
                tooltip = includeTime ? "Today's date and the current time, in this timestamp's zone." : "Today's date.",
            };
            now.AddToClassList("mdix-btn");
            now.AddToClassList("mdix-cal__now");
            _root.Add(now);

            Refresh(null);
        }

        // ── Building ──────────────────────────────────────────────────────────

        private void BuildTimeRows()
        {
            var time = Row("mdix-pop__row");
            time.Add(Caption("Time"));
            _hour     = TimeBox(2, "Hour, 00 to 23");
            _minute   = TimeBox(2, "Minute, 00 to 59");
            _second   = TimeBox(2, "Second, 00 to 59");
            _fraction = TimeBox(7, "Fraction of a second, up to 7 digits (optional)");
            _fraction.style.width = 62;

            time.Add(_hour);   time.Add(Separator(":"));
            time.Add(_minute); time.Add(Separator(":"));
            time.Add(_second); time.Add(Separator("."));
            time.Add(_fraction);
            _root.Add(time);

            var zone = Row("mdix-pop__row");
            zone.Add(Caption("Zone"));
            _zoneNone   = Segment("None",   "No zone written",              () => OnZone(MdixZoneKind.None));
            _zoneUtc    = Segment("UTC",    "Written with a Z",             () => OnZone(MdixZoneKind.Utc));
            _zoneOffset = Segment("Offset", "Written as +hh:mm or -hh:mm",  () => OnZone(MdixZoneKind.Offset));
            zone.Add(_zoneNone); zone.Add(_zoneUtc); zone.Add(_zoneOffset);

            _offset = new TextField { tooltip = "Offset from UTC, like +05:30 or -08:00 (up to 14:00)." };
            _offset.maxLength    = 6;
            _offset.style.width  = 64;
            _offset.RegisterValueChangedCallback(_ => OnOffsetEdited());
            zone.Add(_offset);
            _root.Add(zone);
        }

        private TextField TimeBox(int maxLength, string tooltip)
        {
            var box = new TextField { tooltip = tooltip };
            box.maxLength   = maxLength;
            box.style.width = 30;
            box.RegisterValueChangedCallback(_ => OnTimeEdited(box));
            return box;
        }

        private Button NavButton(string glyph, string tooltip, int months)
        {
            var button = new Button(() => OnMove(months)) { text = glyph, tooltip = tooltip };
            button.AddToClassList("mdix-btn");
            button.AddToClassList("mdix-cal__navbtn");
            return button;
        }

        private static Button Segment(string text, string tooltip, Action click)
        {
            var button = new Button(click) { text = text, tooltip = tooltip };
            button.AddToClassList("mdix-btn");
            button.AddToClassList("mdix-pop__seg");
            return button;
        }

        private static VisualElement Row(string className)
        {
            var row = new VisualElement();
            row.AddToClassList(className);
            return row;
        }

        private static Label Cell(string text, string className)
        {
            var label = new Label(text);
            label.AddToClassList(className);
            return label;
        }

        private static Label Caption(string text) { return Cell(text, "mdix-pop__caption"); }
        private static Label Separator(string text) { return Cell(text, "mdix-pop__sep"); }

        // ── Inputs ────────────────────────────────────────────────────────────

        private void OnTextEdited(string value)
        {
            if (_syncing) return;

            string error;
            if (_model.TrySetText(value, out error)) { Fail(null, null);   Refresh(_text); }
            else                                      { Fail(error, _text);                  }
            RaiseChanged();
        }

        private void OnTimeEdited(TextField source)
        {
            if (_syncing) return;

            string error;
            if (_model.TrySetTime(_hour.value, _minute.value, _second.value, _fraction.value, out error))
            {
                Fail(null, null);
                Refresh(source);
            }
            else
            {
                Fail(error, source);
            }
            RaiseChanged();
        }

        private void OnOffsetEdited()
        {
            if (_syncing) return;

            string error;
            if (_model.TrySetOffsetText(_offset.value, out error)) { Fail(null, null);    Refresh(_offset); }
            else                                                    { Fail(error, _offset);                  }
            RaiseChanged();
        }

        private void OnZone(MdixZoneKind kind)
        {
            _model.SetZone(kind);
            Fail(null, null);
            Refresh(null);
            RaiseChanged();
        }

        private void OnDayClicked(int index)
        {
            var cell = _cells[index];
            if (!cell.Exists) return;

            _model.SelectDay(cell.Year, cell.Month, cell.Day);
            Fail(null, null);
            Refresh(null);
            RaiseChanged();
        }

        private void OnMove(int months)
        {
            _model.MoveMonth(months);        // the view only: the chosen value and any error stay
            Refresh(null);
        }

        private void OnNowClicked()
        {
            _model.SetNow(DateTime.UtcNow, DateTime.Now);
            Fail(null, null);
            Refresh(null);
            RaiseChanged();
        }

        // ── Showing the model ─────────────────────────────────────────────────

        // Writes the model into every input except the one being typed in (<paramref name="typing"/>),
        // so the caret does not jump.
        private void Refresh(VisualElement typing)
        {
            _syncing = true;
            try
            {
                _cells = _model.Cells();
                var today = DateTime.Now;

                _month.text = DateTimeFormatInfo.InvariantInfo.GetMonthName(_model.ViewMonth) + " " + _model.ViewYear.ToString("D4", Inv);

                for (int i = 0; i < _days.Length; i++)
                {
                    var cell  = _cells[i];
                    var label = _days[i];

                    label.text = cell.Exists ? cell.Day.ToString(Inv) : string.Empty;
                    label.EnableInClassList("mdix-cal__day--none",     !cell.Exists);
                    label.EnableInClassList("mdix-cal__day--out",      cell.Exists && !cell.InViewMonth);
                    label.EnableInClassList("mdix-cal__day--selected", _model.IsSelected(cell));
                    label.EnableInClassList("mdix-cal__day--today",
                        cell.Exists && cell.Year == today.Year && cell.Month == today.Month && cell.Day == today.Day);
                }

                if (!ReferenceEquals(typing, _text)) _text.SetValueWithoutNotify(_model.Text);

                if (_model.IncludeTime)
                {
                    var v = _model.Value;
                    Put(_hour,     typing, v.Hour.ToString("D2", Inv));
                    Put(_minute,   typing, v.Minute.ToString("D2", Inv));
                    Put(_second,   typing, v.Second.ToString("D2", Inv));
                    Put(_fraction, typing, v.Fraction ?? string.Empty);
                    Put(_offset,   typing, _model.OffsetText);

                    _zoneNone.EnableInClassList("mdix-pop__seg--on",   v.Zone == MdixZoneKind.None);
                    _zoneUtc.EnableInClassList("mdix-pop__seg--on",    v.Zone == MdixZoneKind.Utc);
                    _zoneOffset.EnableInClassList("mdix-pop__seg--on", v.Zone == MdixZoneKind.Offset);
                    _offset.style.display = v.Zone == MdixZoneKind.Offset ? DisplayStyle.Flex : DisplayStyle.None;
                }
            }
            finally
            {
                _syncing = false;
            }
        }

        private static void Put(TextField field, VisualElement typing, string value)
        {
            if (!ReferenceEquals(field, typing)) field.SetValueWithoutNotify(value);
        }

        // ── Errors ────────────────────────────────────────────────────────────

        private void Fail(string error, VisualElement source)
        {
            Error = error;

            if (_errorSource != null) _errorSource.RemoveFromClassList("mdix-pop__input--bad");
            _errorSource = null;

            if (error != null && source != null)
            {
                source.AddToClassList("mdix-pop__input--bad");
                _errorSource = source;
            }
        }

        private void RaiseChanged()
        {
            var handler = Changed;
            if (handler != null) handler();
        }
    }
}
