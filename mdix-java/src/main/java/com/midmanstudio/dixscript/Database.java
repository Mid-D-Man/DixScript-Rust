// Database.java
package com.midmanstudio.dixscript;

import com.midmanstudio.dixscript.internal.MdixNative;
import java.io.Closeable;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

/**
 * A loaded, read-only DixScript database.
 *
 * Always close when done — it releases native memory:
 * <pre>
 *   try (Database db = DixScript.loadStr("@DATA( port = 8080 )")) {
 *       int port = db.getInt("port");
 *   }
 * </pre>
 *
 * Implements {@link Closeable} so it works in try-with-resources.
 * Thread-safe for concurrent reads.
 */
public final class Database implements Closeable {

    private volatile long handle;
    private volatile boolean closed = false;

    Database(long handle) {
        this.handle = handle;
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    @Override
    public synchronized void close() {
        if (!closed && handle != 0) {
            MdixNative.free(handle);
            handle = 0;
            closed = true;
        }
    }

    public boolean isValid() {
        return !closed && handle != 0 && MdixNative.isValid(handle);
    }

    /** Total number of data entries. */
    public int entryCount() {
        checkOpen();
        return MdixNative.entryCount(handle);
    }

    // ── Existence and type ────────────────────────────────────────────────────

    public boolean exists(String path) {
        if (closed || path == null) return false;
        return MdixNative.exists(handle, path);
    }

    public ValueType valueTypeAt(String path) {
        checkOpen();
        return ValueType.fromCode(MdixNative.getType(handle, path));
    }

    // ── Typed getters ─────────────────────────────────────────────────────────

    /**
     * Returns the string at {@code path}.
     * @throws MdixException if the path is not found or is not a string type.
     */
    public String getString(String path) {
        checkOpen();
        checkPath(path);
        String result = MdixNative.getString(handle, path);
        if (result == null) throw new MdixException(MdixException.Kind.NOT_FOUND,
            "getString: path not found: " + path);
        return result;
    }

    /** Returns the string at {@code path}, or {@code defaultValue} if absent. */
    public String getString(String path, String defaultValue) {
        if (!exists(path)) return defaultValue;
        return getString(path);
    }

    public int getInt(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getInt(handle, path);
    }

    public int getInt(String path, int defaultValue) {
        if (!exists(path)) return defaultValue;
        return getInt(path);
    }

    /** Also accepts Int values (widened without loss). */
    public long getLong(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getLong(handle, path);
    }

    public long getLong(String path, long defaultValue) {
        if (!exists(path)) return defaultValue;
        return getLong(path);
    }

    public float getFloat(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getFloat(handle, path);
    }

    public float getFloat(String path, float defaultValue) {
        if (!exists(path)) return defaultValue;
        return getFloat(path);
    }

    public double getDouble(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getDouble(handle, path);
    }

    public double getDouble(String path, double defaultValue) {
        if (!exists(path)) return defaultValue;
        return getDouble(path);
    }

    public boolean getBool(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getBool(handle, path);
    }

    public boolean getBool(String path, boolean defaultValue) {
        if (!exists(path)) return defaultValue;
        return getBool(path);
    }

    // ── Enum ──────────────────────────────────────────────────────────────────

    public String getEnumName(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getEnumName(handle, path);
    }

    public String getEnumField(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getEnumField(handle, path);
    }

    public int getEnumValue(String path) {
        return getInt(path);
    }

    // ── Array ─────────────────────────────────────────────────────────────────

    public int arrayLength(String path) {
        checkOpen();
        checkPath(path);
        int n = MdixNative.getArrayLength(handle, path);
        if (n < 0) throw new MdixException(MdixException.Kind.TYPE_MISMATCH,
            "arrayLength: not an array at: " + path);
        return n;
    }

    // ── JSON escape hatch ─────────────────────────────────────────────────────

    /**
     * Serializes the value at {@code path} to a JSON string.
     * Useful for complex types like Blob, Regex, Tuple, or nested objects.
     */
    public String getJson(String path) {
        checkOpen();
        checkPath(path);
        String result = MdixNative.getJson(handle, path);
        if (result == null) throw new MdixException(MdixException.Kind.NOT_FOUND,
            "getJson: path not found: " + path);
        return result;
    }

    // ── Key enumeration ───────────────────────────────────────────────────────

    /**
     * Returns direct child key names under {@code prefix}.
     * Pass {@code ""} or {@code null} for top-level keys.
     */
    public List<String> keys(String prefix) {
        checkOpen();
        String[] arr = MdixNative.getKeys(handle, prefix != null ? prefix : "");
        if (arr == null || arr.length == 0) return Collections.emptyList();
        return Collections.unmodifiableList(Arrays.asList(arr));
    }

    public List<String> keys() {
        return keys("");
    }

    // ── Package-private: raw handle for Converter ─────────────────────────────

    long rawHandle() {
        checkOpen();
        return handle;
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    private void checkOpen() {
        if (closed || handle == 0)
            throw new MdixException(MdixException.Kind.CLOSED, "Database has been closed");
    }

    private void checkPath(String path) {
        if (path == null || path.isEmpty())
            throw new MdixException(MdixException.Kind.INVALID_PATH, "path must not be null or empty");
    }
            }
