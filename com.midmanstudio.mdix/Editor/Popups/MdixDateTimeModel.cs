using System;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>One square of the month grid.</summary>
    internal struct MdixCalendarCell
    {
        public int  Year, Month, Day;     // Day is 0 for a square outside the calendar (before year 1, after year 9999)
        public bool InViewMonth;

        public bool Exists { get { return Day > 0; } }
    }

    /// <summary>
    /// What the date and timestamp picker is editing, without Unity types so it can be tested on its own.
    ///
    /// It holds the chosen value and the month the grid is showing. Every input of the picker (the text box,
    /// a day, the time boxes, the zone) changes the value through one method here, so they cannot disagree.
    /// A change that does not parse is refused and leaves the value as it was. A timestamp keeps its zone
    /// and fraction digits unless the person changes them.
    /// </summary>
    internal sealed class MdixDateTimeModel
    {
        /// <summary>Six weeks of seven days, Monday first.</summary>
        public const int GridCells = 42;

        private MdixTimestampParts _value;

        /// <summary>True for a timestamp, false for a date.</summary>
        public bool IncludeTime { get; private set; }

        /// <summary>The month the grid shows.</summary>
        public int ViewYear  { get; private set; }
        public int ViewMonth { get; private set; }

        public MdixTimestampParts Value { get { return _value; } }

        /// <param name="initialText">The literal's text; if it cannot be read the value starts at <paramref name="localNow"/>.</param>
        public MdixDateTimeModel(bool includeTime, string initialText, DateTime localNow)
        {
            IncludeTime = includeTime;
            _value      = new MdixTimestampParts { Fraction = string.Empty };

            string ignored;
            if (!TrySetText(initialText, out ignored))
            {
                _value.Year = localNow.Year; _value.Month = localNow.Month; _value.Day = localNow.Day;
                if (includeTime) { _value.Hour = localNow.Hour; _value.Minute = localNow.Minute; _value.Second = localNow.Second; }
            }

            ViewYear  = _value.Year;
            ViewMonth = _value.Month;
        }

        /// <summary>The literal that would be written: YYYY-MM-DD, or the canonical timestamp.</summary>
        public string Text
        {
            get
            {
                return IncludeTime
                    ? MdixTemporalText.FormatTimestamp(_value)
                    : MdixTemporalText.FormatDate(_value.Year, _value.Month, _value.Day);
            }
        }

        // ── Whole value ───────────────────────────────────────────────────────

        /// <summary>Takes typed text (the loose forms are fine). On success the grid moves to that month.</summary>
        public bool TrySetText(string text, out string error)
        {
            error = null;

            if (IncludeTime)
            {
                MdixTimestampParts parts;
                if (!MdixTemporalText.TryParseTimestamp(text, false, out parts, out error)) return false;
                _value = parts;
            }
            else
            {
                int y, m, d;
                if (!MdixTemporalText.TryParseDate(text, false, out y, out m, out d))
                {
                    error = "Enter a real date, like 2026-01-15.";
                    return false;
                }
                _value.Year = y; _value.Month = m; _value.Day = d;
            }

            ViewYear  = _value.Year;
            ViewMonth = _value.Month;
            return true;
        }

        /// <summary>Chooses a day. The grid moves to its month, so a day shown greyed from the next month is fine to pick.</summary>
        public void SelectDay(int year, int month, int day)
        {
            _value.Year = year; _value.Month = month; _value.Day = day;
            ViewYear    = year; ViewMonth    = month;
        }

        /// <summary>
        /// Sets the date to today and, for a timestamp, the time to now (seconds only, the fraction is cleared).
        /// The time is read in the timestamp's own zone: local time with no zone, UTC for Z, UTC shifted for an offset.
        /// </summary>
        public void SetNow(DateTime utcNow, DateTime localNow)
        {
            var t = localNow;
            if (IncludeTime)
            {
                if (_value.Zone == MdixZoneKind.Utc)         t = utcNow;
                else if (_value.Zone == MdixZoneKind.Offset) t = utcNow.AddMinutes(_value.OffsetMinutes);
            }

            _value.Year = t.Year; _value.Month = t.Month; _value.Day = t.Day;
            if (IncludeTime)
            {
                _value.Hour = t.Hour; _value.Minute = t.Minute; _value.Second = t.Second;
                _value.Fraction = string.Empty;
            }

            ViewYear  = _value.Year;
            ViewMonth = _value.Month;
        }

        // ── The month grid ────────────────────────────────────────────────────

        /// <summary>Moves the grid by whole months. Only the view moves; the chosen day stays. Stops at 0001-01 and 9999-12.</summary>
        public void MoveMonth(int delta)
        {
            long index = (long)ViewYear * 12 + (ViewMonth - 1) + delta;

            const long first = 1L * 12;                 // 0001-01
            const long last  = 9999L * 12 + 11;         // 9999-12
            if (index < first) index = first;
            if (index > last)  index = last;

            ViewYear  = (int)(index / 12);
            ViewMonth = (int)(index % 12) + 1;
        }

        public void MoveYear(int delta) { MoveMonth(delta * 12); }

        /// <summary>The 42 squares of the grid shown now: the month, padded with the days around it.</summary>
        public MdixCalendarCell[] Cells()
        {
            var cells = new MdixCalendarCell[GridCells];
            var lead  = MdixTemporalText.WeekdayMondayFirst(ViewYear, ViewMonth, 1);
            var first = new DateTime(ViewYear, ViewMonth, 1);

            for (int i = 0; i < GridCells; i++)
            {
                var ticks = first.Ticks + (long)(i - lead) * TimeSpan.TicksPerDay;
                if (ticks < DateTime.MinValue.Ticks || ticks > DateTime.MaxValue.Ticks) continue;   // stays Day 0

                var d = new DateTime(ticks);
                cells[i] = new MdixCalendarCell
                {
                    Year = d.Year, Month = d.Month, Day = d.Day,
                    InViewMonth = d.Year == ViewYear && d.Month == ViewMonth,
                };
            }
            return cells;
        }

        public bool IsSelected(MdixCalendarCell cell)
        {
            return cell.Exists && cell.Year == _value.Year && cell.Month == _value.Month && cell.Day == _value.Day;
        }

        // ── Time and zone (timestamps) ────────────────────────────────────────

        /// <summary>Sets hour, minute, second and the fraction of a second from what was typed. The zone is untouched.</summary>
        public bool TrySetTime(string hour, string minute, string second, string fraction, out string error)
        {
            error = null;
            if (!IncludeTime) return true;

            var text = MdixTemporalText.FormatDate(_value.Year, _value.Month, _value.Day) + "T" +
                       (hour ?? string.Empty).Trim() + ":" + (minute ?? string.Empty).Trim() + ":" + (second ?? string.Empty).Trim();
            fraction = (fraction ?? string.Empty).Trim();
            if (fraction.Length > 0) text += "." + fraction;

            MdixTimestampParts parts;
            if (!MdixTemporalText.TryParseTimestamp(text, false, out parts, out error)) return false;

            _value.Hour = parts.Hour; _value.Minute = parts.Minute; _value.Second = parts.Second;
            _value.Fraction = parts.Fraction;
            return true;
        }

        /// <summary>No zone, UTC (Z) or an offset. Choosing Offset starts at +00:00 unless it already was one.</summary>
        public void SetZone(MdixZoneKind kind)
        {
            if (kind == MdixZoneKind.Offset && _value.Zone != MdixZoneKind.Offset) _value.OffsetMinutes = 0;
            _value.Zone = kind;
        }

        public string OffsetText { get { return MdixTemporalText.FormatOffset(_value.OffsetMinutes); } }

        /// <summary>Sets the offset from text like +05:30, -08:00 or +05 (up to 14:00) and makes the zone an offset.</summary>
        public bool TrySetOffsetText(string text, out string error)
        {
            error = null;

            MdixTimestampParts parts;
            string ignored;
            var probe = "2000-01-01T00:00:00" + (text ?? string.Empty).Trim();
            if (!MdixTemporalText.TryParseTimestamp(probe, false, out parts, out ignored) || parts.Zone != MdixZoneKind.Offset)
            {
                error = "The offset must be like +05:30 or -08:00, up to 14:00.";
                return false;
            }

            _value.Zone          = MdixZoneKind.Offset;
            _value.OffsetMinutes = parts.OffsetMinutes;
            return true;
        }
    }
}
