
package com.midmanstudio.dixscript;

import com.midmanstudio.dixscript.internal.MdixNative;
import java.io.Closeable;
import java.time.Instant;
import java.time.LocalDate;
import java.time.format.DateTimeFormatter;

/**
 * Builds a .mdix file programmatically by setting key-value pairs.
 *
 * <pre>
 *   try (Builder b = new Builder()) {
 *       b.setString("profile.name", "player1");
 *       b.setInt("profile.level", 42);
 *       b.setDouble("profile.score", 9876.5);
 *       b.saveToFile("profile.mdix");
 *   }
 * </pre>
 *
 * Implements {@link Closeable} for try-with-resources.
 * Thread-safe.
 */
public final class Builder implements Closeable {

    private volatile long handle;
    private volatile boolean closed = false;

    /** Creates a new, empty builder. */
    public Builder() {
        this.handle = MdixNative.builderNew();
        if (this.handle == 0)
            throw new MdixException("Failed to create native builder");
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /** Releases the underlying native handle. Safe to call more than once. */
    @Override
    public synchronized void close() {
        if (!closed && handle != 0) {
            MdixNative.builderFree(handle);
            handle = 0;
            closed = true;
        }
    }

    /** Number of key-value pairs currently set on this builder. */
    public int entryCount() {
        checkOpen();
        return MdixNative.builderEntryCount(handle);
    }

    /** Removes every key-value pair, leaving the builder empty. */
    public void clear() {
        checkOpen();
        MdixNative.builderClear(handle);
    }

    // ── Write ─────────────────────────────────────────────────────────────────

    /** Sets the string at {@code path}. A {@code null} value is stored as an empty string. */
    public Builder setString(String path, String value) {
        checkOpen();
        checkPath(path);
        if (!MdixNative.builderSetString(handle, path, value != null ? value : ""))
            throw new MdixException("setString failed for path: " + path);
        return this;
    }

    /** Sets the 32-bit integer at {@code path}. */
    public Builder setInt(String path, int value) {
        checkOpen();
        checkPath(path);
        if (!MdixNative.builderSetInt(handle, path, value))
            throw new MdixException("setInt failed for path: " + path);
        return this;
    }

    /** Sets the 64-bit integer at {@code path}. */
    public Builder setLong(String path, long value) {
        checkOpen();
        checkPath(path);
        if (!MdixNative.builderSetLong(handle, path, value))
            throw new MdixException("setLong failed for path: " + path);
        return this;
    }

    /** Sets the 32-bit float at {@code path}. */
    public Builder setFloat(String path, float value) {
        checkOpen();
        checkPath(path);
        if (!MdixNative.builderSetFloat(handle, path, value))
            throw new MdixException("setFloat failed for path: " + path);
        return this;
    }

    /** Sets the 64-bit double at {@code path}. */
    public Builder setDouble(String path, double value) {
        checkOpen();
        checkPath(path);
        if (!MdixNative.builderSetDouble(handle, path, value))
            throw new MdixException("setDouble failed for path: " + path);
        return this;
    }

    /** Sets the boolean at {@code path}. */
    public Builder setBool(String path, boolean value) {
        checkOpen();
        checkPath(path);
        if (!MdixNative.builderSetBool(handle, path, value))
            throw new MdixException("setBool failed for path: " + path);
        return this;
    }

    /** Stores a {@link LocalDate} as a YYYY-MM-DD string. */
    public Builder setDate(String path, LocalDate value) {
        return setString(path, value.format(DateTimeFormatter.ISO_LOCAL_DATE));
    }

    /** Stores an {@link Instant} as an ISO-8601 timestamp string. */
    public Builder setTimestamp(String path, Instant value) {
        return setString(path, value.toString());
    }

    /** Removes a key from the builder. Returns true if the key existed. */
    public boolean remove(String path) {
        checkOpen();
        checkPath(path);
        return MdixNative.builderRemove(handle, path);
    }

    // ── Read back ──────────────────────────────────────────────────────────────

    /** {@code true} if {@code path} has been set on this builder. Never throws — returns {@code false} on a closed builder or a null path. */
    public boolean hasKey(String path) {
        if (closed || path == null) return false;
        return MdixNative.builderHasKey(handle, path);
    }

    // ── Persistence ───────────────────────────────────────────────────────────

    /**
     * Saves the builder contents to a .mdix file.
     * Intermediate directories are created automatically.
     */
    public void saveToFile(String path) {
        checkOpen();
        checkPath(path);
        if (!MdixNative.builderSave(handle, path))
            throw new MdixException(MdixException.Kind.IO_ERROR,
                "saveToFile failed for path: " + path);
    }

    /**
     * Serializes the builder contents to a .mdix format string.
     */
    public String toMdixString() {
        checkOpen();
        String result = MdixNative.builderToString(handle);
        if (result == null)
            throw new MdixException("builderToString failed");
        return result;
    }

    /**
     * Serializes and immediately loads the builder contents into a new {@link Database}.
     * The caller is responsible for closing the returned Database.
     */
    public Database toDatabase() {
        return DixScript.loadStr(toMdixString());
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    private void checkOpen() {
        if (closed || handle == 0)
            throw new MdixException(MdixException.Kind.CLOSED, "Builder has been closed");
    }

    private void checkPath(String path) {
        if (path == null || path.isEmpty())
            throw new MdixException(MdixException.Kind.INVALID_PATH,
                "path must not be null or empty");
    }
}
