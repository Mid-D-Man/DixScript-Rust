use crate::error::{freed_err, invalid_path_err, runtime_err};
use dixscript::Runtime::{DixData, DixLoadOptions, DixLoader, DixValue};
use wasm_bindgen::prelude::*;

/// Seed the cloud-import cache before compiling.
///
/// `@IMPORTS(...)` cloud (http/https) URLs can't be fetched from inside a
/// wasm build — there's no way to do a real network request synchronously
/// from within the compile pipeline. This is the actual working path
/// instead: `fetch()` the URL yourself in JS (normal `async`/`await`, no
/// wasm involved), then call this with the URL and the text you got back,
/// *before* calling `loadStr()` on source that references it. The
/// synchronous resolver checks this cache first and will find it already
/// there — no network access happens inside wasm at all.
///
/// Call once per URL. Cached in the browser's `localStorage` for the
/// current origin, so it also persists across page reloads — call it again
/// any time you want to force a re-fetch to be picked up (there's no
/// separate "evict one entry" call; `MdixDatabase` doesn't expose
/// `clear_cache`/`get_statistics` yet either — those exist on the Rust
/// side in `CloudFileCache` but aren't wired through to this binding).
///
/// No-op on native targets — this function only exists in the wasm build.
#[wasm_bindgen(js_name = prefetchImport)]
pub fn prefetch_import(url: &str, content: &str) {
    let cache = dixscript::Compiler::ImportsResolution::CloudFileCache::new(
        dixscript::ErrorManager::ErrorManager::get_shared_instance(),
    );
    cache.prefetch(url, content);
}

/// A loaded DixScript database.
///
/// Construct via `MdixDatabase.load_str()` or `MdixDatabase.from_json()`.
/// Call `free()` when done — the GC will also clean up but explicit
/// freeing is recommended in hot loops.
#[wasm_bindgen]
pub struct MdixDatabase {
    inner: Option<DixData>,
}

#[wasm_bindgen]
impl MdixDatabase {
    // ── Construction ──────────────────────────────────────────────────────

    /// Load a DixScript database from a raw .mdix source string.
    #[wasm_bindgen(js_name = loadStr)]
    pub fn load_str(source: &str) -> Result<MdixDatabase, JsValue> {
        if source.is_empty() {
            return Err(invalid_path_err("source string is empty"));
        }
        let loader = DixLoader::new();
        loader
            .load_from_str(source, &DixLoadOptions::new())
            .map(|data| MdixDatabase { inner: Some(data) })
            .map_err(|e| runtime_err("load_str", e))
    }

    /// Load from a JSON object string.
    /// The JSON must have an object at the top level.
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<MdixDatabase, JsValue> {
        if json.is_empty() {
            return Err(invalid_path_err("json string is empty"));
        }
        let converter = dixscript::Runtime::DixConverter::new();
        let ast = converter
            .from_json(json)
            .map_err(|e| runtime_err("from_json parse", e))?;
        let src = converter
            .to_mdix(&ast, None)
            .map_err(|e| runtime_err("from_json re-serialize", e))?;
        let loader = DixLoader::new();
        loader
            .load_from_str(&src, &DixLoadOptions::new())
            .map(|data| MdixDatabase { inner: Some(data) })
            .map_err(|e| runtime_err("from_json load", e))
    }

