package com.midmanstudio.dixscript;

/**
 * Thrown by any DixScript operation that fails.
 *
 * The message comes directly from the Rust error string.
 * Check {@link #getKind()} to branch on the category without string matching.
 */
public class MdixException extends RuntimeException {

    public enum Kind {
        NOT_FOUND,
        TYPE_MISMATCH,
        NULL_HANDLE,
        INVALID_PATH,
        NATIVE_ERROR,
        IO_ERROR,
        PARSE_ERROR,
        CLOSED,
        UNKNOWN
    }

    private final Kind kind;

    public MdixException(String message) {
        super(message);
        this.kind = inferKind(message);
    }

    public MdixException(String message, Throwable cause) {
        super(message, cause);
        this.kind = inferKind(message);
    }

    public MdixException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    /** The category of this error. */
    public Kind getKind() { return kind; }

    private static Kind inferKind(String msg) {
        if (msg == null) return Kind.UNKNOWN;
        String lo = msg.toLowerCase();
        if (lo.contains("not found") || lo.contains("path not found")) return Kind.NOT_FOUND;
        if (lo.contains("type mismatch") || lo.contains("cannot convert")) return Kind.TYPE_MISMATCH;
        if (lo.contains("null handle") || lo.contains("null pointer")) return Kind.NULL_HANDLE;
        if (lo.contains("invalid path") || lo.contains("path is null")) return Kind.INVALID_PATH;
        if (lo.contains("parse") || lo.contains("syntax")) return Kind.PARSE_ERROR;
        if (lo.contains("io") || lo.contains("file") || lo.contains("write")) return Kind.IO_ERROR;
        if (lo.contains("closed") || lo.contains("disposed")) return Kind.CLOSED;
        return Kind.NATIVE_ERROR;
    }
}
