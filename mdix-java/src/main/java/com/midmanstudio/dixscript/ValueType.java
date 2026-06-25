// ValueType.java
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
    LONG(3),
    FLOAT(4),
    DOUBLE(5),
    STRING(6),
    DATE(7),
    TIMESTAMP(8),
    HEX_COLOR(9),
    BLOB(10),
    REGEX(11),
    ARRAY(12),
    OBJECT(13),
    TUPLE(14),
    ENUM(15);

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
