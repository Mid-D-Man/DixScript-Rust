// MdixNative.java
package com.midmanstudio.dixscript.internal;

/**
 * Raw JNI method declarations.
 * Every method here maps 1:1 to a Rust #[no_mangle] extern "system" function
 * in mdix-java/src/lib.rs.
 *
 * Do NOT call these methods directly — use the public API classes:
 * {@link com.midmanstudio.dixscript.Database},
 * {@link com.midmanstudio.dixscript.Builder},
 * {@link com.midmanstudio.dixscript.DixScript}.
 */
public final class MdixNative {

    static {
        NativeLoader.load();
    }

    private MdixNative() {}

    // ── Metadata ──────────────────────────────────────────────────────────────
    public static native String version();

    // ── Load / Free ───────────────────────────────────────────────────────────
    /** Returns a handle (long) on success, throws MdixException on failure. */
    public static native long load(String path);
    public static native long loadStr(String source);
    public static native long loadEncrypted(String encPath, String keyPath);
    public static native long loadEncryptedPassword(String encPath, String password);
    public static native void free(long handle);

    // ── Validity / metadata ───────────────────────────────────────────────────
    public static native boolean isValid(long handle);
    public static native int     entryCount(long handle);

    // ── Type inspection ───────────────────────────────────────────────────────
    /**
     * Returns a ValueType ordinal:
     * -1=Unknown 0=Null 1=Bool 2=Int 3=Long 4=Float 5=Double 6=String
     *  7=Date 8=Timestamp 9=HexColor 10=Blob 11=Regex 12=Array
     * 13=Object 14=Tuple 15=Enum
     */
    public static native int     getType(long handle, String path);
    public static native int     getArrayLength(long handle, String path);

    // ── Typed getters ─────────────────────────────────────────────────────────
    public static native String  getString(long handle, String path);
    public static native int     getInt(long handle, String path);
    /** Also accepts Int values (widened without loss). */
    public static native long    getLong(long handle, String path);
    public static native float   getFloat(long handle, String path);
    public static native double  getDouble(long handle, String path);
    public static native boolean getBool(long handle, String path);
    public static native String  getEnumName(long handle, String path);
    public static native String  getEnumField(long handle, String path);
    public static native String  getJson(long handle, String path);

    // ── Key existence / enumeration ───────────────────────────────────────────
    public static native boolean   exists(long handle, String path);
    public static native String[]  getKeys(long handle, String prefix);

    // ── Conversion — export ───────────────────────────────────────────────────
    public static native String  toJson(long handle, boolean indented);
    public static native String  toMdix(long handle, int mode);
    public static native String  toToml(long handle);

    // ── Conversion — import ───────────────────────────────────────────────────
    public static native long    fromJson(String json);
    public static native long    fromToml(String toml);

    // ── Source text formatting ─────────────────────────────────────────────────
    /** mode: 0=Default 1=Pretty 2=Compact 3=Minified */
    public static native String  formatSource(String source, int mode);

    // ── Builder ───────────────────────────────────────────────────────────────
    public static native long    builderNew();
    public static native void    builderFree(long handle);
    public static native boolean builderSetString(long handle, String path, String value);
    public static native boolean builderSetInt(long handle, String path, int value);
    public static native boolean builderSetLong(long handle, String path, long value);
    public static native boolean builderSetFloat(long handle, String path, float value);
    public static native boolean builderSetDouble(long handle, String path, double value);
    public static native boolean builderSetBool(long handle, String path, boolean value);
    public static native boolean builderRemove(long handle, String path);
    public static native void    builderClear(long handle);
    public static native boolean builderHasKey(long handle, String path);
    public static native boolean builderSave(long handle, String path);
    public static native String  builderToString(long handle);
                                                 }
