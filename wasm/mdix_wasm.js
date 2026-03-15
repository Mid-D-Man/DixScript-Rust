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
}
if (Symbol.dispose) MdixDatabase.prototype[Symbol.dispose] = MdixDatabase.prototype.free;

export function init() {
    wasm.init();
}

function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_be289d5034ed271b: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_getTime_1e3cd1391c5c3995: function(arg0) {
            const ret = arg0.getTime();
            return ret;
        },
        __wbg_getTimezoneOffset_81776d10a4ec18a8: function(arg0) {
            const ret = arg0.getTimezoneOffset();
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
        __wbindgen_cast_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
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
