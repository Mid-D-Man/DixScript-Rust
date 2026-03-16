package com.midmanstudio.dixscript;

/**
 * DixScript value type discriminants, returned by {@link Database#valueTypeAt(String)}.
 * Ordinal values match the integers returned by the native getType() function.
 */
public enum ValueType {
    UNKNOWN(-1),
    NULL(0),
    BOOL(1),
    INT(2),
    FLOAT(3),
    DOUBLE(4),
    STRING(5),
    DATE(6),
    TIMESTAMP(7),
    HEX_COLOR(8),
    BLOB(9),
    REGEX(10),
    ARRAY(11),
    OBJECT(12),
    TUPLE(13),
    ENUM(14);

    private final int code;

    ValueType(int code) { this.code = code; }

    public int getCode() { return code; }

    public static ValueType fromCode(int code) {
        for (ValueType t : values()) {
            if (t.code == code) return t;
        }
        return UNKNOWN;
    }
                }
