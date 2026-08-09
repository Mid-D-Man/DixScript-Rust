@file:JvmName("DixScriptKt")

package com.midmanstudio.dixscript

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
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

// ── Query extension functions ─────────────────────────────────────────────────

/** Kotlin-idiomatic alias for [MdixQuery.where_] — reads more naturally than a trailing underscore in Kotlin. */
fun MdixQuery.filter(predicate: (MdixValue) -> Boolean): MdixQuery = where_(predicate)

/** [Database.query] on the IO dispatcher (JSON parsing for a large array can be worth moving off the calling thread). */
suspend fun Database.queryAsync(path: String): MdixQuery =
    withContext(Dispatchers.IO) { query(path) }

/** [Database.queryMany] on the IO dispatcher. */
suspend fun Database.queryManyAsync(pattern: String): MdixQuery =
    withContext(Dispatchers.IO) { queryMany(pattern) }

// ── Merge extension functions ─────────────────────────────────────────────────

/**
 * Kotlin-idiomatic entry point for [Merge.sourcesWeighted] with default arguments,
 * since Java has no default parameters:
 *   val result = mergeSources(listOf(base, override), weights = doubleArrayOf(1.0, 0.5))
 */
fun mergeSources(
    sources: List<String>,
    weights: DoubleArray? = null,
    strategy: Merge.Strategy = Merge.Strategy.WEIGHTED_PRIORITY,
    arrayStrategy: Merge.ArrayStrategy = Merge.ArrayStrategy.REPLACE,
): Merge.Result = Merge.sourcesWeighted(sources, weights, strategy, arrayStrategy)

/** As [mergeSources], starting from already-loaded databases instead of source text. */
fun mergeDatabases(
    databases: List<Database>,
    weights: DoubleArray? = null,
    strategy: Merge.Strategy = Merge.Strategy.WEIGHTED_PRIORITY,
    arrayStrategy: Merge.ArrayStrategy = Merge.ArrayStrategy.REPLACE,
): Merge.Result = Merge.databasesWeighted(databases, weights, strategy, arrayStrategy)

/** [mergeSources] on the IO dispatcher. */
suspend fun mergeSourcesAsync(
    sources: List<String>,
    weights: DoubleArray? = null,
    strategy: Merge.Strategy = Merge.Strategy.WEIGHTED_PRIORITY,
    arrayStrategy: Merge.ArrayStrategy = Merge.ArrayStrategy.REPLACE,
): Merge.Result = withContext(Dispatchers.IO) { mergeSources(sources, weights, strategy, arrayStrategy) }

// ── Schema extension functions ────────────────────────────────────────────────

/** Kotlin DSL builder for [SchemaBuilder], mirroring [buildMdix]'s shape. */
inline fun buildSchema(block: SchemaBuilder.() -> Unit): SchemaBuilder =
    SchemaBuilder().apply(block)

/** Fluent alternative to [SchemaBuilder.validate]: `db.validate(schema)`. */
fun Database.validate(schema: SchemaBuilder): SchemaBuilder.Report = schema.validate(this)

// ── HotReload extension functions ─────────────────────────────────────────────

/** Kotlin DSL — runs [block] with a [HotReload] watcher, closing it automatically afterward. */
inline fun <T> watchMdix(path: String, block: HotReload.() -> T): T =
    HotReload(path).use { it.block() }

/**
 * Suspends, polling every [pollIntervalMs], until the watched file changes and reloads
 * successfully, then returns the fresh [Database]. The caller owns the returned Database.
 * Cancelling the calling coroutine stops the poll loop cleanly.
 */
suspend fun HotReload.awaitChange(pollIntervalMs: Long = 250L): Database {
    while (true) {
        val reloaded = checkAndReload()
        if (reloaded.isPresent) return reloaded.get()
        delay(pollIntervalMs)
    }
}
