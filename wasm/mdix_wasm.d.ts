/* tslint:disable */
/* eslint-disable */

/**
 * Programmatic .mdix builder for JavaScript callers.
 * Mirrors the C# MdixBuilder three-section structure with
 * full two-tier DATA ordering enforcement.
 *
 * ```js
 * const db = await new MdixBuilder()
 *   .setConfigVersion("1.0.0")
 *   .addEnum("LogLevel", JSON.stringify([["DEBUG",0],["INFO",1],["WARN",2]]))
 *   .withString("app_name", "MyGame")
 *   .withInt("port", 8080)
 *   .withBool("ssl", true)
 *   .withTableProperties("server", JSON.stringify({host:"localhost",port:8080}))
 *   .withGroupArray("tags", JSON.stringify(["alpha","beta"]))
 *   .toDatabase();
 * ```
 */
export class MdixBuilder {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Adds an enum definition to @ENUMS.
     *
     * `fields_json` must be either:
     *   - A JSON array of strings for auto-increment: `["DEBUG","INFO","WARN"]`
     *   - A JSON array of [name, value] pairs:  `[["DEBUG",0],["INFO",1]]`
     */
    addEnum(name: string, fields_json: string): MdixBuilder;
    free(): void;
    constructor();
    /**
     * Serializes all sections to a valid .mdix source string.
     */
    serialize(): string;
    /**
     * Sets any custom key in @CONFIG.
     */
    setConfig(key: string, value: string): MdixBuilder;
    /**
     * Sets the author field in @CONFIG.
     */
    setConfigAuthor(author: string): MdixBuilder;
    /**
     * Sets the debug_mode field in @CONFIG.
     * Valid values: "off", "regular", "verbose"
     */
    setConfigDebugMode(mode: string): MdixBuilder;
    /**
     * Sets the encoding field in @CONFIG.
     */
    setConfigEncoding(encoding: string): MdixBuilder;
    /**
     * Sets the version field in @CONFIG.
     */
    setConfigVersion(version: string): MdixBuilder;
    /**
     * Serializes and loads the result, returning a MdixDatabase.
     */
    toDatabase(): MdixDatabase;
    /**
     * Adds a homogeneous scalar array as a flat property.
     * `items_json` must be a JSON array of scalars: `[1,2,3]` or `["a","b"]`
     */
    withArray(path: string, items_json: string): MdixBuilder;
    /**
     * Adds a blob value. `base64` must be valid base64.
     */
    withBlob(path: string, base64: string): MdixBuilder;
    withBool(path: string, value: boolean): MdixBuilder;
    /**
     * Adds a date value. `date` must be in YYYY-MM-DD format.
     */
    withDate(path: string, date: string): MdixBuilder;
    withDouble(path: string, value: number): MdixBuilder;
    /**
     * Adds an enum reference as a flat property.
     * Example: `withEnumValue("log_level", "LogLevel", "INFO")`
     * Produces: `log_level = LogLevel.INFO`
     */
    withEnumValue(path: string, enum_name: string, field_name: string): MdixBuilder;
    withFloat(path: string, value: number): MdixBuilder;
    /**
     * Adds a group array (double-colon syntax).
     * Produces: `path:: item, item, item`
     *
     * `items_json` must be a JSON array of scalars or objects.
     *
     * Once this is called, no further flat properties may be added
     * (two-tier rule enforced).
     */
    withGroupArray(path: string, items_json: string): MdixBuilder;
    /**
     * Adds a hex color value. `hex` must start with `#`.
     */
    withHexColor(path: string, hex: string): MdixBuilder;
    withInt(path: string, value: number): MdixBuilder;
    /**
     * Adds a 64-bit integer value, explicitly typed as Long.
     *
     * Takes a JS `bigint`, not `number` — e.g. `withLong("id", 123n)`,
     * not `withLong("id", 123)`. wasm-bindgen will throw a TypeError if
     * you pass a plain number here. Values that overflow i32 are
     * auto-promoted to Long by the parser regardless of suffix, but a
     * small value (e.g. `5n`) would otherwise re-parse as Int — the `L`
     * suffix pins the type to Long no matter the magnitude, matching
     * DixScript's own `123L` literal syntax.
     */
    withLong(path: string, value: bigint): MdixBuilder;
    /**
     * Adds an inline object literal as a flat property.
     * `props_json` must be a flat JSON object: `{"host":"localhost","port":8080}`
     */
    withObject(path: string, props_json: string): MdixBuilder;
    /**
     * Adds a regex value.
     */
    withRegex(path: string, pattern: string): MdixBuilder;
    withString(path: string, value: string): MdixBuilder;
    /**
     * Adds a table property block (single-colon syntax).
     * Produces: `path: key = val, key = val`
     *
     * `props_json` must be a flat JSON object: `{"host":"localhost","port":8080}`
     *
     * Once this is called, no further flat properties may be added
     * (two-tier rule enforced).
     */
    withTableProperties(path: string, props_json: string): MdixBuilder;
    /**
     * Adds a timestamp value. `ts` must be ISO 8601.
     */
    withTimestamp(path: string, ts: string): MdixBuilder;
    /**
     * Adds a tuple (max 6 elements).
     * `items_json` must be a JSON array: `[1,"hello",true]`
     */
    withTuple(path: string, items_json: string): MdixBuilder;
    readonly isValid: boolean;
}

