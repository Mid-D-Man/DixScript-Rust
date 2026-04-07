using System;
using System.Collections.Concurrent;
using System.Text.RegularExpressions;

namespace MidManStudio.Mdix.Core
{
    #region MdixHexColor

    /// <summary>
    /// A color parsed from a DixScript hex color literal (e.g. <c>#FF5733</c> or <c>#FF5733FF</c>).
    /// Channel values are normalised to the 0–1 range as floats.
    /// No UnityEngine dependency — Unity layer converts to UnityEngine.Color separately.
    /// </summary>
    public readonly struct MdixHexColor : IEquatable<MdixHexColor>
    {
        /// <summary>The raw hex string as it appeared in the data (e.g. <c>#FF5733</c>).</summary>
        public string RawString { get; }

        /// <summary>Red channel, 0–1.</summary>
        public float R { get; }

        /// <summary>Green channel, 0–1.</summary>
        public float G { get; }

        /// <summary>Blue channel, 0–1.</summary>
        public float B { get; }

        /// <summary>Alpha channel, 0–1. Defaults to 1 when no alpha byte is present.</summary>
        public float A { get; }

        private MdixHexColor(string raw, float r, float g, float b, float a)
        {
            RawString = raw;
            R = r; G = g; B = b; A = a;
        }

        // ── Parsing ───────────────────────────────────────────────────────────

        /// <summary>
        /// Parses a hex string into a <see cref="MdixHexColor"/>.
        /// Supports <c>#RGB</c>, <c>#RRGGBB</c>, and <c>#RRGGBBAA</c>.
        /// Returns a failed result if the string is malformed.
        /// </summary>
        public static MdixResult<MdixHexColor> Parse(string raw)
        {
            if (string.IsNullOrEmpty(raw))
                return MdixError.NativeError("Hex color string is null or empty.");

            var hex = raw.StartsWith("#", StringComparison.Ordinal) ? raw.Substring(1) : raw;

            try
            {
                float r, g, b, a;

                switch (hex.Length)
                {
                    case 3:
                        r = ParseNibble(hex[0]) / 15f;
                        g = ParseNibble(hex[1]) / 15f;
                        b = ParseNibble(hex[2]) / 15f;
                        a = 1f;
                        break;

                    case 6:
                        r = ParseByte(hex, 0) / 255f;
                        g = ParseByte(hex, 2) / 255f;
                        b = ParseByte(hex, 4) / 255f;
                        a = 1f;
                        break;

                    case 8:
                        r = ParseByte(hex, 0) / 255f;
                        g = ParseByte(hex, 2) / 255f;
                        b = ParseByte(hex, 4) / 255f;
                        a = ParseByte(hex, 6) / 255f;
                        break;

                    default:
                        return MdixError.NativeError(
                            $"Invalid hex color length '{raw}'. Expected #RGB, #RRGGBB, or #RRGGBBAA.");
                }

                return MdixResult<MdixHexColor>.Ok(new MdixHexColor(raw, r, g, b, a));
            }
            catch (Exception ex)
            {
                return MdixError.NativeError($"Failed to parse hex color '{raw}': {ex.Message}");
            }
        }

        private static int ParseByte(string hex, int offset) =>
            (HexVal(hex[offset]) << 4) | HexVal(hex[offset + 1]);

        private static int ParseNibble(char c) => HexVal(c);

        private static int HexVal(char c)
        {
            if (c >= '0' && c <= '9') return c - '0';
            if (c >= 'a' && c <= 'f') return c - 'a' + 10;
            if (c >= 'A' && c <= 'F') return c - 'A' + 10;
            throw new FormatException($"Invalid hex character: '{c}'");
        }

        // ── Equality ──────────────────────────────────────────────────────────

        public bool Equals(MdixHexColor other) => RawString == other.RawString;
        public override bool Equals(object? obj) => obj is MdixHexColor other && Equals(other);
        public override int GetHashCode() => RawString?.GetHashCode() ?? 0;
        public override string ToString() => RawString;

        public static bool operator ==(MdixHexColor left, MdixHexColor right) =>  left.Equals(right);
        public static bool operator !=(MdixHexColor left, MdixHexColor right) => !left.Equals(right);
    }

    #endregion

    #region MdixBlob

