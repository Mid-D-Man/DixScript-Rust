package com.midmanstudio.dixscript;

import org.junit.jupiter.api.*;
import static org.assertj.core.api.Assertions.*;

/**
 * Integration tests for {@link SchemaBuilder}.
 * Requires the native lib to be on java.library.path (set by build.gradle.kts).
 */
class SchemaTest {

    private static final String VALID_SRC =
        "@DATA( app_name = \"MyApp\" port = 8080 debug = true )";

    private static final String MISSING_FIELD_SRC =
        "@DATA( port = 8080 )";

    private static final String WRONG_TYPE_SRC =
        "@DATA( app_name = \"MyApp\" port = \"not-a-number\" )";

    // ── passing validation ────────────────────────────────────────────────────

    @Test void validate_allFieldsPresentAndTyped_passes() {
        try (Database db = DixScript.loadStr(VALID_SRC)) {
            SchemaBuilder.Report report = new SchemaBuilder()
                .requireString("app_name")
                .requireInt("port")
                .optionalBool("debug")
                .validate(db);
            assertThat(report.isValid()).isTrue();
            assertThat(report.errorCount()).isZero();
        }
    }

    @Test void validate_missingOptionalField_stillPasses() {
        try (Database db = DixScript.loadStr(VALID_SRC)) {
            SchemaBuilder.Report report = new SchemaBuilder()
                .requireString("app_name")
                .optionalString("not_present")
                .validate(db);
            assertThat(report.isValid()).isTrue();
        }
    }

    // ── failing validation ────────────────────────────────────────────────────

    @Test void validate_missingRequiredField_reportsMissing() {
        try (Database db = DixScript.loadStr(MISSING_FIELD_SRC)) {
            SchemaBuilder.Report report = new SchemaBuilder()
                .requireString("app_name")
                .requireInt("port")
                .validate(db);
            assertThat(report.isValid()).isFalse();
            assertThat(report.errorsOfKind(SchemaBuilder.ErrorKind.MISSING)).hasSize(1);
            assertThat(report.failedPaths()).containsExactly("app_name");
        }
    }

    @Test void validate_wrongType_reportsWrongType() {
        try (Database db = DixScript.loadStr(WRONG_TYPE_SRC)) {
            SchemaBuilder.Report report = new SchemaBuilder()
                .requireString("app_name")
                .requireInt("port")
                .validate(db);
            assertThat(report.isValid()).isFalse();
            assertThat(report.errorsOfKind(SchemaBuilder.ErrorKind.WRONG_TYPE)).hasSize(1);
        }
    }

    @Test void validate_multipleErrors_reportsAll() {
        try (Database db = DixScript.loadStr(MISSING_FIELD_SRC)) {
            SchemaBuilder.Report report = new SchemaBuilder()
                .requireString("app_name")
                .requireString("author")
                .validate(db);
            assertThat(report.errorCount()).isEqualTo(2);
        }
    }

    // ── custom validators (requireWith / optionalWith) ───────────────────────

    @Test void requireWith_customValidatorPasses() {
        try (Database db = DixScript.loadStr(VALID_SRC)) {
            SchemaBuilder.Report report = new SchemaBuilder()
                .requireWith("port", SchemaBuilder.ExpectedType.INT, data -> {
                    int port = data.getInt("port");
                    return (port >= 1025 && port <= 65535) ? null : "port out of range";
                })
                .validate(db);
            assertThat(report.isValid()).isTrue();
        }
    }

    @Test void requireWith_customValidatorFails_reportsInvalidValue() {
        try (Database db = DixScript.loadStr(VALID_SRC)) {
            SchemaBuilder.Report report = new SchemaBuilder()
                .requireWith("port", SchemaBuilder.ExpectedType.INT, data -> "always fails")
                .validate(db);
            assertThat(report.isValid()).isFalse();
            assertThat(report.errorsOfKind(SchemaBuilder.ErrorKind.INVALID_VALUE)).hasSize(1);
        }
    }

    @Test void requireWith_skipsCustomValidator_whenTypeCheckAlreadyFailed() {
        try (Database db = DixScript.loadStr(MISSING_FIELD_SRC)) {
            boolean[] validatorRan = { false };
            SchemaBuilder.Report report = new SchemaBuilder()
                .requireWith("app_name", SchemaBuilder.ExpectedType.STRING, data -> {
                    validatorRan[0] = true;
                    return null;
                })
                .validate(db);
            assertThat(report.isValid()).isFalse();
            assertThat(validatorRan[0]).isFalse(); // never ran — the field was missing (type check failed first)
        }
    }

    // ── metadata ──────────────────────────────────────────────────────────────

    @Test void fieldCount_and_paths_reflectAddedFields() {
        SchemaBuilder schema = new SchemaBuilder().requireString("a").requireInt("b").optionalBool("c");
        assertThat(schema.fieldCount()).isEqualTo(3);
        assertThat(schema.paths()).containsExactly("a", "b", "c");
    }

    @Test void withDescription_beforeAnyField_throws() {
        assertThatThrownBy(() -> new SchemaBuilder().withDescription("oops")).isInstanceOf(MdixException.class);
    }

    @Test void report_toString_isHumanReadable() {
        try (Database db = DixScript.loadStr(MISSING_FIELD_SRC)) {
            SchemaBuilder.Report report = new SchemaBuilder().requireString("app_name").validate(db);
            assertThat(report.toString()).contains("Validation failed").contains("app_name");
        }
    }
}
