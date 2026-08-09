// MdixJson.java
package com.midmanstudio.dixscript.internal;

import com.midmanstudio.dixscript.MdixException;
import com.midmanstudio.dixscript.MdixValue;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A minimal recursive-descent JSON parser producing {@link MdixValue} trees.
 * <p>
 * Deliberately hand-rolled rather than pulling in Gson/Jackson/org.json:
 * {@code dixscript-java} otherwise has zero runtime dependencies beyond the
 * Kotlin stdlib, and a library meant to be embedded in games/engines
 * (Unity, mid-engine) is exactly the kind of consumer where an extra
 * transitive JSON dependency risks a version clash with whatever the host
 * project already pulls in. This only needs to parse the JSON DixScript's
 * own {@code serde_json} emits (see {@code Database#getJson} /
 * {@code MdixNative#selectManyAsJson} / {@code MdixNative#mergeSources}),
 * not arbitrary third-party JSON.
 * <p>
 * Internal — not part of the public API. Use {@link com.midmanstudio.dixscript.Database#query}
 * / {@link com.midmanstudio.dixscript.Database#queryMany}, not this class directly.
 */
public final class MdixJson {

    private MdixJson() {}

    /** Parses {@code json} into an {@link MdixValue} tree. Throws {@link MdixException} on malformed input. */
    public static MdixValue parse(String json) {
        Parser p = new Parser(json);
        p.skipWhitespace();
        MdixValue v = p.parseValue();
        p.skipWhitespace();
        if (!p.atEnd()) {
            throw new MdixException("MdixJson: trailing content at offset " + p.pos);
        }
        return v;
    }

    private static final class Parser {
        private final String s;
        private int pos = 0;

        Parser(String s) { this.s = s; }

        boolean atEnd() { return pos >= s.length(); }

        char peek() {
            if (atEnd()) throw new MdixException("MdixJson: unexpected end of input");
            return s.charAt(pos);
        }

        void expect(char c) {
            if (atEnd() || s.charAt(pos) != c) {
                throw new MdixException("MdixJson: expected '" + c + "' at offset " + pos);
            }
            pos++;
        }

        void skipWhitespace() {
            while (!atEnd()) {
                char c = s.charAt(pos);
                if (c == ' ' || c == '\t' || c == '\n' || c == '\r') pos++;
                else break;
            }
        }

        MdixValue parseValue() {
            skipWhitespace();
            char c = peek();
            switch (c) {
                case '{': return parseObject();
                case '[': return parseArray();
                case '"': return MdixValue.ofString(parseStringRaw());
                case 't':
                    expectLiteral("true");
                    return MdixValue.ofBool(true);
                case 'f':
                    expectLiteral("false");
                    return MdixValue.ofBool(false);
                case 'n':
                    expectLiteral("null");
                    return MdixValue.NULL;
                default:
                    return parseNumber();
            }
        }

        void expectLiteral(String literal) {
            if (pos + literal.length() > s.length() || !s.regionMatches(pos, literal, 0, literal.length())) {
                throw new MdixException("MdixJson: expected '" + literal + "' at offset " + pos);
            }
            pos += literal.length();
        }

        MdixValue parseNumber() {
            int start = pos;
            if (!atEnd() && s.charAt(pos) == '-') pos++;
            boolean isFloating = false;
            while (!atEnd() && Character.isDigit(s.charAt(pos))) pos++;
            if (!atEnd() && s.charAt(pos) == '.') {
                isFloating = true;
                pos++;
                while (!atEnd() && Character.isDigit(s.charAt(pos))) pos++;
            }
            if (!atEnd() && (s.charAt(pos) == 'e' || s.charAt(pos) == 'E')) {
                isFloating = true;
                pos++;
                if (!atEnd() && (s.charAt(pos) == '+' || s.charAt(pos) == '-')) pos++;
                while (!atEnd() && Character.isDigit(s.charAt(pos))) pos++;
            }
            if (pos == start) throw new MdixException("MdixJson: expected a number at offset " + pos);
            String num = s.substring(start, pos);
            try {
                return isFloating ? MdixValue.ofDouble(Double.parseDouble(num)) : MdixValue.ofLong(Long.parseLong(num));
            } catch (NumberFormatException e) {
                // Overflowed a long (e.g. a very large literal) — fall back to double rather than failing.
                return MdixValue.ofDouble(Double.parseDouble(num));
            }
        }

        String parseStringRaw() {
            expect('"');
            StringBuilder sb = new StringBuilder();
            while (true) {
                if (atEnd()) throw new MdixException("MdixJson: unterminated string");
                char c = s.charAt(pos++);
                if (c == '"') break;
                if (c == '\\') {
                    if (atEnd()) throw new MdixException("MdixJson: unterminated escape");
                    char esc = s.charAt(pos++);
                    switch (esc) {
                        case '"': sb.append('"'); break;
                        case '\\': sb.append('\\'); break;
                        case '/': sb.append('/'); break;
                        case 'b': sb.append('\b'); break;
                        case 'f': sb.append('\f'); break;
                        case 'n': sb.append('\n'); break;
                        case 'r': sb.append('\r'); break;
                        case 't': sb.append('\t'); break;
                        case 'u':
                            if (pos + 4 > s.length()) throw new MdixException("MdixJson: truncated \\u escape");
                            sb.append((char) Integer.parseInt(s.substring(pos, pos + 4), 16));
                            pos += 4;
                            break;
                        default:
                            throw new MdixException("MdixJson: invalid escape '\\" + esc + "'");
                    }
                } else {
                    sb.append(c);
                }
            }
            return sb.toString();
        }

        MdixValue parseArray() {
            expect('[');
            List<MdixValue> items = new ArrayList<>();
            skipWhitespace();
            if (!atEnd() && peek() == ']') {
                pos++;
                return MdixValue.ofArray(items);
            }
            while (true) {
                items.add(parseValue());
                skipWhitespace();
                char c = peek();
                if (c == ',') { pos++; continue; }
                if (c == ']') { pos++; break; }
                throw new MdixException("MdixJson: expected ',' or ']' at offset " + pos);
            }
            return MdixValue.ofArray(items);
        }

        MdixValue parseObject() {
            expect('{');
            Map<String, MdixValue> fields = new LinkedHashMap<>();
            skipWhitespace();
            if (!atEnd() && peek() == '}') {
                pos++;
                return MdixValue.ofObject(fields);
            }
            while (true) {
                skipWhitespace();
                String key = parseStringRaw();
                skipWhitespace();
                expect(':');
                MdixValue value = parseValue();
                fields.put(key, value);
                skipWhitespace();
                char c = peek();
                if (c == ',') { pos++; continue; }
                if (c == '}') { pos++; break; }
                throw new MdixException("MdixJson: expected ',' or '}' at offset " + pos);
            }
            return objectOrEnum(fields);
        }

        /**
         * {@code DixValue::Enum { enum_name, field_name, value }} is
         * untagged, so on the wire it's indistinguishable from a plain
         * object with exactly those three keys — detected structurally
         * here, same tradeoff documented on {@link MdixValue}'s class doc.
         */
        private MdixValue objectOrEnum(Map<String, MdixValue> fields) {
            if (fields.size() == 3
                && fields.get("enum_name") != null && fields.get("enum_name").asString() != null
                && fields.get("field_name") != null && fields.get("field_name").asString() != null
                && fields.get("value") != null && fields.get("value").asLong() != null) {
                return MdixValue.ofEnum(
                    fields.get("enum_name").asString(),
                    fields.get("field_name").asString(),
                    fields.get("value").asLong().intValue());
            }
            return MdixValue.ofObject(fields);
        }
    }
}