    /// <summary>
    /// A binary blob stored as a base-64 encoded string in DixScript
    /// (syntax: <c>b:("base64data...")</c>).
    /// Call <see cref="ToBytes"/> to decode. Decoding is on-demand — this struct
    /// holds only the raw string.
    /// </summary>
    public readonly struct MdixBlob : IEquatable<MdixBlob>
    {
        /// <summary>The raw base-64 string as stored in the data.</summary>
        public string RawBase64 { get; }

        public MdixBlob(string rawBase64)
        {
            RawBase64 = rawBase64 ?? throw new ArgumentNullException(nameof(rawBase64));
        }

        /// <summary>
        /// Decodes the base-64 string to a byte array.
        /// Returns a failed result if the string is not valid base-64.
        /// </summary>
        public MdixResult<byte[]> ToBytes()
        {
            try
            {
                return MdixResult<byte[]>.Ok(Convert.FromBase64String(RawBase64));
            }
            catch (FormatException ex)
            {
                return MdixError.NativeError($"Blob is not valid base-64: {ex.Message}");
            }
        }

        /// <summary>
        /// Returns the decoded byte count without allocating the full array.
        /// Returns -1 if the base-64 string is invalid.
        /// </summary>
        public int DecodedByteCount()
        {
            if (string.IsNullOrEmpty(RawBase64)) return 0;
            try
            {
                // Base64 length formula: every 4 chars = 3 bytes, minus padding
                int len     = RawBase64.Length;
                int padding = RawBase64.EndsWith("==", StringComparison.Ordinal) ? 2
                            : RawBase64.EndsWith("=",  StringComparison.Ordinal) ? 1
                            : 0;
                return (len / 4 * 3) - padding;
            }
            catch
            {
                return -1;
            }
        }

        // ── Equality ──────────────────────────────────────────────────────────

        public bool Equals(MdixBlob other) => RawBase64 == other.RawBase64;
        public override bool Equals(object? obj) => obj is MdixBlob other && Equals(other);
        public override int GetHashCode() => RawBase64?.GetHashCode() ?? 0;
        public override string ToString() => $"b:({RawBase64})";

        public static bool operator ==(MdixBlob left, MdixBlob right) =>  left.Equals(right);
        public static bool operator !=(MdixBlob left, MdixBlob right) => !left.Equals(right);
    }

    #endregion

    #region MdixRegex

    /// <summary>
    /// A regular expression pattern stored in DixScript
    /// (syntax: <c>r:("^[a-z@.]+$")</c>).
    /// Call <see cref="ToRegex"/> to get a compiled <see cref="Regex"/> instance.
    /// Compiled instances are cached by pattern string.
    /// </summary>
    public readonly struct MdixRegex : IEquatable<MdixRegex>
    {
        // Compiled instances are expensive — cache them keyed by pattern.
        private static readonly ConcurrentDictionary<string, Regex> CompiledCache =
            new ConcurrentDictionary<string, Regex>();

        /// <summary>The raw pattern string as stored in the data.</summary>
        public string Pattern { get; }

        public MdixRegex(string pattern)
        {
            Pattern = pattern ?? throw new ArgumentNullException(nameof(pattern));
        }

        /// <summary>
        /// Returns a compiled <see cref="Regex"/> for this pattern.
        /// The instance is cached — calling this multiple times is safe and cheap.
        /// Returns a failed result if the pattern is invalid.
        /// </summary>
        public MdixResult<Regex> ToRegex()
        {
            try
            {
                var compiled = CompiledCache.GetOrAdd(
                    Pattern,
                    p => new Regex(p, RegexOptions.Compiled, TimeSpan.FromSeconds(5)));

                return MdixResult<Regex>.Ok(compiled);
            }
            catch (ArgumentException ex)
            {
                return MdixError.NativeError($"Invalid regex pattern '{Pattern}': {ex.Message}");
            }
        }

        /// <summary>
        /// Tests whether <paramref name="input"/> matches this pattern.
        /// Returns a failed result if the pattern is invalid.
        /// </summary>
        public MdixResult<bool> IsMatch(string input)
        {
            var regexResult = ToRegex();
            if (regexResult.IsFailure) return MdixResult<bool>.Err(regexResult.Error);
            return MdixResult<bool>.Ok(regexResult.SuccessResult.IsMatch(input));
        }

        // ── Equality ──────────────────────────────────────────────────────────

        public bool Equals(MdixRegex other) => Pattern == other.Pattern;
        public override bool Equals(object? obj) => obj is MdixRegex other && Equals(other);
        public override int GetHashCode() => Pattern?.GetHashCode() ?? 0;
        public override string ToString() => $"r:({Pattern})";

        public static bool operator ==(MdixRegex left, MdixRegex right) =>  left.Equals(right);
        public static bool operator !=(MdixRegex left, MdixRegex right) => !left.Equals(right);
    }

    #endregion

    #region MdixDate

