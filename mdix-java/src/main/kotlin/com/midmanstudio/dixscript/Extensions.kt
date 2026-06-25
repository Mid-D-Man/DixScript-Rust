@file:JvmName("DixScriptKt")

package com.midmanstudio.dixscript

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.time.Instant
import java.time.LocalDate

// ── Database extension functions ──────────────────────────────────────────────

/**
 * Kotlin inline operator for dotted-path access — syntactic sugar:
 *   val port: Int = db["server.port"]
 *
 * Type is inferred via a reified generic.
 */
@Suppress("UNCHECKED_CAST")
inline operator fun <reified T> Database.get(path: String): T = when (T::class) {
    String::class  -> getString(path) as T
    Int::class     -> getInt(path) as T
    Long::class    -> getLong(path) as T
    Float::class   -> getFloat(path) as T
    Double::class  -> getDouble(path) as T
    Boolean::class -> getBool(path) as T
    else -> throw MdixException("get<${T::class.simpleName}>: unsupported type at '$path'")
}

/** Nullable variant — returns null instead of throwing when path is absent. */
@Suppress("UNCHECKED_CAST")
inline fun <reified T> Database.getOrNull(path: String): T? =
    if (!exists(path)) null else get(path)

/** Returns the string at [path], or [default] if absent. */
fun Database.getStringOrDefault(path: String, default: String = ""): String =
    getString(path, default)

/** Returns the int at [path], or [default] if absent. */
fun Database.getIntOrDefault(path: String, default: Int = 0): Int =
    getInt(path, default)

/** Returns the bool at [path], or [default] if absent. */
fun Database.getBoolOrDefault(path: String, default: Boolean = false): Boolean =
    getBool(path, default)

/** Returns the double at [path], or [default] if absent. */
fun Database.getDoubleOrDefault(path: String, default: Double = 0.0): Double =
    getDouble(path, default)

// ── Builder extension functions ───────────────────────────────────────────────

/** Kotlin DSL builder — inline lambda for fluent construction. */
fun buildMdix(block: Builder.() -> Unit): Database {
    return Builder().use { builder ->
        builder.block()
        builder.toDatabase()
    }
}

/** Fluent operator: builder["path"] = value */
operator fun Builder.set(path: String, value: String)  = setString(path, value)
operator fun Builder.set(path: String, value: Int)     = setInt(path, value)
operator fun Builder.set(path: String, value: Long)    = setLong(path, value)
operator fun Builder.set(path: String, value: Float)   = setFloat(path, value)
operator fun Builder.set(path: String, value: Double)  = setDouble(path, value)
operator fun Builder.set(path: String, value: Boolean) = setBool(path, value)
operator fun Builder.set(path: String, value: LocalDate)  = setDate(path, value)
operator fun Builder.set(path: String, value: Instant)    = setTimestamp(path, value)

// ── Coroutine support ─────────────────────────────────────────────────────────

/** Loads a .mdix file on the IO dispatcher. */
suspend fun loadAsync(path: String): Database =
    withContext(Dispatchers.IO) { DixScript.load(path) }

/** Loads .mdix source on the IO dispatcher. */
suspend fun loadStrAsync(source: String): Database =
    withContext(Dispatchers.IO) { DixScript.loadStr(source) }

/** Loads an encrypted file on the IO dispatcher. */
suspend fun loadEncryptedAsync(encPath: String, keyPath: String? = null): Database =
    withContext(Dispatchers.IO) { DixScript.loadEncrypted(encPath, keyPath) }

/** Saves a [Builder] to disk on the IO dispatcher. */
suspend fun Builder.saveToFileAsync(path: String) =
    withContext(Dispatchers.IO) { saveToFile(path) }

// ── Converter extension functions ─────────────────────────────────────────────

/** Exports [db] to JSON. Returns empty string on failure rather than throwing. */
fun Converter.toJsonOrEmpty(db: Database, indented: Boolean = true): String =
    runCatching { toJson(db, indented) }.getOrDefault("")

/** Parses JSON or returns null on failure. Caller must close the result. */
fun Converter.fromJsonOrNull(json: String): Database? =
    runCatching { fromJson(json) }.getOrNull()

// ── Result-style helpers ──────────────────────────────────────────────────────

/** Wraps a load call in a [Result], avoiding thrown exceptions. */
fun safeLoad(path: String): Result<Database> =
    runCatching { DixScript.load(path) }

fun safeLoadStr(source: String): Result<Database> =
    runCatching { DixScript.loadStr(source) }
