// SchemaBuilder.java
package com.midmanstudio.dixscript;

import com.midmanstudio.dixscript.internal.MdixJson;
import com.midmanstudio.dixscript.internal.MdixNative;

import java.util.ArrayList;
import java.util.List;
import java.util.stream.Collectors;

/**
 * Fluent builder for schema definitions. Mirrors Rust's {@code dixscript::Runtime::SchemaBuilder}.
 * <p>
 * Every {@code require*} / {@code optional*} call chains; each adds one field. {@link #withDescription}
 * annotates the most recently added field. Call {@link #validate} to run the check — the same builder
 * can be reused across multiple databases.
 * <pre>{@code
 * SchemaBuilder.Report report = new SchemaBuilder()
 *     .requireString("app_name")
 *     .requireInt("port")
 *     .requireWith("port", SchemaBuilder.ExpectedType.INT, data -> {
 *         int port = data.getInt("port");
 *         return (port >= 1025 && port <= 65535) ? null : "port " + port + " out of range 1025-65535";
 *     })
 *     .optionalBool("debug")
 *     .validate(db);
 *
 * if (!report.isValid()) {
 *     System.err.println(report);
 *     // Validation failed with 1 error(s):
 *     // [Missing] 'app_name': expected string (required), got missing
 * }
 * }</pre>
 * <p>
 * The type/required check (this class's {@code require*}/{@code optional*} methods) runs natively —
 * the same {@code SchemaBuilder} DixScript's Rust runtime uses. Custom validators
 * ({@link #requireWith} / {@link #optionalWith}) can't cross the JNI boundary as a Rust closure the
 * way they do in the Rust API, so they run afterward in pure Java instead, against the already-loaded
 * {@link Database} — functionally equivalent, just evaluated managed-side.
 */
public final class SchemaBuilder {

    /** The value type a schema field must satisfy. Mirrors {@code DixValue}'s variants. */
    public enum ExpectedType {
        STRING("String"), INT("Int"), LONG("Long"), FLOAT("Float"), DOUBLE("Double"), BOOL("Bool"),
        ARRAY("Array"), OBJECT("Object"), DATE("Date"), TIMESTAMP("Timestamp"), HEX_COLOR("HexColor"),
        BLOB("Blob"), REGEX("Regex"), ENUM("Enum"),
        /** Accepts any value type. */
        ANY("Any");

        final String wire;
        ExpectedType(String wire) { this.wire = wire; }
    }

    /** Why a field failed validation. */
    public enum ErrorKind {
        /** The field is required but absent. */
        MISSING,
        /** The field is present but has the wrong value type. */
        WRONG_TYPE,
        /** The field passes the type check but fails a custom validator. */
        INVALID_VALUE;

        static ErrorKind fromWire(String wire) {
            if ("WrongType".equals(wire)) return WRONG_TYPE;
            if ("InvalidValue".equals(wire)) return INVALID_VALUE;
            return MISSING;
        }
    }

    /** One field that failed validation. */
    public static final class ValidationError {
        public final String path;
        public final String expected;
        public final String actual;
        public final ErrorKind kind;

        ValidationError(String path, String expected, String actual, ErrorKind kind) {
            this.path = path;
            this.expected = expected;
            this.actual = actual;
            this.kind = kind;
        }

        @Override
        public String toString() {
            return "[" + kind + "] '" + path + "': expected " + expected + ", got " + actual;
        }
    }

    /** A custom, whole-database validator. Return {@code null} if valid, or an error message if not. */
    @FunctionalInterface
    public interface Validator {
        String validate(Database data);
    }

    /** The result of a schema validation pass. Never throws — always returned. */
    public static final class Report {
        public final List<ValidationError> errors;

        Report(List<ValidationError> errors) { this.errors = errors; }

        /** {@code true} when no errors were found. */
        public boolean isValid() { return errors.isEmpty(); }

        public int errorCount() { return errors.size(); }

        public List<ValidationError> errorsOfKind(ErrorKind kind) {
            return errors.stream().filter(e -> e.kind == kind).collect(Collectors.toList());
        }

        public List<String> failedPaths() {
            return errors.stream().map(e -> e.path).collect(Collectors.toList());
        }

        @Override
        public String toString() {
            if (isValid()) return "Validation passed.";
            StringBuilder sb = new StringBuilder("Validation failed with " + errors.size() + " error(s):");
            for (ValidationError e : errors) sb.append('\n').append(e);
            return sb.toString();
        }
    }

    private static final class Field {
        final String path;
        final boolean required;
        final ExpectedType type;
        String description;

        Field(String path, boolean required, ExpectedType type) {
            this.path = path;
            this.required = required;
            this.type = type;
        }
    }

    private static final class PendingValidator {
        final String path;
        final Validator validator;

        PendingValidator(String path, Validator validator) {
            this.path = path;
            this.validator = validator;
        }
    }

    private final List<Field> fields = new ArrayList<>();
    private final List<PendingValidator> validators = new ArrayList<>();

    // ── required ─────────────────────────────────────────────────────────────

    /** Adds a required field with the given type. */
    public SchemaBuilder require(String path, ExpectedType type) {
        fields.add(new Field(path, true, type));
        return this;
    }

