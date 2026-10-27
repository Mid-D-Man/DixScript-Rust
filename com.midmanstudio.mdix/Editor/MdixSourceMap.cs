using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// The kinds of literal the Explorer can edit. Regex and Blob are written <c>r:("...")</c> and
    /// <c>b:("...")</c>; their span covers the whole of that, prefix and parentheses included.
    /// </summary>
    internal enum MdixLiteralKind { Int, Long, Float, Double, Bool, String, HexColor, RawString, Date, Timestamp, Regex, Blob }

    /// <summary>Where one literal sits in the source text.</summary>
    internal readonly struct MdixValueSpan
    {
        public readonly int              Start;
        public readonly int              Length;
        public readonly MdixLiteralKind  Kind;

        public MdixValueSpan(int start, int length, MdixLiteralKind kind)
        {
            Start = start; Length = length; Kind = kind;
        }

        public int End => Start + Length;

        public string TextIn(string source) => source.Substring(Start, Length);
    }

    /// <summary>
    /// Finds the plain literals of the @DATA section and the data path each one produces, so the Explorer
    /// can change a value by replacing exactly those characters.
    ///
    /// Only what can be located with certainty is mapped: a literal that is the whole value of a property,
    /// an object member or an array item. That includes `backtick` raw strings, dates, timestamps and the
    /// regex and blob constructors <c>r:("...")</c> / <c>b:("...")</c>. Anything computed (function-call
    /// arguments, enum members, expressions, tuples) is left out, so it stays read-only.
    ///
    /// This is a small reader of the syntax, not the compiler's parser, so nothing it says is trusted on
    /// its own: <see cref="Matches"/> checks each span against the compiled data, and
    /// <see cref="MdixLiteralEdit.Apply"/> compiles the edited text before accepting it.
    /// </summary>
    internal static class MdixSourceMap
    {
        private const int MaxDepth = 48;

        // ── Public ────────────────────────────────────────────────────────────

        /// <summary>The literal spans of the source's @DATA section, by data path.</summary>
        public static Dictionary<string, MdixValueSpan> Build(string source)
        {
            var spans = new Dictionary<string, MdixValueSpan>(StringComparer.Ordinal);
            if (string.IsNullOrEmpty(source)) return spans;

            try
            {
                var tokens = Tokenize(source);
                new Parser(source, tokens, spans).ParseData();
            }
            catch (Exception)
            {
                // A reader that cannot make sense of the text maps nothing; the Explorer stays read-only.
                spans.Clear();
            }
            return spans;
        }

        /// <summary>True when the compiled value at <paramref name="path"/> is exactly the literal in <paramref name="span"/>.</summary>
        public static bool Matches(MdixDatabase db, string path, MdixValueSpan span, string source)
        {
            if (span.Start < 0 || span.End > source.Length) return false;
            var text = span.TextIn(source);
            var type = db.GetValueType(path);
            var inv  = CultureInfo.InvariantCulture;

            switch (span.Kind)
            {
                case MdixLiteralKind.Int:
                {
                    long n;
                    if (type != MdixValueType.Int || !TryParseWhole(text, out n)) return false;
                    var r = db.GetInt(path);
                    return r.IsSuccess && r.SuccessResult == n;
                }
                case MdixLiteralKind.Long:
                {
                    long n;
                    if (type != MdixValueType.Long || !TryParseWhole(TrimSuffix(text, 'L', 'l'), out n)) return false;
                    var r = db.GetLong(path);
                    return r.IsSuccess && r.SuccessResult == n;
                }
                case MdixLiteralKind.Float:
                {
                    float f;
                    if (type != MdixValueType.Float ||
                        !float.TryParse(Digits(TrimSuffix(text, 'f', 'F')), NumberStyles.Float, inv, out f)) return false;
                    var r = db.GetFloat(path);
                    return r.IsSuccess && r.SuccessResult.Equals(f);
                }
                case MdixLiteralKind.Double:
                {
                    double d;
                    if (type != MdixValueType.Double || !double.TryParse(Digits(text), NumberStyles.Float, inv, out d)) return false;
                    var r = db.GetDouble(path);
                    return r.IsSuccess && r.SuccessResult.Equals(d);
                }
                case MdixLiteralKind.Bool:
                {
                    if (type != MdixValueType.Bool) return false;
                    var r = db.GetBool(path);
                    return r.IsSuccess && r.SuccessResult == (text == "true");
                }
                case MdixLiteralKind.String:
                {
                    if (type != MdixValueType.String) return false;
                    var r = db.GetString(path);
                    return r.IsSuccess && string.Equals(r.SuccessResult, DecodeString(text), StringComparison.Ordinal);
                }
                case MdixLiteralKind.HexColor:
                {
                    if (type != MdixValueType.HexColor) return false;
                    float r1, g1, b1, a1;
                    if (!TryParseHex(text, out r1, out g1, out b1, out a1)) return false;
                    var c = db.GetHexColor(path);
                    if (c.IsFailure) return false;
                    var v = c.SuccessResult;
                    return Near(v.R, r1) && Near(v.G, g1) && Near(v.B, b1) && Near(v.A, a1);
                }
                case MdixLiteralKind.RawString:
                {
                    if (type != MdixValueType.String || text.Length < 2) return false;
                    var r = db.GetString(path);
                    return r.IsSuccess && string.Equals(r.SuccessResult, text.Substring(1, text.Length - 2), StringComparison.Ordinal);
                }
                case MdixLiteralKind.Date:
                case MdixLiteralKind.Timestamp:
                {
                    // The engine keeps a date or timestamp as the text that was written.
                    var want = span.Kind == MdixLiteralKind.Date ? MdixValueType.Date : MdixValueType.Timestamp;
                    if (type != want) return false;
                    var r = db.GetString(path);
                    return r.IsSuccess && string.Equals(r.SuccessResult, text, StringComparison.Ordinal);
                }
                case MdixLiteralKind.Regex:
                {
                    string arg;
                    if (type != MdixValueType.Regex || !TryReadCallArgument(text, out arg)) return false;
                    var r = db.GetRegex(path);
                    return r.IsSuccess && string.Equals(r.SuccessResult.Pattern, arg, StringComparison.Ordinal);
                }
                case MdixLiteralKind.Blob:
                {
                    string arg;
                    if (type != MdixValueType.Blob || !TryReadCallArgument(text, out arg)) return false;
                    var r = db.GetBlob(path);
                    return r.IsSuccess && string.Equals(r.SuccessResult.RawBase64, arg, StringComparison.Ordinal);
                }
            }
            return false;
        }

        // ── Literal text helpers (shared with MdixLiteralEdit) ────────────────

        internal static string Digits(string s) => s.Replace("_", string.Empty);

        internal static string TrimSuffix(string s, char a, char b) =>
            s.Length > 0 && (s[s.Length - 1] == a || s[s.Length - 1] == b) ? s.Substring(0, s.Length - 1) : s;

        internal static bool TryParseWhole(string text, out long value) =>
            long.TryParse(Digits(text), NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out value);

        /// <summary>The characters of a quoted literal, with the compiler's escapes applied.</summary>
        internal static string DecodeString(string literal)
        {
            if (literal.Length < 2) return string.Empty;
            var sb = new StringBuilder(literal.Length);
            for (int i = 1; i < literal.Length - 1; i++)
            {
                var c = literal[i];
                if (c != '\\' || i + 1 >= literal.Length - 1) { sb.Append(c); continue; }

                var e = literal[++i];
                switch (e)
                {
                    case 'n': sb.Append('\n'); break;
                    case 't': sb.Append('\t'); break;
                    case 'r': sb.Append('\r'); break;
                    case '0': sb.Append('\0'); break;
                    default:  sb.Append(e);    break;
                }
            }
            return sb.ToString();
        }

        /// <summary>
        /// Splits <c>r:("...")</c> or <c>b:("...")</c> into its argument literal exactly as written (quotes or
        /// backticks included). False when the text is not that shape.
        /// </summary>
        internal static bool TrySplitCall(string literal, out string argument)
        {
            argument = null;
            if (literal == null || literal.Length < 6 || literal[1] != ':' || literal[2] != '(') return false;

            int i = 3;
            while (i < literal.Length && char.IsWhiteSpace(literal[i])) i++;
            if (i >= literal.Length) return false;

            var open = literal[i];
            int j;
            if (open == '`')
            {
                j = literal.IndexOf('`', i + 1);
                if (j < 0) return false;
            }
            else if (open == '"' || open == '\'')
            {
                j = i + 1;
                while (j < literal.Length)
                {
                    if (literal[j] == '\\') { j += 2; continue; }
                    if (literal[j] == open) break;
                    j++;
                }
                if (j >= literal.Length) return false;
            }
            else return false;

            argument = literal.Substring(i, j - i + 1);

            int k = j + 1;
            while (k < literal.Length && char.IsWhiteSpace(literal[k])) k++;
            return k == literal.Length - 1 && literal[k] == ')';
        }

        /// <summary>The characters of an argument literal: quoted text with the compiler's escapes applied, raw text as it is.</summary>
        internal static string ReadArgument(string argument) =>
            argument.Length >= 2 && argument[0] == '`'
                ? argument.Substring(1, argument.Length - 2)
                : DecodeString(argument);

        /// <summary>The decoded text inside <c>r:(...)</c> or <c>b:(...)</c>.</summary>
        internal static bool TryReadCallArgument(string literal, out string value)
        {
            value = null;
            string argument;
            if (!TrySplitCall(literal, out argument)) return false;
            value = ReadArgument(argument);
            return true;
        }

        internal static bool TryParseHex(string text, out float r, out float g, out float b, out float a)
        {
            r = g = b = 0f; a = 1f;
            if (text.Length < 2 || text[0] != '#') return false;
            var h = text.Substring(1);

            if (h.Length == 3)      h = "" + h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
            else if (h.Length != 6 && h.Length != 8) return false;

            int v;
            for (int i = 0; i < h.Length; i += 2)
                if (!int.TryParse(h.Substring(i, 2), NumberStyles.HexNumber, CultureInfo.InvariantCulture, out v)) return false;

            r = Byte(h, 0); g = Byte(h, 2); b = Byte(h, 4);
            if (h.Length == 8) a = Byte(h, 6);
            return true;
        }

        private static float Byte(string h, int i) =>
            int.Parse(h.Substring(i, 2), NumberStyles.HexNumber, CultureInfo.InvariantCulture) / 255f;

        private static bool Near(float x, float y) => Math.Abs(x - y) < 0.002f;

        // ── Tokens ────────────────────────────────────────────────────────────

        private enum TokType { Ident, Number, Other, Str, RawStr, Hex, Sym, Temporal }

        private sealed class Tok
        {
            public TokType         Type;
            public int             Start;
            public int             End;
            public bool            Newline;        // a line break came before this token
            public bool            BlankLine;      // two or more did
            public string          Sym = string.Empty;
            public MdixLiteralKind Num;            // for Number and Temporal
        }

        private static bool IsIdentStart(char c) => (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
        private static bool IsIdentPart(char c)  => IsIdentStart(c) || (c >= '0' && c <= '9');
        private static bool IsDigit(char c)      => c >= '0' && c <= '9';
        private static bool IsHexDigit(char c)   => IsDigit(c) || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');

        private static readonly string[] TwoCharOps =
        {
            "::", "=>", "->", "==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "**", "++", "--",
            "+=", "-=", "*=", "/=", "%=", "^=", "&=", "|=",
        };

        private static List<Tok> Tokenize(string s)
        {
            var list = new List<Tok>(s.Length / 4 + 8);
            int n = s.Length, i = 0, newlines = 0;

            while (i < n)
            {
                var c = s[i];

                if (c == '\n') { newlines++; i++; continue; }
                if (char.IsWhiteSpace(c)) { i++; continue; }

                if (c == '/' && i + 1 < n && s[i + 1] == '/')
                {
                    while (i < n && s[i] != '\n') i++;
                    continue;
                }
                if (c == '/' && i + 1 < n && s[i + 1] == '*')
                {
                    var close = s.IndexOf("*/", i + 2, StringComparison.Ordinal);
                    var stop  = close < 0 ? n : close + 2;
                    for (int k = i; k < stop; k++) if (s[k] == '\n') newlines++;
                    i = stop;
                    continue;
                }

                var tok = new Tok { Start = i, Newline = newlines > 0, BlankLine = newlines > 1 };
                newlines = 0;

                if (c == '"' || c == '\'')
                {
                    int j = i + 1;
                    var closed = false;
                    while (j < n)
                    {
                        if (s[j] == '\\') { j += 2; continue; }
                        if (s[j] == c)    { closed = true; j++; break; }
                        if (s[j] == '\n') break;                       // the compiler stops a string at a line break
                        j++;
                    }
                    tok.Type = closed ? TokType.Str : TokType.Other;
                    tok.End  = Math.Min(j, n);
                }
                else if (c == '`')
                {
                    var close = s.IndexOf('`', i + 1);
                    tok.Type = TokType.RawStr;
                    tok.End  = close < 0 ? n : close + 1;
                }
                else if (c == '0' && i + 1 < n && (s[i + 1] == 'x' || s[i + 1] == 'X' || s[i + 1] == 'b' || s[i + 1] == 'B'))
                {
                    int j = i + 2;
                    while (j < n && (IsIdentPart(s[j]))) j++;
                    tok.Type = TokType.Other;
                    tok.End  = j;
                }
                else if (IsDigit(c) || (c == '-' && i + 1 < n && IsDigit(s[i + 1])))
                {
                    ScanNumber(s, i, tok);
                }
                else if (c == '#')
                {
                    int j = i + 1;
                    while (j < n && IsHexDigit(s[j]) && j - i < 9) j++;
                    tok.Type = TokType.Hex;
                    tok.End  = j;
                }
                else if (IsIdentStart(c))
                {
                    int j = i + 1;
                    while (j < n && IsIdentPart(s[j])) j++;
                    tok.Type = TokType.Ident;
                    tok.End  = j;
                }
                else
                {
                    tok.Type = TokType.Sym;
                    tok.Sym  = c.ToString();
                    tok.End  = i + 1;

                    if (i + 1 < n)
                    {
                        var two = s.Substring(i, 2);
                        foreach (var op in TwoCharOps)
                            if (op == two) { tok.Sym = op; tok.End = i + 2; break; }
                    }
                }

                list.Add(tok);
                i = tok.End;
                if (tok.End <= tok.Start) i = tok.Start + 1;            // never stand still
            }
            return list;
        }

        /// <summary>Follows the compiler's own scan of decimal numbers, dates and timestamps.</summary>
        private static void ScanNumber(string s, int start, Tok tok)
        {
            int n = s.Length, j = start;
            bool neg = false, hasDot = false, hasExp = false, isDate = false, isTs = false, inTz = false;
            int dashes = 0, colons = 0;

            if (s[j] == '-') { neg = true; j++; }

            while (j < n)
            {
                var b = s[j];
                if (IsDigit(b)) { j++; }
                else if (b == '_' && !isDate && !isTs) { j++; }
                else if (b == '.' && !hasDot && !hasExp && !isDate)
                {
                    if (j + 1 < n && IsDigit(s[j + 1])) { hasDot = true; j++; } else break;
                }
                else if ((b == 'e' || b == 'E') && !hasExp && !isDate)
                {
                    hasExp = true; j++;
                    if (j < n && (s[j] == '+' || s[j] == '-')) j++;
                }
                else if (b == '-' && dashes < 2 && !neg) { dashes++; isDate = true; j++; }
                else if (b == 'T' && isDate) { isTs = true; j++; }
                else if (b == ':' && isTs)
                {
                    if (inTz && colons >= 2) j++;
                    else if (colons < 2) { colons++; j++; }
                    else break;
                }
                else if (b == '.' && isTs) { j++; }
                else if (b == 'Z' && isTs) { j++; break; }
                else if ((b == '+' || b == '-') && isTs)
                {
                    if (s.IndexOf('T', start, j - start) >= 0) { inTz = true; j++; } else break;
                }
                else break;
            }

            if (isDate || isTs)
            {
                // The scan is as loose as the compiler's, so "5-3" comes this far too. Only text that is
                // really a date or a timestamp becomes one; anything else stays an unmapped token.
                tok.End  = j;
                tok.Type = TokType.Other;

                var text = s.Substring(start, j - start);
                if (isTs ? MdixTemporalText.IsTimestampLiteral(text) : MdixTemporalText.IsDateLiteral(text))
                {
                    tok.Type = TokType.Temporal;
                    tok.Num  = isTs ? MdixLiteralKind.Timestamp : MdixLiteralKind.Date;
                }
                return;
            }

            var longSuffix  = false;
            var floatSuffix = false;
            if (j < n)
            {
                var nb = s[j];
                if ((nb == 'L' || nb == 'l') && !hasDot && !hasExp)
                {
                    if (!(j + 1 < n && IsIdentPart(s[j + 1]))) { j++; longSuffix = true; }
                }
                else if (nb == 'f' || nb == 'F') { j++; floatSuffix = true; }
            }

            tok.End = j;
            tok.Type = TokType.Number;

            if (longSuffix)                 tok.Num = MdixLiteralKind.Long;
            else if (floatSuffix)           tok.Num = MdixLiteralKind.Float;
            else if (hasDot || hasExp)      tok.Num = MdixLiteralKind.Double;
            else
            {
                long whole;
                var digits = Digits(s.Substring(start, j - start));
                if (!long.TryParse(digits, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out whole))
                    tok.Type = TokType.Other;
                else
                    tok.Num = whole >= int.MinValue && whole <= int.MaxValue ? MdixLiteralKind.Int : MdixLiteralKind.Long;
            }
        }

        // ── Parser ────────────────────────────────────────────────────────────

        private sealed class Parser
        {
            private readonly string                             _src;
            private readonly List<Tok>                          _t;
            private readonly Dictionary<string, MdixValueSpan>  _spans;
            private int                                         _p;

            public Parser(string src, List<Tok> tokens, Dictionary<string, MdixValueSpan> spans)
            {
                _src = src; _t = tokens; _spans = spans;
            }

            private bool End => _p >= _t.Count;
            private Tok Cur => _t[_p];

            private bool IsSym(int idx, string sym) => idx >= 0 && idx < _t.Count && _t[idx].Type == TokType.Sym && _t[idx].Sym == sym;
            private bool IsIdent(int idx)            => idx >= 0 && idx < _t.Count && _t[idx].Type == TokType.Ident;
            private bool IsIdentText(int idx, string text) =>
                IsIdent(idx) && string.CompareOrdinal(_src, _t[idx].Start, text, 0, text.Length) == 0 &&
                _t[idx].End - _t[idx].Start == text.Length;

            // Section names are not case-sensitive (@Data is @DATA), unlike true/false.
            private bool IsSectionNameText(int idx, string name) =>
                IsIdent(idx) && _t[idx].End - _t[idx].Start == name.Length &&
                string.Compare(_src, _t[idx].Start, name, 0, name.Length, StringComparison.OrdinalIgnoreCase) == 0;

            // Finds "@DATA(" (in any case) and reads what is inside.
            public void ParseData()
            {
                for (int i = 0; i + 2 < _t.Count; i++)
                {
                    if (IsSym(i, "@") && IsSectionNameText(i + 1, "DATA") && IsSym(i + 2, "(") && _t[i + 1].Start == _t[i].End)
                    {
                        _p = i + 3;
                        ParseEntries();
                        return;
                    }
                }
            }

            private void SkipCommas()
            {
                while (!End && IsSym(_p, ",")) _p++;
            }

            private void ParseEntries()
            {
                var guard = 0;
                while (!End && guard++ < 1_000_000)
                {
                    SkipCommas();
                    if (End || IsSym(_p, ")")) return;

                    var before = _p;
                    ParseEntry();
                    if (_p == before) _p++;                       // never stand still
                }
            }

            // ── Entries ───────────────────────────────────────────────────────

            private void ParseEntry()
            {
                if (!IsIdent(_p)) { SkipExpression(topLevel: true); return; }

                var name = Text(_p);
                _p++;

                var fusedEquals = SkipAnnotation();

                if (fusedEquals || IsSym(_p, "="))
                {
                    if (!fusedEquals) _p++;
                    ParseValue(name, topLevel: true, depth: 0);
                    return;
                }

                // name.path:   name:   name::   — tables and group arrays
                var path = name;
                while (IsSym(_p, ".") && IsIdent(_p + 1))
                {
                    path += "." + Text(_p + 1);
                    _p += 2;
                }

                if (IsSym(_p, "::"))
                {
                    _p++;
                    ParseGroupRows(path);
                    return;
                }
                if (IsSym(_p, ":"))
                {
                    _p++;
                    if (IsSym(_p, ":")) { _p++; ParseGroupRows(path); return; }
                    ParseTable(path);
                    return;
                }

                SkipExpression(topLevel: true);
            }

            /// <summary>Steps over &lt;type&gt;; true when its closing bracket was fused with the "=" after it.</summary>
            private bool SkipAnnotation()
            {
                if (!IsSym(_p, "<")) return false;

                var depth = 0;
                while (!End)
                {
                    var sym = Cur.Type == TokType.Sym ? Cur.Sym : string.Empty;
                    _p++;

                    if (sym == "<") depth++;
                    else if (sym == ">") depth--;
                    else if (sym == ">>") depth -= 2;
                    else if (sym == ">=") { depth--; if (depth <= 0) return true; }
                    else if (sym == "<<") depth += 2;
                    else if (sym == ")" || sym == "(" || sym == "{" || sym == "}") return false;     // not an annotation

                    if (depth <= 0) return false;
                }
                return false;
            }

            private void ParseTable(string basePath)
            {
                var count = 0;
                var guard = 0;
                while (!End && guard++ < 1_000_000)
                {
                    SkipCommas();
                    if (End || IsSym(_p, ")")) return;
                    if (!IsIdent(_p)) return;
                    if (IsSym(_p + 1, ".") || IsSym(_p + 1, ":") || IsSym(_p + 1, "::")) return;   // the next table or group array

                    var save = _p;
                    var key  = Text(_p);
                    _p++;
                    var fused = SkipAnnotation();
                    if (!fused && !IsSym(_p, "=")) { _p = save; return; }
                    if (!fused) _p++;

                    ParseValue(basePath + "." + key, topLevel: true, depth: 0);
                    count++;
                }
            }

            private void ParseGroupRows(string basePath)
            {
                var row   = 0;
                var guard = 0;
                while (!End && guard++ < 1_000_000)
                {
                    SkipCommas();
                    if (End || IsSym(_p, ")")) return;
                    if (row > 0 && Cur.Newline && IsEntryStart(_p)) return;
                    if (row == 0 && IsEntryStart(_p) && !IsSym(_p, "{")) { /* a header with no rows */ return; }

                    var before = _p;
                    if (IsSym(_p, "{")) ParseObject(basePath + "[" + row + "]", 0);
                    else                SkipExpression(topLevel: true);

                    if (_p == before) _p++;
                    row++;
                }
            }

            /// <summary>True when the token at idx begins a new DATA entry (name =, name:, name::, name.path:).</summary>
            private bool IsEntryStart(int idx)
            {
                if (!IsIdent(idx)) return false;

                var save = _p;
                _p = idx + 1;
                var fused = SkipAnnotation();
                var at    = _p;
                _p = save;

                if (fused) return true;
                if (IsSym(at, "=") || IsSym(at, "::") || IsSym(at, ":")) return true;

                // name.path:  or  name.path::
                var j = at;
                var dots = 0;
                while (IsSym(j, ".") && IsIdent(j + 1)) { j += 2; dots++; }
                return dots > 0 && (IsSym(j, ":") || IsSym(j, "::"));
            }

            // ── Values ────────────────────────────────────────────────────────

            private void ParseValue(string path, bool topLevel, int depth)
            {
                if (End || depth > MaxDepth) return;
                var tok = Cur;

                if (tok.Type == TokType.Sym && tok.Sym == "{") { ParseObject(path, depth + 1); return; }
                if (tok.Type == TokType.Sym && tok.Sym == "[") { ParseArray(path, depth + 1);  return; }

                if (tok.Type == TokType.Ident && TryMapPrefixed(path)) return;

                MdixLiteralKind kind;
                var literal = IsEditableLiteral(tok, out kind);

                // A value that is one token (a number, text, date, colour, word): done when nothing joins it
                // into a longer expression. Calls, members and arithmetic are stepped over whole.
                if (tok.Type == TokType.Number || tok.Type == TokType.Str || tok.Type == TokType.RawStr ||
                    tok.Type == TokType.Hex    || tok.Type == TokType.Other || tok.Type == TokType.Ident ||
                    tok.Type == TokType.Temporal)
                {
                    var at = _p;
                    _p++;
                    if (!End && ContinuesExpression(_p))
                    {
                        SkipExpression(topLevel);
                        return;
                    }

                    if (literal)
                        _spans[path] = new MdixValueSpan(tok.Start, tok.End - tok.Start, kind);
                    else if (tok.Type == TokType.Ident && IsBoolWord(at))
                        _spans[path] = new MdixValueSpan(tok.Start, tok.End - tok.Start, MdixLiteralKind.Bool);
                    return;
                }

                SkipExpression(topLevel);
            }

            private bool IsBoolWord(int idx) => IsIdentText(idx, "true") || IsIdentText(idx, "false");

            /// <summary>
            /// Maps <c>r:("...")</c> and <c>b:("...")</c> as one literal. The compiler reads these only when the
            /// letter, the colon and the opening parenthesis touch, and takes exactly one string as the argument
            /// (a quoted string or a raw backtick one); anything else, <c>t:(...)</c> tuples included, is not mapped.
            /// </summary>
            private bool TryMapPrefixed(string path)
            {
                var at = _p;
                var id = _t[at];
                if (id.End - id.Start != 1) return false;

                var letter = _src[id.Start];
                if (letter != 'r' && letter != 'b') return false;

                if (!IsSym(at + 1, ":") || _t[at + 1].Start != id.End)         return false;
                if (!IsSym(at + 2, "(") || _t[at + 2].Start != _t[at + 1].End) return false;

                var argAt = at + 3;
                if (argAt >= _t.Count) return false;

                var arg = _t[argAt];
                var isText = arg.Type == TokType.Str ||
                             (arg.Type == TokType.RawStr && arg.End - arg.Start >= 2 && _src[arg.End - 1] == '`');
                if (!isText || !IsSym(argAt + 1, ")")) return false;

                var close = argAt + 1;
                if (close + 1 < _t.Count && ContinuesExpression(close + 1)) return false;

                _spans[path] = new MdixValueSpan(
                    id.Start, _t[close].End - id.Start,
                    letter == 'r' ? MdixLiteralKind.Regex : MdixLiteralKind.Blob);

                _p = close + 1;
                return true;
            }

            private bool IsEditableLiteral(Tok tok, out MdixLiteralKind kind)
            {
                kind = MdixLiteralKind.String;
                switch (tok.Type)
                {
                    case TokType.Number: kind = tok.Num; return true;
                    case TokType.Str:    kind = MdixLiteralKind.String; return true;
                    case TokType.Temporal: kind = tok.Num; return true;
                    case TokType.RawStr:
                        // Only a raw string that has its closing backtick.
                        kind = MdixLiteralKind.RawString;
                        return tok.End - tok.Start >= 2 && _src[tok.End - 1] == '`';
                    case TokType.Hex:
                    {
                        var len = tok.End - tok.Start - 1;
                        if (len != 3 && len != 6 && len != 8) return false;
                        kind = MdixLiteralKind.HexColor;
                        return true;
                    }
                }
                return false;
            }

            /// <summary>An operator right after a value turns it into an expression.</summary>
            private bool ContinuesExpression(int idx)
            {
                if (idx >= _t.Count) return false;
                var tok = _t[idx];
                if (tok.Type != TokType.Sym) return false;
                return tok.Sym != "," && tok.Sym != ")" && tok.Sym != "]" && tok.Sym != "}";
            }

            private void ParseObject(string path, int depth)
            {
                var open = _p;
                _p++;                                                       // {
                var guard = 0;

                while (!End && guard++ < 1_000_000)
                {
                    SkipCommas();
                    if (End) { return; }
                    if (IsSym(_p, "}")) { _p++; return; }

                    string key = null;
                    if (IsIdent(_p)) key = Text(_p);
                    else if (Cur.Type == TokType.Str) key = DecodeString(_src.Substring(Cur.Start, Cur.End - Cur.Start));

                    if (key == null) { Recover(open); return; }
                    _p++;
                    var fused = SkipAnnotation();
                    if (!fused && !IsSym(_p, "=") && !IsSym(_p, ":")) { Recover(open); return; }
                    if (!fused) _p++;

                    ParseValue(path + "." + key, topLevel: false, depth: depth);
                }
            }

            private void ParseArray(string path, int depth)
            {
                _p++;                                                       // [
                var index = 0;
                var guard = 0;

                while (!End && guard++ < 1_000_000)
                {
                    SkipCommas();
                    if (End) return;
                    if (IsSym(_p, "]")) { _p++; return; }

                    var before = _p;
                    ParseValue(path + "[" + index + "]", topLevel: false, depth: depth);
                    if (_p == before) _p++;
                    index++;
                }
            }

            /// <summary>Gives up on a bracket group the reader does not understand and steps over it whole.</summary>
            private void Recover(int open)
            {
                _p = open;
                SkipBalanced();
            }

            private void SkipBalanced()
            {
                var depth = 0;
                while (!End)
                {
                    var tok = Cur;
                    if (tok.Type == TokType.Sym)
                    {
                        if (tok.Sym == "(" || tok.Sym == "[" || tok.Sym == "{") depth++;
                        else if (tok.Sym == ")" || tok.Sym == "]" || tok.Sym == "}") depth--;
                    }
                    _p++;
                    if (depth <= 0) return;
                }
            }

            /// <summary>Steps over one value the reader does not map (a call, an enum member, an expression, ...).</summary>
            private void SkipExpression(bool topLevel)
            {
                var depth = 0;
                var first = true;

                while (!End)
                {
                    var tok = Cur;

                    if (depth == 0 && tok.Type == TokType.Sym &&
                        (tok.Sym == "," || tok.Sym == ")" || tok.Sym == "]" || tok.Sym == "}"))
                        return;

                    if (depth == 0 && topLevel && !first && tok.Newline && IsEntryStart(_p))
                        return;

                    if (tok.Type == TokType.Sym)
                    {
                        if (tok.Sym == "(" || tok.Sym == "[" || tok.Sym == "{") depth++;
                        else if (tok.Sym == ")" || tok.Sym == "]" || tok.Sym == "}") depth--;
                    }

                    _p++;
                    first = false;
                }
            }

            private string Text(int idx) => _src.Substring(_t[idx].Start, _t[idx].End - _t[idx].Start);
        }
    }
}
