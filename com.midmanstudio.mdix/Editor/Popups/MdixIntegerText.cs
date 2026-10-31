using System;
using System.Globalization;
using System.Text;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Reads and writes DixScript integer literals the way the compiler's lexer does, without Unity types, so it can
    /// be tested on its own.
    ///
    /// A literal is decimal (<c>1_000</c>, a leading minus is allowed), hex (<c>0xFF</c>) or binary (<c>0b1010</c>).
    /// <c>_</c> may separate digits, and a trailing <c>L</c> makes it a Long. Without the L it is an Int when it fits
    /// in 32 bits and is promoted to a Long when it does not (so <c>0xFFFFFFFF</c> is the Long 4294967295, not -1).
    /// Hex and binary literals are never negative.
    /// </summary>
    internal static class MdixIntegerText
    {
        private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        /// <summary>16 for a 0x literal, 2 for a 0b literal, 10 for anything else.</summary>
        public static int RadixOf(string literal)
        {
            if (literal == null) return 10;

            var t = literal.TrimStart();
            if (t.Length >= 2 && t[0] == '0')
            {
                if (t[1] == 'x' || t[1] == 'X') return 16;
                if (t[1] == 'b' || t[1] == 'B') return 2;
            }
            return 10;
        }

        /// <summary>True when the literal ends in L or l.</summary>
        public static bool HasLongSuffix(string literal)
        {
            if (string.IsNullOrEmpty(literal)) return false;
            var last = literal[literal.Length - 1];
            return last == 'L' || last == 'l';
        }

        /// <summary>
        /// Reads an integer literal. <paramref name="isLong"/> says whether the compiler would make it a Long;
        /// <paramref name="radix"/> is the notation it was written in. False for anything that is not one.
        /// </summary>
        public static bool TryParse(string text, out long value, out bool isLong, out int radix)
        {
            value = 0; isLong = false;
            radix = RadixOf(text);

            if (string.IsNullOrWhiteSpace(text)) return false;
            text = text.Trim();

            var suffix = HasLongSuffix(text);
            var body   = suffix ? text.Substring(0, text.Length - 1) : text;

            if (radix == 10)
            {
                if (!long.TryParse(body.Replace("_", string.Empty), NumberStyles.AllowLeadingSign, Inv, out value)) return false;
                if (body.IndexOf('_') == 0 || body.EndsWith("_", StringComparison.Ordinal)) return false;
            }
            else
            {
                var digits = body.Substring(2).Replace("_", string.Empty);
                if (digits.Length == 0) return false;

                if (radix == 16)
                {
                    ulong u;
                    if (!ulong.TryParse(digits, NumberStyles.AllowHexSpecifier, Inv, out u) || u > long.MaxValue) return false;
                    value = (long)u;
                }
                else
                {
                    foreach (var c in digits) if (c != '0' && c != '1') return false;

                    var significant = digits.TrimStart('0');
                    if (significant.Length > 63) return false;
                    value = significant.Length == 0 ? 0 : Convert.ToInt64(significant, 2);
                }
            }

            isLong = suffix || value < int.MinValue || value > int.MaxValue;
            return true;
        }

        /// <summary>
        /// Writes a value in the given notation: decimal, <c>0x</c> hex (capitals) or <c>0b</c> binary, with an L when
        /// <paramref name="longSuffix"/> is set. A negative value can only be written in decimal.
        /// </summary>
        public static string Format(long value, int radix, bool longSuffix)
        {
            string body;
            if (radix == 16)      body = "0x" + value.ToString("X", Inv);
            else if (radix == 2)  body = "0b" + Convert.ToString(value, 2);
            else                  body = value.ToString(Inv);

            return longSuffix ? body + "L" : body;
        }

        /// <summary>
        /// The literal for a new value that keeps the old literal's notation and type. An Int stays in the 32-bit range.
        /// A Long keeps an L it already had, and gets one when the value would otherwise read as an Int.
        /// </summary>
        public static bool TryRewrite(
            string oldLiteral, bool isLongKind, string input, out string literal, out long value, out string error)
        {
            literal = null; error = null;

            bool inputLong; int ignored;
            if (!TryParse(input, out value, out inputLong, out ignored))
            {
                error = isLongKind ? "Enter a whole number." : "Enter a whole number from -2147483648 to 2147483647.";
                return false;
            }

            var radix = RadixOf(oldLiteral);
            if (radix != 10 && value < 0)
            {
                error = "A " + (radix == 16 ? "hex" : "binary") + " literal cannot be negative.";
                return false;
            }

            if (!isLongKind)
            {
                if (value < int.MinValue || value > int.MaxValue)
                {
                    error = "This is a 32-bit int: use a value up to " + (radix == 16 ? "0x7FFFFFFF" : "2147483647") + ".";
                    return false;
                }
                literal = Format(value, radix, false);
                return true;
            }

            var inIntRange = value >= int.MinValue && value <= int.MaxValue;
            literal = Format(value, radix, HasLongSuffix(oldLiteral) || inIntRange);
            return true;
        }
    }
}
