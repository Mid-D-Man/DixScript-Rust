using System;
using System.Collections.Generic;
using System.Text;
using System.Text.RegularExpressions;

namespace MidManStudio.Mdix.Unity.Editor.Highlight
{
    /// <summary>A diagnostic range to underline in the editor, in absolute character offsets.</summary>
    internal readonly struct MdixMark
    {
        public readonly int Start;
        public readonly int Length;

        /// <summary>LSP DiagnosticSeverity: 1 error, 2 warning, 3 information, 4 hint.</summary>
        public readonly int Severity;

        public MdixMark(int start, int length, int severity)
        {
            Start    = start;
            Length   = length;
            Severity = severity;
        }
    }

    /// <summary>
    /// Colours for each <see cref="MdixTokenClass"/>, as "#RRGGBB" strings
    /// (the form Unity's rich text parser takes). Tuned for MDIX Studio's very
    /// dark editor background; values follow the familiar VSCode Dark+ palette.
    /// </summary>
    internal static class MdixPalette
    {
        public const string BaseText = "#E8EDF5";

        private static readonly string[] Colors =
        {
            /* None               */ BaseText,
            /* Comment            */ "#6A9955",
            /* Section            */ "#C586C0",
            /* String             */ "#CE9178",
            /* StringEscape       */ "#D7BA7D",
            /* Interpolation      */ "#4FC1FF",
            /* Number             */ "#B5CEA8",
            /* Constant           */ "#569CD6",
            /* KeywordControl     */ "#C586C0",
            /* KeywordDeclaration */ "#569CD6",
            /* TypeName           */ "#4EC9B0",
            /* StaticObject       */ "#4EC9B0",
            /* FunctionName       */ "#DCDCAA",
            /* EnumType           */ "#4EC9B0",
            /* EnumMember         */ "#4FC1FF",
            /* Property           */ "#9CDCFE",
            /* Operator           */ "#8AB4F8",
        };

        public static string For(MdixTokenClass cls)
        {
            var i = (int)cls;
            return i >= 0 && i < Colors.Length ? Colors[i] : BaseText;
        }

        /// <summary>LSP DiagnosticSeverity -> underline/text colour.</summary>
        public static string ForSeverity(int severity)
        {
            switch (severity)
            {
                case 1:  return "#F14C4C"; // error
                case 2:  return "#E5B93C"; // warning
                case 3:  return "#75BEFF"; // information
                default: return "#9AA5B1"; // hint
            }
        }
    }

    /// <summary>
    /// Turns source text + token classes (+ optional diagnostic marks) into a
    /// Unity rich-text string whose visible characters are EXACTLY the source
    /// text. That equality is what lets MDIX Studio draw the coloured copy
    /// underneath a transparent TextField and have every glyph line up with
    /// the real caret and selection.
    ///
    /// Literal '&lt;' characters are emitted as &lt;noparse&gt;&lt;&lt;/noparse&gt;.
    /// Verified against Unity's TextCore TextGenerator.ValidateHtmlTag: a tag
    /// scan aborts the moment it meets another '&lt;', and inside noparse every
    /// tag but the closing noparse is rejected — so exactly one literal '&lt;'
    /// is produced and no tag in the source (even ones Unity knows, like &lt;b&gt;
    /// or &lt;br&gt;) can ever be interpreted. mdix source has plenty of
    /// angle-bracket text (&lt;int&gt;, &lt;object&gt;).
    ///
    /// Pure C#, no UnityEngine/UnityEditor references — unit-testable.
    /// </summary>
    internal static class MdixRichText
    {
        private const string LiteralLessThan = "<noparse><</noparse>";

