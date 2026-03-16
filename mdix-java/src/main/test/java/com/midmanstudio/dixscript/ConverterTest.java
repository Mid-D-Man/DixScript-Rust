package com.midmanstudio.dixscript;

import org.junit.jupiter.api.*;
import static org.assertj.core.api.Assertions.*;

class ConverterTest {

    private static final String SRC =
        "@DATA( port = 8080, host = \"localhost\", ssl = true )";

    // ── toJson ────────────────────────────────────────────────────────────────

    @Test void toJson_indented_containsNewlines() {
        try (Database db = DixScript.loadStr(SRC)) {
            String json = DixScript.convert().toJson(db, true);
            assertThat(json).contains("\n");
            assertThat(json).contains("8080");
            assertThat(json).contains("localhost");
        }
    }

    @Test void toJson_compact_noNewlines() {
        try (Database db = DixScript.loadStr(SRC)) {
            String json = DixScript.convert().toJson(db, false);
            assertThat(json.trim()).doesNotContain("\n");
            assertThat(json).contains("8080");
        }
    }

    @Test void toJson_nullDb_throws() {
        assertThatThrownBy(() -> DixScript.convert().toJson(null, true))
            .isInstanceOf(MdixException.class)
            .extracting(e -> ((MdixException) e).getKind())
            .isEqualTo(MdixException.Kind.NULL_HANDLE);
    }

    // ── fromJson ──────────────────────────────────────────────────────────────

    @Test void fromJson_validObject_readable() {
        String json = "{\"port\": 9000, \"host\": \"db.local\", \"ssl\": false}";
        try (Database db = DixScript.convert().fromJson(json)) {
            assertThat(db.getInt("port")).isEqualTo(9000);
            assertThat(db.getString("host")).isEqualTo("db.local");
            assertThat(db.getBool("ssl")).isFalse();
        }
    }

    @Test void fromJson_emptyString_throws() {
        assertThatThrownBy(() -> DixScript.convert().fromJson(""))
            .isInstanceOf(MdixException.class)
            .extracting(e -> ((MdixException) e).getKind())
            .isEqualTo(MdixException.Kind.PARSE_ERROR);
    }

    @Test void fromJson_invalidJson_throws() {
        assertThatThrownBy(() -> DixScript.convert().fromJson("not json at all"))
            .isInstanceOf(MdixException.class);
    }

    // ── toToml ────────────────────────────────────────────────────────────────

    @Test void toToml_containsValues() {
        try (Database db = DixScript.loadStr(SRC)) {
            String toml = DixScript.convert().toToml(db);
            assertThat(toml).contains("8080");
            assertThat(toml).contains("localhost");
        }
    }

    @Test void toToml_nullDb_throws() {
        assertThatThrownBy(() -> DixScript.convert().toToml(null))
            .isInstanceOf(MdixException.class);
    }

    // ── fromToml ──────────────────────────────────────────────────────────────

    @Test void fromToml_validTable_readable() {
        String toml = "port = 7070\nhost = \"toml.local\"\nssl = true\n";
        try (Database db = DixScript.convert().fromToml(toml)) {
            assertThat(db.getInt("port")).isEqualTo(7070);
            assertThat(db.getString("host")).isEqualTo("toml.local");
            assertThat(db.getBool("ssl")).isTrue();
        }
    }

    @Test void fromToml_emptyString_throws() {
        assertThatThrownBy(() -> DixScript.convert().fromToml(""))
            .isInstanceOf(MdixException.class);
    }

    // ── round-trip ────────────────────────────────────────────────────────────

    @Test void jsonRoundTrip_valuesPreserved() {
        try (Database original = DixScript.loadStr(SRC);
             Database restored = DixScript.convert().jsonRoundTrip(original)) {
            assertThat(restored.getInt("port")).isEqualTo(8080);
            assertThat(restored.getString("host")).isEqualTo("localhost");
            assertThat(restored.getBool("ssl")).isTrue();
        }
    }

    @Test void toJson_thenFromJson_roundTrips() {
        try (Database original = DixScript.loadStr(SRC)) {
            String json = DixScript.convert().toJson(original, false);
            try (Database restored = DixScript.convert().fromJson(json)) {
                assertThat(restored.getInt("port")).isEqualTo(8080);
                assertThat(restored.getString("host")).isEqualTo("localhost");
            }
        }
    }

    @Test void toToml_thenFromToml_roundTrips() {
        try (Database original = DixScript.loadStr(SRC)) {
            String toml = DixScript.convert().toToml(original);
            try (Database restored = DixScript.convert().fromToml(toml)) {
                assertThat(restored.getInt("port")).isEqualTo(8080);
                assertThat(restored.getString("host")).isEqualTo("localhost");
            }
        }
    }

    // ── toMdix ────────────────────────────────────────────────────────────────

    @Test void toMdix_default_containsDataSection() {
        try (Database db = DixScript.loadStr(SRC)) {
            String mdix = DixScript.convert().toMdix(db, Converter.FormatMode.DEFAULT);
            assertThat(mdix).contains("@DATA(");
            assertThat(mdix).contains("8080");
        }
    }

    @Test void toMdix_minified_shorterThanDefault() {
        try (Database db = DixScript.loadStr(SRC)) {
            String normal   = DixScript.convert().toMdix(db, Converter.FormatMode.DEFAULT);
            String minified = DixScript.convert().toMdix(db, Converter.FormatMode.MINIFIED);
            assertThat(minified.length()).isLessThan(normal.length());
        }
    }

    // ── formatSource ─────────────────────────────────────────────────────────

    @Test void minifySource_removesComments() {
        String src = "@DATA( x = 1 // comment\n)";
        String result = DixScript.convert().minifySource(src);
        assertThat(result).doesNotContain("//");
        assertThat(result).contains("x");
    }

    // ── loadJson / loadToml shortcuts ─────────────────────────────────────────

    @Test void dixScript_loadJson_works() {
        try (Database db = DixScript.loadJson("{\"score\": 99}")) {
            assertThat(db.getInt("score")).isEqualTo(99);
        }
    }

    @Test void dixScript_loadToml_works() {
        try (Database db = DixScript.loadToml("retries = 3\n")) {
            assertThat(db.getInt("retries")).isEqualTo(3);
        }
    }
              }
