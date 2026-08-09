
package com.midmanstudio.dixscript;

import com.midmanstudio.dixscript.internal.MdixJson;
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

    /** Releases the underlying native handle. Safe to call more than once. */
    @Override
    public synchronized void close() {
        if (!closed && handle != 0) {
            MdixNative.free(handle);
            handle = 0;
            closed = true;
        }
    }

    /** {@code true} if this database is open and its native handle is non-null. */
    public boolean isValid() {
        return !closed && handle != 0 && MdixNative.isValid(handle);
    }

    /** Total number of data entries. */
    public int entryCount() {
        checkOpen();
        return MdixNative.entryCount(handle);
    }

    // ── Existence and type ────────────────────────────────────────────────────

    /** {@code true} if {@code path} resolves to a value. Never throws — returns {@code false} on a closed database or a null path. */
    public boolean exists(String path) {
        if (closed || path == null) return false;
        return MdixNative.exists(handle, path);
    }

    /** The {@link ValueType} of the value at {@code path}. */
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

    /**
     * Returns the 32-bit integer at {@code path}.
     * @throws MdixException if the path is not found or is not an int-compatible type.
     */
    public int getInt(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getInt(handle, path);
    }

    /** Returns the 32-bit integer at {@code path}, or {@code defaultValue} if absent. */
    public int getInt(String path, int defaultValue) {
        if (!exists(path)) return defaultValue;
        return getInt(path);
    }

    /**
     * Returns the 64-bit integer at {@code path}. Also accepts {@code Int} values
     * (widened without loss).
     * @throws MdixException if the path is not found or is not a long-compatible type.
     */
    public long getLong(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getLong(handle, path);
    }

    /** Returns the 64-bit integer at {@code path}, or {@code defaultValue} if absent. */
    public long getLong(String path, long defaultValue) {
        if (!exists(path)) return defaultValue;
        return getLong(path);
    }

    /**
     * Returns the 32-bit float at {@code path}.
     * @throws MdixException if the path is not found or is not a float-compatible type.
     */
    public float getFloat(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getFloat(handle, path);
    }

    /** Returns the 32-bit float at {@code path}, or {@code defaultValue} if absent. */
    public float getFloat(String path, float defaultValue) {
        if (!exists(path)) return defaultValue;
        return getFloat(path);
    }

    /**
     * Returns the 64-bit double at {@code path}.
     * @throws MdixException if the path is not found or is not a double-compatible type.
     */
    public double getDouble(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getDouble(handle, path);
    }

    /** Returns the 64-bit double at {@code path}, or {@code defaultValue} if absent. */
    public double getDouble(String path, double defaultValue) {
        if (!exists(path)) return defaultValue;
        return getDouble(path);
    }

    /**
     * Returns the boolean at {@code path}.
     * @throws MdixException if the path is not found or is not a bool type.
     */
    public boolean getBool(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getBool(handle, path);
    }

    /** Returns the boolean at {@code path}, or {@code defaultValue} if absent. */
    public boolean getBool(String path, boolean defaultValue) {
        if (!exists(path)) return defaultValue;
        return getBool(path);
    }

    // ── Enum ──────────────────────────────────────────────────────────────────

    /**
     * Returns the declared enum type's name for the enum value at {@code path}
     * (e.g. {@code "Status"} for a field declared as {@code status: Status.ACTIVE}).
     * @throws MdixException if the path is not found or is not an enum value.
     */
    public String getEnumName(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getEnumName(handle, path);
    }

    /**
     * Returns the enum field's name for the enum value at {@code path} (e.g. {@code "ACTIVE"}).
     * @throws MdixException if the path is not found or is not an enum value.
     */
    public String getEnumField(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.getEnumField(handle, path);
    }

    /** Returns the enum value's underlying {@code int}, exactly like {@link #getInt}. */
    public int getEnumValue(String path) {
        return getInt(path);
    }

    // ── Array ─────────────────────────────────────────────────────────────────

    /**
     * Returns the number of elements in the array at {@code path}.
     * @throws MdixException if the path is not found or is not an array type.
     */
    public int arrayLength(String path) {
        checkOpen();
        checkPath(path);
        int n = MdixNative.getArrayLength(handle, path);
        if (n < 0) throw new MdixException(MdixException.Kind.TYPE_MISMATCH,
            "arrayLength: not an array at: " + path);
        return n;
    }

    // ── Query ─────────────────────────────────────────────────────────────────

    /**
     * Starts a chainable {@link MdixQuery} over the array (or single value) at {@code path}.
     * Equivalent to Rust's {@code data.query(path)}. See {@link MdixQuery} for the full
     * filter/sort/group/aggregate surface.
     * <pre>{@code
     * List<String> bossNames = db.query("enemies")
     *     .where_(e -> "BOSS".equals(e.field("aiType").asString()))
     *     .select(e -> e.field("name").asString());
     * }</pre>
     * @throws MdixException if {@code path} is not found.
     */
    public MdixQuery query(String path) {
        String json = getJson(path);
        MdixValue parsed = MdixJson.parse(json);
        List<MdixValue> items = parsed.asArray();
        return new MdixQuery(items != null ? items : Collections.singletonList(parsed));
    }

    /**
     * Starts a chainable {@link MdixQuery} over every value matching the whole-segment glob
     * {@code pattern} (e.g. {@code "levels.*.enemies"}) — sibling paths sharing structure,
     * gathered natively via {@code DixData::select_many}. Equivalent to Rust's
     * {@code data.query_many(pattern)}.
     */
    public MdixQuery queryMany(String pattern) {
        checkOpen();
        checkPath(pattern);
        String json = MdixNative.selectManyAsJson(handle, pattern);
        if (json == null) throw new MdixException("queryMany: native call returned no result for: " + pattern);
        List<MdixValue> items = MdixJson.parse(json).asArray();
        return new MdixQuery(items != null ? items : Collections.emptyList());
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

    /** Returns all top-level key names. Equivalent to {@code keys("")}. */
    public List<String> keys() {
        return keys("");
    }

    // ── Package-private: raw handle for Converter / SchemaBuilder ─────────────

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
