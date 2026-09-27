using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace MidManStudio.Mdix.Unity.Editor.Lsp
{
    internal enum MdixJsonKind
    {
        Null,
        Bool,
        Number,
        String,
        Array,
        Object,
    }

    /// <summary>
    /// Minimal, dependency-free JSON value model: parse, build, and serialize.
    ///
    /// Why hand-rolled instead of Newtonsoft.Json or System.Text.Json: neither
    /// is referenced anywhere in this codebase or GrandTheftGrimoire's package
    /// manifest today. mdix-csharp's MdixJson.cs does use System.Text.Json, but
    /// only inside the precompiled MidManStudio.Mdix.Core.dll — whether those
    /// types are actually resolvable from NEW source compiled directly in a
    /// Unity project (as opposed to already being merged into that one
    /// precompiled assembly) isn't something I could confirm without the
    /// actual DLL's build pipeline, which lives outside this package. Rather
    /// than gamble on a compile-time dependency that might not resolve, this
    /// is a small, self-contained JSON layer scoped to exactly what LSP
    /// JSON-RPC needs. If System.Text.Json/Newtonsoft turn out to already be
    /// safely available, swapping this out is a contained, low-risk follow-up
    /// — nothing outside MdixLspClient touches this type.
    ///
    /// Deliberately dynamic (dictionary/list-backed), not attribute-driven
    /// strong typing — LSP messages are deeply nested with many optional
    /// fields, and callers only ever need a handful of fields out of any
    /// given message, so `msg["params"]["textDocument"]["uri"].AsString()`
    /// style access is more practical here than defining a POCO per LSP
    /// message shape.
    /// </summary>
    internal sealed class MdixJsonValue
    {
        public MdixJsonKind Kind { get; private set; }

        private bool                                _boolValue;
        private double                               _numberValue;
        private string?                              _stringValue;
        private List<MdixJsonValue>?                 _arrayValue;
        private Dictionary<string, MdixJsonValue>?   _objectValue;

        // Preserves insertion order for object keys (Dictionary alone doesn't
        // guarantee it across all runtimes) so serialized output is stable
        // and readable in logs/traces.
        private List<string>? _objectKeyOrder;

        public static readonly MdixJsonValue Null = new MdixJsonValue { Kind = MdixJsonKind.Null };

        // ── Factories ─────────────────────────────────────────────────────────

        public static MdixJsonValue Bool(bool value) =>
            new MdixJsonValue { Kind = MdixJsonKind.Bool, _boolValue = value };

        public static MdixJsonValue Number(double value) =>
            new MdixJsonValue { Kind = MdixJsonKind.Number, _numberValue = value };

        public static MdixJsonValue String(string? value) =>
            value == null
                ? Null
                : new MdixJsonValue { Kind = MdixJsonKind.String, _stringValue = value };

        public static MdixJsonValue Array() =>
            new MdixJsonValue { Kind = MdixJsonKind.Array, _arrayValue = new List<MdixJsonValue>() };

        public static MdixJsonValue Object() =>
            new MdixJsonValue
            {
                Kind            = MdixJsonKind.Object,
                _objectValue    = new Dictionary<string, MdixJsonValue>(),
                _objectKeyOrder = new List<string>(),
            };

        // ── Mutation (object/array builders) ─────────────────────────────────

        /// <summary>Set (or add) a property. Only valid on an Object value.</summary>
        public MdixJsonValue this[string key]
        {
            get
            {
                if (Kind != MdixJsonKind.Object || _objectValue == null)
                    return Null;
                return _objectValue.TryGetValue(key, out var v) ? v : Null;
            }
            set
            {
                if (Kind != MdixJsonKind.Object)
                    throw new InvalidOperationException(
                        $"Cannot set property '{key}' on a {Kind} value.");
                if (!_objectValue!.ContainsKey(key))
                    _objectKeyOrder!.Add(key);
                _objectValue[key] = value;
            }
        }

        /// <summary>Index into an Array value. Out-of-range returns Null rather than throwing.</summary>
        public MdixJsonValue this[int index]
        {
            get
            {
                if (Kind != MdixJsonKind.Array || _arrayValue == null) return Null;
                return index >= 0 && index < _arrayValue.Count ? _arrayValue[index] : Null;
            }
        }

        public void Add(MdixJsonValue value)
        {
            if (Kind != MdixJsonKind.Array)
                throw new InvalidOperationException($"Cannot Add to a {Kind} value.");
            _arrayValue!.Add(value);
        }

        // ── Access helpers ────────────────────────────────────────────────────

        public bool IsNull => Kind == MdixJsonKind.Null;

        public bool TryGet(string key, out MdixJsonValue value)
        {
            if (Kind == MdixJsonKind.Object && _objectValue != null &&
                _objectValue.TryGetValue(key, out var v))
            {
                value = v;
                return true;
            }
            value = Null;
            return false;
        }

        public bool ContainsKey(string key) =>
            Kind == MdixJsonKind.Object && _objectValue != null && _objectValue.ContainsKey(key);

        public IReadOnlyList<MdixJsonValue> AsArray() =>
            Kind == MdixJsonKind.Array && _arrayValue != null
                ? _arrayValue
                : System.Array.Empty<MdixJsonValue>();

        public int Count =>
            Kind switch
            {
                MdixJsonKind.Array  => _arrayValue?.Count ?? 0,
                MdixJsonKind.Object => _objectValue?.Count ?? 0,
                _                   => 0,
            };

        public string AsString(string fallback = "") =>
            Kind == MdixJsonKind.String ? _stringValue ?? fallback : fallback;

        public int AsInt(int fallback = 0) =>
            Kind == MdixJsonKind.Number ? (int)_numberValue : fallback;

        public double AsDouble(double fallback = 0d) =>
            Kind == MdixJsonKind.Number ? _numberValue : fallback;

        public bool AsBool(bool fallback = false) =>
            Kind == MdixJsonKind.Bool ? _boolValue : fallback;

        // ── Parsing ───────────────────────────────────────────────────────────

        public static MdixJsonValue Parse(string json)
        {
            var pos = 0;
            var result = ParseValue(json, ref pos);
            SkipWhitespace(json, ref pos);
            if (pos != json.Length)
                throw new FormatException(
                    $"Unexpected trailing content at position {pos} while parsing JSON.");
            return result;
        }

        private static MdixJsonValue ParseValue(string s, ref int pos)
        {
            SkipWhitespace(s, ref pos);
            if (pos >= s.Length)
                throw new FormatException("Unexpected end of JSON input.");

            switch (s[pos])
            {
                case '{': return ParseObject(s, ref pos);
                case '[': return ParseArray(s, ref pos);
                case '"': return String(ParseStringLiteral(s, ref pos));
                case 't':
                    Expect(s, ref pos, "true");
                    return Bool(true);
                case 'f':
                    Expect(s, ref pos, "false");
                    return Bool(false);
                case 'n':
                    Expect(s, ref pos, "null");
                    return Null;
                default:
                    return ParseNumber(s, ref pos);
            }
        }

        private static MdixJsonValue ParseObject(string s, ref int pos)
        {
            var obj = Object();
            pos++; // consume '{'
            SkipWhitespace(s, ref pos);

            if (pos < s.Length && s[pos] == '}')
            {
                pos++;
                return obj;
            }

            while (true)
            {
                SkipWhitespace(s, ref pos);
                if (pos >= s.Length || s[pos] != '"')
                    throw new FormatException($"Expected object key at position {pos}.");

                var key = ParseStringLiteral(s, ref pos);
                SkipWhitespace(s, ref pos);

                if (pos >= s.Length || s[pos] != ':')
                    throw new FormatException($"Expected ':' at position {pos}.");
                pos++; // consume ':'

                var value = ParseValue(s, ref pos);
                obj[key] = value;

                SkipWhitespace(s, ref pos);
                if (pos >= s.Length)
                    throw new FormatException("Unterminated object.");

                if (s[pos] == ',')
                {
                    pos++;
                    continue;
                }
                if (s[pos] == '}')
                {
                    pos++;
                    break;
                }
                throw new FormatException($"Expected ',' or '}}' at position {pos}.");
            }

            return obj;
        }

        private static MdixJsonValue ParseArray(string s, ref int pos)
        {
            var arr = Array();
            pos++; // consume '['
            SkipWhitespace(s, ref pos);

            if (pos < s.Length && s[pos] == ']')
            {
                pos++;
                return arr;
            }

            while (true)
            {
                var value = ParseValue(s, ref pos);
                arr.Add(value);

                SkipWhitespace(s, ref pos);
                if (pos >= s.Length)
                    throw new FormatException("Unterminated array.");

                if (s[pos] == ',')
                {
                    pos++;
                    continue;
                }
                if (s[pos] == ']')
                {
                    pos++;
                    break;
                }
                throw new FormatException($"Expected ',' or ']' at position {pos}.");
            }

            return arr;
        }

        private static string ParseStringLiteral(string s, ref int pos)
        {
            pos++; // consume opening '"'
            var sb = new StringBuilder();

            while (true)
            {
                if (pos >= s.Length)
                    throw new FormatException("Unterminated string literal.");

                var c = s[pos];

                if (c == '"')
                {
                    pos++;
                    return sb.ToString();
                }

                if (c == '\\')
                {
                    pos++;
                    if (pos >= s.Length)
                        throw new FormatException("Unterminated escape sequence.");

                    var esc = s[pos];
                    switch (esc)
                    {
                        case '"':  sb.Append('"');  break;
                        case '\\': sb.Append('\\'); break;
                        case '/':  sb.Append('/');  break;
                        case 'b':  sb.Append('\b'); break;
                        case 'f':  sb.Append('\f'); break;
                        case 'n':  sb.Append('\n'); break;
                        case 'r':  sb.Append('\r'); break;
                        case 't':  sb.Append('\t'); break;
                        case 'u':
                            if (pos + 4 >= s.Length)
                                throw new FormatException("Truncated \\u escape sequence.");
                            var hex = s.Substring(pos + 1, 4);
                            var code = ushort.Parse(
                                hex, NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture);
                            sb.Append((char)code);
                            pos += 4;
                            break;
                        default:
                            throw new FormatException($"Unknown escape sequence '\\{esc}'.");
                    }
                    pos++;
                    continue;
                }

                sb.Append(c);
                pos++;
            }
        }

        private static MdixJsonValue ParseNumber(string s, ref int pos)
        {
            var start = pos;
            if (pos < s.Length && (s[pos] == '-' || s[pos] == '+')) pos++;
            while (pos < s.Length && char.IsDigit(s[pos])) pos++;
            if (pos < s.Length && s[pos] == '.')
            {
                pos++;
                while (pos < s.Length && char.IsDigit(s[pos])) pos++;
            }
            if (pos < s.Length && (s[pos] == 'e' || s[pos] == 'E'))
            {
                pos++;
                if (pos < s.Length && (s[pos] == '-' || s[pos] == '+')) pos++;
                while (pos < s.Length && char.IsDigit(s[pos])) pos++;
            }

            if (pos == start)
                throw new FormatException($"Invalid number at position {pos}.");

            var text = s.Substring(start, pos - start);
            return Number(double.Parse(text, CultureInfo.InvariantCulture));
        }

        private static void Expect(string s, ref int pos, string literal)
        {
            if (pos + literal.Length > s.Length || s.Substring(pos, literal.Length) != literal)
                throw new FormatException($"Expected '{literal}' at position {pos}.");
            pos += literal.Length;
        }

        private static void SkipWhitespace(string s, ref int pos)
        {
            while (pos < s.Length &&
                   (s[pos] == ' ' || s[pos] == '\t' || s[pos] == '\n' || s[pos] == '\r'))
                pos++;
        }

        // ── Serialization ─────────────────────────────────────────────────────

        public override string ToString()
        {
            var sb = new StringBuilder();
            WriteTo(sb);
            return sb.ToString();
        }

        private void WriteTo(StringBuilder sb)
        {
            switch (Kind)
            {
                case MdixJsonKind.Null:
                    sb.Append("null");
                    break;

                case MdixJsonKind.Bool:
                    sb.Append(_boolValue ? "true" : "false");
                    break;

                case MdixJsonKind.Number:
                    // Whole numbers serialize without a trailing ".0" — LSP
                    // integer fields (id, line, character, ...) round-trip
                    // through servers that type-check strictly on this.
                    if (Math.Abs(_numberValue % 1) < double.Epsilon &&
                        !double.IsInfinity(_numberValue))
                        sb.Append(((long)_numberValue).ToString(CultureInfo.InvariantCulture));
                    else
                        sb.Append(_numberValue.ToString("R", CultureInfo.InvariantCulture));
                    break;

                case MdixJsonKind.String:
                    WriteJsonString(sb, _stringValue ?? string.Empty);
                    break;

                case MdixJsonKind.Array:
                    sb.Append('[');
                    for (int i = 0; i < _arrayValue!.Count; i++)
                    {
                        if (i > 0) sb.Append(',');
                        _arrayValue[i].WriteTo(sb);
                    }
                    sb.Append(']');
                    break;

                case MdixJsonKind.Object:
                    sb.Append('{');
                    for (int i = 0; i < _objectKeyOrder!.Count; i++)
                    {
                        if (i > 0) sb.Append(',');
                        var key = _objectKeyOrder[i];
                        WriteJsonString(sb, key);
                        sb.Append(':');
                        _objectValue![key].WriteTo(sb);
                    }
                    sb.Append('}');
                    break;
            }
        }

        private static void WriteJsonString(StringBuilder sb, string value)
        {
            sb.Append('"');
            foreach (var c in value)
            {
                switch (c)
                {
                    case '"':  sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\b': sb.Append("\\b");  break;
                    case '\f': sb.Append("\\f");  break;
                    case '\n': sb.Append("\\n");  break;
                    case '\r': sb.Append("\\r");  break;
                    case '\t': sb.Append("\\t");  break;
                    default:
                        if (c < 0x20)
                            sb.Append("\\u").Append(((int)c).ToString("x4", CultureInfo.InvariantCulture));
                        else
                            sb.Append(c);
                        break;
                }
            }
            sb.Append('"');
        }
    }
}
