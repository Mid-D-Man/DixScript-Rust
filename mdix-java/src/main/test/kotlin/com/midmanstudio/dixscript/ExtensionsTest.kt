package com.midmanstudio.dixscript

import kotlinx.coroutines.runBlocking
import org.assertj.core.api.Assertions.*
import org.junit.jupiter.api.*
import java.nio.file.Files

class ExtensionsTest {

    private val src = """@DATA( port = 8080, host = "localhost", active = true, pi = 3.14 )"""

    // ── get<T> operator ───────────────────────────────────────────────────────

    @Test fun `get operator returns String`() {
        DixScript.loadStr(src).use { db ->
            val host: String = db["host"]
            assertThat(host).isEqualTo("localhost")
        }
    }

    @Test fun `get operator returns Int`() {
        DixScript.loadStr(src).use { db ->
            val port: Int = db["port"]
            assertThat(port).isEqualTo(8080)
        }
    }

    @Test fun `get operator returns Boolean`() {
        DixScript.loadStr(src).use { db ->
            val active: Boolean = db["active"]
            assertThat(active).isTrue()
        }
    }

    @Test fun `get operator returns Double`() {
        DixScript.loadStr(src).use { db ->
            val pi: Double = db["pi"]
            assertThat(pi).isCloseTo(3.14, within(0.001))
        }
    }

    @Test fun `get operator throws for missing path`() {
        DixScript.loadStr(src).use { db ->
            assertThatThrownBy { val v: String = db["nope"] }
                .isInstanceOf(MdixException::class.java)
        }
    }

    // ── getOrNull ─────────────────────────────────────────────────────────────

    @Test fun `getOrNull returns value when present`() {
        DixScript.loadStr(src).use { db ->
            val port: Int? = db.getOrNull("port")
            assertThat(port).isEqualTo(8080)
        }
    }

    @Test fun `getOrNull returns null when absent`() {
        DixScript.loadStr(src).use { db ->
            val v: String? = db.getOrNull("nope")
            assertThat(v).isNull()
        }
    }

    // ── getXxxOrDefault ───────────────────────────────────────────────────────

    @Test fun `getStringOrDefault returns default when absent`() {
        DixScript.loadStr(src).use { db ->
            assertThat(db.getStringOrDefault("nope", "fallback")).isEqualTo("fallback")
        }
    }

    @Test fun `getIntOrDefault returns present value`() {
        DixScript.loadStr(src).use { db ->
            assertThat(db.getIntOrDefault("port", -1)).isEqualTo(8080)
        }
    }

    @Test fun `getBoolOrDefault returns default when absent`() {
        DixScript.loadStr(src).use { db ->
            assertThat(db.getBoolOrDefault("nope", true)).isTrue()
        }
    }

    // ── Builder operator set ──────────────────────────────────────────────────

    @Test fun `builder set operator works for String`() {
        DixScript.newBuilder().use { b ->
            b["name"] = "DixScript"
            assertThat(b.hasKey("name")).isTrue()
        }
    }

    @Test fun `builder set operator works for Int`() {
        DixScript.newBuilder().use { b ->
            b["count"] = 42
            assertThat(b.hasKey("count")).isTrue()
        }
    }

    @Test fun `builder set operator works for Boolean`() {
        DixScript.newBuilder().use { b ->
            b["flag"] = false
            assertThat(b.hasKey("flag")).isTrue()
        }
    }

    @Test fun `builder set operator works for Double`() {
        DixScript.newBuilder().use { b ->
            b["rate"] = 9.99
            assertThat(b.hasKey("rate")).isTrue()
        }
    }

    // ── buildMdix DSL ─────────────────────────────────────────────────────────

    @Test fun `buildMdix creates readable Database`() {
        buildMdix {
            this["app"] = "TestApp"
            this["version"] = 3
            this["debug"] = false
        }.use { db ->
            assertThat(db.getString("app")).isEqualTo("TestApp")
            assertThat(db.getInt("version")).isEqualTo(3)
            assertThat(db.getBool("debug")).isFalse()
        }
    }

    @Test fun `buildMdix closes Builder automatically`() {
        var builderRef: Builder? = null
        buildMdix {
            builderRef = this
            this["x"] = 1
        }.use { }
        assertThatThrownBy { builderRef!!.setInt("y", 2) }
            .isInstanceOf(MdixException::class.java)
            .extracting { (it as MdixException).kind }
            .isEqualTo(MdixException.Kind.CLOSED)
    }

    // ── saveToFileAsync ───────────────────────────────────────────────────────

    @Test fun `saveToFileAsync writes file`() = runBlocking {
        val tmp = Files.createTempFile("mdix_kt_", ".mdix").toFile()
        tmp.deleteOnExit()
        DixScript.newBuilder().use { b ->
            b["saved"] = "yes"
            b.saveToFileAsync(tmp.absolutePath)
        }
        assertThat(tmp).exists()
        assertThat(tmp.length()).isGreaterThan(0)
    }

    // ── loadAsync / loadStrAsync ──────────────────────────────────────────────

    @Test fun `loadStrAsync loads correctly`() = runBlocking {
        loadStrAsync(src).use { db ->
            assertThat(db.getInt("port")).isEqualTo(8080)
        }
    }

    // ── safeLoad / safeLoadStr ────────────────────────────────────────────────

    @Test fun `safeLoadStr success returns Ok`() {
        val result = safeLoadStr(src)
        assertThat(result.isSuccess).isTrue()
        result.getOrThrow().close()
    }

    @Test fun `safeLoadStr failure returns Failure`() {
        val result = safeLoadStr("@@@INVALID$$$")
        assertThat(result.isFailure).isTrue()
        assertThat(result.exceptionOrNull()).isInstanceOf(MdixException::class.java)
    }

    @Test fun `safeLoad nonExistentFile returns Failure`() {
        val result = safeLoad("/absolutely/does/not/exist.mdix")
        assertThat(result.isFailure).isTrue()
    }

    // ── Converter extensions ──────────────────────────────────────────────────

    @Test fun `toJsonOrEmpty returns json on success`() {
        DixScript.loadStr(src).use { db ->
            val json = DixScript.convert().toJsonOrEmpty(db)
            assertThat(json).isNotBlank()
            assertThat(json).contains("8080")
        }
    }

    @Test fun `fromJsonOrNull returns Database on valid json`() {
        val db = DixScript.convert().fromJsonOrNull("{\"x\": 5}")
        assertThat(db).isNotNull
        db!!.use {
            assertThat(it.getInt("x")).isEqualTo(5)
        }
    }

    @Test fun `fromJsonOrNull returns null on invalid json`() {
        val db = DixScript.convert().fromJsonOrNull("not json")
        assertThat(db).isNull()
    }
}