    /// Load from a TOML string.
    #[wasm_bindgen(js_name = fromToml)]
    pub fn from_toml(toml: &str) -> Result<MdixDatabase, JsValue> {
        if toml.is_empty() {
            return Err(invalid_path_err("toml string is empty"));
        }
        let converter = dixscript::Runtime::DixConverter::new();
        let ast = converter
            .from_toml(toml)
            .map_err(|e| runtime_err("from_toml parse", e))?;
        let src = converter
            .to_mdix(&ast, None)
            .map_err(|e| runtime_err("from_toml re-serialize", e))?;
        let loader = DixLoader::new();
        loader
            .load_from_str(&src, &DixLoadOptions::new())
            .map(|data| MdixDatabase { inner: Some(data) })
            .map_err(|e| runtime_err("from_toml load", e))
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────

    /// Explicitly free the database. Safe to call multiple times.
    #[wasm_bindgen]
    pub fn free(&mut self) {
        self.inner = None;
    }

    /// Returns true if the database is still valid (not freed).
    #[wasm_bindgen(getter, js_name = isValid)]
    pub fn is_valid(&self) -> bool {
        self.inner.is_some()
    }

    /// Total number of entries loaded.
    #[wasm_bindgen(getter, js_name = entryCount)]
    pub fn entry_count(&self) -> Result<i32, JsValue> {
        let data = self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))?;
        Ok(data.entry_count() as i32)
    }

    // ── Type inspection ───────────────────────────────────────────────────

    /// Returns the type discriminant string for the value at `path`.
    /// Returns `"unknown"` if the path does not exist.
    #[wasm_bindgen(js_name = getValueType)]
    pub fn get_value_type(&self, path: &str) -> Result<String, JsValue> {
        let data = self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))?;
        if path.is_empty() {
            return Err(invalid_path_err(path));
        }
        let type_name = match data.get_value(path) {
            None => "unknown",
            Some(DixValue::Null) => "null",
            Some(DixValue::Bool(_)) => "bool",
            Some(DixValue::Int(_)) => "int",
            Some(DixValue::Long(_)) => "long",
            Some(DixValue::Float(_)) => "float",
            Some(DixValue::Double(_)) => "double",
            Some(DixValue::String(_)) => "string",
            Some(DixValue::Date(_)) => "date",
            Some(DixValue::Timestamp(_)) => "timestamp",
            Some(DixValue::HexColor(_)) => "hex_color",
            Some(DixValue::Blob(_)) => "blob",
            Some(DixValue::Regex(_)) => "regex",
            Some(DixValue::Array(_)) => "array",
            Some(DixValue::Object(_)) => "object",
            Some(DixValue::Tuple(_)) => "tuple",
            Some(DixValue::Enum { .. }) => "enum",
        };
        Ok(type_name.to_string())
    }

    // ── Existence ─────────────────────────────────────────────────────────

    /// Returns true if the dotted path exists in the loaded data.
    #[wasm_bindgen]
    pub fn exists(&self, path: &str) -> Result<bool, JsValue> {
        let data = self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))?;
        if path.is_empty() {
            return Ok(false);
        }
        Ok(data.exists(path))
    }

    // ── Typed getters ─────────────────────────────────────────────────────

    #[wasm_bindgen(js_name = getString)]
    pub fn get_string(&self, path: &str) -> Result<String, JsValue> {
        let data = self.data(path)?;
        data.get::<String>(path)
            .map_err(|e| runtime_err("get_string", e))
    }

    /// Strict: only succeeds on an actual Int (or Enum ordinal) value —
    /// does NOT silently coerce from Long/Float/Double. Matches the
    /// widening rule dixscript's schema.rs type_matches() uses, not the
    /// looser DixData::get::<T>() convenience used elsewhere in this
    /// file, which would happily truncate a Float/Double into this with
    /// no error.
    #[wasm_bindgen(js_name = getInt)]
    pub fn get_int(&self, path: &str) -> Result<i32, JsValue> {
        let data = self.data(path)?;
        match data.get_value(path) {
            Some(DixValue::Int(v))             => Ok(*v),
            Some(DixValue::Enum { value, .. }) => Ok(*value),
            Some(other) => Err(runtime_err("get_int",
                format!("'{}' is {}, not int", path, other.type_name()))),
            None => Err(runtime_err("get_int", format!("path not found: '{}'", path))),
        }
    }

    /// Get a 64-bit integer value. Accepts Long (exact) or Int (widened —
    /// i32 -> i64 is always lossless). Rejects Float/Double: silently
    /// truncating one into a Long is exactly the bug this guards against.
    /// Returns a JS `bigint`, not `number` — JS numbers are f64 and lose
    /// precision above 2^53, so this must be a bigint to carry the full
    /// 64-bit range. Pass one in too: `db.getLong(...)` returns
    /// `9223372036854775807n`-style values, and the matching
    /// `MdixBuilder.withLong(path, value)` expects a bigint argument
    /// (e.g. `withLong("id", 123n)`), not a plain `number`.
    #[wasm_bindgen(js_name = getLong)]
    pub fn get_long(&self, path: &str) -> Result<i64, JsValue> {
        let data = self.data(path)?;
        match data.get_value(path) {
            Some(DixValue::Long(v)) => Ok(*v),
            Some(DixValue::Int(v))  => Ok(*v as i64),
            Some(other) => Err(runtime_err("get_long",
                format!("'{}' is {}, not long", path, other.type_name()))),
            None => Err(runtime_err("get_long", format!("path not found: '{}'", path))),
        }
    }

    /// Get a Float value strictly — rejects Double, Int, and Long. Was
    /// previously implemented via the lenient f64 path then narrowed to
    /// f32, which silently accepted (and truncated) Int/Long/Double; that
    /// defeats the point of having a typed getter at all.
    #[wasm_bindgen(js_name = getFloat)]
    pub fn get_float(&self, path: &str) -> Result<f32, JsValue> {
        let data = self.data(path)?;
        match data.get_value(path) {
            Some(DixValue::Float(v)) => Ok(*v),
            Some(other) => Err(runtime_err("get_float",
                format!("'{}' is {}, not float", path, other.type_name()))),
            None => Err(runtime_err("get_float", format!("path not found: '{}'", path))),
        }
    }

    /// Get a Double value, widened from Float/Int/Long if needed — all
    /// three are always exact when promoted to f64 at the magnitudes
    /// DixScript configs realistically use. This matches schema.rs's own
    /// Double widening rule exactly, so it's left on the lenient
    /// DixData::get::<T>() path rather than rewritten like get_int/
    /// get_long/get_float above.
    #[wasm_bindgen(js_name = getDouble)]
    pub fn get_double(&self, path: &str) -> Result<f64, JsValue> {
        let data = self.data(path)?;
        data.get::<f64>(path)
            .map_err(|e| runtime_err("get_double", e))
    }

    #[wasm_bindgen(js_name = getBool)]
    pub fn get_bool(&self, path: &str) -> Result<bool, JsValue> {
        let data = self.data(path)?;
        data.get::<bool>(path)
            .map_err(|e| runtime_err("get_bool", e))
    }

    /// Returns the JSON serialization of the value at `path`.
    /// Useful for arrays, objects, tuples, and blobs.
    #[wasm_bindgen(js_name = getJson)]
    pub fn get_json(&self, path: &str) -> Result<String, JsValue> {
        let data = self.data(path)?;
        match data.get_value(path) {
            None => Err(runtime_err("get_json", format!("path not found: '{}'", path))),
            Some(value) => serde_json::to_string(value)
                .map_err(|e| runtime_err("get_json serialize", e)),
        }
    }

    // ── Arrays ────────────────────────────────────────────────────────────

    #[wasm_bindgen(js_name = getArrayLength)]
    pub fn get_array_length(&self, path: &str) -> Result<i32, JsValue> {
        let data = self.data(path)?;
        match data.get_value(path) {
            Some(DixValue::Array(arr)) => Ok(arr.len() as i32),
            Some(_) => Err(runtime_err("get_array_length", format!("'{}' is not an array", path))),
            None => Err(runtime_err("get_array_length", format!("path not found: '{}'", path))),
        }
    }

    // ── Querying ──────────────────────────────────────────────────────────
    //
    // DixQuery (dixscript/src/Runtime/query.rs) is a chain of Rust closures
    // -- where_(|v| ...), order_by_desc(|v| ...), select(|v| ...), group_by,
    // any/all, min_by_key/max_by_key. None of that can cross the
    // wasm-bindgen boundary as-is; JS doesn't have Rust closures, and
    // marshaling a JS callback in and calling it per-element (js_sys::
    // Function) would add real overhead for something with an exact native
    // equivalent already sitting on the other side of the boundary: every
    // one of those closures just calls `.field(name)` on a `&DixValue`,
    // which is exactly object-property access once you have plain decoded
    // JSON. So query()/queryMany() below don't try to port the chain --
    // they resolve and hand back the same DixValue data get_json() already
    // hands back for a single path (same direct serde_json::to_string on
    // the raw values, no DixConverter round-trip -- consistent with
    // get_json above, not to_json's whole-database hashmap reconstruction),
    // and native Array.filter/sort/map/reduce/some/every on the decoded
    // result covers where_/order_by/select/group_by/any/all/sum/avg/
    // min_by_key/max_by_key, with no parallel API to keep in sync with
    // query.rs as it grows. See mdix-npm/src/query.ts for the JS-side
    // convenience wrapper and the equivalent-native-JS cheat sheet.

    /// Query the array at `path` and return its elements as a JSON array
    /// string. `path` itself must be a plain `Array` value, or a
    /// `GroupArray`'s own path (the flattener already stores a
    /// `GroupArray`'s items as a real Array there) -- for gathering across
    /// multiple *sibling* paths instead, use `query_many`.
    ///
    /// Returns `"[]"` for a path that doesn't exist or isn't an Array,
    /// rather than erroring -- mirrors `DixData::query`, which returns
    /// `None` for exactly the same two cases: "no matching data" is a
    /// normal, common outcome for a query, not a caller mistake worth
    /// throwing over the way e.g. `getString` on the wrong type is.
    #[wasm_bindgen(js_name = query)]
    pub fn query(&self, path: &str) -> Result<String, JsValue> {
        let data = self.data(path)?;
        match data.query(path) {
            Some(q) => serde_json::to_string(q.as_slice())
                .map_err(|e| runtime_err("query serialize", e)),
            None => Ok("[]".to_string()),
        }
    }

    /// Query across every sibling path matched by a glob `pattern` (same
    /// whole-segment `*` syntax as the core's `select_many` -- see
    /// query.rs's module doc for exactly what that does and doesn't
    /// match), gathered into one JSON array string. For a single Array or
    /// GroupArray path's own items, use `query` instead.
    ///
    /// Always returns a JSON array (possibly `"[]"`) -- `DixData::
    /// query_many` has no `None` case, an empty match set is just an
    /// empty `DixQuery`.
    #[wasm_bindgen(js_name = queryMany)]
    pub fn query_many(&self, pattern: &str) -> Result<String, JsValue> {
        let data = self.data(pattern)?;
        let q = data.query_many(pattern);
        serde_json::to_string(q.as_slice())
            .map_err(|e| runtime_err("query_many serialize", e))
    }

    // ── Keys ──────────────────────────────────────────────────────────────

    /// Returns the direct child key names under `prefix`.
    /// Pass an empty string for top-level keys.
    #[wasm_bindgen(js_name = getKeys)]
    pub fn get_keys(&self, prefix: &str) -> Result<Vec<String>, JsValue> {
        let data = self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))?;
        Ok(data.get_keys(prefix))
    }

    // ── Enum ──────────────────────────────────────────────────────────────

    #[wasm_bindgen(js_name = getEnumName)]
    pub fn get_enum_name(&self, path: &str) -> Result<String, JsValue> {
        let data = self.data(path)?;
        match data.get_value(path) {
            Some(DixValue::Enum { enum_name, .. }) => Ok(enum_name.clone()),
            Some(_) => Err(runtime_err("get_enum_name", format!("'{}' is not an enum", path))),
            None => Err(runtime_err("get_enum_name", format!("path not found: '{}'", path))),
        }
    }

    #[wasm_bindgen(js_name = getEnumField)]
    pub fn get_enum_field(&self, path: &str) -> Result<String, JsValue> {
        let data = self.data(path)?;
        match data.get_value(path) {
            Some(DixValue::Enum { field_name, .. }) => Ok(field_name.clone()),
            Some(_) => Err(runtime_err("get_enum_field", format!("'{}' is not an enum", path))),
            None => Err(runtime_err("get_enum_field", format!("path not found: '{}'", path))),
        }
    }

    // ── Export ────────────────────────────────────────────────────────────

    /// Exports the entire database as a JSON string.
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self, indented: bool) -> Result<String, JsValue> {
        let data = self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))?;
        let entries = data.to_hashmap();
        let converter = dixscript::Runtime::DixConverter::new();
        let ast = converter
            .from_hashmap(entries)
            .map_err(|e| runtime_err("to_json ast", e))?;
        // NOTE: this must go through converter.to_json(), not to_hashmap() +
        // serde_json::to_string() — that was the bug here. to_hashmap()
        // returns DixData's *flat* key-value store (bracket-notation keys
        // like "some[0]"/"some[1]" for array elements), which is exactly
        // what to_json_flat() is for (mdix-scaffold's directory-grouping
        // needs specifically). A plain group array like `some:: 1, 2, 3`
        // was round-tripping as {"some[0]":1,"some[1]":2,"some[2]":3}
        // instead of {"some":[1,2,3]} -- to_json() reconstructs proper
        // nested JSON from the AST directly and is what every other
        // language binding's to_json/toJson/ToJson actually calls.
        converter
            .to_json(&ast, indented)
            .map_err(|e| runtime_err("to_json serialize", e))
    }

    /// Exports the entire database as a TOML string.
    #[wasm_bindgen(js_name = toToml)]
    pub fn to_toml(&self) -> Result<String, JsValue> {
        let data = self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))?;
        let entries = data.to_hashmap();
        let converter = dixscript::Runtime::DixConverter::new();
        let ast = converter
            .from_hashmap(entries)
            .map_err(|e| runtime_err("to_toml ast", e))?;
        converter
            .to_toml(&ast)
            .map_err(|e| runtime_err("to_toml serialize", e))
    }

    /// Re-serializes the database back to .mdix source text.
    #[wasm_bindgen(js_name = toMdix)]
    pub fn to_mdix(&self) -> Result<String, JsValue> {
        let data = self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))?;
        let entries = data.to_hashmap();
        let converter = dixscript::Runtime::DixConverter::new();
        let ast = converter
            .from_hashmap(entries)
            .map_err(|e| runtime_err("to_mdix ast", e))?;
        converter
            .to_mdix(&ast, None)
            .map_err(|e| runtime_err("to_mdix serialize", e))
    }

    /// Merges this database with `other` and returns a fresh
    /// `MdixMergeOutcome` — leaves both original databases untouched.
    ///
    /// `strategy`: "weighted" (default) | "primary_wins" | "secondary_wins"
    /// | "throw_on_conflict". `array_strategy`: "concat_dedup" (default) |
    /// "replace" | "concat". `this` merges as the primary source
    /// (weight 1.0), `other` as secondary (weight 0.5).
    ///
    /// NOTE: this method previously didn't exist as a real binding despite
    /// being documented at the top of merge.rs — `crate::merge::merge_with`
    /// was a plain Rust free function, never wired into a #[wasm_bindgen]
    /// impl block or re-exported from lib.rs, so `MdixDatabase.mergeWith`
    /// was unreachable from JS entirely. This is that wiring.
    #[wasm_bindgen(js_name = mergeWith)]
    pub fn merge_with(
        &self,
        other:          &MdixDatabase,
        strategy:       Option<String>,
        array_strategy: Option<String>,
    ) -> Result<crate::merge::MdixMergeOutcome, JsValue> {
        crate::merge::merge_with(self, other, strategy, array_strategy)
    }

    // ── Schema validation ─────────────────────────────────────────────────

    /// Validates this database against `schema` and returns a
    /// `MdixValidationReport`.
    ///
    /// `schema` is borrowed, not consumed — the same `MdixSchema` instance
    /// can validate multiple databases (mirrors the underlying
    /// `SchemaBuilder::validate(&self, ..)` in dixscript core, and the same
    /// pattern mdix-python's `MdixDatabase.validate_schema` already uses).
    ///
    /// NOTE: this method was documented at the top of schema.rs
    /// (`db.validateSchema(schema)`) but was never actually wired up here —
    /// `MdixSchema` and `MdixValidationReport` existed with no way to reach
    /// them from `MdixDatabase` at all. Same shape of bug as `mergeWith`
    /// above (see the regression test docs on that one); this is that
    /// wiring for schema.rs.
    #[wasm_bindgen(js_name = validateSchema)]
    pub fn validate_schema(
        &self,
        schema: &crate::schema::MdixSchema,
    ) -> Result<crate::schema::MdixValidationReport, JsValue> {
        let data = self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))?;
        Ok(crate::schema::MdixValidationReport::new(schema.inner.validate(data)))
    }

    // ── Private helpers ───────────────────────────────────────────────────

    fn data(&self, path: &str) -> Result<&DixData, JsValue> {
        if path.is_empty() {
            return Err(invalid_path_err(path));
        }
        self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))
    }
}


// Not part of the #[wasm_bindgen] impl block above on purpose: `DixData` is
// a native Rust type with no wasm-bindgen binding of its own, so this isn't
// (and can't be) exposed to JS directly — unlike load_str/from_json/
// from_toml, which all parse from a JS-facing string. This exists purely
// for other Rust modules in this crate (merge.rs) that already have a
// DixData in hand — e.g. the result of merging two databases — and just
// need to wrap it back into the JS-facing MdixDatabase type to return.
impl MdixDatabase {
    pub(crate) fn from_data(data: DixData) -> Self {
        MdixDatabase { inner: Some(data) }
    }
                     }