        /// <summary>
        /// Builds the rich-text string. <paramref name="tokens"/> must be
        /// non-overlapping (as produced by <see cref="MdixTokenizer"/>);
        /// out-of-range spans are clamped, never thrown on.
        /// </summary>
        public static string Build(
            string text,
            IReadOnlyList<MdixToken> tokens,
            IReadOnlyList<MdixMark>? marks = null)
        {
            var n = text?.Length ?? 0;
            if (n == 0) return string.Empty;

            var cls = new byte[n];
            for (var t = 0; t < tokens.Count; t++)
            {
                var tok = tokens[t];
                var s = Math.Max(0, tok.Start);
                var e = Math.Min(n, tok.Start + tok.Length);
                for (var i = s; i < e; i++) cls[i] = (byte)tok.Class;
            }

            byte[]? sev = null;
            if (marks != null && marks.Count > 0)
            {
                sev = new byte[n];
                for (var k = 0; k < marks.Count; k++)
                {
                    var mark = marks[k];
                    if (mark.Severity < 1 || mark.Severity > 4) continue;

                    var s = Math.Max(0, mark.Start);
                    var e = Math.Min(n, mark.Start + Math.Max(1, mark.Length));
                    for (var i = s; i < e; i++)
                    {
                        // Lower number = more severe; keep the most severe.
                        if (sev[i] == 0 || mark.Severity < sev[i])
                            sev[i] = (byte)mark.Severity;
                    }
                }
            }

            var sb  = new StringBuilder(n + (n >> 2) + 64);
            var pos = 0;

            while (pos < n)
            {
                var c = cls[pos];
                var m = sev != null ? sev[pos] : (byte)0;

                var end = pos + 1;
                while (end < n && cls[end] == c && (sev == null || sev[end] == m))
                    end++;

                AppendRun(sb, text!, pos, end, (MdixTokenClass)c, m);
                pos = end;
            }

            return sb.ToString();
        }

        private static void AppendRun(
            StringBuilder sb, string text, int start, int end, MdixTokenClass cls, int severity)
        {
            string? colour = null;
            if (severity != 0)             colour = MdixPalette.ForSeverity(severity);
            else if (cls != MdixTokenClass.None) colour = MdixPalette.For(cls);

            if (colour != null) sb.Append("<color=").Append(colour).Append('>');
            if (severity != 0)  sb.Append("<u>");

            AppendEscaped(sb, text, start, end);

            if (severity != 0)  sb.Append("</u>");
            if (colour != null) sb.Append("</color>");
        }

        /// <summary>Appends text[start..end) with every '&lt;' neutralised.</summary>
        private static void AppendEscaped(StringBuilder sb, string text, int start, int end)
        {
            for (var i = start; i < end; i++)
            {
                var ch = text[i];
                if (ch == '<') sb.Append(LiteralLessThan);
                else           sb.Append(ch);
            }
        }

        /// <summary>
        /// Escapes arbitrary text for use inside rich text WITHOUT adding any
        /// colour/underline markup (used for hover popups and the like).
        /// </summary>
        public static string Escape(string? text)
        {
            if (string.IsNullOrEmpty(text)) return string.Empty;
            var sb = new StringBuilder(text!.Length + 16);
            AppendEscaped(sb, text, 0, text.Length);
            return sb.ToString();
        }

        // ── Test support ──────────────────────────────────────────────────────

        private static readonly Regex MarkupPattern = new Regex(
            @"<noparse><</noparse>|<color=#[0-9A-Fa-f]{6,8}>|</color>|<u>|</u>|<b>|</b>|<i>|</i>",
            RegexOptions.CultureInvariant);

        /// <summary>
        /// Removes the markup this class emits and un-escapes literal '&lt;',
        /// in a single left-to-right pass (so an escaped "&lt;color&gt;" in the
        /// source can't be mistaken for a real tag). For anything produced by
        /// <see cref="Build"/> the result equals the original text — the
        /// invariant the overlay depends on.
        /// </summary>
        public static string StripMarkup(string rich) =>
            MarkupPattern.Replace(rich, m => m.Value == LiteralLessThan ? "<" : string.Empty);
    }
}