    /**
     * Adds a required field with a type check AND a custom validator. The validator runs
     * only when the type check passes, evaluated against the whole {@link Database}.
     */
    public SchemaBuilder requireWith(String path, ExpectedType type, Validator validator) {
        require(path, type);
        validators.add(new PendingValidator(path, validator));
        return this;
    }

    public SchemaBuilder requireString(String path) { return require(path, ExpectedType.STRING); }
    public SchemaBuilder requireInt(String path) { return require(path, ExpectedType.INT); }
    public SchemaBuilder requireLong(String path) { return require(path, ExpectedType.LONG); }
    public SchemaBuilder requireFloat(String path) { return require(path, ExpectedType.FLOAT); }
    public SchemaBuilder requireDouble(String path) { return require(path, ExpectedType.DOUBLE); }
    public SchemaBuilder requireBool(String path) { return require(path, ExpectedType.BOOL); }
    public SchemaBuilder requireArray(String path) { return require(path, ExpectedType.ARRAY); }
    public SchemaBuilder requireObject(String path) { return require(path, ExpectedType.OBJECT); }
    public SchemaBuilder requireEnum(String path) { return require(path, ExpectedType.ENUM); }

    // ── optional ─────────────────────────────────────────────────────────────

    /** Adds an optional field with the given type. Only checked (for type) when present. */
    public SchemaBuilder optional(String path, ExpectedType type) {
        fields.add(new Field(path, false, type));
        return this;
    }

    /** As {@link #requireWith}, but the field (and its custom validator) is only checked when present. */
    public SchemaBuilder optionalWith(String path, ExpectedType type, Validator validator) {
        optional(path, type);
        validators.add(new PendingValidator(path, validator));
        return this;
    }

    public SchemaBuilder optionalString(String path) { return optional(path, ExpectedType.STRING); }
    public SchemaBuilder optionalInt(String path) { return optional(path, ExpectedType.INT); }
    public SchemaBuilder optionalLong(String path) { return optional(path, ExpectedType.LONG); }
    public SchemaBuilder optionalFloat(String path) { return optional(path, ExpectedType.FLOAT); }
    public SchemaBuilder optionalDouble(String path) { return optional(path, ExpectedType.DOUBLE); }
    public SchemaBuilder optionalBool(String path) { return optional(path, ExpectedType.BOOL); }
    public SchemaBuilder optionalArray(String path) { return optional(path, ExpectedType.ARRAY); }
    public SchemaBuilder optionalObject(String path) { return optional(path, ExpectedType.OBJECT); }
    public SchemaBuilder optionalEnum(String path) { return optional(path, ExpectedType.ENUM); }

    // ── metadata ─────────────────────────────────────────────────────────────

    /** Annotates the most recently added field with a human-readable description. */
    public SchemaBuilder withDescription(String description) {
        if (fields.isEmpty()) {
            throw new MdixException("SchemaBuilder: withDescription() called before any require/optional field");
        }
        fields.get(fields.size() - 1).description = description;
        return this;
    }

    public int fieldCount() { return fields.size(); }

    public List<String> paths() { return fields.stream().map(f -> f.path).collect(Collectors.toList()); }

    // ── validate ─────────────────────────────────────────────────────────────

    /** Runs every field check (natively) and every custom validator (in Java) against {@code data}. */
    public Report validate(Database data) {
        String errorsJson = MdixNative.schemaValidate(data.rawHandle(), buildFieldsJson());
        List<ValidationError> errors = parseErrors(errorsJson);

        for (PendingValidator pv : validators) {
            boolean typeCheckFailed = errors.stream().anyMatch(e -> e.path.equals(pv.path));
            if (typeCheckFailed) continue; // matches Rust: the custom validator only runs once the type check passes

            String message;
            try {
                message = pv.validator.validate(data);
            } catch (RuntimeException e) {
                message = e.getMessage() != null ? e.getMessage() : e.toString();
            }
            if (message != null) {
                errors.add(new ValidationError(pv.path, "custom validation to pass", message, ErrorKind.INVALID_VALUE));
            }
        }
        return new Report(errors);
    }

    private String buildFieldsJson() {
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < fields.size(); i++) {
            if (i > 0) sb.append(',');
            Field f = fields.get(i);
            sb.append("{\"path\":").append(jsonString(f.path))
              .append(",\"required\":").append(f.required)
              .append(",\"type\":").append(jsonString(f.type.wire));
            if (f.description != null) sb.append(",\"description\":").append(jsonString(f.description));
            sb.append('}');
        }
        return sb.append(']').toString();
    }

    private static List<ValidationError> parseErrors(String json) {
        List<MdixValue> arr = MdixJson.parse(json).asArray();
        List<ValidationError> out = new ArrayList<>();
        if (arr == null) return out;
        for (MdixValue v : arr) {
            out.add(new ValidationError(
                v.field("path").asString(),
                v.field("expected").asString(),
                v.field("actual").asString(),
                ErrorKind.fromWire(v.field("kind").asString())));
        }
        return out;
    }

    private static String jsonString(String s) {
        StringBuilder sb = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"': sb.append("\\\""); break;
                case '\\': sb.append("\\\\"); break;
                case '\n': sb.append("\\n"); break;
                case '\r': sb.append("\\r"); break;
                case '\t': sb.append("\\t"); break;
                default:
                    if (c < 0x20) sb.append(String.format("\\u%04x", (int) c));
                    else sb.append(c);
            }
        }
        return sb.append('"').toString();
    }
}