    /// <summary>
    /// A date value parsed from a DixScript date literal (format: <c>YYYY-MM-DD</c>).
    /// Wraps <see cref="DateTime"/> with the time component always set to midnight UTC.
    /// </summary>
    public readonly struct MdixDate : IEquatable<MdixDate>, IComparable<MdixDate>
    {
        private static readonly string DateFormat = "yyyy-MM-dd";

        /// <summary>The underlying <see cref="DateTime"/>, always midnight UTC.</summary>
        public DateTime Value { get; }

        /// <summary>The raw date string as stored in the data (e.g. <c>2025-12-31</c>).</summary>
        public string RawString { get; }

        public int Year  => Value.Year;
        public int Month => Value.Month;
        public int Day   => Value.Day;

        private MdixDate(DateTime value, string raw)
        {
            Value     = value;
            RawString = raw;
        }

        // ── Parsing ───────────────────────────────────────────────────────────

        /// <summary>
        /// Parses a <c>YYYY-MM-DD</c> string into a <see cref="MdixDate"/>.
        /// Returns a failed result if the string is malformed.
        /// </summary>
        public static MdixResult<MdixDate> Parse(string raw)
        {
            if (string.IsNullOrEmpty(raw))
                return MdixError.NativeError("Date string is null or empty.");

            if (DateTime.TryParseExact(
                    raw,
                    DateFormat,
                    System.Globalization.CultureInfo.InvariantCulture,
                    System.Globalization.DateTimeStyles.AssumeUniversal |
                    System.Globalization.DateTimeStyles.AdjustToUniversal,
                    out var dt))
            {
                return MdixResult<MdixDate>.Ok(new MdixDate(dt, raw));
            }

            return MdixError.NativeError(
                $"Invalid date '{raw}'. Expected format: {DateFormat}");
        }

        // ── Equality and ordering ─────────────────────────────────────────────

        public bool Equals(MdixDate other) => Value == other.Value;
        public override bool Equals(object? obj) => obj is MdixDate other && Equals(other);
        public override int GetHashCode() => Value.GetHashCode();
        public int CompareTo(MdixDate other) => Value.CompareTo(other.Value);
        public override string ToString() => RawString;

        public static bool operator ==(MdixDate left, MdixDate right) =>  left.Equals(right);
        public static bool operator !=(MdixDate left, MdixDate right) => !left.Equals(right);
        public static bool operator  <(MdixDate left, MdixDate right) => left.CompareTo(right) <  0;
        public static bool operator  >(MdixDate left, MdixDate right) => left.CompareTo(right) >  0;
        public static bool operator <=(MdixDate left, MdixDate right) => left.CompareTo(right) <= 0;
        public static bool operator >=(MdixDate left, MdixDate right) => left.CompareTo(right) >= 0;
    }

    #endregion

    #region MdixTimestamp

    /// <summary>
    /// A timestamp value parsed from a DixScript timestamp literal (ISO 8601).
    /// Wraps <see cref="DateTimeOffset"/> to preserve timezone information.
    /// </summary>
    public readonly struct MdixTimestamp : IEquatable<MdixTimestamp>, IComparable<MdixTimestamp>
    {
        /// <summary>The underlying <see cref="DateTimeOffset"/>.</summary>
        public DateTimeOffset Value { get; }

        /// <summary>The raw timestamp string as stored in the data.</summary>
        public string RawString { get; }

        private MdixTimestamp(DateTimeOffset value, string raw)
        {
            Value     = value;
            RawString = raw;
        }

        // ── Parsing ───────────────────────────────────────────────────────────

        /// <summary>
        /// Parses an ISO 8601 string into a <see cref="MdixTimestamp"/>.
        /// Returns a failed result if the string is malformed.
        /// </summary>
        public static MdixResult<MdixTimestamp> Parse(string raw)
        {
            if (string.IsNullOrEmpty(raw))
                return MdixError.NativeError("Timestamp string is null or empty.");

            if (DateTimeOffset.TryParse(
                    raw,
                    System.Globalization.CultureInfo.InvariantCulture,
                    System.Globalization.DateTimeStyles.RoundtripKind,
                    out var dto))
            {
                return MdixResult<MdixTimestamp>.Ok(new MdixTimestamp(dto, raw));
            }

            return MdixError.NativeError(
                $"Invalid timestamp '{raw}'. Expected ISO 8601 format.");
        }

        /// <summary>Converts to UTC.</summary>
        public MdixTimestamp ToUtc() => new MdixTimestamp(Value.ToUniversalTime(), RawString);

        // ── Equality and ordering ─────────────────────────────────────────────

        public bool Equals(MdixTimestamp other) => Value == other.Value;
        public override bool Equals(object? obj) => obj is MdixTimestamp other && Equals(other);
        public override int GetHashCode() => Value.GetHashCode();
        public int CompareTo(MdixTimestamp other) => Value.CompareTo(other.Value);
        public override string ToString() => RawString;

        public static bool operator ==(MdixTimestamp left, MdixTimestamp right) =>  left.Equals(right);
        public static bool operator !=(MdixTimestamp left, MdixTimestamp right) => !left.Equals(right);
        public static bool operator  <(MdixTimestamp left, MdixTimestamp right) => left.CompareTo(right) <  0;
        public static bool operator  >(MdixTimestamp left, MdixTimestamp right) => left.CompareTo(right) >  0;
        public static bool operator <=(MdixTimestamp left, MdixTimestamp right) => left.CompareTo(right) <= 0;
        public static bool operator >=(MdixTimestamp left, MdixTimestamp right) => left.CompareTo(right) >= 0;
    }

    #endregion
}
