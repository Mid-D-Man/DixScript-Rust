package com.midmanstudio.dixscript;

import com.midmanstudio.dixscript.internal.MdixNative;

/**
 * Static entry point for all DixScript operations.
 *
 * <pre>
 * // Load from file
 * try (Database db = DixScript.load("config.mdix")) {
 *     int port = db.getInt("server.port");
 * }
 *
 * // Load from string
 * try (Database db = DixScript.loadStr("@DATA( x = 42 )")) {
 *     int x = db.getInt("x");
 * }
 *
 * // Build
 * try (Builder b = DixScript.newBuilder()) {
 *     b.setString("name", "MyApp").setInt("version", 1);
 *     b.saveToFile("out.mdix");
 * }
 * </pre>
 */
public final class DixScript {

    private static final Converter CONVERTER = new Converter();

    private DixScript() {}

    // ── Metadata ──────────────────────────────────────────────────────────────

    public static String version() {
        return MdixNative.version();
    }

    // ── Loading ───────────────────────────────────────────────────────────────

    /**
     * Loads a .mdix file from disk.
     * @throws MdixException on parse or IO error
     */
    public static Database load(String path) {
        if (path == null || path.isEmpty())
            throw new MdixException(MdixException.Kind.INVALID_PATH, "path is null or empty");
        long h = MdixNative.load(path);
        if (h == 0)
            throw new MdixException(MdixException.Kind.IO_ERROR, "failed to load: " + path);
        return new Database(h);
    }

    /**
     * Loads .mdix content from a source string.
     * @throws MdixException on parse error
     */
    public static Database loadStr(String source) {
        if (source == null || source.isEmpty())
            throw new MdixException(MdixException.Kind.PARSE_ERROR, "source is null or empty");
        long h = MdixNative.loadStr(source);
        if (h == 0)
            throw new MdixException(MdixException.Kind.PARSE_ERROR, "failed to parse source");
        return new Database(h);
    }

    /**
     * Loads an encrypted .mdix.enc file using a key file.
     * Pass {@code null} for {@code keyPath} to auto-detect next to the enc file.
     */
    public static Database loadEncrypted(String encPath, String keyPath) {
        if (encPath == null || encPath.isEmpty())
            throw new MdixException(MdixException.Kind.INVALID_PATH, "encPath is null or empty");
        long h = MdixNative.loadEncrypted(encPath, keyPath != null ? keyPath : "");
        if (h == 0)
            throw new MdixException(MdixException.Kind.IO_ERROR, "failed to load encrypted: " + encPath);
        return new Database(h);
    }

    /**
     * Loads an encrypted .mdix.enc file using a password.
     */
    public static Database loadEncryptedPassword(String encPath, String password) {
        if (encPath == null || encPath.isEmpty())
            throw new MdixException(MdixException.Kind.INVALID_PATH, "encPath is null or empty");
        if (password == null || password.isEmpty())
            throw new MdixException(MdixException.Kind.INVALID_PATH, "password is null or empty");
        long h = MdixNative.loadEncryptedPassword(encPath, password);
        if (h == 0)
            throw new MdixException(MdixException.Kind.IO_ERROR,
                "failed to load encrypted (password): " + encPath);
        return new Database(h);
    }

    /** Shortcut: parses a JSON object string into a Database. */
    public static Database loadJson(String json) {
        return CONVERTER.fromJson(json);
    }

    /** Shortcut: parses a TOML table string into a Database. */
    public static Database loadToml(String toml) {
        return CONVERTER.fromToml(toml);
    }

    // ── Builder ───────────────────────────────────────────────────────────────

    /** Creates a new empty {@link Builder}. Remember to close it. */
    public static Builder newBuilder() {
        return new Builder();
    }

    // ── Converter ─────────────────────────────────────────────────────────────

    /** Returns the shared {@link Converter} instance. */
    public static Converter convert() {
        return CONVERTER;
    }
}
