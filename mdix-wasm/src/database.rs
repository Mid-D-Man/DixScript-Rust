// mdix-wasm/src/database.rs

use crate::error::{freed_err, invalid_path_err, runtime_err};
use dixscript::Runtime::{DixData, DixLoadOptions, DixLoader, DixValue};
use wasm_bindgen::prelude::*;

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

    #[wasm_bindgen(js_name = getInt)]
    pub fn get_int(&self, path: &str) -> Result<i32, JsValue> {
        let data = self.data(path)?;
        data.get::<i32>(path)
            .map_err(|e| runtime_err("get_int", e))
    }

    /// Get a 64-bit integer value. Also accepts Int values (widened
    /// without loss). Returns a JS `bigint`, not `number` — JS numbers
    /// are f64 and lose precision above 2^53, so this must be a bigint
    /// to carry the full 64-bit range. Pass one in too: `db.getLong(...)`
    /// returns `9223372036854775807n`-style values, and the matching
    /// `MdixBuilder.withLong(path, value)` expects a bigint argument
    /// (e.g. `withLong("id", 123n)`), not a plain `number`.
    #[wasm_bindgen(js_name = getLong)]
    pub fn get_long(&self, path: &str) -> Result<i64, JsValue> {
        let data = self.data(path)?;
        data.get::<i64>(path)
            .map_err(|e| runtime_err("get_long", e))
    }

    #[wasm_bindgen(js_name = getFloat)]
    pub fn get_float(&self, path: &str) -> Result<f32, JsValue> {
        let data = self.data(path)?;
        data.get::<f64>(path)
            .map(|v| v as f32)
            .map_err(|e| runtime_err("get_float", e))
    }

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
        let map = converter.to_hashmap(&ast);
        let result = if indented {
            serde_json::to_string_pretty(&map)
        } else {
            serde_json::to_string(&map)
        };
        result.map_err(|e| runtime_err("to_json serialize", e))
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

    // ── Private helpers ───────────────────────────────────────────────────

    fn data(&self, path: &str) -> Result<&DixData, JsValue> {
        if path.is_empty() {
            return Err(invalid_path_err(path));
        }
        self.inner.as_ref().ok_or_else(|| freed_err("MdixDatabase"))
    }
        }
