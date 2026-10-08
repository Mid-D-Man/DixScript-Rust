using System;
using System.Collections.Generic;
using System.Text.RegularExpressions;
using MidManStudio.Mdix.Unity.Editor.Highlight;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>One .mdix text to search: where it lives and what it says right now.</summary>
    internal readonly struct MdixSearchSource
    {
        public readonly string Path;
        public readonly string Text;

        public MdixSearchSource(string path, string text)
        {
            Path = path;
            Text = text ?? string.Empty;
        }
    }

    internal sealed class MdixSearchOptions
    {
        public string Query          = string.Empty;
        public bool   MatchCase;
        public bool   UseRegex;

        /// <summary>Only count hits on declared names: keys, sections, enums and their members, functions.</summary>
        public bool   NamesOnly;

        public int    MaxHits        = 500;
        public int    MaxHitsPerFile = 100;
    }

    internal readonly struct MdixSearchHit
    {
        public readonly string         Path;
        public readonly int            Line;          // 0-based
        public readonly int            Column;        // 0-based, in the file's own line
        public readonly int            Offset;        // into the whole text
        public readonly int            Length;        // of the match
        public readonly string         LineText;      // the line, cut around the match when it is very long
        public readonly int            MatchInLine;   // where the match starts inside LineText
        public readonly int            MatchLength;   // how much of the match lies inside LineText
        public readonly MdixTokenClass Class;         // what the match sits on, from the syntax tokens

        public MdixSearchHit(
            string path, int line, int column, int offset, int length,
            string lineText, int matchInLine, int matchLength, MdixTokenClass cls)
        {
            Path        = path;
            Line        = line;
            Column      = column;
            Offset      = offset;
            Length      = length;
            LineText    = lineText;
            MatchInLine = matchInLine;
            MatchLength = matchLength;
            Class       = cls;
        }
    }

    internal sealed class MdixSearchResult
    {
        public readonly List<MdixSearchHit> Hits = new List<MdixSearchHit>();
        public int    FilesSearched;
        public int    FilesWithHits;
        public bool   Truncated;

        /// <summary>Set when the search could not run (an invalid pattern) or was cut short (a pattern that is too slow).</summary>
        public string Error;
    }

    /// <summary>
    /// Finds text across .mdix files. Pure logic, no Unity types, so it can be tested on its own.
    /// Hits are classified with the same tokenizer that colours the Editor tab, which is what makes
    /// "names only" possible: a hit counts only if it lands on a key, section, enum or function name.
    /// </summary>
    internal static class MdixProjectSearch
    {
        private const int MaxLineChars = 200;
        private const int ContextBefore = 60;

        private static readonly TimeSpan RegexTimeout = TimeSpan.FromMilliseconds(500);

        public static MdixSearchResult Run(IEnumerable<MdixSearchSource> files, MdixSearchOptions options)
        {
            var result = new MdixSearchResult();
            if (files == null || options == null || string.IsNullOrEmpty(options.Query)) return result;

            Regex regex = null;
            if (options.UseRegex)
            {
                try
                {
                    var flags = RegexOptions.CultureInvariant | RegexOptions.Multiline;
                    if (!options.MatchCase) flags |= RegexOptions.IgnoreCase;
                    regex = new Regex(options.Query, flags, RegexTimeout);
                }
                catch (ArgumentException ex)
                {
                    result.Error = "Not a valid pattern: " + ex.Message;
                    return result;
                }
            }

            var comparison = options.MatchCase ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

            foreach (var file in files)
            {
                if (result.Hits.Count >= options.MaxHits)
                {
                    result.Truncated = true;
                    break;
                }

                result.FilesSearched++;
                var before = result.Hits.Count;

                try
                {
                    SearchFile(file, options, regex, comparison, result);
                }
                catch (RegexMatchTimeoutException)
                {
                    result.Error = "The pattern is too slow on " + file.Path + ". Make it more specific.";
                }

                if (result.Hits.Count > before) result.FilesWithHits++;
            }

            return result;
        }

        private static void SearchFile(
            MdixSearchSource file, MdixSearchOptions options, Regex regex,
            StringComparison comparison, MdixSearchResult result)
        {
            var text = file.Text;
            if (text.Length == 0) return;

            int[]             lineStarts = null;
            List<MdixToken>   tokens     = null;
            var               inFile     = 0;

            // Both are only worth building for a file that has at least one match.
            void Prepare()
            {
                if (lineStarts == null) lineStarts = LineStarts(text);
                if (tokens == null)     tokens     = MdixTokenizer.Tokenize(text);
            }

            foreach (var span in Matches(text, options.Query, regex, comparison))
            {
                Prepare();

                var cls = ClassAt(tokens, span.Start);
                if (options.NamesOnly && !IsDeclaredName(cls)) continue;

                result.Hits.Add(MakeHit(file.Path, text, lineStarts, span.Start, span.Length, cls));

                if (++inFile >= options.MaxHitsPerFile || result.Hits.Count >= options.MaxHits)
                {
                    result.Truncated = true;
                    return;
                }
            }
        }

        private readonly struct Span
        {
            public readonly int Start;
            public readonly int Length;
            public Span(int start, int length) { Start = start; Length = length; }
        }

        private static IEnumerable<Span> Matches(string text, string query, Regex regex, StringComparison comparison)
        {
            if (regex != null)
            {
                var m = regex.Match(text);
                while (m.Success)
                {
                    if (m.Length > 0) yield return new Span(m.Index, m.Length);   // an empty match has nothing to show
                    m = m.NextMatch();
                }
                yield break;
            }

            var from = 0;
            while (from <= text.Length - query.Length)
            {
                var at = text.IndexOf(query, from, comparison);
                if (at < 0) yield break;

                yield return new Span(at, query.Length);
                from = at + Math.Max(1, query.Length);
            }
        }

        // ── Classification ────────────────────────────────────────────────────

        private static bool IsDeclaredName(MdixTokenClass cls)
        {
            switch (cls)
            {
                case MdixTokenClass.Section:
                case MdixTokenClass.Property:
                case MdixTokenClass.EnumType:
                case MdixTokenClass.EnumMember:
                case MdixTokenClass.FunctionName:
                    return true;
                default:
                    return false;
            }
        }

        /// <summary>The class of the token that contains <paramref name="offset"/>; None when no token does.</summary>
        private static MdixTokenClass ClassAt(List<MdixToken> tokens, int offset)
        {
            var lo = 0;
            var hi = tokens.Count - 1;

            while (lo <= hi)
            {
                var mid = (lo + hi) / 2;
                var t   = tokens[mid];

                if (offset < t.Start)      hi = mid - 1;
                else if (offset >= t.End)  lo = mid + 1;
                else                       return t.Class;
            }
            return MdixTokenClass.None;
        }

        /// <summary>A short readable name for a class, for the tag next to a hit. Empty when it adds nothing.</summary>
        public static string KindLabel(MdixTokenClass cls)
        {
            switch (cls)
            {
                case MdixTokenClass.Comment:      return "comment";
                case MdixTokenClass.Section:      return "section";
                case MdixTokenClass.String:
                case MdixTokenClass.StringEscape:
                case MdixTokenClass.Interpolation: return "text";
                case MdixTokenClass.Number:       return "number";
                case MdixTokenClass.Constant:     return "constant";
                case MdixTokenClass.EnumType:     return "enum";
                case MdixTokenClass.EnumMember:   return "enum value";
                case MdixTokenClass.FunctionName: return "function";
                case MdixTokenClass.Property:     return "key";
                case MdixTokenClass.TypeName:     return "type";
                default:                          return string.Empty;
            }
        }

        // ── Positions ─────────────────────────────────────────────────────────

        private static int[] LineStarts(string text)
        {
            var starts = new List<int> { 0 };
            for (int i = 0; i < text.Length; i++)
                if (text[i] == '\n') starts.Add(i + 1);
            return starts.ToArray();
        }

        private static MdixSearchHit MakeHit(
            string path, string text, int[] lineStarts, int start, int length, MdixTokenClass cls)
        {
            var line = Array.BinarySearch(lineStarts, start);
            if (line < 0) line = ~line - 1;

            var lineStart = lineStarts[line];
            var lineEnd   = line + 1 < lineStarts.Length ? lineStarts[line + 1] - 1 : text.Length;
            if (lineEnd > lineStart && text[lineEnd - 1] == '\r') lineEnd--;       // CRLF
            if (lineEnd < lineStart) lineEnd = lineStart;

            var column = start - lineStart;

            // A very long line (a minified blob, a long string) is cut around the match.
            var cutFrom = 0;
            var cutTo   = lineEnd - lineStart;
            if (cutTo > MaxLineChars)
            {
                cutFrom = Math.Max(0, column - ContextBefore);
                cutTo   = Math.Min(lineEnd - lineStart, cutFrom + MaxLineChars);
            }

            var lineText = text.Substring(lineStart + cutFrom, cutTo - cutFrom);
            var matchIn  = column - cutFrom;
            var matchLen = Math.Max(0, Math.Min(length, lineText.Length - matchIn));

            if (cutFrom > 0)            { lineText = "…" + lineText; matchIn += 1; }
            if (cutTo < lineEnd - lineStart) lineText += "…";

            return new MdixSearchHit(path, line, column, start, length, lineText, matchIn, matchLen, cls);
        }
    }
}
