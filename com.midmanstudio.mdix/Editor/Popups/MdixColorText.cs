using System;
using System.Globalization;
using System.Text;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Reads and writes DixScript colour literals as bytes, for the colour picker, without Unity types so it can be
    /// tested on its own.
    ///
    /// A colour is <c>#RGB</c>, <c>#RGBA</c>, <c>#RRGGBB</c> or <c>#RRGGBBAA</c>. The short forms repeat each digit
    /// (<c>#F80C</c> is <c>#FF8800CC</c>), so they can only hold 4 bits per channel: a colour chosen with a picker
    /// is written in a short form only when it is exactly representable in one.
    /// </summary>
    internal static class MdixColorText
    {
        private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        /// <summary>Reads a colour. <paramref name="hasAlpha"/> and <paramref name="isShort"/> say how it was written.</summary>
        public static bool TryParse(string text, out byte r, out byte g, out byte b, out byte a, out bool hasAlpha, out bool isShort)
        {
            r = g = b = 0; a = 255; hasAlpha = false; isShort = false;

            if (text == null) return false;
            var t = text.Trim();
            if (t.Length < 4 || t[0] != '#') return false;

            var h = t.Substring(1);
            foreach (var c in h)
                if (!Uri.IsHexDigit(c)) return false;

            isShort = h.Length == 3 || h.Length == 4;
            if (!isShort && h.Length != 6 && h.Length != 8) return false;

            hasAlpha = h.Length == 4 || h.Length == 8;

            if (isShort)
            {
                r = (byte)(Nibble(h[0]) * 17);
                g = (byte)(Nibble(h[1]) * 17);
                b = (byte)(Nibble(h[2]) * 17);
                if (hasAlpha) a = (byte)(Nibble(h[3]) * 17);
            }
            else
            {
                r = Byte(h, 0); g = Byte(h, 2); b = Byte(h, 4);
                if (hasAlpha) a = Byte(h, 6);
            }
            return true;
        }

        /// <summary>
        /// Writes a colour in capitals. Alpha is written when <paramref name="withAlpha"/> is set or the colour is not
        /// opaque. The short form is used only when <paramref name="preferShort"/> is set and it loses nothing.
        /// </summary>
        public static string Format(byte r, byte g, byte b, byte a, bool withAlpha, bool preferShort)
        {
            var alpha = withAlpha || a != 255;

            if (preferShort && Repeats(r) && Repeats(g) && Repeats(b) && (!alpha || Repeats(a)))
            {
                var s = new StringBuilder("#");
                s.Append(Hex(r >> 4)).Append(Hex(g >> 4)).Append(Hex(b >> 4));
                if (alpha) s.Append(Hex(a >> 4));
                return s.ToString();
            }

            var sb = new StringBuilder("#");
            sb.Append(r.ToString("X2", Inv)).Append(g.ToString("X2", Inv)).Append(b.ToString("X2", Inv));
            if (alpha) sb.Append(a.ToString("X2", Inv));
            return sb.ToString();
        }

        private static bool Repeats(byte value) { return (value & 0x0F) == (value >> 4); }
        private static char Hex(int nibble)     { return "0123456789ABCDEF"[nibble & 0xF]; }
        private static int  Nibble(char c)      { return int.Parse(c.ToString(), NumberStyles.HexNumber, Inv); }
        private static byte Byte(string h, int at) { return byte.Parse(h.Substring(at, 2), NumberStyles.HexNumber, Inv); }
    }
}
