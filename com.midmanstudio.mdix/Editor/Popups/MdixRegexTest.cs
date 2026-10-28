using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using System.Text.RegularExpressions;
using MidManStudio.Mdix.Unity.Editor.Lsp;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>A capture group of one match. Name is null for an unnamed group; Value is null when the group took no part.</summary>
    internal sealed class MdixRegexGroupInfo
    {
        public string Name;
        public string Value;
    }

    /// <summary>One match, positioned in the test text by UTF-16 index (what a C# string uses).</summary>
    internal sealed class MdixRegexHit
    {
        public int    Start;
        public int    Length;
        public string Text = string.Empty;
        public readonly List<MdixRegexGroupInfo> Groups = new List<MdixRegexGroupInfo>();
    }

    /// <summary>The outcome of running a pattern over some text.</summary>
    internal sealed class MdixRegexTestResult
    {
        public bool   Valid;
        public string Error;                  // one line; set when not Valid
        public bool   Approximate;            // true when .NET produced it, not the engine DixScript uses
        public bool   HitsCapped;             // more matches existed than are listed
        public readonly List<MdixRegexHit> Hits = new List<MdixRegexHit>();
    }

    /// <summary>
    /// Runs and reads regex tests, without Unity types so it can be tested on its own.
    ///
    /// The answer that counts comes from the language server (<c>mdix.testRegex</c>): it uses the same Rust
    /// <c>regex</c> crate the compiler checks a DixScript regex with. Its match positions are UTF-8 byte
    /// offsets, so they are converted here to the UTF-16 indexes C# strings use.
    ///
    /// When the server cannot be reached, <see cref="FromDotNet"/> gives an approximate answer instead, and says
    /// so. .NET is a different dialect: it accepts look-around and backreferences, which the Rust crate rejects.
    /// An edit is still compiled by the real engine before it is accepted, so an approximate answer never lets a
    /// bad pattern into the file.
    /// </summary>
    internal static class MdixRegexTest
    {
        public const string Command = "mdix.testRegex";

        /// <summary>No more than this many matches are listed.</summary>
        public const int MaxHits = 500;

        /// <summary>.NET backtracks, so a bad pattern can run for ever; the fallback gives up after this.</summary>
        public static readonly TimeSpan DotNetTimeout = TimeSpan.FromMilliseconds(500);

        private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        // ── The request ───────────────────────────────────────────────────────

        /// <summary>The params of the <c>workspace/executeCommand</c> request: the pattern and the text, both as plain strings.</summary>
        public static MdixJsonValue BuildParams(string pattern, string testText)
        {
            var args = MdixJsonValue.Array();
            args.Add(MdixJsonValue.String(pattern ?? string.Empty));
            args.Add(MdixJsonValue.String(testText ?? string.Empty));

            var p = MdixJsonValue.Object();
            p["command"]   = MdixJsonValue.String(Command);
            p["arguments"] = args;
            return p;
        }

        // ── The server's answer ───────────────────────────────────────────────

        /// <summary>
        /// Reads <c>{valid, error, matches: [{start, end, text, groups: [{name, value}]}]}</c>. Null when the answer is
        /// not in that shape, which the caller treats as "no answer".
        /// </summary>
        public static MdixRegexTestResult FromServer(MdixJsonValue json, string testText)
        {
            if (json == null || json.Kind != MdixJsonKind.Object || !json.ContainsKey("valid")) return null;

            testText = testText ?? string.Empty;
            var result = new MdixRegexTestResult { Valid = json["valid"].AsBool() };

            if (!result.Valid)
            {
                result.Error = FriendlyError(json["error"].AsString("The pattern is not valid."));
                return result;
            }

            var offsets = ByteOffsets(testText);
            foreach (var m in json["matches"].AsArray())
            {
                if (result.Hits.Count >= MaxHits) { result.HitsCapped = true; break; }

                var start = CharIndexAtByte(offsets, m["start"].AsInt());
                var end   = CharIndexAtByte(offsets, m["end"].AsInt());
                if (end < start) end = start;

                var hit = new MdixRegexHit
                {
                    Start  = start,
                    Length = end - start,
                    Text   = m["text"].AsString(testText.Substring(start, end - start)),
                };

                foreach (var g in m["groups"].AsArray())
                {
                    hit.Groups.Add(new MdixRegexGroupInfo
                    {
                        Name  = g["name"].IsNull  ? null : g["name"].AsString(),
                        Value = g["value"].IsNull ? null : g["value"].AsString(),
                    });
                }
                result.Hits.Add(hit);
            }
            return result;
        }

        // ── The approximate answer ────────────────────────────────────────────

        /// <summary>An approximate test with .NET's engine, for when the server cannot be reached.</summary>
        public static MdixRegexTestResult FromDotNet(string pattern, string testText)
        {
            var result = new MdixRegexTestResult { Approximate = true };

            Regex regex;
            try
            {
                regex = new Regex(pattern ?? string.Empty, RegexOptions.CultureInvariant, DotNetTimeout);
            }
            catch (ArgumentException e)
            {
                result.Error = FriendlyError(e.Message);
                return result;
            }

            result.Valid = true;
            try
            {
                for (var m = regex.Match(testText ?? string.Empty); m.Success; m = m.NextMatch())
                {
                    if (result.Hits.Count >= MaxHits) { result.HitsCapped = true; break; }

                    var hit = new MdixRegexHit { Start = m.Index, Length = m.Length, Text = m.Value };
                    for (int i = 1; i < m.Groups.Count; i++)
                    {
                        var group = m.Groups[i];
                        var name  = regex.GroupNameFromNumber(i);
                        hit.Groups.Add(new MdixRegexGroupInfo
                        {
                            Name  = name == i.ToString(Inv) ? null : name,      // .NET names an unnamed group by its number
                            Value = group.Success ? group.Value : null,
                        });
                    }
                    result.Hits.Add(hit);
                }
            }
            catch (RegexMatchTimeoutException)
            {
                result.Valid = false;
                result.Error = "Matching took too long (this check uses .NET, which can be slow on some patterns).";
                result.Hits.Clear();
            }
            return result;
        }

        // ── Text for the popup ────────────────────────────────────────────────

        /// <summary>
        /// One line of the Rust crate's multi-line parse error: its last line, without the "error: " prefix
        /// ("unclosed character class"). A one-line message is returned as it is.
        /// </summary>
        public static string FriendlyError(string raw)
        {
            if (string.IsNullOrWhiteSpace(raw)) return "The pattern is not valid.";

            var lines = raw.Replace("\r", string.Empty).Split('\n');
            string last = null, first = null;
            foreach (var line in lines)
            {
                var t = line.Trim();
                if (t.Length == 0) continue;
                if (first == null) first = t;
                last = t;
            }

            if (last != null && last.StartsWith("error:", StringComparison.Ordinal))
                return last.Substring("error:".Length).Trim();

            return first ?? "The pattern is not valid.";
        }

        public static string Summary(MdixRegexTestResult result)
        {
            if (result == null || !result.Valid) return string.Empty;
            if (result.HitsCapped) return "Valid. " + MaxHits + "+ matches (the first " + MaxHits + " are listed).";

            var n = result.Hits.Count;
            return n == 0 ? "Valid. No matches." : "Valid. " + n + (n == 1 ? " match." : " matches.");
        }

        /// <summary>The text with its line breaks and tabs shown as \n, \r and \t, cut to <paramref name="max"/> characters.</summary>
        public static string Visible(string text, int max = 60)
        {
            if (text == null) return string.Empty;

            var sb = new StringBuilder();
            foreach (var c in text)
            {
                if (sb.Length >= max) { sb.Append('…'); break; }
                if (c == '\n')      sb.Append("\\n");
                else if (c == '\r') sb.Append("\\r");
                else if (c == '\t') sb.Append("\\t");
                else                sb.Append(c);
            }
            return sb.ToString();
        }

        /// <summary>For example <c>#2  7–10  "abc"</c>: the number, the range in the test text (end excluded), the text.</summary>
        public static string DescribeHit(int number, MdixRegexHit hit)
        {
            return "#" + number + "  " + hit.Start + "–" + (hit.Start + hit.Length) + "  \"" + Visible(hit.Text) + "\"";
        }

        /// <summary>For example <c>1: "x"</c>, <c>year: "2026"</c> or <c>2: (no match)</c>. <paramref name="number"/> counts from 1.</summary>
        public static string DescribeGroup(int number, MdixRegexGroupInfo group)
        {
            var label = group.Name ?? number.ToString(Inv);
            return label + ": " + (group.Value == null ? "(no match)" : "\"" + Visible(group.Value) + "\"");
        }

        // ── UTF-8 byte offsets to UTF-16 indexes ──────────────────────────────

        /// <summary>
        /// <c>result[i]</c> is the number of UTF-8 bytes before the UTF-16 char at index i; the last entry is the
        /// total. The second half of a surrogate pair has no boundary of its own, so it shares the first half's value.
        /// </summary>
        internal static int[] ByteOffsets(string text)
        {
            var offsets = new int[text.Length + 1];
            var bytes   = 0;

            for (int i = 0; i < text.Length; i++)
            {
                offsets[i] = bytes;
                var c = text[i];

                if (c < 0x80)         bytes += 1;
                else if (c < 0x800)   bytes += 2;
                else if (char.IsHighSurrogate(c) && i + 1 < text.Length && char.IsLowSurrogate(text[i + 1]))
                {
                    offsets[i + 1] = bytes;
                    bytes += 4;
                    i++;
                }
                else                  bytes += 3;
            }

            offsets[text.Length] = bytes;
            return offsets;
        }

        /// <summary>The UTF-16 index of the char that starts at the given UTF-8 byte offset (past the end gives the text's length).</summary>
        internal static int CharIndexAtByte(int[] offsets, int byteOffset)
        {
            var last = offsets.Length - 1;
            if (byteOffset <= 0)               return 0;
            if (byteOffset >= offsets[last])   return last;

            int lo = 0, hi = last;             // the first index whose offset is not before the target
            while (lo < hi)
            {
                var mid = (lo + hi) / 2;
                if (offsets[mid] < byteOffset) lo = mid + 1; else hi = mid;
            }
            return lo;
        }

        public static int ByteToCharIndex(string text, int byteOffset)
        {
            return CharIndexAtByte(ByteOffsets(text ?? string.Empty), byteOffset);
        }
    }
}
