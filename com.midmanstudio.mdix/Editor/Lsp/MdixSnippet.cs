using System;
using System.Collections.Generic;
using System.Text;

namespace MidManStudio.Mdix.Unity.Editor.Lsp
{
    /// <summary>One tab stop in an expanded snippet. Offsets are relative to the expanded text.</summary>
    internal readonly struct MdixSnippetStop
    {
        public readonly int Index;
        public readonly int Start;
        public readonly int Length;

        public MdixSnippetStop(int index, int start, int length)
        {
            Index  = index;
            Start  = start;
            Length = length;
        }

        public int End => Start + Length;
    }

    internal sealed class MdixExpandedSnippet
    {
        public string Text { get; }

        /// <summary>
        /// Tab stops in visiting order: 1, 2, 3 ... then the final stop ($0),
        /// which is always present (at the end of the text if the snippet
        /// didn't name one).
        /// </summary>
        public List<MdixSnippetStop> Stops { get; }

        public MdixExpandedSnippet(string text, List<MdixSnippetStop> stops)
        {
            Text  = text;
            Stops = stops;
        }

        /// <summary>True if there is anything to tab through besides the final stop.</summary>
        public bool HasInteractiveStops => Stops.Count > 1;
    }

    /// <summary>
    /// Expands LSP snippet syntax into plain text plus tab stop positions.
    ///
    /// mdix-lsp marks most of its completions `insertTextFormat = Snippet`
    /// (section headers, quickfunc declarations, config keys, bracket pairs).
    /// A client that inserts that text verbatim — as MDIX Studio's first
    /// release did — leaves literal "${1:key}" junk in the document.
    ///
    /// Supported (the parts of the grammar the server actually uses, plus the
    /// rest of the common spec so a future server change doesn't regress):
    ///   $1   ${1}   ${1:default}   ${1:nested ${2:default}}   ${1|a,b,c|}
    ///   $0   (final cursor position)
    ///   $VAR ${VAR} ${VAR:default}   (variables are unsupported; the default,
    ///                                 or nothing, is inserted)
    ///   \$ \} \\   escapes
    ///
    /// A tab stop index that appears more than once is only interactive at
    /// its first occurrence; later occurrences are filled with the first
    /// one's default text (VSCode additionally mirrors edits live — not done
    /// here).
    ///
    /// <paramref name="indent"/> is added after every newline in the snippet,
    /// so a multi-line snippet inserted on an indented line stays aligned.
    ///
    /// Pure C#, no Unity references — unit-testable.
    /// </summary>
    internal static class MdixSnippet
    {
        public static MdixExpandedSnippet Expand(string snippet, string indent = "")
        {
            snippet ??= string.Empty;
            indent  ??= string.Empty;

            var first = Run(snippet, indent, prefill: null);

            // A bare "$1" that appears BEFORE the "${1:default}" which defines it
            // has nothing to show on the first pass. Per the spec every instance of
            // a tab stop shares the placeholder's text, so redo the expansion with
            // the defaults discovered on the first pass.
            if (first.Defaults.Count > 0 && first.NeedsSecondPass)
                return Run(snippet, indent, first.Defaults).Result;

            return first.Result;
        }

        private readonly struct PassResult
        {
            public readonly MdixExpandedSnippet Result;
            public readonly Dictionary<int, string> Defaults;
            public readonly bool NeedsSecondPass;

            public PassResult(MdixExpandedSnippet result, Dictionary<int, string> defaults, bool needsSecondPass)
            {
                Result          = result;
                Defaults        = defaults;
                NeedsSecondPass = needsSecondPass;
            }
        }

