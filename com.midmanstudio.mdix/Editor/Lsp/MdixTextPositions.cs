using System;

namespace MidManStudio.Mdix.Unity.Editor.Lsp
{
    /// <summary>
    /// Offset &lt;-&gt; LSP position conversion and word helpers.
    ///
    /// LSP positions are (line, character) with character counted in UTF-16
    /// code units — the same unit as a C# string index — so no re-encoding is
    /// needed. A line ends at '\n' (which also covers "\r\n": the '\r' simply
    /// counts as the last character of the line it terminates).
    ///
    /// Pure C#, no Unity references — unit-testable.
    /// </summary>
    internal static class MdixTextPositions
    {
        public static (int line, int character) OffsetToPosition(string text, int offset)
        {
            var end       = Math.Max(0, Math.Min(offset, text.Length));
            var line      = 0;
            var lineStart = 0;

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

        /// <summary>
        /// Inverse of <see cref="OffsetToPosition"/>. A character past the end
        /// of its line clamps to the end of that line (as the LSP spec asks),
        /// and a line past the end of the text clamps to the end of the text.
        /// </summary>
        public static int PositionToOffset(string text, int line, int character)
        {
            var offset = 0;
            var current = 0;

            while (current < line)
            {
                var nl = text.IndexOf('\n', offset);
                if (nl < 0) return text.Length;
                offset = nl + 1;
                current++;
            }

            var lineEnd = text.IndexOf('\n', offset);
            if (lineEnd < 0) lineEnd = text.Length;

            // Don't let a "\r\n" ending put the caret between the two characters.
            if (lineEnd > offset && lineEnd <= text.Length && lineEnd > 0 && text[lineEnd - 1] == '\r')
                lineEnd--;

            return Math.Min(offset + Math.Max(0, character), lineEnd);
        }

        public static bool IsWordChar(char c) => char.IsLetterOrDigit(c) || c == '_';

        /// <summary>Start of the identifier ending at <paramref name="offset"/> (== offset if none).</summary>
        public static int FindWordStart(string text, int offset)
        {
            var i = Math.Max(0, Math.Min(offset, text.Length));
            while (i > 0 && IsWordChar(text[i - 1])) i--;
            return i;
        }

        /// <summary>The leading spaces/tabs of the line containing <paramref name="offset"/>.</summary>
        public static string LineIndent(string text, int offset)
        {
            var clamped   = Math.Max(0, Math.Min(offset, text.Length));
            var lineStart = clamped == 0 ? 0 : text.LastIndexOf('\n', clamped - 1) + 1;

            var end = lineStart;
            while (end < text.Length && (text[end] == ' ' || text[end] == '\t')) end++;

            return text.Substring(lineStart, end - lineStart);
        }

        /// <summary>True if every character in [from, to) is an identifier character.</summary>
        public static bool IsAllWordChars(string text, int from, int to)
        {
            var a = Math.Max(0, from);
            var b = Math.Min(text.Length, to);
            for (var i = a; i < b; i++)
                if (!IsWordChar(text[i])) return false;
            return true;
        }
    }
}