/**
 * A loaded DixScript database.
 *
 * Construct via `MdixDatabase.load_str()` or `MdixDatabase.from_json()`.
 * Call `free()` when done — the GC will also clean up but explicit
 * freeing is recommended in hot loops.
 */
export class MdixDatabase {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Returns true if the dotted path exists in the loaded data.
     */
    exists(path: string): boolean;
    /**
     * Explicitly free the database. Safe to call multiple times.
     */
    free(): void;
    /**
     * Load from a JSON object string.
     * The JSON must have an object at the top level.
     */
    static fromJson(json: string): MdixDatabase;
    /**
     * Load from a TOML string.
     */
    static fromToml(toml: string): MdixDatabase;
    getArrayLength(path: string): number;
    getBool(path: string): boolean;
    /**
     * Get a Double value, widened from Float/Int/Long if needed — all
     * three are always exact when promoted to f64 at the magnitudes
     * DixScript configs realistically use. This matches schema.rs's own
     * Double widening rule exactly, so it's left on the lenient
     * DixData::get::<T>() path rather than rewritten like get_int/
     * get_long/get_float above.
     */
    getDouble(path: string): number;
    getEnumField(path: string): string;
    getEnumName(path: string): string;
    /**
     * Get a Float value strictly — rejects Double, Int, and Long. Was
     * previously implemented via the lenient f64 path then narrowed to
     * f32, which silently accepted (and truncated) Int/Long/Double; that
     * defeats the point of having a typed getter at all.
     */
    getFloat(path: string): number;
    /**
     * Strict: only succeeds on an actual Int (or Enum ordinal) value —
     * does NOT silently coerce from Long/Float/Double. Matches the
     * widening rule dixscript's schema.rs type_matches() uses, not the
     * looser DixData::get::<T>() convenience used elsewhere in this
     * file, which would happily truncate a Float/Double into this with
     * no error.
     */
    getInt(path: string): number;
    /**
     * Returns the JSON serialization of the value at `path`.
     * Useful for arrays, objects, tuples, and blobs.
     */
    getJson(path: string): string;
    /**
     * Returns the direct child key names under `prefix`.
     * Pass an empty string for top-level keys.
     */
    getKeys(prefix: string): string[];
    /**
     * Get a 64-bit integer value. Accepts Long (exact) or Int (widened —
     * i32 -> i64 is always lossless). Rejects Float/Double: silently
     * truncating one into a Long is exactly the bug this guards against.
     * Returns a JS `bigint`, not `number` — JS numbers are f64 and lose
     * precision above 2^53, so this must be a bigint to carry the full
     * 64-bit range. Pass one in too: `db.getLong(...)` returns
     * `9223372036854775807n`-style values, and the matching
     * `MdixBuilder.withLong(path, value)` expects a bigint argument
     * (e.g. `withLong("id", 123n)`), not a plain `number`.
     */
    getLong(path: string): bigint;
    getString(path: string): string;
    /**
     * Returns the type discriminant string for the value at `path`.
     * Returns `"unknown"` if the path does not exist.
     */
    getValueType(path: string): string;
    /**
     * Load a DixScript database from a raw .mdix source string.
     */
    static loadStr(source: string): MdixDatabase;
    /**
     * Exports the entire database as a JSON string.
     */
    toJson(indented: boolean): string;
    /**
     * Re-serializes the database back to .mdix source text.
     */
    toMdix(): string;
    /**
     * Exports the entire database as a TOML string.
     */
    toToml(): string;
    /**
     * Total number of entries loaded.
     */
    readonly entryCount: number;
    /**
     * Returns true if the database is still valid (not freed).
     */
    readonly isValid: boolean;
}