        private static PassResult Run(string snippet, string indent, Dictionary<int, string>? prefill)
        {
            var state = new State(snippet, indent, prefill);
            ParseSequence(state, nested: false);

            var stops = new List<MdixSnippetStop>(state.Stops.Count + 1);

            // 1..n in ascending order, regardless of where they appear in the text.
            var numbered = new List<MdixSnippetStop>();
            MdixSnippetStop? final = null;

            foreach (var stop in state.Stops)
            {
                if (stop.Index == 0) final ??= stop;
                else                 numbered.Add(stop);
            }

            numbered.Sort((a, b) => a.Index.CompareTo(b.Index));
            stops.AddRange(numbered);
            stops.Add(final ?? new MdixSnippetStop(0, state.Output.Length, 0));

            var result = new MdixExpandedSnippet(state.Output.ToString(), stops);
            return new PassResult(result, state.FirstNonEmptyDefault, state.BareBeforeDefinition);
        }

        // ── Parser ────────────────────────────────────────────────────────────

        private sealed class State
        {
            public readonly string Source;
            public readonly string Indent;
            public readonly StringBuilder Output = new StringBuilder();
            public readonly List<MdixSnippetStop> Stops = new List<MdixSnippetStop>();
            public readonly Dictionary<int, string> DefaultText = new Dictionary<int, string>();

            /// <summary>Defaults found on an earlier pass, used to fill bare stops that precede their definition.</summary>
            public readonly Dictionary<int, string>? Prefill;

            /// <summary>First non-empty default seen for each index during this pass.</summary>
            public readonly Dictionary<int, string> FirstNonEmptyDefault = new Dictionary<int, string>();

            /// <summary>True if some index's first occurrence was bare but a later one had a default.</summary>
            public bool BareBeforeDefinition;

            public int Pos;

            public State(string source, string indent, Dictionary<int, string>? prefill)
            {
                Source  = source;
                Indent  = indent;
                Prefill = prefill;
            }

            public bool End => Pos >= Source.Length;
        }

        /// <summary>
        /// Reads literal text and $-constructs until the end of input, or —
        /// when <paramref name="nested"/> — until the '}' that closes the
        /// enclosing placeholder (left unconsumed for the caller).
        /// </summary>
        private static void ParseSequence(State st, bool nested)
        {
            while (!st.End)
            {
                var c = st.Source[st.Pos];

                if (nested && c == '}')
                    return;

                if (c == '\\' && st.Pos + 1 < st.Source.Length)
                {
                    var next = st.Source[st.Pos + 1];
                    if (next == '$' || next == '}' || next == '\\')
                    {
                        st.Output.Append(next);
                        st.Pos += 2;
                        continue;
                    }
                }

                if (c == '$' && TryParseDollar(st))
                    continue;

                st.Output.Append(c);
                if (c == '\n') st.Output.Append(st.Indent);
                st.Pos++;
            }
        }

        private static bool TryParseDollar(State st)
        {
            var s = st.Source;
            var i = st.Pos + 1; // after '$'
            if (i >= s.Length) return false;

            // $1  $0
            if (char.IsDigit(s[i]))
            {
                var index = ReadInt(s, ref i);
                st.Pos = i;
                AddStop(st, index, st.Output.Length, 0, defaultText: null);
                return true;
            }

            // $NAME (variable)
            if (IsNameStart(s[i]))
            {
                while (i < s.Length && IsNameChar(s[i])) i++;
                st.Pos = i;
                return true; // unsupported variable → inserts nothing
            }

            if (s[i] != '{') return false;
            i++; // after '{'
            if (i >= s.Length) return false;

            // ${1...}
            if (char.IsDigit(s[i]))
            {
                var index = ReadInt(s, ref i);
                if (i >= s.Length) { st.Pos = i; return true; }

                switch (s[i])
                {
                    case '}':
                        st.Pos = i + 1;
                        AddStop(st, index, st.Output.Length, 0, defaultText: null);
                        return true;

                    case ':':
                    {
                        st.Pos = i + 1;
                        var start = st.Output.Length;
                        ParseSequence(st, nested: true);
                        if (!st.End && st.Source[st.Pos] == '}') st.Pos++;
                        var length = st.Output.Length - start;
                        AddStop(st, index, start, length, st.Output.ToString(start, length));
                        return true;
                    }

                    case '|':
                    {
                        st.Pos = i + 1;
                        var choice = ReadFirstChoice(st);
                        var start  = st.Output.Length;
                        st.Output.Append(choice);
                        AddStop(st, index, start, choice.Length, choice);
                        return true;
                    }

                    default:
                        // Unsupported form such as ${1/regex/format/} — drop the
                        // whole construct rather than leak syntax into the text.
                        SkipToClosingBrace(st, i);
                        AddStop(st, index, st.Output.Length, 0, defaultText: null);
                        return true;
                }
            }

            // ${NAME} / ${NAME:default} (variable)
            if (IsNameStart(s[i]))
            {
                while (i < s.Length && IsNameChar(s[i])) i++;

                if (i < s.Length && s[i] == ':')
                {
                    st.Pos = i + 1;
                    ParseSequence(st, nested: true); // emit the default text
                    if (!st.End && st.Source[st.Pos] == '}') st.Pos++;
                    return true;
                }

                SkipToClosingBrace(st, i);
                return true;
            }

            return false;
        }

