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
    getDouble(path: string): number;
    getEnumField(path: string): string;
    getEnumName(path: string): string;
    getFloat(path: string): number;
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

export function init(): void;