/**
 * Returned by `mergeSources`, `mergeSourcesWeighted`, and
 * `MdixDatabase.mergeWith`. wasm-bindgen can't return a Rust tuple
 * directly, so this small wrapper carries both results instead.
 */
export class MdixMergeOutcome {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Conflicts as a real JS array of plain objects:
     * `{path, winningSource, winningLabel}`.
     */
    conflicts(): any;
    /**
     * Consumes and returns the merged database. Can only be called once —
     * like other consuming methods in this crate, calling it again raises
     * rather than silently returning something stale.
     */
    database(): MdixDatabase;
}

export class MdixSchema {
    free(): void;
    [Symbol.dispose](): void;
    constructor();
    optionalArray(path: string): MdixSchema;
    optionalBool(path: string): MdixSchema;
    optionalDouble(path: string): MdixSchema;
    optionalFloat(path: string): MdixSchema;
    optionalInt(path: string): MdixSchema;
    optionalLong(path: string): MdixSchema;
    optionalObject(path: string): MdixSchema;
    optionalString(path: string): MdixSchema;
    paths(): string[];
    requireArray(path: string): MdixSchema;
    requireBool(path: string): MdixSchema;
    requireDouble(path: string): MdixSchema;
    requireEnum(path: string): MdixSchema;
    requireFloat(path: string): MdixSchema;
    requireInt(path: string): MdixSchema;
    /**
     * Requires a 64-bit integer field. Also accepts Int values (an i32
     * widens into the i64 field with no precision loss).
     */
    requireLong(path: string): MdixSchema;
    requireObject(path: string): MdixSchema;
    requireString(path: string): MdixSchema;
    /**
     * Annotates the most recently added field with a description.
     */
    withDescription(description: string): MdixSchema;
    readonly fieldCount: number;
}

/**
 * Returned by `MdixDatabase.validateSchema`.
 */
export class MdixValidationReport {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * All errors as a real JS array of plain objects:
     * `{path, expected, actual, kind}` where kind is one of
     * "Missing" | "WrongType" | "InvalidValue". Built via
     * `js_sys::JSON::parse` over a hand-built JSON string rather than
     * requiring `ValidationError`/`ValidationErrorKind` to derive
     * `Serialize` in the core (they don't, and adding that derive purely
     * for this one binding's convenience isn't worth the core-wide change).
     */
    errors(): any;
    /**
     * Dotted paths that failed validation, in order.
     */
    failedPaths(): string[];
    /**
     * Human-readable multi-line summary. Mapped to `toString` so
     * `String(report)` / template-literal interpolation work naturally.
     */
    toString(): string;
    readonly errorCount: number;
    readonly isValid: boolean;
}

/**
 * Returned by `MdixWatcher.check()`. Two fields instead of a tuple
 * since wasm-bindgen can't return a Rust tuple directly — same pattern
 * as `MdixMergeOutcome` in merge.rs.
 */
export class MdixWatchOutcome {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Consumes and returns the freshly parsed database. Only valid when
     * `changed` is true — raises if called when nothing changed (there
     * is nothing to take in that case) or if called a second time.
     */
    database(): MdixDatabase;
    readonly changed: boolean;
}