        private static void AddStop(State st, int index, int start, int length, string? defaultText)
        {
            if (defaultText != null && defaultText.Length > 0 && !st.FirstNonEmptyDefault.ContainsKey(index))
                st.FirstNonEmptyDefault[index] = defaultText;

            if (st.DefaultText.TryGetValue(index, out var first))
            {
                // Repeat of an index already seen. Not interactive; for a bare
                // "$1" echo the first occurrence's default so the text reads right.
                if (defaultText == null && length == 0)
                {
                    if (first.Length > 0) st.Output.Append(first);
                    else if (st.Prefill != null && st.Prefill.TryGetValue(index, out var pre)) st.Output.Append(pre);
                }
                else if (first.Length == 0 && defaultText != null && defaultText.Length > 0)
                {
                    // The definition arrived after a bare first occurrence.
                    st.BareBeforeDefinition = true;
                }
                return;
            }

            // First occurrence of this index. A bare one may still have a known
            // default from an earlier pass.
            if (defaultText == null && st.Prefill != null && st.Prefill.TryGetValue(index, out var known))
            {
                st.Output.Append(known);
                length      = known.Length;
                defaultText = known;
            }

            st.DefaultText[index] = defaultText ?? string.Empty;
            st.Stops.Add(new MdixSnippetStop(index, start, length));
        }

        /// <summary>Reads "a,b,c|}" and returns "a" (escapes \, \| \\ honoured), consuming through "|}".</summary>
        private static string ReadFirstChoice(State st)
        {
            var s     = st.Source;
            var first = new StringBuilder();
            var inFirst = true;

            while (st.Pos < s.Length)
            {
                var c = s[st.Pos];

                if (c == '\\' && st.Pos + 1 < s.Length)
                {
                    var next = s[st.Pos + 1];
                    if (inFirst && (next == ',' || next == '|' || next == '\\' || next == '$' || next == '}'))
                        first.Append(next);
                    st.Pos += 2;
                    continue;
                }

                if (c == '|' && st.Pos + 1 < s.Length && s[st.Pos + 1] == '}')
                {
                    st.Pos += 2;
                    return first.ToString();
                }

                if (c == ',') inFirst = false;
                else if (inFirst) first.Append(c);

                st.Pos++;
            }

            return first.ToString();
        }

        private static void SkipToClosingBrace(State st, int from)
        {
            var depth = 1;
            var i = from;
            while (i < st.Source.Length && depth > 0)
            {
                var c = st.Source[i];
                if (c == '\\' && i + 1 < st.Source.Length) { i += 2; continue; }
                if (c == '{') depth++;
                else if (c == '}') depth--;
                i++;
            }
            st.Pos = i;
        }

        private static int ReadInt(string s, ref int i)
        {
            var value = 0;
            while (i < s.Length && char.IsDigit(s[i]))
            {
                value = Math.Min(value * 10 + (s[i] - '0'), 1_000_000);
                i++;
            }
            return value;
        }

        private static bool IsNameStart(char c) => char.IsLetter(c) || c == '_';
        private static bool IsNameChar(char c)  => char.IsLetterOrDigit(c) || c == '_';
    }
}
