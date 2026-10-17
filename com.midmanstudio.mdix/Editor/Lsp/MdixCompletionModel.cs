using System;
using System.Collections.Generic;

namespace MidManStudio.Mdix.Unity.Editor.Lsp
{
    /// <summary>One completion candidate, flattened from the server's CompletionItem.</summary>
    internal sealed class MdixCompletionEntry
    {
        public string Label      = string.Empty;
        public string Detail     = string.Empty;
        public string FilterText = string.Empty;
        public string SortText   = string.Empty;

        /// <summary>LSP CompletionItemKind (1..25), 0 when absent.</summary>
        public int Kind;

        /// <summary>The text to insert: textEdit.newText, else insertText, else the label.</summary>
        public string InsertText = string.Empty;

        /// <summary>insertTextFormat == 2 (Snippet) — InsertText needs MdixSnippet.Expand.</summary>
        public bool IsSnippet;

        /// <summary>True when the server supplied an explicit textEdit range to replace.</summary>
        public bool HasEditRange;
        public int  StartLine, StartCharacter, EndLine, EndCharacter;

        // Filled by FilterAndSort for ordering; not part of the server data.
        internal int Score;
    }

    /// <summary>
    /// Parsing, filtering and ordering of completion results.
    ///
    /// The server returns every candidate valid at the cursor; it does NOT
    /// narrow them by what the user has already typed (mdix-lsp's section
    /// list, for instance, is the same seven entries whether you typed "@"
    /// or "@DA"). Narrowing is the client's job, as it is in VSCode.
    ///
    /// Pure C# apart from <see cref="MdixJsonValue"/> (also pure) — unit-testable.
    /// </summary>
    internal static class MdixCompletionModel
    {
        public static List<MdixCompletionEntry> Parse(MdixJsonValue? result)
        {
            var entries = new List<MdixCompletionEntry>();
            if (result == null || result.IsNull) return entries;

            // CompletionList { items: [...] } or a bare CompletionItem[].
            var items = result.Kind == MdixJsonKind.Array
                ? result
                : (result.TryGet("items", out var inner) ? inner : MdixJsonValue.Array());

            foreach (var item in items.AsArray())
            {
                if (item.Kind != MdixJsonKind.Object) continue;

                var label = item.TryGet("label", out var l) ? l.AsString() : string.Empty;
                if (label.Length == 0) continue;

                var entry = new MdixCompletionEntry
                {
                    Label      = label,
                    Detail     = item.TryGet("detail", out var d) ? d.AsString() : string.Empty,
                    FilterText = item.TryGet("filterText", out var f) ? f.AsString() : string.Empty,
                    SortText   = item.TryGet("sortText", out var s) ? s.AsString() : string.Empty,
                    Kind       = item.TryGet("kind", out var k) ? k.AsInt() : 0,
                    IsSnippet  = item.TryGet("insertTextFormat", out var fmt) && fmt.AsInt() == 2,
                };

                entry.InsertText = item.TryGet("insertText", out var it) && it.Kind == MdixJsonKind.String
                    ? it.AsString()
                    : label;

                // An explicit textEdit wins over insertText. It is either a TextEdit
                // {range, newText} or an InsertReplaceEdit {insert, replace, newText}.
                if (item.TryGet("textEdit", out var edit) && edit.Kind == MdixJsonKind.Object)
                {
                    if (edit.TryGet("newText", out var nt) && nt.Kind == MdixJsonKind.String)
                        entry.InsertText = nt.AsString();

                    MdixJsonValue range;
                    if (!edit.TryGet("range", out range))
                        edit.TryGet("replace", out range);

                    if (range.Kind == MdixJsonKind.Object &&
                        range.TryGet("start", out var a) && range.TryGet("end", out var b))
                    {
                        entry.HasEditRange   = true;
                        entry.StartLine      = a.TryGet("line", out var al) ? al.AsInt() : 0;
                        entry.StartCharacter = a.TryGet("character", out var ac) ? ac.AsInt() : 0;
                        entry.EndLine        = b.TryGet("line", out var bl) ? bl.AsInt() : 0;
                        entry.EndCharacter   = b.TryGet("character", out var bc) ? bc.AsInt() : 0;
                    }
                }

                entries.Add(entry);
            }

            return entries;
        }

        /// <summary>
        /// Keeps the entries that match <paramref name="typed"/> (the
        /// identifier prefix before the caret) and orders them best-first:
        /// prefix matches, then substring matches, then in-order subsequence
        /// matches; ties fall back to the server's sortText (else the label).
        /// Matching ignores case and any leading punctuation on the label
        /// ("@DATA" is matched as "DATA").
        /// </summary>
        public static List<MdixCompletionEntry> FilterAndSort(
            IReadOnlyList<MdixCompletionEntry> all, string typed, int max = 60)
        {
            var kept = new List<MdixCompletionEntry>(all.Count);

            for (var i = 0; i < all.Count; i++)
            {
                var entry = all[i];
                var score = Score(entry.FilterText.Length > 0 ? entry.FilterText : entry.Label, typed);
                if (score < 0) continue;

                entry.Score = score;
                kept.Add(entry);
            }

            // Stable: Sort() isn't, so decorate with the original index.
            var order = new Dictionary<MdixCompletionEntry, int>(kept.Count);
            for (var i = 0; i < kept.Count; i++) order[kept[i]] = i;

            kept.Sort((a, b) =>
            {
                var byScore = a.Score.CompareTo(b.Score);
                if (byScore != 0) return byScore;

                var ka = a.SortText.Length > 0 ? a.SortText : a.Label;
                var kb = b.SortText.Length > 0 ? b.SortText : b.Label;
                var byKey = string.Compare(ka, kb, StringComparison.OrdinalIgnoreCase);
                return byKey != 0 ? byKey : order[a].CompareTo(order[b]);
            });

            if (kept.Count > max) kept.RemoveRange(max, kept.Count - max);
            return kept;
        }

        /// <summary>0 = prefix match, 1 = substring, 2 = subsequence, -1 = no match.</summary>
        internal static int Score(string candidate, string typed)
        {
            if (string.IsNullOrEmpty(typed)) return 0;

            var start = 0;
            while (start < candidate.Length && !char.IsLetterOrDigit(candidate[start]) && candidate[start] != '_')
                start++;

            var norm = candidate.Substring(start);

            if (norm.StartsWith(typed, StringComparison.OrdinalIgnoreCase)) return 0;
            if (norm.IndexOf(typed, StringComparison.OrdinalIgnoreCase) >= 0) return 1;

            // In-order subsequence ("cfg" matches "config").
            var t = 0;
            for (var i = 0; i < norm.Length && t < typed.Length; i++)
            {
                if (char.ToLowerInvariant(norm[i]) == char.ToLowerInvariant(typed[t])) t++;
            }
            return t == typed.Length ? 2 : -1;
        }

        /// <summary>Short ASCII tag for a CompletionItemKind, shown dimmed beside the label.</summary>
        public static string KindTag(int kind)
        {
            switch (kind)
            {
                case 2:
                case 3:
                case 4:  return "fn";
                case 5:  return "field";
                case 6:  return "var";
                case 7:
                case 22: return "type";
                case 9:  return "mod";
                case 10: return "prop";
                case 12: return "val";
                case 13: return "enum";
                case 14: return "kw";
                case 15: return "snip";
                case 16: return "color";
                case 20: return "member";
                case 21: return "const";
                case 24: return "op";
                default: return string.Empty;
            }
        }
    }
}