export class MdixWatcher {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Compares `source` against the last content seen, by hash. If it
     * differs (or this is the first call), parses it and returns an
     * outcome with `changed = true` and a usable database. If it is
     * identical to last time, returns `changed = false` and does NOT
     * parse — call `.database()` only when `changed` is true.
     */
    check(source: string): MdixWatchOutcome;
    /**
     * Returns true if `source` differs from the last content seen by
     * `check()` (or unconditionally true if `check()` has never been
     * called). Does not parse or update any state — use this for a
     * cheap pre-check before doing anything more expensive than hashing.
     */
    hasChanged(source: string): boolean;
    constructor();
    /**
     * Forgets any previously seen content — the next `check()` call
     * will always report `changed = true`, regardless of whether the
     * content actually matches what was seen before this reset.
     */
    reset(): void;
}

export function init(): void;

/**
 * Merge two or more .mdix source strings.
 *
 * Sources are weighted in descending order: the first gets weight 1.0,
 * the last gets the lowest weight (only matters under "weighted" strategy).
 */
export function mergeSources(sources: string[], strategy?: string | null, array_strategy?: string | null): MdixMergeOutcome;

/**
 * Merge .mdix source strings with explicit per-source weights.
 * `entries` is a JS array of `[source, weight]` pairs.
 */
export function mergeSourcesWeighted(entries: any[], strategy?: string | null, array_strategy?: string | null): MdixMergeOutcome;

/**
 * Seed the cloud-import cache before compiling.
 *
 * `@IMPORTS(...)` cloud (http/https) URLs can't be fetched from inside a
 * wasm build — there's no way to do a real network request synchronously
 * from within the compile pipeline. This is the actual working path
 * instead: `fetch()` the URL yourself in JS (normal `async`/`await`, no
 * wasm involved), then call this with the URL and the text you got back,
 * *before* calling `loadStr()` on source that references it. The
 * synchronous resolver checks this cache first and will find it already
 * there — no network access happens inside wasm at all.
 *
 * Call once per URL. Cached in the browser's `localStorage` for the
 * current origin, so it also persists across page reloads — call it again
 * any time you want to force a re-fetch to be picked up (there's no
 * separate "evict one entry" call; `MdixDatabase` doesn't expose
 * `clear_cache`/`get_statistics` yet either — those exist on the Rust
 * side in `CloudFileCache` but aren't wired through to this binding).
 *
 * No-op on native targets — this function only exists in the wasm build.
 */
