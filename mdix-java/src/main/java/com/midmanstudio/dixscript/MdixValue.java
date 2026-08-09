// MdixValue.java
package com.midmanstudio.dixscript;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * A dynamic, immutable value tree mirroring Rust's {@code dixscript::Runtime::DixValue}.
 * <p>
 * {@link MdixQuery} and {@link Database#query} hand these back instead of typed getters
 * because a query result's shape isn't known ahead of time. Every {@code as*()} accessor
 * returns {@code null} rather than throwing when the value isn't that variant — chains
 * stay unwind-free through any number of missing links, e.g.
 * {@code value.field("owner").field("name").asString()} is {@code null}, not a crash,
 * the same "index into a shared Null" behavior {@code DixValue}'s own {@code Index} impl
 * documents on the Rust side.
 * <p>
 * <b>Construction note:</b> values reach Java as JSON (see {@code Database#getJson} /
 * {@code MdixNative#selectManyAsJson}), and {@code DixValue}'s {@code #[serde(untagged)]}
 * representation means the JSON alone can't distinguish every Rust variant — {@code Int}
 * vs {@code Long} vs {@code Float} vs {@code Double} all become a bare JSON number (this
 * class infers {@link Kind#LONG} for whole numbers and {@link Kind#DOUBLE} for anything
 * with a fractional part or exponent), and {@code Date}/{@code Timestamp}/{@code HexColor}/
 * {@code Blob}/{@code Regex}/{@code String} all become a bare JSON string (this class
 * always reports those as {@link Kind#STRING} — use {@link Database#getEnumField} etc. or
 * {@link Database#valueTypeAt} on the original path when the exact DixScript type matters).
 * An {@code Enum} value's {@code {enum_name, field_name, value}} shape is detected
 * structurally and reported as {@link Kind#ENUM}. This is the same lossy-but-practical
 * tradeoff {@code mdix-ffi}'s {@code mdix_select_many_as_json} already makes for C/C++
 * consumers — exact for querying, not a substitute for {@code Database}'s typed getters.
 */
public final class MdixValue {

    /** Which shape this value holds. Mirrors {@code DixValue}'s variants, JSON-collapsed (see class doc). */
    public enum Kind {
        NULL, BOOL, LONG, DOUBLE, STRING, ARRAY, OBJECT, ENUM
    }

    public static final MdixValue NULL = new MdixValue(Kind.NULL, null);

    private final Kind kind;
    private final Object value; // Boolean | Long | Double | String | List<MdixValue> | Map<String,MdixValue> | EnumRef

    private MdixValue(Kind kind, Object value) {
        this.kind = kind;
        this.value = value;
    }

    // ── Factories ────────────────────────────────────────────────────────────

    public static MdixValue ofBool(boolean b) { return new MdixValue(Kind.BOOL, b); }
    public static MdixValue ofLong(long l) { return new MdixValue(Kind.LONG, l); }
    public static MdixValue ofDouble(double d) { return new MdixValue(Kind.DOUBLE, d); }
    public static MdixValue ofString(String s) { return new MdixValue(Kind.STRING, Objects.requireNonNull(s)); }

    public static MdixValue ofArray(List<MdixValue> items) {
        return new MdixValue(Kind.ARRAY, Collections.unmodifiableList(items));
    }

    public static MdixValue ofObject(Map<String, MdixValue> fields) {
        return new MdixValue(Kind.OBJECT, Collections.unmodifiableMap(new LinkedHashMap<>(fields)));
    }

    public static MdixValue ofEnum(String enumName, String fieldName, int enumValue) {
        return new MdixValue(Kind.ENUM, new EnumRef(enumName, fieldName, enumValue));
    }

    // ── Kind / null check ────────────────────────────────────────────────────

    public Kind kind() { return kind; }

    public boolean isNull() { return kind == Kind.NULL; }

    // ── Typed accessors — all null-safe, all return null on a variant mismatch ──

    public Boolean asBool() { return kind == Kind.BOOL ? (Boolean) value : null; }

    /** Any numeric variant, truncated toward zero if it was a {@link Kind#DOUBLE}. */
    public Long asLong() {
        if (kind == Kind.LONG) return (Long) value;
        if (kind == Kind.DOUBLE) return ((Double) value).longValue();
        if (kind == Kind.ENUM) return (long) ((EnumRef) value).value;
        return null;
    }

    /** Any numeric variant, widened to {@code double}. */
    public Double asDouble() {
        if (kind == Kind.DOUBLE) return (Double) value;
        if (kind == Kind.LONG) return ((Long) value).doubleValue();
        if (kind == Kind.ENUM) return (double) ((EnumRef) value).value;
        return null;
    }

    public String asString() { return kind == Kind.STRING ? (String) value : null; }

    @SuppressWarnings("unchecked")
    public List<MdixValue> asArray() { return kind == Kind.ARRAY ? (List<MdixValue>) value : null; }

    @SuppressWarnings("unchecked")
    public Map<String, MdixValue> asObject() { return kind == Kind.OBJECT ? (Map<String, MdixValue>) value : null; }

    /** The {@code (enumName, fieldName, value)} triple, or {@code null} if this isn't {@link Kind#ENUM}. */
    public EnumRef asEnum() { return kind == Kind.ENUM ? (EnumRef) value : null; }

    /** {@code (enumName, fieldName, value)} — see {@link #asEnum()}. */
    public static final class EnumRef {
        public final String enumName;
        public final String fieldName;
        public final int value;

        EnumRef(String enumName, String fieldName, int value) {
            this.enumName = enumName;
            this.fieldName = fieldName;
            this.value = value;
        }

        @Override public String toString() { return enumName + "." + fieldName + " = " + value; }
    }

    // ── Chainable / dynamic-style access ────────────────────────────────────

    /**
     * Borrow a named field out of an {@link Kind#OBJECT} value. Returns
     * {@link #NULL} (never Java {@code null}) for any other variant or a
     * missing key — safe to chain further {@code .field()} / {@code .as*()}
     * calls without a null check at every step.
     */
    public MdixValue field(String name) {
        Map<String, MdixValue> obj = asObject();
        if (obj == null) return NULL;
        MdixValue v = obj.get(name);
        return v != null ? v : NULL;
    }

    /** Dotted-path field access through nested objects. {@code v.fieldPath("owner.name")} is {@code v.field("owner").field("name")}. */
    public MdixValue fieldPath(String path) {
        MdixValue cur = this;
        for (String segment : path.split("\\.")) {
            cur = cur.field(segment);
        }
        return cur;
    }

    /** Index into an {@link Kind#ARRAY} value. {@link #NULL} for any other variant or an out-of-range index. */
    public MdixValue at(int index) {
        List<MdixValue> arr = asArray();
        if (arr == null || index < 0 || index >= arr.size()) return NULL;
        return arr.get(index);
    }

    @Override
    public String toString() {
        switch (kind) {
            case NULL: return "null";
            case BOOL: return value.toString();
            case LONG: return value.toString();
            case DOUBLE: return value.toString();
            case STRING: return "\"" + value + "\"";
            case ENUM: return value.toString();
            case ARRAY: return asArray().toString();
            case OBJECT: return asObject().toString();
            default: return String.valueOf(value);
        }
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof MdixValue)) return false;
        MdixValue other = (MdixValue) o;
        return kind == other.kind && Objects.equals(value, other.value);
    }

    @Override
    public int hashCode() { return Objects.hash(kind, value); }
}
