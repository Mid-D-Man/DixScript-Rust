// csharp/src/MidManStudio.Mdix.Core/MdixJson.cs
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace MidManStudio.Mdix.Core
{
    // NOTICE: see docs/mdix-csharp.md for module docs.
    //
    // Replaces System.Text.Json. mdix-csharp only ever reads the small JSON
    // payloads mdix_ffi hands back over FFI (GetJson scalars, tuple arrays,
    // merge-conflict reports) -- never writes JSON, never binds it onto an
    // arbitrary POCO graph. Unity has no NuGet resolution of its own, so a
    // System.Text.Json dependency meant shipping (and keeping version-synced)
    // extra managed plugin DLLs inside the Unity package for what amounts to
    // "parse a few small strings" -- not worth it when every other package
    // in this org ships zero external managed dependencies. This file is the
    // entire replacement: a parser, a read-only element type, and a re-
    // serializer used only by GetRawText() on Object/Array values.

    /// <summary>Value kind of a parsed JSON token.</summary>
    internal enum MdixJsonValueKind
    {
        Undefined,
        Object,
        Array,
        String,
        Number,
        True,
        False,
        Null,
    }

    /// <summary>Thrown for malformed JSON input or a value-kind/accessor mismatch.</summary>
    internal sealed class MdixJsonException : Exception
    {
        public MdixJsonException(string message) : base(message) { }
    }

    /// <summary>A single parsed JSON value. Immutable -- see <see cref="Clone"/>.</summary>
    internal sealed class MdixJsonElement
    {
        public MdixJsonValueKind ValueKind { get; }

        // String: decoded value. Number: raw literal text (parsed lazily by
        // the Get*() accessor actually called, same laziness System.Text.Json
        // uses, and it sidesteps picking a single numeric type up front).
        private readonly string? _text;
        private readonly List<MdixJsonElement>? _items;
        private readonly Dictionary<string, MdixJsonElement>? _props;

        private MdixJsonElement(
            MdixJsonValueKind kind,
            string? text = null,
            List<MdixJsonElement>? items = null,
            Dictionary<string, MdixJsonElement>? props = null)
        {
            ValueKind = kind;
            _text = text;
            _items = items;
            _props = props;
        }

        internal static readonly MdixJsonElement NullValue  = new(MdixJsonValueKind.Null);
        internal static readonly MdixJsonElement TrueValue  = new(MdixJsonValueKind.True);
        internal static readonly MdixJsonElement FalseValue = new(MdixJsonValueKind.False);

        internal static MdixJsonElement OfString(string s) => new(MdixJsonValueKind.String, text: s);
        internal static MdixJsonElement OfNumber(string literal) => new(MdixJsonValueKind.Number, text: literal);
        internal static MdixJsonElement OfArray(List<MdixJsonElement> items) => new(MdixJsonValueKind.Array, items: items);
        internal static MdixJsonElement OfObject(Dictionary<string, MdixJsonElement> props) => new(MdixJsonValueKind.Object, props: props);

        // ── Scalar accessors ────────────────────────────────────────────

        public string? GetString()
        {
            if (ValueKind == MdixJsonValueKind.String) return _text;
            if (ValueKind == MdixJsonValueKind.Null) return null;
            throw new MdixJsonException($"Cannot get the string value of a token of type '{ValueKind}'.");
        }

        public int GetInt32() => int.Parse(RequireNumberText(), NumberStyles.Integer, CultureInfo.InvariantCulture);
        public long GetInt64() => long.Parse(RequireNumberText(), NumberStyles.Integer, CultureInfo.InvariantCulture);
        public double GetDouble() => double.Parse(RequireNumberText(), NumberStyles.Float, CultureInfo.InvariantCulture);
        public decimal GetDecimal() => decimal.Parse(RequireNumberText(), NumberStyles.Float, CultureInfo.InvariantCulture);

        public bool GetBoolean() => ValueKind switch
        {
            MdixJsonValueKind.True  => true,
            MdixJsonValueKind.False => false,
            _ => throw new MdixJsonException($"Cannot get the boolean value of a token of type '{ValueKind}'."),
        };

        private string RequireNumberText() =>
            ValueKind == MdixJsonValueKind.Number
                ? _text!
                : throw new MdixJsonException($"Cannot get a numeric value of a token of type '{ValueKind}'.");

        /// <summary>Raw JSON text of this value (re-serialized for Object/Array).</summary>
        public string GetRawText()
        {
            switch (ValueKind)
            {
                case MdixJsonValueKind.String: return MdixJsonWriter.QuoteString(_text!);
                case MdixJsonValueKind.Number: return _text!;
                case MdixJsonValueKind.True:   return "true";
                case MdixJsonValueKind.False:  return "false";
                case MdixJsonValueKind.Null:   return "null";
                case MdixJsonValueKind.Array:
                case MdixJsonValueKind.Object:
                    var sb = new StringBuilder();
                    MdixJsonWriter.Write(this, sb);
                    return sb.ToString();
                default:
                    return string.Empty;
            }
        }

        /// <summary>Mirrors System.Text.Json.JsonElement.ToString(): decoded value for
        /// String, raw text for everything else.</summary>
        public override string ToString() =>
            ValueKind == MdixJsonValueKind.String ? (_text ?? string.Empty) : GetRawText();

        // ── Object accessors ────────────────────────────────────────────

        public MdixJsonElement GetProperty(string name)
        {
            if (TryGetProperty(name, out var value)) return value;
            throw new MdixJsonException($"The requested property '{name}' was not found.");
        }

        public bool TryGetProperty(string name, out MdixJsonElement value)
        {
            if (ValueKind == MdixJsonValueKind.Object && _props != null && _props.TryGetValue(name, out var found))
            {
                value = found;
                return true;
            }
            value = NullValue;
            return false;
        }

        public IEnumerable<KeyValuePair<string, MdixJsonElement>> EnumerateObject() =>
            ValueKind == MdixJsonValueKind.Object
                ? _props!
                : throw new MdixJsonException($"Cannot enumerate a token of type '{ValueKind}' as an object.");

        // ── Array accessors ─────────────────────────────────────────────

        public int GetArrayLength() =>
            ValueKind == MdixJsonValueKind.Array
                ? _items!.Count
                : throw new MdixJsonException($"Cannot get the array length of a token of type '{ValueKind}'.");

        public MdixJsonElement this[int index] =>
            ValueKind == MdixJsonValueKind.Array
                ? _items![index]
                : throw new MdixJsonException($"Cannot index into a token of type '{ValueKind}'.");

        public IEnumerable<MdixJsonElement> EnumerateArray() =>
            ValueKind == MdixJsonValueKind.Array
                ? _items!
                : throw new MdixJsonException($"Cannot enumerate a token of type '{ValueKind}' as an array.");

        /// <summary>
        /// No-op: unlike System.Text.Json.JsonElement, this type owns its data
        /// outright -- there is no shared document buffer it could outlive.
        /// Kept only so call sites written against the JsonDocument disposal
        /// pattern (Clone() to escape a `using var doc`) needed no restructuring.
        /// </summary>
        public MdixJsonElement Clone() => this;
    }

    /// <summary>Entry point: parses a complete JSON document into its root <see cref="MdixJsonElement"/>.</summary>
    internal static class MdixJson
    {
        public static MdixJsonElement Parse(string json)
        {
            if (json is null) throw new MdixJsonException("Input JSON was null.");
            int pos = 0;
            SkipWhitespace(json, ref pos);
            var value = ParseValue(json, ref pos);
            SkipWhitespace(json, ref pos);
            if (pos != json.Length)
                throw new MdixJsonException($"Unexpected trailing content at position {pos}.");
            return value;
        }

        private static MdixJsonElement ParseValue(string s, ref int pos)
        {
            if (pos >= s.Length) throw new MdixJsonException("Unexpected end of JSON input.");
            char c = s[pos];
            switch (c)
            {
                case '{': return ParseObject(s, ref pos);
                case '[': return ParseArray(s, ref pos);
                case '"': return MdixJsonElement.OfString(ParseStringLiteral(s, ref pos));
                case 't': Expect(s, ref pos, "true");  return MdixJsonElement.TrueValue;
                case 'f': Expect(s, ref pos, "false"); return MdixJsonElement.FalseValue;
                case 'n': Expect(s, ref pos, "null");  return MdixJsonElement.NullValue;
                default:
                    if (c == '-' || (c >= '0' && c <= '9')) return ParseNumber(s, ref pos);
                    throw new MdixJsonException($"Unexpected character '{c}' at position {pos}.");
            }
        }

        private static void Expect(string s, ref int pos, string literal)
        {
            if (pos + literal.Length > s.Length || string.CompareOrdinal(s, pos, literal, 0, literal.Length) != 0)
                throw new MdixJsonException($"Expected '{literal}' at position {pos}.");
            pos += literal.Length;
        }

        private static MdixJsonElement ParseObject(string s, ref int pos)
        {
            pos++; // consume '{'
            var props = new Dictionary<string, MdixJsonElement>();
            SkipWhitespace(s, ref pos);
            if (pos < s.Length && s[pos] == '}') { pos++; return MdixJsonElement.OfObject(props); }

            while (true)
            {
                SkipWhitespace(s, ref pos);
                if (pos >= s.Length || s[pos] != '"')
                    throw new MdixJsonException($"Expected property name string at position {pos}.");
                var key = ParseStringLiteral(s, ref pos);

                SkipWhitespace(s, ref pos);
                if (pos >= s.Length || s[pos] != ':')
                    throw new MdixJsonException($"Expected ':' at position {pos}.");
                pos++;

                SkipWhitespace(s, ref pos);
                props[key] = ParseValue(s, ref pos);

                SkipWhitespace(s, ref pos);
                if (pos >= s.Length) throw new MdixJsonException("Unexpected end of JSON input in object.");
                if (s[pos] == ',') { pos++; continue; }
                if (s[pos] == '}') { pos++; break; }
                throw new MdixJsonException($"Expected ',' or '}}' at position {pos}.");
            }
            return MdixJsonElement.OfObject(props);
        }

        private static MdixJsonElement ParseArray(string s, ref int pos)
        {
            pos++; // consume '['
            var items = new List<MdixJsonElement>();
            SkipWhitespace(s, ref pos);
            if (pos < s.Length && s[pos] == ']') { pos++; return MdixJsonElement.OfArray(items); }

            while (true)
            {
                SkipWhitespace(s, ref pos);
                items.Add(ParseValue(s, ref pos));
                SkipWhitespace(s, ref pos);
                if (pos >= s.Length) throw new MdixJsonException("Unexpected end of JSON input in array.");
                if (s[pos] == ',') { pos++; continue; }
                if (s[pos] == ']') { pos++; break; }
                throw new MdixJsonException($"Expected ',' or ']' at position {pos}.");
            }
            return MdixJsonElement.OfArray(items);
        }

        private static string ParseStringLiteral(string s, ref int pos)
        {
            pos++; // consume opening quote
            var sb = new StringBuilder();
            while (true)
            {
                if (pos >= s.Length) throw new MdixJsonException("Unterminated string literal.");
                char c = s[pos++];
                if (c == '"') break;
                if (c == '\\')
                {
                    if (pos >= s.Length) throw new MdixJsonException("Unterminated escape sequence.");
                    char esc = s[pos++];
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
                            if (pos + 4 > s.Length)
                                throw new MdixJsonException("Truncated \\u escape sequence.");
                            var hex = s.Substring(pos, 4);
                            if (!ushort.TryParse(hex, NumberStyles.AllowHexSpecifier, CultureInfo.InvariantCulture, out var code))
                                throw new MdixJsonException($"Invalid \\u escape sequence '\\u{hex}'.");
                            sb.Append((char)code);
                            pos += 4;
                            break;
                        default:
                            throw new MdixJsonException($"Invalid escape sequence '\\{esc}'.");
                    }
                }
                else
                {
                    sb.Append(c);
                }
            }
            return sb.ToString();
        }

        private static MdixJsonElement ParseNumber(string s, ref int pos)
        {
            int start = pos;
            if (pos < s.Length && s[pos] == '-') pos++;
            if (pos >= s.Length || s[pos] < '0' || s[pos] > '9')
                throw new MdixJsonException($"Invalid number at position {start}.");
            while (pos < s.Length && s[pos] >= '0' && s[pos] <= '9') pos++;
            if (pos < s.Length && s[pos] == '.')
            {
                pos++;
                if (pos >= s.Length || s[pos] < '0' || s[pos] > '9')
                    throw new MdixJsonException($"Invalid fractional part at position {pos}.");
                while (pos < s.Length && s[pos] >= '0' && s[pos] <= '9') pos++;
            }
            if (pos < s.Length && (s[pos] == 'e' || s[pos] == 'E'))
            {
                pos++;
                if (pos < s.Length && (s[pos] == '+' || s[pos] == '-')) pos++;
                if (pos >= s.Length || s[pos] < '0' || s[pos] > '9')
                    throw new MdixJsonException($"Invalid exponent at position {pos}.");
                while (pos < s.Length && s[pos] >= '0' && s[pos] <= '9') pos++;
            }
            return MdixJsonElement.OfNumber(s.Substring(start, pos - start));
        }

        private static void SkipWhitespace(string s, ref int pos)
        {
            while (pos < s.Length)
            {
                char c = s[pos];
                if (c == ' ' || c == '\t' || c == '\n' || c == '\r') pos++;
                else break;
            }
        }
    }

    /// <summary>Re-serializes an element back to JSON text. Only reached by
    /// GetRawText() on Object/Array values -- scalars short-circuit in place.</summary>
    internal static class MdixJsonWriter
    {
        public static void Write(MdixJsonElement el, StringBuilder sb)
        {
            switch (el.ValueKind)
            {
                case MdixJsonValueKind.Object:
                    sb.Append('{');
                    bool firstProp = true;
                    foreach (var kv in el.EnumerateObject())
                    {
                        if (!firstProp) sb.Append(',');
                        firstProp = false;
                        sb.Append(QuoteString(kv.Key)).Append(':');
                        Write(kv.Value, sb);
                    }
                    sb.Append('}');
                    break;
                case MdixJsonValueKind.Array:
                    sb.Append('[');
                    bool firstItem = true;
                    foreach (var item in el.EnumerateArray())
                    {
                        if (!firstItem) sb.Append(',');
                        firstItem = false;
                        Write(item, sb);
                    }
                    sb.Append(']');
                    break;
                default:
                    sb.Append(el.GetRawText());
                    break;
            }
        }

        public static string QuoteString(string s)
        {
            var sb = new StringBuilder(s.Length + 2);
            sb.Append('"');
            foreach (char c in s)
            {
                switch (c)
                {
                    case '"':  sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\n': sb.Append("\\n");  break;
                    case '\r': sb.Append("\\r");  break;
                    case '\t': sb.Append("\\t");  break;
                    default:
                        if (c < 0x20) sb.Append("\\u").Append(((int)c).ToString("x4"));
                        else sb.Append(c);
                        break;
                }
            }
            sb.Append('"');
            return sb.ToString();
        }
    }
}