export function prefetchImport(url: string, content: string): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_mdixbuilder_free: (a: number, b: number) => void;
    readonly __wbg_mdixdatabase_free: (a: number, b: number) => void;
    readonly __wbg_mdixmergeoutcome_free: (a: number, b: number) => void;
    readonly __wbg_mdixschema_free: (a: number, b: number) => void;
    readonly __wbg_mdixvalidationreport_free: (a: number, b: number) => void;
    readonly __wbg_mdixwatcher_free: (a: number, b: number) => void;
    readonly __wbg_mdixwatchoutcome_free: (a: number, b: number) => void;
    readonly init: () => void;
    readonly mdixbuilder_addEnum: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_free: (a: number) => void;
    readonly mdixbuilder_isValid: (a: number) => number;
    readonly mdixbuilder_new: () => number;
    readonly mdixbuilder_serialize: (a: number) => [number, number, number, number];
    readonly mdixbuilder_setConfig: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_setConfigAuthor: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixbuilder_setConfigDebugMode: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixbuilder_setConfigEncoding: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixbuilder_setConfigVersion: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixbuilder_toDatabase: (a: number) => [number, number, number];
    readonly mdixbuilder_withArray: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_withBlob: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_withBool: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly mdixbuilder_withDate: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_withDouble: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly mdixbuilder_withEnumValue: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number];
    readonly mdixbuilder_withFloat: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly mdixbuilder_withGroupArray: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_withHexColor: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_withInt: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly mdixbuilder_withLong: (a: number, b: number, c: number, d: bigint) => [number, number, number];
    readonly mdixbuilder_withObject: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_withRegex: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_withString: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_withTableProperties: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixbuilder_withTuple: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly mdixdatabase_entryCount: (a: number) => [number, number, number];
    readonly mdixdatabase_exists: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixdatabase_free: (a: number) => void;
    readonly mdixdatabase_fromJson: (a: number, b: number) => [number, number, number];
    readonly mdixdatabase_fromToml: (a: number, b: number) => [number, number, number];
    readonly mdixdatabase_getArrayLength: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixdatabase_getBool: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixdatabase_getDouble: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixdatabase_getEnumField: (a: number, b: number, c: number) => [number, number, number, number];
    readonly mdixdatabase_getEnumName: (a: number, b: number, c: number) => [number, number, number, number];
    readonly mdixdatabase_getFloat: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixdatabase_getInt: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixdatabase_getJson: (a: number, b: number, c: number) => [number, number, number, number];
    readonly mdixdatabase_getKeys: (a: number, b: number, c: number) => [number, number, number, number];
    readonly mdixdatabase_getLong: (a: number, b: number, c: number) => [bigint, number, number];
    readonly mdixdatabase_getString: (a: number, b: number, c: number) => [number, number, number, number];
    readonly mdixdatabase_getValueType: (a: number, b: number, c: number) => [number, number, number, number];
    readonly mdixdatabase_isValid: (a: number) => number;
    readonly mdixdatabase_loadStr: (a: number, b: number) => [number, number, number];
    readonly mdixdatabase_toJson: (a: number, b: number) => [number, number, number, number];
    readonly mdixdatabase_toMdix: (a: number) => [number, number, number, number];
    readonly mdixdatabase_toToml: (a: number) => [number, number, number, number];
    readonly mdixmergeoutcome_conflicts: (a: number) => [number, number, number];
    readonly mdixmergeoutcome_database: (a: number) => [number, number, number];
    readonly mdixschema_fieldCount: (a: number) => number;
    readonly mdixschema_new: () => number;
    readonly mdixschema_optionalArray: (a: number, b: number, c: number) => number;
    readonly mdixschema_optionalBool: (a: number, b: number, c: number) => number;
    readonly mdixschema_optionalDouble: (a: number, b: number, c: number) => number;
    readonly mdixschema_optionalFloat: (a: number, b: number, c: number) => number;
    readonly mdixschema_optionalInt: (a: number, b: number, c: number) => number;
    readonly mdixschema_optionalLong: (a: number, b: number, c: number) => number;
    readonly mdixschema_optionalObject: (a: number, b: number, c: number) => number;
    readonly mdixschema_optionalString: (a: number, b: number, c: number) => number;
    readonly mdixschema_paths: (a: number) => [number, number];
    readonly mdixschema_requireArray: (a: number, b: number, c: number) => number;
    readonly mdixschema_requireBool: (a: number, b: number, c: number) => number;
    readonly mdixschema_requireDouble: (a: number, b: number, c: number) => number;
    readonly mdixschema_requireEnum: (a: number, b: number, c: number) => number;
    readonly mdixschema_requireFloat: (a: number, b: number, c: number) => number;
    readonly mdixschema_requireInt: (a: number, b: number, c: number) => number;
    readonly mdixschema_requireLong: (a: number, b: number, c: number) => number;
    readonly mdixschema_requireObject: (a: number, b: number, c: number) => number;
    readonly mdixschema_requireString: (a: number, b: number, c: number) => number;
    readonly mdixschema_withDescription: (a: number, b: number, c: number) => number;
    readonly mdixvalidationreport_errorCount: (a: number) => number;
    readonly mdixvalidationreport_errors: (a: number) => [number, number, number];
    readonly mdixvalidationreport_failedPaths: (a: number) => [number, number];
    readonly mdixvalidationreport_isValid: (a: number) => number;
    readonly mdixvalidationreport_toString: (a: number) => [number, number];
    readonly mdixwatcher_check: (a: number, b: number, c: number) => [number, number, number];
    readonly mdixwatcher_hasChanged: (a: number, b: number, c: number) => number;
    readonly mdixwatcher_new: () => number;
    readonly mdixwatcher_reset: (a: number) => void;
    readonly mdixwatchoutcome_changed: (a: number) => number;
    readonly mdixwatchoutcome_database: (a: number) => [number, number, number];
    readonly mergeSources: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly mergeSourcesWeighted: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
    readonly prefetchImport: (a: number, b: number, c: number, d: number) => void;
    readonly mdixbuilder_withTimestamp: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
