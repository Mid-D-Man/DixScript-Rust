package com.midmanstudio.dixscript;

import com.midmanstudio.dixscript.internal.MdixNative;

/**
 * Format conversion utilities.
 * Obtain via {@link DixScript#convert()} or use the static methods directly.
 */
public final class Converter {

    /** Controls output style for {@link #toMdix(Database, FormatMode)}. */
    public enum FormatMode {
        DEFAULT(0), PRETTY(1), COMPACT(2), MINIFIED(3);

        final int code;
        FormatMode(int code) { this.code = code; }
    }

    // ── Export ────────────────────────────────────────────────────────────────

    /**
     * Exports all entries in {@code db} as a JSON string.
     * @param indented {@code true} for pretty-printed output
     */
    public String toJson(Database db, boolean indented) {
        if (db == null) throw new MdixException(MdixException.Kind.NULL_HANDLE, "db is null");
        String result = MdixNative.toJson(db.rawHandle(), indented);
        if (result == null) throw new MdixException("toJson failed");
        return result;
    }

    /** Re-serializes {@code db} to .mdix text format. */
    public String toMdix(Database db, FormatMode mode) {
        if (db == null) throw new MdixException(MdixException.Kind.NULL_HANDLE, "db is null");
        String result = MdixNative.toMdix(db.rawHandle(), mode.code);
        if (result == null) throw new MdixException("toMdix failed");
        return result;
    }

    /** Exports all entries in {@code db} as a TOML string. */
    public String toToml(Database db) {
        if (db == null) throw new MdixException(MdixException.Kind.NULL_HANDLE, "db is null");
        String result = MdixNative.toToml(db.rawHandle());
        if (result == null) throw new MdixException("toToml failed");
        return result;
    }

    // ── Import ────────────────────────────────────────────────────────────────

    /**
     * Parses a JSON object string into a new {@link Database}.
     * The caller must close the returned Database.
     */
    public Database fromJson(String json) {
        if (json == null || json.isEmpty())
            throw new MdixException(MdixException.Kind.PARSE_ERROR, "json is null or empty");
        long h = MdixNative.fromJson(json);
        if (h == 0) throw new MdixException(MdixException.Kind.PARSE_ERROR, "fromJson failed");
        return new Database(h);
    }

    /**
     * Parses a TOML table string into a new {@link Database}.
     * The caller must close the returned Database.
     */
    public Database fromToml(String toml) {
        if (toml == null || toml.isEmpty())
            throw new MdixException(MdixException.Kind.PARSE_ERROR, "toml is null or empty");
        long h = MdixNative.fromToml(toml);
        if (h == 0) throw new MdixException(MdixException.Kind.PARSE_ERROR, "fromToml failed");
        return new Database(h);
    }

    // ── Source text formatting ────────────────────────────────────────────────

    /** Formats raw .mdix source text according to {@code mode}. */
    public String formatSource(String source, FormatMode mode) {
        if (source == null) throw new MdixException("source is null");
        String result = MdixNative.formatSource(source, mode.code);
        if (result == null) throw new MdixException("formatSource failed");
        return result;
    }

    /** Removes all unnecessary whitespace and comments from raw .mdix source. */
    public String minifySource(String source) {
        return formatSource(source, FormatMode.MINIFIED);
    }

    // ── Round-trip ────────────────────────────────────────────────────────────

    /**
     * Exports {@code db} to JSON and immediately loads it back.
     * The caller must close the returned Database.
     */
    public Database jsonRoundTrip(Database db) {
        return fromJson(toJson(db, false));
    }
    }
