using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>What an Explorer edit did, or why it was refused.</summary>
    internal sealed class MdixEditResult
    {
        public bool          Ok;
        public string        Error      = string.Empty;
        public string        NewSource  = string.Empty;
        public string        OldText    = string.Empty;        // the literal as it was written
        public string        NewText    = string.Empty;        // the literal as it is now written
        public bool          Unchanged;                        // the new value is the one already there
        public int           AlsoChangedCount;
        public List<string>  AlsoChanged = new List<string>(); // other data paths whose value moved, first few

        public static MdixEditResult Fail(string error) => new MdixEditResult { Ok = false, Error = error };
    }

    /// <summary>
    /// Changes one plain literal of a .mdix file and proves the change did what was asked: the edited text is
    /// compiled again, the value at the path must be exactly the new one, and the data must still have the same
    /// paths. Values computed from the edited one are allowed to move, and are listed.
    /// </summary>
    internal static class MdixLiteralEdit
    {
        private const int MaxSnapshot     = 60000;
        private const int MaxListedChange = 4;

        private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        // ── What the field shows ──────────────────────────────────────────────

        /// <summary>The value as plain text for the edit field: no quotes, no type suffix.</summary>
        public static string EditText(MdixDatabase db, string path, MdixValueSpan span, string source)
        {
            switch (span.Kind)
            {
                case MdixLiteralKind.Int:      return db.GetInt(path).UnwrapOr(0).ToString(Inv);
                case MdixLiteralKind.Long:     return db.GetLong(path).UnwrapOr(0L).ToString(Inv);
                case MdixLiteralKind.Float:    return db.GetFloat(path).UnwrapOr(0f).ToString("R", Inv);
                case MdixLiteralKind.Double:   return db.GetDouble(path).UnwrapOr(0d).ToString("R", Inv);
                case MdixLiteralKind.Bool:     return db.GetBool(path).UnwrapOr(false) ? "true" : "false";
                case MdixLiteralKind.String:   return db.GetString(path).UnwrapOr(string.Empty);
                case MdixLiteralKind.HexColor: return span.TextIn(source);
            }
            return string.Empty;
        }

        // ── Writing a literal ─────────────────────────────────────────────────

        /// <summary>
        /// The source text for <paramref name="input"/> as a literal of the given kind. The kind never changes,
        /// so a Long stays a Long (it keeps its L) and a Float keeps its f. <paramref name="expected"/> is how the
        /// compiled value will read back, for the check after compiling (null when it is checked another way).
        /// </summary>
        public static bool TryFormat(
            MdixLiteralKind kind, string oldLiteral, string input, out string literal, out string expected, out string error)
        {
            literal = null; expected = null; error = null;
            var text = (input ?? string.Empty).Trim();

            switch (kind)
            {
                case MdixLiteralKind.Int:
                {
                    long n;
                    if (!MdixSourceMap.TryParseWhole(text, out n) || n < int.MinValue || n > int.MaxValue)
                    { error = "Enter a whole number from -2147483648 to 2147483647."; return false; }
                    literal = expected = n.ToString(Inv);
                    return true;
                }
                case MdixLiteralKind.Long:
                {
                    long n;
                    if (!MdixSourceMap.TryParseWhole(MdixSourceMap.TrimSuffix(text, 'L', 'l'), out n))
                    { error = "Enter a whole number."; return false; }
                    expected = n.ToString(Inv);
                    literal  = expected + "L";
                    return true;
                }
                case MdixLiteralKind.Float:
                {
                    float f;
                    if (!float.TryParse(MdixSourceMap.Digits(MdixSourceMap.TrimSuffix(text, 'f', 'F')),
                                        NumberStyles.Float, Inv, out f) || float.IsNaN(f) || float.IsInfinity(f))
                    { error = "Enter a number."; return false; }

                    var s = f.ToString("R", Inv);
                    float back;
                    if (!float.TryParse(s, NumberStyles.Float, Inv, out back) || back != f) s = f.ToString("G9", Inv);
                    literal  = s + "f";
                    expected = s;
                    return true;
                }
                case MdixLiteralKind.Double:
                {
                    double d;
                    if (!double.TryParse(MdixSourceMap.Digits(text), NumberStyles.Float, Inv, out d) ||
                        double.IsNaN(d) || double.IsInfinity(d))
                    { error = "Enter a number."; return false; }

                    var s = d.ToString("R", Inv);
                    double back;
                    if (!double.TryParse(s, NumberStyles.Float, Inv, out back) || back != d) s = d.ToString("G17", Inv);
                    expected = s;
                    literal  = s.IndexOf('.') < 0 && s.IndexOf('E') < 0 ? s + ".0" : s;     // a bare 5 would become an Int
                    return true;
                }
                case MdixLiteralKind.Bool:
                {
                    var lower = text.ToLowerInvariant();
                    if (lower != "true" && lower != "false") { error = "Enter true or false."; return false; }
                    literal = expected = lower;
                    return true;
                }
                case MdixLiteralKind.String:
                {
                    if (input == null) input = string.Empty;
                    var quote = oldLiteral.Length > 0 && oldLiteral[0] == '\'' ? '\'' : '"';
                    var sb = new StringBuilder(input.Length + 2);
                    sb.Append(quote);
                    foreach (var c in input)
                    {
                        if (c == '\0') { error = "A text cannot contain a null character."; return false; }
                        if (c == '\\')            sb.Append("\\\\");
                        else if (c == quote)      sb.Append('\\').Append(c);
                        else if (c == '\n')       sb.Append("\\n");
                        else if (c == '\r')       sb.Append("\\r");
                        else if (c == '\t')       sb.Append("\\t");
                        else                      sb.Append(c);
                    }
                    sb.Append(quote);
                    literal  = sb.ToString();
                    expected = input;
                    return true;
                }
                case MdixLiteralKind.HexColor:
                {
                    var hex = text.StartsWith("#", StringComparison.Ordinal) ? text.Substring(1) : text;
                    float r, g, b, a;
                    if (!MdixSourceMap.TryParseHex("#" + hex, out r, out g, out b, out a))
                    { error = "Enter a colour like #FF8800 or #FF8800CC."; return false; }
                    literal = "#" + hex.ToUpperInvariant();
                    return true;
                }
            }

            error = "This kind of value cannot be edited here.";
            return false;
        }

        // ── Applying an edit ──────────────────────────────────────────────────

        /// <summary>
        /// Replaces the literal at <paramref name="path"/> with <paramref name="input"/> and checks the result.
        /// Nothing is changed unless the edited text compiles, the path holds exactly the new value, and the data
        /// keeps the same paths.
        /// </summary>
        public static MdixEditResult Apply(string source, string path, string input)
        {
            var spans = MdixSourceMap.Build(source);
            MdixValueSpan span;
            if (!spans.TryGetValue(path, out span))
                return MdixEditResult.Fail("This value is not a plain literal in the file (it is computed), so it cannot be edited here.");

            string literal, expected, error, oldLiteral;
            Dictionary<string, string> before;

            var loaded = Dix.LoadStr(source);
            if (loaded.IsFailure)
                return MdixEditResult.Fail("The file does not compile right now: " + loaded.Error.Message);

            using (var db = loaded.SuccessResult)
            {
                if (!MdixSourceMap.Matches(db, path, span, source))
                    return MdixEditResult.Fail("The file no longer matches what the Explorer shows. Compile it again and retry.");

                oldLiteral = span.TextIn(source);
                if (!TryFormat(span.Kind, oldLiteral, input, out literal, out expected, out error))
                    return MdixEditResult.Fail(error);

                before = Snapshot(db);
            }

            var result = new MdixEditResult { OldText = oldLiteral, NewText = literal };

            if (literal == oldLiteral)
            {
                result.Ok        = true;
                result.Unchanged = true;
                result.NewSource = source;
                return result;
            }

            var edited = source.Substring(0, span.Start) + literal + source.Substring(span.End);

            var reloaded = Dix.LoadStr(edited);
            if (reloaded.IsFailure)
                return MdixEditResult.Fail("That value breaks the file: " + reloaded.Error.Message);

            using (var db = reloaded.SuccessResult)
            {
                var problem = CheckValue(db, path, span.Kind, expected, literal);
                if (problem != null) return MdixEditResult.Fail(problem);

                var after = Snapshot(db);
                if (before != null && after != null)
                {
                    if (before.Count != after.Count)
                        return MdixEditResult.Fail("That value changed the shape of the data (paths were added or removed), so it was not applied.");

                    foreach (var pair in before)
                    {
                        string now;
                        if (!after.TryGetValue(pair.Key, out now))
                            return MdixEditResult.Fail("That value changed the shape of the data (\"" + pair.Key + "\" is gone), so it was not applied.");

                        if (now != pair.Value && pair.Key != path)
                        {
                            result.AlsoChangedCount++;
                            if (result.AlsoChanged.Count < MaxListedChange) result.AlsoChanged.Add(pair.Key);
                        }
                    }
                }
            }

            result.Ok        = true;
            result.NewSource = edited;
            return result;
        }

        private static string CheckValue(MdixDatabase db, string path, MdixLiteralKind kind, string expected, string literal)
        {
            var type = db.GetValueType(path);
            if (type != EngineType(kind))
                return "The new value did not stay a " + kind.ToString().ToLowerInvariant() + " (the file reads it as " + type.ToString().ToLowerInvariant() + ").";

            if (kind == MdixLiteralKind.HexColor)
            {
                float r, g, b, a;
                var c = db.GetHexColor(path);
                if (c.IsFailure || !MdixSourceMap.TryParseHex(literal, out r, out g, out b, out a)) return "The colour could not be read back.";
                var v = c.SuccessResult;
                if (Math.Abs(v.R - r) > 0.002f || Math.Abs(v.G - g) > 0.002f || Math.Abs(v.B - b) > 0.002f || Math.Abs(v.A - a) > 0.002f)
                    return "The colour read back differently from what was entered.";
                return null;
            }

            var shown = Display(db, path, type);
            if (expected != null && shown != expected)
                return "The file reads the new value as \"" + Shorten(shown) + "\", not \"" + Shorten(expected) + "\", so it was not applied.";
            return null;
        }

        private static string Shorten(string s) => s.Length > 40 ? s.Substring(0, 40) + "…" : s;

        private static MdixValueType EngineType(MdixLiteralKind kind)
        {
            switch (kind)
            {
                case MdixLiteralKind.Int:      return MdixValueType.Int;
                case MdixLiteralKind.Long:     return MdixValueType.Long;
                case MdixLiteralKind.Float:    return MdixValueType.Float;
                case MdixLiteralKind.Double:   return MdixValueType.Double;
                case MdixLiteralKind.Bool:     return MdixValueType.Bool;
                case MdixLiteralKind.String:   return MdixValueType.String;
                default:                       return MdixValueType.HexColor;
            }
        }

        // ── Snapshots of the whole data ───────────────────────────────────────

        /// <summary>Every path of the data with its type and exact value; null when there is too much to compare.</summary>
        private static Dictionary<string, string> Snapshot(MdixDatabase db)
        {
            var map    = new Dictionary<string, string>(StringComparer.Ordinal);
            var budget = MaxSnapshot;

            foreach (var name in NamesUnder(db, string.Empty))
                if (!Walk(db, name, map, ref budget)) return null;
            return map;
        }

        private static bool Walk(MdixDatabase db, string path, Dictionary<string, string> map, ref int budget)
        {
            if (budget-- <= 0) return false;

            var type = db.GetValueType(path);
            if (type == MdixValueType.Unknown) return true;

            if (type == MdixValueType.Array)
            {
                var length = db.GetArrayLength(path).UnwrapOr(0);
                map[path] = "array|" + length;
                for (int i = 0; i < length; i++)
                    if (!Walk(db, path + "[" + i + "]", map, ref budget)) return false;
                return true;
            }

            if (type == MdixValueType.Object)
            {
                var names = NamesUnder(db, path);
                map[path] = "object|" + names.Count;
                foreach (var name in names)
                    if (!Walk(db, path + "." + name, map, ref budget)) return false;
                return true;
            }

            map[path] = type + "|" + Display(db, path, type);
            return true;
        }

        private static string Display(MdixDatabase db, string path, MdixValueType type)
        {
            switch (type)
            {
                case MdixValueType.String:    return db.GetString(path).UnwrapOr(string.Empty);
                case MdixValueType.Int:       return db.GetInt(path).UnwrapOr(0).ToString(Inv);
                case MdixValueType.Long:      return db.GetLong(path).UnwrapOr(0L).ToString(Inv);
                case MdixValueType.Float:     return db.GetFloat(path).UnwrapOr(0f).ToString("R", Inv);
                case MdixValueType.Double:    return db.GetDouble(path).UnwrapOr(0d).ToString("R", Inv);
                case MdixValueType.Bool:      return db.GetBool(path).UnwrapOr(false) ? "true" : "false";
                case MdixValueType.Enum:      return db.GetEnumName(path).UnwrapOr("?") + "." + db.GetEnumField(path).UnwrapOr("?");
                case MdixValueType.HexColor:
                {
                    var c = db.GetHexColor(path);
                    if (c.IsFailure) return "#?";
                    var v = c.SuccessResult;
                    return v.R.ToString("F4", Inv) + "," + v.G.ToString("F4", Inv) + "," + v.B.ToString("F4", Inv) + "," + v.A.ToString("F4", Inv);
                }
                case MdixValueType.Date:
                case MdixValueType.Timestamp: return db.GetString(path).UnwrapOr("?");
                case MdixValueType.Null:      return "null";
                default:                      return "(" + type + ")";
            }
        }

        /// <summary>The names directly under a path: the engine also lists "name[0]" and gives no fixed order.</summary>
        private static List<string> NamesUnder(MdixDatabase db, string path)
        {
            var keys = string.IsNullOrEmpty(path) ? db.GetKeys() : db.GetKeys(path);
            var set  = new HashSet<string>(StringComparer.Ordinal);

            foreach (var key in keys.UnwrapOr(Array.Empty<string>()))
            {
                var cut  = key.IndexOfAny(new[] { '.', '[' });
                var name = cut < 0 ? key : key.Substring(0, cut);
                if (name.Length > 0) set.Add(name);
            }

            var list = new List<string>(set);
            list.Sort(StringComparer.Ordinal);
            return list;
        }
    }
}
