package com.midmanstudio.dixscript;

import org.junit.jupiter.api.*;
import static org.assertj.core.api.Assertions.*;

class BuilderTest {

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    @Test void newBuilder_isNotNull() {
        try (Builder b = DixScript.newBuilder()) {
            assertThat(b).isNotNull();
        }
    }

    @Test void close_calledTwice_doesNotThrow() {
        Builder b = DixScript.newBuilder();
        b.close();
        assertThatCode(b::close).doesNotThrowAnyException();
    }

    @Test void setString_afterClose_throws() {
        Builder b = DixScript.newBuilder();
        b.close();
        assertThatThrownBy(() -> b.setString("x", "v"))
            .isInstanceOf(MdixException.class)
            .extracting(e -> ((MdixException) e).getKind())
            .isEqualTo(MdixException.Kind.CLOSED);
    }

    // ── Set / get round-trip ──────────────────────────────────────────────────

    @Test void setString_getBack_roundTrips() {
        try (Builder b = DixScript.newBuilder()) {
            b.setString("app.name", "DixScript");
            assertThat(b.hasKey("app.name")).isTrue();
        }
    }

    @Test void setInt_hasKey_true() {
        try (Builder b = DixScript.newBuilder()) {
            b.setInt("port", 8080);
            assertThat(b.hasKey("port")).isTrue();
        }
    }

    @Test void setFloat_hasKey_true() {
        try (Builder b = DixScript.newBuilder()) {
            b.setFloat("rate", 1.5f);
            assertThat(b.hasKey("rate")).isTrue();
        }
    }

    @Test void setDouble_hasKey_true() {
        try (Builder b = DixScript.newBuilder()) {
            b.setDouble("pi", 3.14159);
            assertThat(b.hasKey("pi")).isTrue();
        }
    }

    @Test void setBool_hasKey_true() {
        try (Builder b = DixScript.newBuilder()) {
            b.setBool("debug", true);
            assertThat(b.hasKey("debug")).isTrue();
        }
    }

    // ── Fluent chaining ───────────────────────────────────────────────────────

    @Test void fluent_chain_works() {
        try (Builder b = DixScript.newBuilder()) {
            assertThatCode(() ->
                b.setString("a", "hello")
                 .setInt("b", 42)
                 .setBool("c", false)
            ).doesNotThrowAnyException();
        }
    }

    // ── Remove ────────────────────────────────────────────────────────────────

    @Test void remove_existingKey_returnsTrue() {
        try (Builder b = DixScript.newBuilder()) {
            b.setInt("x", 1);
            assertThat(b.remove("x")).isTrue();
            assertThat(b.hasKey("x")).isFalse();
        }
    }

    @Test void remove_missingKey_returnsFalse() {
        try (Builder b = DixScript.newBuilder()) {
            assertThat(b.remove("nope")).isFalse();
        }
    }

    // ── Clear ─────────────────────────────────────────────────────────────────

    @Test void clear_removesAllKeys() {
        try (Builder b = DixScript.newBuilder()) {
            b.setString("a", "1").setInt("b", 2).setBool("c", true);
            b.clear();
            assertThat(b.hasKey("a")).isFalse();
            assertThat(b.hasKey("b")).isFalse();
        }
    }

    // ── toString / toDatabase ─────────────────────────────────────────────────

    @Test void toMdixString_nonEmpty() {
        try (Builder b = DixScript.newBuilder()) {
            b.setString("name", "test").setInt("val", 99);
            String s = b.toMdixString();
            assertThat(s).isNotBlank();
        }
    }

    @Test void toDatabase_valuesReadable() {
        try (Builder b = DixScript.newBuilder()) {
            b.setString("greet", "hello");
            b.setInt("num", 7);
            b.setBool("flag", true);

            try (Database db = b.toDatabase()) {
                assertThat(db.getString("greet")).isEqualTo("hello");
                assertThat(db.getInt("num")).isEqualTo(7);
                assertThat(db.getBool("flag")).isTrue();
            }
        }
    }

    @Test void toDatabase_multipleTypes_allReadable() {
        try (Builder b = DixScript.newBuilder()) {
            b.setString("s", "world")
             .setInt("i", 42)
             .setFloat("f", 1.5f)
             .setDouble("d", 3.14)
             .setBool("b", false);

            try (Database db = b.toDatabase()) {
                assertThat(db.getString("s")).isEqualTo("world");
                assertThat(db.getInt("i")).isEqualTo(42);
                assertThat(db.getFloat("f")).isCloseTo(1.5f, within(0.001f));
                assertThat(db.getDouble("d")).isCloseTo(3.14, within(0.001));
                assertThat(db.getBool("b")).isFalse();
            }
        }
    }

    // ── saveToFile ────────────────────────────────────────────────────────────

    @Test void saveToFile_writesFile(@TempDir java.nio.file.Path tmp) {
        try (Builder b = DixScript.newBuilder()) {
            b.setString("saved", "yes");
            String path = tmp.resolve("out.mdix").toString();
            assertThatCode(() -> b.saveToFile(path)).doesNotThrowAnyException();
            assertThat(new java.io.File(path)).exists();
        }
    }

    @Test void saveToFile_loadBack_valuesPresent(@TempDir java.nio.file.Path tmp) {
        String path = tmp.resolve("roundtrip.mdix").toString();
        try (Builder b = DixScript.newBuilder()) {
            b.setInt("answer", 42);
            b.saveToFile(path);
        }
        try (Database db = DixScript.load(path)) {
            assertThat(db.getInt("answer")).isEqualTo(42);
        }
    }

    // ── Null / empty path guards ──────────────────────────────────────────────

    @Test void setString_nullPath_throws() {
        try (Builder b = DixScript.newBuilder()) {
            assertThatThrownBy(() -> b.setString(null, "v"))
                .isInstanceOf(MdixException.class)
                .extracting(e -> ((MdixException) e).getKind())
                .isEqualTo(MdixException.Kind.INVALID_PATH);
        }
    }

    @Test void setString_emptyPath_throws() {
        try (Builder b = DixScript.newBuilder()) {
            assertThatThrownBy(() -> b.setString("", "v"))
                .isInstanceOf(MdixException.class);
        }
    }
          }
