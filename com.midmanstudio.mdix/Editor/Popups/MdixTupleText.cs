using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using MidManStudio.Mdix.Unity.Editor.Lsp;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Tuples in the Explorer, without Unity types so it can be tested on its own.
    ///
    /// The runtime gives a tuple only as a whole, as JSON (<c>[1.5,"x",{"enum_name":...}]</c>): the elements have
    /// no paths of their own and no types of their own. So an element is found by its index in that JSON, and
    /// checked by comparing it with the literal the source map found for it. Ints, longs, floats and doubles all
    /// come back as JSON numbers, so the element's type is taken from its literal, not from the JSON.
    /// </summary>
    internal static class MdixTupleText
    {
        private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        // ── Finding an element ────────────────────────────────────────────────

        /// <summary>
        /// Splits <c>a.b[2][0]</c> into the tuple it belongs to and the indexes inside it. Trailing <c>[n]</c> parts are
        /// peeled off until what is left is a tuple (<paramref name="isTuple"/> says so); a path that is itself a
        /// tuple, or that is inside no tuple, gives false.
        /// </summary>
        public static bool TrySplitPath(string path, Func<string, bool> isTuple, out string tuplePath, out int[] indexes)
        {
            tuplePath = null; indexes = null;
            if (string.IsNullOrEmpty(path) || path[path.Length - 1] != ']') return false;

            var found = new List<int>();
            var rest  = path;

            while (rest.Length > 0 && rest[rest.Length - 1] == ']')
            {
                var open = rest.LastIndexOf('[');
                if (open <= 0) return false;

                int n;
                if (!int.TryParse(rest.Substring(open + 1, rest.Length - open - 2), NumberStyles.None, Inv, out n)) return false;

                found.Add(n);
                rest = rest.Substring(0, open);

                if (isTuple(rest))
                {
                    found.Reverse();
                    tuplePath = rest;
                    indexes   = found.ToArray();
                    return true;
                }
            }
            return false;
        }

        /// <summary>The element at the indexes, or null when there is none.</summary>
        public static MdixJsonValue Navigate(MdixJsonValue root, int[] indexes)
        {
            var current = root;
            foreach (var i in indexes)
            {
                if (current == null || current.Kind != MdixJsonKind.Array || i < 0 || i >= current.Count) return null;
                current = current[i];
            }
            return current;
        }

        /// <summary>
        /// True when two tuple values are the same everywhere except at the element the indexes point to (which is
        /// allowed to differ). Used to prove an edit changed that element and nothing else.
        /// </summary>
        public static bool SameExcept(MdixJsonValue before, MdixJsonValue after, int[] indexes, int depth = 0)
        {
            if (before == null || after == null) return false;
            if (depth == indexes.Length) return true;                                  // this is the edited element

            if (before.Kind != MdixJsonKind.Array || after.Kind != MdixJsonKind.Array) return false;
            if (before.Count != after.Count) return false;

            for (int i = 0; i < before.Count; i++)
            {
                if (i == indexes[depth])
                {
                    if (!SameExcept(before[i], after[i], indexes, depth + 1)) return false;
                }
                else if (before[i].ToString() != after[i].ToString())
                {
                    return false;
                }
            }
            return true;
        }

        // ── Does an element read back as its literal? ─────────────────────────

        /// <summary>True when the JSON element is the value the literal of this kind writes.</summary>
        public static bool ElementMatches(MdixLiteralKind kind, string literal, MdixJsonValue element)
        {
            if (element == null || literal == null) return false;

            switch (kind)
            {
                case MdixLiteralKind.Int:
                case MdixLiteralKind.Long:
                {
                    long n; bool isLong; int radix;
                    if (!MdixIntegerText.TryParse(literal, out n, out isLong, out radix)) return false;
                    if (isLong != (kind == MdixLiteralKind.Long)) return false;
                    return element.Kind == MdixJsonKind.Number && element.AsDouble() == (double)n;
                }
                case MdixLiteralKind.Float:
                {
                    float f;
                    if (!float.TryParse(MdixSourceMap.Digits(MdixSourceMap.TrimSuffix(literal, 'f', 'F')), NumberStyles.Float, Inv, out f)) return false;
                    return element.Kind == MdixJsonKind.Number && Close(element.AsDouble(), f);
                }
                case MdixLiteralKind.Double:
                {
                    double d;
                    if (!double.TryParse(MdixSourceMap.Digits(literal), NumberStyles.Float, Inv, out d)) return false;
                    return element.Kind == MdixJsonKind.Number && Close(element.AsDouble(), d);
                }
                case MdixLiteralKind.Bool:
                    return element.Kind == MdixJsonKind.Bool && element.AsBool() == (literal == "true");

                case MdixLiteralKind.String:
                    return element.Kind == MdixJsonKind.String && string.Equals(element.AsString(), MdixSourceMap.DecodeString(literal), StringComparison.Ordinal);

                case MdixLiteralKind.RawString:
                    return literal.Length >= 2 && element.Kind == MdixJsonKind.String &&
                           string.Equals(element.AsString(), literal.Substring(1, literal.Length - 2), StringComparison.Ordinal);

                case MdixLiteralKind.HexColor:
                case MdixLiteralKind.Date:
                case MdixLiteralKind.Timestamp:
                    return element.Kind == MdixJsonKind.String && string.Equals(element.AsString(), literal, StringComparison.Ordinal);

                case MdixLiteralKind.Regex:
                case MdixLiteralKind.Blob:
                {
                    string arg;
                    return MdixSourceMap.TryReadCallArgument(literal, out arg) && element.Kind == MdixJsonKind.String &&
                           string.Equals(element.AsString(), arg, StringComparison.Ordinal);
                }
                case MdixLiteralKind.Enum:
                {
                    if (element.Kind != MdixJsonKind.Object) return false;
                    return element["enum_name"].AsString() + "." + element["field_name"].AsString() == literal;
                }
            }
            return false;
        }

        private static bool Close(double a, double b)
        {
            return Math.Abs(a - b) <= 1e-6 * Math.Max(1.0, Math.Abs(b));
        }

        /// <summary>The text for the edit field of an element, from its literal: no quotes, no type suffix, the notation kept.</summary>
        public static string ElementEditText(MdixLiteralKind kind, string literal)
        {
            if (literal == null) return string.Empty;

            switch (kind)
            {
                case MdixLiteralKind.Int:
                case MdixLiteralKind.Long:
                {
                    long n; bool isLong; int radix;
                    if (!MdixIntegerText.TryParse(literal, out n, out isLong, out radix)) return literal;
                    return radix == 10 ? n.ToString(Inv) : MdixSourceMap.TrimSuffix(literal, 'L', 'l');
                }
                case MdixLiteralKind.Float:     return MdixSourceMap.TrimSuffix(literal, 'f', 'F');
                case MdixLiteralKind.String:    return MdixSourceMap.DecodeString(literal);
                case MdixLiteralKind.RawString: return literal.Length >= 2 ? literal.Substring(1, literal.Length - 2) : string.Empty;
                case MdixLiteralKind.Regex:
                case MdixLiteralKind.Blob:
                {
                    string arg;
                    return MdixSourceMap.TryReadCallArgument(literal, out arg) ? arg : string.Empty;
                }
                default:                        return literal;
            }
        }

        // ── Showing a tuple ───────────────────────────────────────────────────

        /// <summary>The top-level pieces of a JSON array as raw text (<c>[1,[2,3],"x"]</c> gives <c>1</c>, <c>[2,3]</c>, <c>"x"</c>).</summary>
        public static List<string> SplitElements(string json)
        {
            var parts = new List<string>();
            if (json == null) return parts;

            json = json.Trim();
            if (json.Length < 2 || json[0] != '[' || json[json.Length - 1] != ']') return parts;

            int depth = 0, start = 1;
            var inString = false;

            for (int i = 1; i < json.Length - 1; i++)
            {
                var c = json[i];

                if (inString)
                {
                    if (c == '\\') i++;
                    else if (c == '"') inString = false;
                    continue;
                }

                if (c == '"') inString = true;
                else if (c == '[' || c == '{') depth++;
                else if (c == ']' || c == '}') depth--;
                else if (c == ',' && depth == 0)
                {
                    parts.Add(json.Substring(start, i - start).Trim());
                    start = i + 1;
                }
            }

            var last = json.Substring(start, json.Length - 1 - start).Trim();
            if (last.Length > 0 || parts.Count > 0) parts.Add(last);
            return parts;
        }

        /// <summary>One piece of tuple JSON as the Explorer shows it: a nested tuple in parentheses, an enum member as Type.MEMBER, the rest as written.</summary>
        public static string Format(string token)
        {
            if (string.IsNullOrEmpty(token)) return string.Empty;
            token = token.Trim();

            if (token[0] == '[')
            {
                var sb = new StringBuilder("(");
                var items = SplitElements(token);
                for (int i = 0; i < items.Count; i++)
                {
                    if (i > 0) sb.Append(", ");
                    sb.Append(Format(items[i]));
                }
                return sb.Append(')').ToString();
            }

            if (token[0] == '{')
            {
                string enumText;
                return TryEnum(token, out enumText) ? enumText : "{…}";
            }

            return token;
        }

        /// <summary>The whole tuple on one line, for example <c>(1.5, "Sword", Rarity.RARE)</c>, cut after <paramref name="max"/> characters.</summary>
        public static string Summary(string json, int max = 80)
        {
            var text = Format(json);
            return text.Length > max ? text.Substring(0, max - 1) + "…" : text;
        }

        /// <summary>A short type name for a piece of tuple JSON: int, float, string, bool, enum, tuple, object or null.</summary>
        public static string Chip(string token)
        {
            if (string.IsNullOrEmpty(token)) return "null";
            token = token.Trim();

            switch (token[0])
            {
                case '[': return "tuple";
                case '{': { string ignored; return TryEnum(token, out ignored) ? "enum" : "object"; }
                case '"': return "string";
                case 't': case 'f': return "bool";
                case 'n': return "null";
                default:  return token.IndexOfAny(new[] { '.', 'e', 'E' }) >= 0 ? "float" : "int";
            }
        }

        /// <summary>True for a piece of JSON that is itself a list of elements (a nested tuple).</summary>
        public static bool IsNested(string token)
        {
            return !string.IsNullOrEmpty(token) && token.TrimStart().StartsWith("[", StringComparison.Ordinal);
        }

        private static bool TryEnum(string token, out string text)
        {
            text = null;
            try
            {
                var obj = MdixJsonValue.Parse(token);
                if (obj.Kind != MdixJsonKind.Object || !obj.ContainsKey("enum_name") || !obj.ContainsKey("field_name")) return false;

                text = obj["enum_name"].AsString() + "." + obj["field_name"].AsString();
                return true;
            }
            catch (Exception)
            {
                return false;
            }
        }
    }
}
