package com.midmanstudio.dixscript;

import org.junit.jupiter.api.*;
import static org.assertj.core.api.Assertions.*;

/**
 * Integration tests for {@link Database}.
 * Requires the native lib to be on java.library.path (set by build.gradle.kts).
 */
class DatabaseTest {

    private static final String SIMPLE_SRC =
        "@DATA( " +
        "  greeting = \"hello\"" +
        "  port = 8080 " +
        "  rate = 1.5f " +
        "  pi = 3.14159 " +
        "  active = true " +
        "  server: host = \"localhost\", ssl = false " +
        "  tags:: \"alpha\", \"beta\", \"gamma\" " +
        ")";

    private Database db;

    @BeforeEach
    void setUp() {
        db = DixScript.loadStr(SIMPLE_SRC);
    }

    @AfterEach
    void tearDown() {
        db.close();
    }

    // ── Loading ───────────────────────────────────────────────────────────────

    @Test void loadStr_valid_isValid() {
        assertThat(db.isValid()).isTrue();
    }

    @Test void loadStr_empty_throws() {
        assertThatThrownBy(() -> DixScript.loadStr(""))
            .isInstanceOf(MdixException.class);
    }

    @Test void loadStr_malformed_throws() {
        assertThatThrownBy(() -> DixScript.loadStr("@@@INVALID$$$"))
            .isInstanceOf(MdixException.class);
    }

    // ── String ────────────────────────────────────────────────────────────────

    @Test void getString_knownPath_returnsValue() {
        assertThat(db.getString("greeting")).isEqualTo("hello");
    }

    @Test void getString_missingPath_throws() {
        assertThatThrownBy(() -> db.getString("nope"))
            .isInstanceOf(MdixException.class);
    }

    @Test void getString_withDefault_returnsDefault() {
        assertThat(db.getString("nope", "fallback")).isEqualTo("fallback");
    }

    // ── Int ───────────────────────────────────────────────────────────────────

    @Test void getInt_knownPath_returnsValue() {
        assertThat(db.getInt("port")).isEqualTo(8080);
    }

    @Test void getInt_withDefault_returnsPresentValue() {
        assertThat(db.getInt("port", -1)).isEqualTo(8080);
    }

    @Test void getInt_withDefault_missingReturnsDefault() {
        assertThat(db.getInt("nope", -1)).isEqualTo(-1);
    }

    // ── Float / Double ────────────────────────────────────────────────────────

    @Test void getFloat_returnsValue() {
        assertThat(db.getFloat("rate")).isCloseTo(1.5f, within(0.001f));
    }

    @Test void getDouble_returnsValue() {
        assertThat(db.getDouble("pi")).isCloseTo(3.14159, within(0.00001));
    }

    // ── Bool ──────────────────────────────────────────────────────────────────

    @Test void getBool_true() {
        assertThat(db.getBool("active")).isTrue();
    }

    @Test void getBool_false() {
        assertThat(db.getBool("server.ssl")).isFalse();
    }

    // ── Nested (table property) ───────────────────────────────────────────────

    @Test void getString_nestedPath_returnsValue() {
        assertThat(db.getString("server.host")).isEqualTo("localhost");
    }

    // ── Array ─────────────────────────────────────────────────────────────────

    @Test void arrayLength_returnsCount() {
        assertThat(db.arrayLength("tags")).isEqualTo(3);
    }

    @Test void arrayLength_notArray_throws() {
        assertThatThrownBy(() -> db.arrayLength("port"))
            .isInstanceOf(MdixException.class)
            .hasMessageContaining("TYPE_MISMATCH");
    }

    // ── ValueType ─────────────────────────────────────────────────────────────

    @Test void valueTypeAt_int() {
        assertThat(db.valueTypeAt("port")).isEqualTo(ValueType.INT);
    }

    @Test void valueTypeAt_string() {
        assertThat(db.valueTypeAt("greeting")).isEqualTo(ValueType.STRING);
    }

    @Test void valueTypeAt_bool() {
        assertThat(db.valueTypeAt("active")).isEqualTo(ValueType.BOOL);
    }

    @Test void valueTypeAt_array() {
        assertThat(db.valueTypeAt("tags")).isEqualTo(ValueType.ARRAY);
    }

    @Test void valueTypeAt_missing() {
        assertThat(db.valueTypeAt("nope")).isEqualTo(ValueType.UNKNOWN);
    }

    // ── Exists ────────────────────────────────────────────────────────────────

    @Test void exists_present() {
        assertThat(db.exists("port")).isTrue();
    }

    @Test void exists_absent() {
        assertThat(db.exists("nope")).isFalse();
    }

    // ── Keys ──────────────────────────────────────────────────────────────────

    @Test void keys_topLevel_nonEmpty() {
        assertThat(db.keys()).isNotEmpty();
    }

    // ── Close ─────────────────────────────────────────────────────────────────

    @Test void close_calledTwice_doesNotThrow() {
        Database d = DixScript.loadStr("@DATA( x = 1 )");
        d.close();
        assertThatCode(d::close).doesNotThrowAnyException();
    }

    @Test void getString_afterClose_throws() {
        Database d = DixScript.loadStr("@DATA( x = \"v\" )");
        d.close();
        assertThatThrownBy(() -> d.getString("x"))
            .isInstanceOf(MdixException.class)
            .extracting(e -> ((MdixException) e).getKind())
            .isEqualTo(MdixException.Kind.CLOSED);
    }
  }
