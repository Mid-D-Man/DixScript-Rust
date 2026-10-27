using System;
using System.Globalization;
using System.Text;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>How a timestamp names its time zone.</summary>
    internal enum MdixZoneKind { None, Utc, Offset }

    /// <summary>The parts of a timestamp literal, kept separate so a picker can change one without losing the rest.</summary>
    internal struct MdixTimestampParts
    {
        public int          Year, Month, Day, Hour, Minute, Second;
        public string       Fraction;          // digits after the point; empty for none, 1 to 7 digits
        public MdixZoneKind Zone;
        public int          OffsetMinutes;     // signed; only used when Zone is Offset
    }

    /// <summary>
    /// Reads and writes DixScript date and timestamp text. No Unity types, so it can be tested on its own.
    ///
    /// DixScript has two temporal literals and no time-only one: a date (<c>2026-01-15</c>) and a timestamp
    /// (<c>2025-01-15T10:30:00.250+05:30</c>, with the fraction and the zone both optional). The compiler stores
    /// the text as written, so a value is edited by writing new text. The zone and the fraction digits typed
    /// by the person are kept as they are.
    ///
    /// Strict reading (<see cref="IsDateLiteral"/>, <see cref="IsTimestampLiteral"/>) decides what the
    /// Explorer treats as a literal. Loose reading (<c>strict: false</c>) is for what a person types.
    /// </summary>
    internal static class MdixTemporalText
    {
        /// <summary>The runtime reads timestamps with DateTimeOffset, which takes up to 7 fraction digits.</summary>
        public const int MaxFractionDigits = 7;

        /// <summary>The largest time zone offset, in minutes (DateTimeOffset allows +-14:00).</summary>
        public const int MaxOffsetMinutes = 14 * 60;

        private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        // ── Dates ─────────────────────────────────────────────────────────────

        /// <summary>True when the text is exactly YYYY-MM-DD and a real calendar day.</summary>
        public static bool IsDateLiteral(string text)
        {
            int y, m, d;
            return TryParseDate(text, true, out y, out m, out d);
        }

        /// <summary>
        /// Reads a date. Strict: exactly YYYY-MM-DD. Loose: the month and day may have one digit
        /// (2026-1-5) and spaces around the text are ignored. The day must exist in that month.
        /// </summary>
        public static bool TryParseDate(string text, bool strict, out int year, out int month, out int day)
        {
            year = month = day = 0;
            if (text == null) return false;
            if (!strict) text = text.Trim();

            var parts = text.Split('-');
            if (parts.Length != 3) return false;

            var py = parts[0]; var pm = parts[1]; var pd = parts[2];
            if (py.Length != 4) return false;
            if (strict ? (pm.Length != 2 || pd.Length != 2) : (pm.Length < 1 || pm.Length > 2 || pd.Length < 1 || pd.Length > 2))
                return false;

            if (!AllDigits(py) || !AllDigits(pm) || !AllDigits(pd)) return false;

            year  = int.Parse(py, Inv);
            month = int.Parse(pm, Inv);
            day   = int.Parse(pd, Inv);

            if (year < 1 || year > 9999 || month < 1 || month > 12) return false;
            return day >= 1 && day <= DateTime.DaysInMonth(year, month);
        }

        public static string FormatDate(int year, int month, int day) =>
            year.ToString("D4", Inv) + "-" + month.ToString("D2", Inv) + "-" + day.ToString("D2", Inv);

        // ── Timestamps ────────────────────────────────────────────────────────

        /// <summary>True when the text is a canonical timestamp: the form <see cref="FormatTimestamp"/> writes.</summary>
        public static bool IsTimestampLiteral(string text)
        {
            MdixTimestampParts parts;
            string error;
            if (!TryParseTimestamp(text, true, out parts, out error)) return false;
            return FormatTimestamp(parts) == text;
        }

        /// <summary>
        /// Reads a timestamp. Strict: YYYY-MM-DDThh:mm:ss, then optionally .f to .fffffff, then optionally
        /// Z or +hh:mm / -hh:mm. Loose: the T may be a space, the seconds may be left out, the date may have
        /// one-digit months and days, the zone may be +hh or +hhmm, and spaces around the text are ignored.
        /// </summary>
        public static bool TryParseTimestamp(string text, bool strict, out MdixTimestampParts parts, out string error)
        {
            parts = new MdixTimestampParts { Fraction = string.Empty };
            error = null;
            if (text == null) { error = "Enter a date and time."; return false; }
            if (!strict) text = text.Trim();

            var split = text.IndexOf('T');
            if (split < 0 && !strict) split = text.IndexOf(' ');
            if (split < 0 && !strict) split = text.IndexOf('t');
            if (split < 0)
            {
                error = strict ? "A timestamp is written date T time." : "Enter a date and a time, like 2026-01-15 10:30:00.";
                return false;
            }

            int y, mo, d;
            if (!TryParseDate(text.Substring(0, split), strict, out y, out mo, out d))
            {
                error = "The date part must be a real day, written YYYY-MM-DD.";
                return false;
            }
            parts.Year = y; parts.Month = mo; parts.Day = d;

            var time = text.Substring(split + 1);
            if (!strict) time = time.Trim();

            // The zone starts at Z, + or - (a time itself never contains them).
            var zoneAt = -1;
            for (int i = 0; i < time.Length; i++)
            {
                var c = time[i];
                if (c == 'Z' || c == '+' || c == '-' || (!strict && c == 'z')) { zoneAt = i; break; }
            }

            var clock = zoneAt < 0 ? time : time.Substring(0, zoneAt);
            var zone  = zoneAt < 0 ? string.Empty : time.Substring(zoneAt);
            if (!strict) clock = clock.TrimEnd();

            string fraction = string.Empty;
            var dot = clock.IndexOf('.');
            if (dot >= 0)
            {
                fraction = clock.Substring(dot + 1);
                clock    = clock.Substring(0, dot);

                if (fraction.Length < 1 || fraction.Length > MaxFractionDigits || !AllDigits(fraction))
                {
                    error = "The fraction of a second must be 1 to " + MaxFractionDigits + " digits.";
                    return false;
                }
            }

            var clockParts = clock.Split(':');
            var shapeOk    = strict ? clockParts.Length == 3 : (clockParts.Length == 2 || clockParts.Length == 3);
            if (!shapeOk || (fraction.Length > 0 && clockParts.Length != 3))
            {
                error = "The time must be hh:mm:ss.";
                return false;
            }

            int h, mi, s = 0;
            if (!TryTwoDigits(clockParts[0], strict, 23, out h) ||
                !TryTwoDigits(clockParts[1], strict, 59, out mi) ||
                (clockParts.Length == 3 && !TryTwoDigits(clockParts[2], strict, 59, out s)))
            {
                error = "Hours are 00 to 23, minutes and seconds 00 to 59.";
                return false;
            }
            parts.Hour = h; parts.Minute = mi; parts.Second = s;
            parts.Fraction = fraction;

            if (zone.Length == 0) { parts.Zone = MdixZoneKind.None; return true; }

            if (zone == "Z" || (!strict && zone == "z")) { parts.Zone = MdixZoneKind.Utc; return true; }

            int minutes;
            if (!TryOffset(zone, strict, out minutes))
            {
                error = "The zone must be Z or +hh:mm / -hh:mm, up to 14:00.";
                return false;
            }
            parts.Zone          = MdixZoneKind.Offset;
            parts.OffsetMinutes = minutes;
            return true;
        }

        /// <summary>The canonical text of a timestamp: seconds always, the fraction and the zone only when present.</summary>
        public static string FormatTimestamp(MdixTimestampParts p)
        {
            var sb = new StringBuilder(32);
            sb.Append(FormatDate(p.Year, p.Month, p.Day)).Append('T');
            sb.Append(p.Hour.ToString("D2", Inv)).Append(':').Append(p.Minute.ToString("D2", Inv)).Append(':').Append(p.Second.ToString("D2", Inv));

            if (!string.IsNullOrEmpty(p.Fraction)) sb.Append('.').Append(p.Fraction);

            if (p.Zone == MdixZoneKind.Utc)         sb.Append('Z');
            else if (p.Zone == MdixZoneKind.Offset) sb.Append(FormatOffset(p.OffsetMinutes));
            return sb.ToString();
        }

        /// <summary>+hh:mm or -hh:mm for a signed number of minutes.</summary>
        public static string FormatOffset(int minutes)
        {
            var sign = minutes < 0 ? '-' : '+';
            var abs  = Math.Abs(minutes);
            return sign + (abs / 60).ToString("D2", Inv) + ":" + (abs % 60).ToString("D2", Inv);
        }

        // ── Calendar helpers (for a month grid) ───────────────────────────────

        public static int DaysInMonth(int year, int month) => DateTime.DaysInMonth(year, month);

        /// <summary>0 for Monday to 6 for Sunday: the column of a day in a Monday-first month grid.</summary>
        public static int WeekdayMondayFirst(int year, int month, int day) =>
            ((int)new DateTime(year, month, day).DayOfWeek + 6) % 7;

        // ── Small readers ─────────────────────────────────────────────────────

        private static bool AllDigits(string s)
        {
            if (s.Length == 0) return false;
            foreach (var c in s) if (c < '0' || c > '9') return false;
            return true;
        }

        // Strict: exactly two digits. Loose: one or two.
        private static bool TryTwoDigits(string s, bool strict, int max, out int value)
        {
            value = 0;
            if (strict ? s.Length != 2 : (s.Length < 1 || s.Length > 2)) return false;
            if (!AllDigits(s)) return false;
            value = int.Parse(s, Inv);
            return value <= max;
        }

        // +hh:mm or -hh:mm (loose also +hh and +hhmm), at most 14:00.
        private static bool TryOffset(string zone, bool strict, out int minutes)
        {
            minutes = 0;
            if (zone.Length < 3) return false;
            var sign = zone[0] == '-' ? -1 : zone[0] == '+' ? 1 : 0;
            if (sign == 0) return false;

            var body = zone.Substring(1);
            int hh, mm = 0;

            if (body.Length == 5 && body[2] == ':')
            {
                if (!AllDigits(body.Substring(0, 2)) || !AllDigits(body.Substring(3, 2))) return false;
                hh = int.Parse(body.Substring(0, 2), Inv);
                mm = int.Parse(body.Substring(3, 2), Inv);
            }
            else if (!strict && body.Length == 4 && AllDigits(body))
            {
                hh = int.Parse(body.Substring(0, 2), Inv);
                mm = int.Parse(body.Substring(2, 2), Inv);
            }
            else if (!strict && body.Length == 2 && AllDigits(body))
            {
                hh = int.Parse(body, Inv);
            }
            else return false;

            if (mm > 59) return false;
            var total = hh * 60 + mm;
            if (total > MaxOffsetMinutes) return false;

            minutes = sign * total;
            return true;
        }
    }
}
