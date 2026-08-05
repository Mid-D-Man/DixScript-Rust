/* @ts-self-types="./mdix_wasm.d.ts" */

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
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(MdixBuilder.prototype);
        obj.__wbg_ptr = ptr;
        MdixBuilderFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MdixBuilderFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mdixbuilder_free(ptr, 0);
    }
    /**
     * Adds an enum definition to @ENUMS.
     *
     * `fields_json` must be either:
     *   - A JSON array of strings for auto-increment: `["DEBUG","INFO","WARN"]`
     *   - A JSON array of [name, value] pairs:  `[["DEBUG",0],["INFO",1]]`
     * @param {string} name
     * @param {string} fields_json
     * @returns {MdixBuilder}
     */
    addEnum(name, fields_json) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(fields_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_addEnum(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    free() {
        wasm.mdixbuilder_free(this.__wbg_ptr);
    }
    /**
     * @returns {boolean}
     */
    get isValid() {
        const ret = wasm.mdixbuilder_isValid(this.__wbg_ptr);
        return ret !== 0;
    }
    constructor() {
        const ret = wasm.mdixbuilder_new();
        this.__wbg_ptr = ret >>> 0;
        MdixBuilderFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Serializes all sections to a valid .mdix source string.
     * @returns {string}
     */
    serialize() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.mdixbuilder_serialize(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Sets any custom key in @CONFIG.
     * @param {string} key
     * @param {string} value
     * @returns {MdixBuilder}
     */
    setConfig(key, value) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(key, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(value, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_setConfig(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Sets the author field in @CONFIG.
     * @param {string} author
     * @returns {MdixBuilder}
     */
    setConfigAuthor(author) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(author, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_setConfigAuthor(ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Sets the debug_mode field in @CONFIG.
     * Valid values: "off", "regular", "verbose"
     * @param {string} mode
     * @returns {MdixBuilder}
     */
    setConfigDebugMode(mode) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(mode, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_setConfigDebugMode(ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Sets the encoding field in @CONFIG.
     * @param {string} encoding
     * @returns {MdixBuilder}
     */
    setConfigEncoding(encoding) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(encoding, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_setConfigEncoding(ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Sets the version field in @CONFIG.
     * @param {string} version
     * @returns {MdixBuilder}
     */
    setConfigVersion(version) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(version, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_setConfigVersion(ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Serializes and loads the result, returning a MdixDatabase.
     * @returns {MdixDatabase}
     */
    toDatabase() {
        const ret = wasm.mdixbuilder_toDatabase(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixDatabase.__wrap(ret[0]);
    }
    /**
     * Adds a homogeneous scalar array as a flat property.
     * `items_json` must be a JSON array of scalars: `[1,2,3]` or `["a","b"]`
     * @param {string} path
     * @param {string} items_json
     * @returns {MdixBuilder}
     */
    withArray(path, items_json) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(items_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withArray(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds a blob value. `base64` must be valid base64.
     * @param {string} path
     * @param {string} base64
     * @returns {MdixBuilder}
     */
    withBlob(path, base64) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(base64, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withBlob(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * @param {string} path
     * @param {boolean} value
     * @returns {MdixBuilder}
     */
    withBool(path, value) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withBool(ptr, ptr0, len0, value);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds a date value. `date` must be in YYYY-MM-DD format.
     * @param {string} path
     * @param {string} date
     * @returns {MdixBuilder}
     */
    withDate(path, date) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(date, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withDate(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * @param {string} path
     * @param {number} value
     * @returns {MdixBuilder}
     */
    withDouble(path, value) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withDouble(ptr, ptr0, len0, value);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds an enum reference as a flat property.
     * Example: `withEnumValue("log_level", "LogLevel", "INFO")`
     * Produces: `log_level = LogLevel.INFO`
     * @param {string} path
     * @param {string} enum_name
     * @param {string} field_name
     * @returns {MdixBuilder}
     */
    withEnumValue(path, enum_name, field_name) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(enum_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(field_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len2 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withEnumValue(ptr, ptr0, len0, ptr1, len1, ptr2, len2);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * @param {string} path
     * @param {number} value
     * @returns {MdixBuilder}
     */
    withFloat(path, value) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withFloat(ptr, ptr0, len0, value);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds a group array (double-colon syntax).
     * Produces: `path:: item, item, item`
     *
     * `items_json` must be a JSON array of scalars or objects.
     *
     * Once this is called, no further flat properties may be added
     * (two-tier rule enforced).
     * @param {string} path
     * @param {string} items_json
     * @returns {MdixBuilder}
     */
    withGroupArray(path, items_json) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(items_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withGroupArray(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds a hex color value. `hex` must start with `#`.
     * @param {string} path
     * @param {string} hex
     * @returns {MdixBuilder}
     */
    withHexColor(path, hex) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(hex, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withHexColor(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * @param {string} path
     * @param {number} value
     * @returns {MdixBuilder}
     */
    withInt(path, value) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withInt(ptr, ptr0, len0, value);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
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
     * @param {string} path
     * @param {bigint} value
     * @returns {MdixBuilder}
     */
    withLong(path, value) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withLong(ptr, ptr0, len0, value);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds an inline object literal as a flat property.
     * `props_json` must be a flat JSON object: `{"host":"localhost","port":8080}`
     * @param {string} path
     * @param {string} props_json
     * @returns {MdixBuilder}
     */
    withObject(path, props_json) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(props_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withObject(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds a regex value.
     * @param {string} path
     * @param {string} pattern
     * @returns {MdixBuilder}
     */
    withRegex(path, pattern) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(pattern, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withRegex(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * @param {string} path
     * @param {string} value
     * @returns {MdixBuilder}
     */
    withString(path, value) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(value, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withString(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds a table property block (single-colon syntax).
     * Produces: `path: key = val, key = val`
     *
     * `props_json` must be a flat JSON object: `{"host":"localhost","port":8080}`
     *
     * Once this is called, no further flat properties may be added
     * (two-tier rule enforced).
     * @param {string} path
     * @param {string} props_json
     * @returns {MdixBuilder}
     */
    withTableProperties(path, props_json) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(props_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withTableProperties(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds a timestamp value. `ts` must be ISO 8601.
     * @param {string} path
     * @param {string} ts
     * @returns {MdixBuilder}
     */
    withTimestamp(path, ts) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(ts, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withTimestamp(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
    /**
     * Adds a tuple (max 6 elements).
     * `items_json` must be a JSON array: `[1,"hello",true]`
     * @param {string} path
     * @param {string} items_json
     * @returns {MdixBuilder}
     */
    withTuple(path, items_json) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(items_json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixbuilder_withTuple(ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixBuilder.__wrap(ret[0]);
    }
}
if (Symbol.dispose) MdixBuilder.prototype[Symbol.dispose] = MdixBuilder.prototype.free;

/**
 * A loaded DixScript database.
 *
 * Construct via `MdixDatabase.load_str()` or `MdixDatabase.from_json()`.
 * Call `free()` when done — the GC will also clean up but explicit
 * freeing is recommended in hot loops.
 */
export class MdixDatabase {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(MdixDatabase.prototype);
        obj.__wbg_ptr = ptr;
        MdixDatabaseFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MdixDatabaseFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mdixdatabase_free(ptr, 0);
    }
    /**
     * Total number of entries loaded.
     * @returns {number}
     */
    get entryCount() {
        const ret = wasm.mdixdatabase_entryCount(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * Returns true if the dotted path exists in the loaded data.
     * @param {string} path
     * @returns {boolean}
     */
    exists(path) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_exists(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] !== 0;
    }
    /**
     * Explicitly free the database. Safe to call multiple times.
     */
    free() {
        wasm.mdixdatabase_free(this.__wbg_ptr);
    }
    /**
     * Load from a JSON object string.
     * The JSON must have an object at the top level.
     * @param {string} json
     * @returns {MdixDatabase}
     */
    static fromJson(json) {
        const ptr0 = passStringToWasm0(json, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_fromJson(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixDatabase.__wrap(ret[0]);
    }
    /**
     * Load from a TOML string.
     * @param {string} toml
     * @returns {MdixDatabase}
     */
    static fromToml(toml) {
        const ptr0 = passStringToWasm0(toml, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_fromToml(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixDatabase.__wrap(ret[0]);
    }
    /**
     * @param {string} path
     * @returns {number}
     */
    getArrayLength(path) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_getArrayLength(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * @param {string} path
     * @returns {boolean}
     */
    getBool(path) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_getBool(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0] !== 0;
    }
    /**
     * Get a Double value, widened from Float/Int/Long if needed — all
     * three are always exact when promoted to f64 at the magnitudes
     * DixScript configs realistically use. This matches schema.rs's own
     * Double widening rule exactly, so it's left on the lenient
     * DixData::get::<T>() path rather than rewritten like get_int/
     * get_long/get_float above.
     * @param {string} path
     * @returns {number}
     */
    getDouble(path) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_getDouble(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * @param {string} path
     * @returns {string}
     */
    getEnumField(path) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.mdixdatabase_getEnumField(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * @param {string} path
     * @returns {string}
     */
    getEnumName(path) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.mdixdatabase_getEnumName(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * Get a Float value strictly — rejects Double, Int, and Long. Was
     * previously implemented via the lenient f64 path then narrowed to
     * f32, which silently accepted (and truncated) Int/Long/Double; that
     * defeats the point of having a typed getter at all.
     * @param {string} path
     * @returns {number}
     */
    getFloat(path) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_getFloat(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * Strict: only succeeds on an actual Int (or Enum ordinal) value —
     * does NOT silently coerce from Long/Float/Double. Matches the
     * widening rule dixscript's schema.rs type_matches() uses, not the
     * looser DixData::get::<T>() convenience used elsewhere in this
     * file, which would happily truncate a Float/Double into this with
     * no error.
     * @param {string} path
     * @returns {number}
     */
    getInt(path) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_getInt(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * Returns the JSON serialization of the value at `path`.
     * Useful for arrays, objects, tuples, and blobs.
     * @param {string} path
     * @returns {string}
     */
    getJson(path) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.mdixdatabase_getJson(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * Returns the direct child key names under `prefix`.
     * Pass an empty string for top-level keys.
     * @param {string} prefix
     * @returns {string[]}
     */
    getKeys(prefix) {
        const ptr0 = passStringToWasm0(prefix, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_getKeys(this.__wbg_ptr, ptr0, len0);
        if (ret[3]) {
            throw takeFromExternrefTable0(ret[2]);
        }
        var v2 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v2;
    }
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
     * @param {string} path
     * @returns {bigint}
     */
    getLong(path) {
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_getLong(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return ret[0];
    }
    /**
     * @param {string} path
     * @returns {string}
     */
    getString(path) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.mdixdatabase_getString(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * Returns the type discriminant string for the value at `path`.
     * Returns `"unknown"` if the path does not exist.
     * @param {string} path
     * @returns {string}
     */
    getValueType(path) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.mdixdatabase_getValueType(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * Returns true if the database is still valid (not freed).
     * @returns {boolean}
     */
    get isValid() {
        const ret = wasm.mdixdatabase_isValid(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Load a DixScript database from a raw .mdix source string.
     * @param {string} source
     * @returns {MdixDatabase}
     */
    static loadStr(source) {
        const ptr0 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_loadStr(ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixDatabase.__wrap(ret[0]);
    }
    /**
     * Merges this database with `other` and returns a fresh
     * `MdixMergeOutcome` — leaves both original databases untouched.
     *
     * `strategy`: "weighted" (default) | "primary_wins" | "secondary_wins"
     * | "throw_on_conflict". `array_strategy`: "concat_dedup" (default) |
     * "replace" | "concat". `this` merges as the primary source
     * (weight 1.0), `other` as secondary (weight 0.5).
     *
     * NOTE: this method previously didn't exist as a real binding despite
     * being documented at the top of merge.rs — `crate::merge::merge_with`
     * was a plain Rust free function, never wired into a #[wasm_bindgen]
     * impl block or re-exported from lib.rs, so `MdixDatabase.mergeWith`
     * was unreachable from JS entirely. This is that wiring.
     * @param {MdixDatabase} other
     * @param {string | null} [strategy]
     * @param {string | null} [array_strategy]
     * @returns {MdixMergeOutcome}
     */
    mergeWith(other, strategy, array_strategy) {
        _assertClass(other, MdixDatabase);
        var ptr0 = isLikeNone(strategy) ? 0 : passStringToWasm0(strategy, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len0 = WASM_VECTOR_LEN;
        var ptr1 = isLikeNone(array_strategy) ? 0 : passStringToWasm0(array_strategy, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len1 = WASM_VECTOR_LEN;
        const ret = wasm.mdixdatabase_mergeWith(this.__wbg_ptr, other.__wbg_ptr, ptr0, len0, ptr1, len1);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixMergeOutcome.__wrap(ret[0]);
    }
    /**
     * Query the array at `path` and return its elements as a JSON array
     * string. `path` itself must be a plain `Array` value, or a
     * `GroupArray`'s own path (the flattener already stores a
     * `GroupArray`'s items as a real Array there) -- for gathering across
     * multiple *sibling* paths instead, use `query_many`.
     *
     * Returns `"[]"` for a path that doesn't exist or isn't an Array,
     * rather than erroring -- mirrors `DixData::query`, which returns
     * `None` for exactly the same two cases: "no matching data" is a
     * normal, common outcome for a query, not a caller mistake worth
     * throwing over the way e.g. `getString` on the wrong type is.
     * @param {string} path
     * @returns {string}
     */
    query(path) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.mdixdatabase_query(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * Query across every sibling path matched by a glob `pattern` (same
     * whole-segment `*` syntax as the core's `select_many` -- see
     * query.rs's module doc for exactly what that does and doesn't
     * match), gathered into one JSON array string. For a single Array or
     * GroupArray path's own items, use `query` instead.
     *
     * Always returns a JSON array (possibly `"[]"`) -- `DixData::
     * query_many` has no `None` case, an empty match set is just an
     * empty `DixQuery`.
     * @param {string} pattern
     * @returns {string}
     */
    queryMany(pattern) {
        let deferred3_0;
        let deferred3_1;
        try {
            const ptr0 = passStringToWasm0(pattern, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len0 = WASM_VECTOR_LEN;
            const ret = wasm.mdixdatabase_queryMany(this.__wbg_ptr, ptr0, len0);
            var ptr2 = ret[0];
            var len2 = ret[1];
            if (ret[3]) {
                ptr2 = 0; len2 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred3_0 = ptr2;
            deferred3_1 = len2;
            return getStringFromWasm0(ptr2, len2);
        } finally {
            wasm.__wbindgen_free(deferred3_0, deferred3_1, 1);
        }
    }
    /**
     * Exports the entire database as a JSON string.
     * @param {boolean} indented
     * @returns {string}
     */
    toJson(indented) {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.mdixdatabase_toJson(this.__wbg_ptr, indented);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Re-serializes the database back to .mdix source text.
     * @returns {string}
     */
    toMdix() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.mdixdatabase_toMdix(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Exports the entire database as a TOML string.
     * @returns {string}
     */
    toToml() {
        let deferred2_0;
        let deferred2_1;
        try {
            const ret = wasm.mdixdatabase_toToml(this.__wbg_ptr);
            var ptr1 = ret[0];
            var len1 = ret[1];
            if (ret[3]) {
                ptr1 = 0; len1 = 0;
                throw takeFromExternrefTable0(ret[2]);
            }
            deferred2_0 = ptr1;
            deferred2_1 = len1;
            return getStringFromWasm0(ptr1, len1);
        } finally {
            wasm.__wbindgen_free(deferred2_0, deferred2_1, 1);
        }
    }
    /**
     * Validates this database against `schema` and returns a
     * `MdixValidationReport`.
     *
     * `schema` is borrowed, not consumed — the same `MdixSchema` instance
     * can validate multiple databases (mirrors the underlying
     * `SchemaBuilder::validate(&self, ..)` in dixscript core, and the same
     * pattern mdix-python's `MdixDatabase.validate_schema` already uses).
     *
     * NOTE: this method was documented at the top of schema.rs
     * (`db.validateSchema(schema)`) but was never actually wired up here —
     * `MdixSchema` and `MdixValidationReport` existed with no way to reach
     * them from `MdixDatabase` at all. Same shape of bug as `mergeWith`
     * above (see the regression test docs on that one); this is that
     * wiring for schema.rs.
     * @param {MdixSchema} schema
     * @returns {MdixValidationReport}
     */
    validateSchema(schema) {
        _assertClass(schema, MdixSchema);
        const ret = wasm.mdixdatabase_validateSchema(this.__wbg_ptr, schema.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixValidationReport.__wrap(ret[0]);
    }
}
if (Symbol.dispose) MdixDatabase.prototype[Symbol.dispose] = MdixDatabase.prototype.free;

export class MdixDlmOutcome {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(MdixDlmOutcome.prototype);
        obj.__wbg_ptr = ptr;
        MdixDlmOutcomeFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MdixDlmOutcomeFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mdixdlmoutcome_free(ptr, 0);
    }
    /**
     * @returns {string[]}
     */
    errors() {
        const ret = wasm.mdixdlmoutcome_errors(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * Which DLM modules actually ran, e.g. `["DCompressor.xz",
     * "DEncryptor.aes256"]` — empty when `source` had no `@DLM` section.
     * @returns {string[]}
     */
    executedModules() {
        const ret = wasm.mdixdlmoutcome_executedModules(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {boolean}
     */
    isSuccess() {
        const ret = wasm.mdixdlmoutcome_isSuccess(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * The `.mdix.key` file's content as a plain string, ready to hand
     * straight to `decompileWithDlm`. `undefined` when `source` had no
     * `@DLM` modules to apply (nothing to decrypt on the way back
     * either — see the module doc comment above).
     * @returns {string | undefined}
     */
    keyFileContent() {
        const ret = wasm.mdixdlmoutcome_keyFileContent(this.__wbg_ptr);
        let v1;
        if (ret[0] !== 0) {
            v1 = getStringFromWasm0(ret[0], ret[1]).slice();
            wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        }
        return v1;
    }
    /**
     * The compressed/encrypted (or, with no `@DLM` modules, plain
     * binary-packed) bytes — always populated in memory regardless of
     * whether any on-disk artifact could be written (never possible on
     * wasm32 in the first place).
     * @returns {Uint8Array}
     */
    processedData() {
        const ret = wasm.mdixdlmoutcome_processedData(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * @returns {string[]}
     */
    warnings() {
        const ret = wasm.mdixdlmoutcome_warnings(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
}
if (Symbol.dispose) MdixDlmOutcome.prototype[Symbol.dispose] = MdixDlmOutcome.prototype.free;

/**
 * Returned by `mergeSources`, `mergeSourcesWeighted`, and
 * `MdixDatabase.mergeWith`. wasm-bindgen can't return a Rust tuple
 * directly, so this small wrapper carries both results instead.
 */
export class MdixMergeOutcome {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(MdixMergeOutcome.prototype);
        obj.__wbg_ptr = ptr;
        MdixMergeOutcomeFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MdixMergeOutcomeFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mdixmergeoutcome_free(ptr, 0);
    }
    /**
     * Conflicts as a real JS array of plain objects:
     * `{path, winningSource, winningLabel}`.
     * @returns {any}
     */
    conflicts() {
        const ret = wasm.mdixmergeoutcome_conflicts(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Consumes and returns the merged database. Can only be called once —
     * like other consuming methods in this crate, calling it again raises
     * rather than silently returning something stale.
     * @returns {MdixDatabase}
     */
    database() {
        const ret = wasm.mdixmergeoutcome_database(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixDatabase.__wrap(ret[0]);
    }
}
if (Symbol.dispose) MdixMergeOutcome.prototype[Symbol.dispose] = MdixMergeOutcome.prototype.free;

export class MdixSchema {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(MdixSchema.prototype);
        obj.__wbg_ptr = ptr;
        MdixSchemaFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MdixSchemaFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mdixschema_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get fieldCount() {
        const ret = wasm.mdixschema_fieldCount(this.__wbg_ptr);
        return ret;
    }
    constructor() {
        const ret = wasm.mdixschema_new();
        this.__wbg_ptr = ret >>> 0;
        MdixSchemaFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    optionalArray(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_optionalArray(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    optionalBool(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_optionalBool(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    optionalDouble(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_optionalDouble(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    optionalFloat(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_optionalFloat(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    optionalInt(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_optionalInt(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    optionalLong(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_optionalLong(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    optionalObject(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_optionalObject(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    optionalString(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_optionalString(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @returns {string[]}
     */
    paths() {
        const ret = wasm.mdixschema_paths(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    requireArray(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_requireArray(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    requireBool(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_requireBool(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    requireDouble(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_requireDouble(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    requireEnum(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_requireEnum(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    requireFloat(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_requireFloat(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    requireInt(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_requireInt(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * Requires a 64-bit integer field. Also accepts Int values (an i32
     * widens into the i64 field with no precision loss).
     * @param {string} path
     * @returns {MdixSchema}
     */
    requireLong(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_requireLong(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    requireObject(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_requireObject(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * @param {string} path
     * @returns {MdixSchema}
     */
    requireString(path) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(path, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_requireString(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
    /**
     * Annotates the most recently added field with a description.
     * @param {string} description
     * @returns {MdixSchema}
     */
    withDescription(description) {
        const ptr = this.__destroy_into_raw();
        const ptr0 = passStringToWasm0(description, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixschema_withDescription(ptr, ptr0, len0);
        return MdixSchema.__wrap(ret);
    }
}
if (Symbol.dispose) MdixSchema.prototype[Symbol.dispose] = MdixSchema.prototype.free;

/**
 * Returned by `MdixDatabase.validateSchema`.
 */
export class MdixValidationReport {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(MdixValidationReport.prototype);
        obj.__wbg_ptr = ptr;
        MdixValidationReportFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MdixValidationReportFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mdixvalidationreport_free(ptr, 0);
    }
    /**
     * @returns {number}
     */
    get errorCount() {
        const ret = wasm.mdixvalidationreport_errorCount(this.__wbg_ptr);
        return ret;
    }
    /**
     * All errors as a real JS array of plain objects:
     * `{path, expected, actual, kind}` where kind is one of
     * "Missing" | "WrongType" | "InvalidValue". Built via
     * `js_sys::JSON::parse` over a hand-built JSON string rather than
     * requiring `ValidationError`/`ValidationErrorKind` to derive
     * `Serialize` in the core (they don't, and adding that derive purely
     * for this one binding's convenience isn't worth the core-wide change).
     * @returns {any}
     */
    errors() {
        const ret = wasm.mdixvalidationreport_errors(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return takeFromExternrefTable0(ret[0]);
    }
    /**
     * Dotted paths that failed validation, in order.
     * @returns {string[]}
     */
    failedPaths() {
        const ret = wasm.mdixvalidationreport_failedPaths(this.__wbg_ptr);
        var v1 = getArrayJsValueFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 4, 4);
        return v1;
    }
    /**
     * @returns {boolean}
     */
    get isValid() {
        const ret = wasm.mdixvalidationreport_isValid(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Human-readable multi-line summary. Mapped to `toString` so
     * `String(report)` / template-literal interpolation work naturally.
     * @returns {string}
     */
    toString() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.mdixvalidationreport_toString(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
}
if (Symbol.dispose) MdixValidationReport.prototype[Symbol.dispose] = MdixValidationReport.prototype.free;

/**
 * Returned by `MdixWatcher.check()`. Two fields instead of a tuple
 * since wasm-bindgen can't return a Rust tuple directly — same pattern
 * as `MdixMergeOutcome` in merge.rs.
 */
export class MdixWatchOutcome {
    static __wrap(ptr) {
        ptr = ptr >>> 0;
        const obj = Object.create(MdixWatchOutcome.prototype);
        obj.__wbg_ptr = ptr;
        MdixWatchOutcomeFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MdixWatchOutcomeFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mdixwatchoutcome_free(ptr, 0);
    }
    /**
     * @returns {boolean}
     */
    get changed() {
        const ret = wasm.mdixwatchoutcome_changed(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Consumes and returns the freshly parsed database. Only valid when
     * `changed` is true — raises if called when nothing changed (there
     * is nothing to take in that case) or if called a second time.
     * @returns {MdixDatabase}
     */
    database() {
        const ret = wasm.mdixwatchoutcome_database(this.__wbg_ptr);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixDatabase.__wrap(ret[0]);
    }
}
if (Symbol.dispose) MdixWatchOutcome.prototype[Symbol.dispose] = MdixWatchOutcome.prototype.free;

export class MdixWatcher {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        MdixWatcherFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_mdixwatcher_free(ptr, 0);
    }
    /**
     * Compares `source` against the last content seen, by hash. If it
     * differs (or this is the first call), parses it and returns an
     * outcome with `changed = true` and a usable database. If it is
     * identical to last time, returns `changed = false` and does NOT
     * parse — call `.database()` only when `changed` is true.
     * @param {string} source
     * @returns {MdixWatchOutcome}
     */
    check(source) {
        const ptr0 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixwatcher_check(this.__wbg_ptr, ptr0, len0);
        if (ret[2]) {
            throw takeFromExternrefTable0(ret[1]);
        }
        return MdixWatchOutcome.__wrap(ret[0]);
    }
    /**
     * Returns true if `source` differs from the last content seen by
     * `check()` (or unconditionally true if `check()` has never been
     * called). Does not parse or update any state — use this for a
     * cheap pre-check before doing anything more expensive than hashing.
     * @param {string} source
     * @returns {boolean}
     */
    hasChanged(source) {
        const ptr0 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.mdixwatcher_hasChanged(this.__wbg_ptr, ptr0, len0);
        return ret !== 0;
    }
    constructor() {
        const ret = wasm.mdixwatcher_new();
        this.__wbg_ptr = ret >>> 0;
        MdixWatcherFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Forgets any previously seen content — the next `check()` call
     * will always report `changed = true`, regardless of whether the
     * content actually matches what was seen before this reset.
     */
    reset() {
        wasm.mdixwatcher_reset(this.__wbg_ptr);
    }
}
if (Symbol.dispose) MdixWatcher.prototype[Symbol.dispose] = MdixWatcher.prototype.free;

/**
 * Compiles `source` and, if it declares an `@DLM(DCompressor...
 * DEncryptor...)` section, runs compression/encryption on the result —
 * entirely in memory. `sourceLabel` is just an identifier used for
 * error messages and (if `@DLM` includes `DAuditor`) as the audit
 * trail's localStorage key — it doesn't need to be a real file name,
 * though using one consistently is what makes the audit trail track a
 * given config's history across compiles.
 * @param {string} source
 * @param {string} source_label
 * @returns {MdixDlmOutcome}
 */
export function compileWithDlm(source, source_label) {
    const ptr0 = passStringToWasm0(source, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(source_label, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.compileWithDlm(ptr0, len0, ptr1, len1);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return MdixDlmOutcome.__wrap(ret[0]);
}

/**
 * Reverse of `compileWithDlm`: takes the bytes from `processedData()`
 * and the string from `keyFileContent()` and returns a normal
 * `MdixDatabase`, exactly as if you'd `loadStr()`'d the original source.
 *
 * Pass `""` for `keyFileContent` when the original `compileWithDlm` call
 * returned `undefined` for it (source had no `@DLM` modules) — this then
 * unpacks `data` directly rather than attempting decryption.
 * @param {Uint8Array} data
 * @param {string} key_file_content
 * @param {string} source_label
 * @returns {MdixDatabase}
 */
export function decompileWithDlm(data, key_file_content, source_label) {
    const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(key_file_content, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ptr2 = passStringToWasm0(source_label, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len2 = WASM_VECTOR_LEN;
    const ret = wasm.decompileWithDlm(ptr0, len0, ptr1, len1, ptr2, len2);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return MdixDatabase.__wrap(ret[0]);
}

export function init() {
    wasm.init();
}

/**
 * Merge two or more .mdix source strings.
 *
 * Sources are weighted in descending order: the first gets weight 1.0,
 * the last gets the lowest weight (only matters under "weighted" strategy).
 * @param {string[]} sources
 * @param {string | null} [strategy]
 * @param {string | null} [array_strategy]
 * @returns {MdixMergeOutcome}
 */
export function mergeSources(sources, strategy, array_strategy) {
    const ptr0 = passArrayJsValueToWasm0(sources, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    var ptr1 = isLikeNone(strategy) ? 0 : passStringToWasm0(strategy, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len1 = WASM_VECTOR_LEN;
    var ptr2 = isLikeNone(array_strategy) ? 0 : passStringToWasm0(array_strategy, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len2 = WASM_VECTOR_LEN;
    const ret = wasm.mergeSources(ptr0, len0, ptr1, len1, ptr2, len2);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return MdixMergeOutcome.__wrap(ret[0]);
}

/**
 * Merge .mdix source strings with explicit per-source weights.
 * `entries` is a JS array of `[source, weight]` pairs.
 * @param {any[]} entries
 * @param {string | null} [strategy]
 * @param {string | null} [array_strategy]
 * @returns {MdixMergeOutcome}
 */
export function mergeSourcesWeighted(entries, strategy, array_strategy) {
    const ptr0 = passArrayJsValueToWasm0(entries, wasm.__wbindgen_malloc);
    const len0 = WASM_VECTOR_LEN;
    var ptr1 = isLikeNone(strategy) ? 0 : passStringToWasm0(strategy, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len1 = WASM_VECTOR_LEN;
    var ptr2 = isLikeNone(array_strategy) ? 0 : passStringToWasm0(array_strategy, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len2 = WASM_VECTOR_LEN;
    const ret = wasm.mergeSourcesWeighted(ptr0, len0, ptr1, len1, ptr2, len2);
    if (ret[2]) {
        throw takeFromExternrefTable0(ret[1]);
    }
    return MdixMergeOutcome.__wrap(ret[0]);
}

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
 * @param {string} url
 * @param {string} content
 */
export function prefetchImport(url, content) {
    const ptr0 = passStringToWasm0(url, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(content, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    wasm.prefetchImport(ptr0, len0, ptr1, len1);
}

function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_is_function_0095a73b8b156f76: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_object_5ae8e5880f2c1fbd: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_cd444516edc5b180: function(arg0) {
            const ret = typeof(arg0) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_9e4d92534c42d778: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_number_get_8ff4255516ccad3e: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_72fb696202c56729: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_be289d5034ed271b: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_call_389efe28435a9388: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.call(arg1);
            return ret;
        }, arguments); },
        __wbg_call_4708e0c13bdc8e95: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.call(arg1, arg2);
            return ret;
        }, arguments); },
        __wbg_crypto_574e78ad8b13b65f: function(arg0) {
            const ret = arg0.crypto;
            return ret;
        },
        __wbg_error_7534b8e9a36f1ab4: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_getItem_0c792d344808dcf5: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            const ret = arg1.getItem(getStringFromWasm0(arg2, arg3));
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        }, arguments); },
        __wbg_getRandomValues_9b655bdd369112f2: function() { return handleError(function (arg0, arg1) {
            globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1));
        }, arguments); },
        __wbg_getRandomValues_b8f5dbd5f3995a9e: function() { return handleError(function (arg0, arg1) {
            arg0.getRandomValues(arg1);
        }, arguments); },
        __wbg_getTime_1e3cd1391c5c3995: function(arg0) {
            const ret = arg0.getTime();
            return ret;
        },
        __wbg_getTimezoneOffset_81776d10a4ec18a8: function(arg0) {
            const ret = arg0.getTimezoneOffset();
            return ret;
        },
        __wbg_get_9b94d73e6221f75c: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_instanceof_Window_ed49b2db8df90359: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Window;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_isArray_d314bb98fcf08331: function(arg0) {
            const ret = Array.isArray(arg0);
            return ret;
        },
        __wbg_length_32ed9a279acd054c: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_localStorage_a22d31b9eacc4594: function() { return handleError(function (arg0) {
            const ret = arg0.localStorage;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_msCrypto_a61aeb35a24c1329: function(arg0) {
            const ret = arg0.msCrypto;
            return ret;
        },
        __wbg_new_0_73afc35eb544e539: function() {
            const ret = new Date();
            return ret;
        },
        __wbg_new_245cd5c49157e602: function(arg0) {
            const ret = new Date(arg0);
            return ret;
        },
        __wbg_new_72b49615380db768: function(arg0, arg1) {
            const ret = new Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_8a6f238a6ece86ea: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_new_no_args_1c7c842f08d00ebb: function(arg0, arg1) {
            const ret = new Function(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_new_with_length_a2c39cbe88fd8ff1: function(arg0) {
            const ret = new Uint8Array(arg0 >>> 0);
            return ret;
        },
        __wbg_node_905d3e251edff8a2: function(arg0) {
            const ret = arg0.node;
            return ret;
        },
        __wbg_now_2c95c9de01293173: function(arg0) {
            const ret = arg0.now();
            return ret;
        },
        __wbg_parse_708461a1feddfb38: function() { return handleError(function (arg0, arg1) {
            const ret = JSON.parse(getStringFromWasm0(arg0, arg1));
            return ret;
        }, arguments); },
        __wbg_performance_7a3ffd0b17f663ad: function(arg0) {
            const ret = arg0.performance;
            return ret;
        },
        __wbg_process_dc0fbacc7c1c06f7: function(arg0) {
            const ret = arg0.process;
            return ret;
        },
        __wbg_prototypesetcall_bdcdcc5842e4d77d: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_randomFillSync_ac0988aba3254290: function() { return handleError(function (arg0, arg1) {
            arg0.randomFillSync(arg1);
        }, arguments); },
        __wbg_removeItem_f6369b1a6fa39850: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.removeItem(getStringFromWasm0(arg1, arg2));
        }, arguments); },
        __wbg_require_60cc747a6bc5215a: function() { return handleError(function () {
            const ret = module.require;
            return ret;
        }, arguments); },
        __wbg_setItem_cf340bb2edbd3089: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.setItem(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4));
        }, arguments); },
        __wbg_stack_0ed75d68575b0f3c: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_static_accessor_GLOBAL_12837167ad935116: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_e628e89ab3b1c95f: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_a621d3dfbb60d0ce: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_f8727f0cf888e0bd: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_subarray_a96e1fef17ed23cb: function(arg0, arg1, arg2) {
            const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0);
            return ret;
        },
        __wbg_versions_c01dfd4722a88165: function(arg0) {
            const ret = arg0.versions;
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
            const ret = getArrayU8FromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_cast_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./mdix_wasm_bg.js": import0,
    };
}

const MdixBuilderFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mdixbuilder_free(ptr >>> 0, 1));
const MdixDatabaseFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mdixdatabase_free(ptr >>> 0, 1));
const MdixDlmOutcomeFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mdixdlmoutcome_free(ptr >>> 0, 1));
const MdixMergeOutcomeFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mdixmergeoutcome_free(ptr >>> 0, 1));
const MdixSchemaFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mdixschema_free(ptr >>> 0, 1));
const MdixValidationReportFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mdixvalidationreport_free(ptr >>> 0, 1));
const MdixWatchOutcomeFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mdixwatchoutcome_free(ptr >>> 0, 1));
const MdixWatcherFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_mdixwatcher_free(ptr >>> 0, 1));

function addToExternrefTable0(obj) {
    const idx = wasm.__externref_table_alloc();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
}

function _assertClass(instance, klass) {
    if (!(instance instanceof klass)) {
        throw new Error(`expected instance of ${klass.name}`);
    }
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getDataViewMemory0();
    const result = [];
    for (let i = ptr; i < ptr + 4 * len; i += 4) {
        result.push(wasm.__wbindgen_externrefs.get(mem.getUint32(i, true)));
    }
    wasm.__externref_drop_slice(ptr, len);
    return result;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_exn_store(idx);
    }
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passArrayJsValueToWasm0(array, malloc) {
    const ptr = malloc(array.length * 4, 4) >>> 0;
    for (let i = 0; i < array.length; i++) {
        const add = addToExternrefTable0(array[i]);
        getDataViewMemory0().setUint32(ptr + 4 * i, add, true);
    }
    WASM_VECTOR_LEN = array.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__externref_table_dealloc(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasm;
function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('mdix_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
