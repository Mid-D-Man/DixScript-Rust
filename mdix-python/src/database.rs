//! MdixDatabase — loaded DixScript database with raising and railway access.

use pyo3::prelude::*;
use dixscript::Runtime::{
    DixConverter, DixData, DixFormatOptions, DixLoadOptions, DixLoader, DixValue,
};
use crate::error::{disposed_err, invalid_path_err, runtime_err};
use crate::result::MdixResult;

#[pyclass(module = "midmanstudio.mdix")]
pub struct MdixDatabase {
    inner: Option<DixData>,
}

impl MdixDatabase {
    fn from_data(data: DixData) -> Self {
        MdixDatabase { inner: Some(data) }
    }

    fn data(&self) -> PyResult<&DixData> {
        self.inner.as_ref().ok_or_else(|| disposed_err("MdixDatabase"))
    }
}

#[pymethods]
impl MdixDatabase {
    // ── Lifecycle ──────────────────────────────────────────────────────────

    /// Context manager entry — returns `self`.
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    /// Context manager exit — frees the database.
    fn __exit__(
        &mut self,
        _exc_type: PyObject,
        _exc_val: PyObject,
        _exc_tb: PyObject,
    ) -> bool {
        self.inner = None;
        false
    }

    /// Explicitly free the database. Safe to call multiple times.
    fn close(&mut self) {
        self.inner = None;
    }

    /// Alias for `close()`.
    fn free(&mut self) {
        self.inner = None;
    }

    /// `True` if the database is loaded and not yet freed.
    #[getter]
    fn is_valid(&self) -> bool {
        self.inner.is_some()
    }

    /// Total number of entries in the loaded database.
    #[getter]
    fn entry_count(&self) -> PyResult<i32> {
        Ok(self.data()?.entry_count() as i32)
    }

    fn __repr__(&self) -> String {
        match &self.inner {
            Some(d) => format!("MdixDatabase(entries={})", d.entry_count()),
            None    => "MdixDatabase(freed)".to_string(),
        }
    }

    // ── Loading — raising ──────────────────────────────────────────────────

    /// Load a `.mdix` file from disk. Raises `MdixError` on failure.
    #[staticmethod]
    fn load(path: &str) -> PyResult<MdixDatabase> {
        if path.is_empty() {
            return Err(invalid_path_err(path));
        }
        let loader = DixLoader::new();
        loader
            .load_text(path, &DixLoadOptions::new())
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("load", e))
    }

    /// Load from a raw `.mdix` source string. Raises `MdixError` on failure.
    #[staticmethod]
    fn load_str(source: &str) -> PyResult<MdixDatabase> {
        if source.is_empty() {
            return Err(invalid_path_err("source string is empty"));
        }
        let loader = DixLoader::new();
        loader
            .load_from_str(source, &DixLoadOptions::new())
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("load_str", e))
    }

    /// Load from a JSON object string. Raises `MdixError` on failure.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<MdixDatabase> {
        if json.is_empty() {
            return Err(invalid_path_err("json string is empty"));
        }
        let converter = DixConverter::new();
        let ast = converter
            .from_json(json)
            .map_err(|e| runtime_err("from_json", e))?;
        let src = converter
            .to_mdix(&ast, None)
            .map_err(|e| runtime_err("from_json:reserialize", e))?;
        let loader = DixLoader::new();
        loader
            .load_from_str(&src, &DixLoadOptions::new())
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("from_json:load", e))
    }

    /// Load from a TOML string. Raises `MdixError` on failure.
    #[staticmethod]
    fn from_toml(toml: &str) -> PyResult<MdixDatabase> {
        if toml.is_empty() {
            return Err(invalid_path_err("toml string is empty"));
        }
        let converter = DixConverter::new();
        let ast = converter
            .from_toml(toml)
            .map_err(|e| runtime_err("from_toml", e))?;
        let src = converter
            .to_mdix(&ast, None)
            .map_err(|e| runtime_err("from_toml:reserialize", e))?;
        let loader = DixLoader::new();
        loader
            .load_from_str(&src, &DixLoadOptions::new())
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("from_toml:load", e))
    }

    /// Load an encrypted `.mdix.enc` file.
    /// `key_path` may be `None` to auto-detect next to the `.enc` file.
    #[staticmethod]
    #[pyo3(signature = (enc_path, key_path = None))]
    fn load_encrypted(enc_path: &str, key_path: Option<&str>) -> PyResult<MdixDatabase> {
        if enc_path.is_empty() {
            return Err(invalid_path_err(enc_path));
        }
        let mut opts = DixLoadOptions::new();
        if let Some(kp) = key_path {
            opts.key_file_path = Some(kp.to_string());
        }
        let loader = DixLoader::new();
        loader
            .load_encrypted(enc_path, &opts)
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("load_encrypted", e))
    }

    /// Load an encrypted `.mdix.enc` file using a password.
    #[staticmethod]
    fn load_encrypted_password(enc_path: &str, password: &str) -> PyResult<MdixDatabase> {
        if enc_path.is_empty() { return Err(invalid_path_err(enc_path)); }
        if password.is_empty() {
            return Err(crate::error::to_py_err("[mdix] password cannot be empty"));
        }
        let loader = DixLoader::new();
        loader
            .load_encrypted(enc_path, &DixLoadOptions::with_password(password))
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("load_encrypted_password", e))
    }

    // ── Loading — railway ──────────────────────────────────────────────────

    #[staticmethod]
    fn try_load(py: Python<'_>, path: &str) -> MdixResult {
        match MdixDatabase::load(path) {
            Ok(db)  => MdixResult::ok(py, db),
            Err(e)  => MdixResult::from_py_err(e),
        }
    }

    #[staticmethod]
    fn try_load_str(py: Python<'_>, source: &str) -> MdixResult {
        match MdixDatabase::load_str(source) {
            Ok(db)  => MdixResult::ok(py, db),
            Err(e)  => MdixResult::from_py_err(e),
        }
    }

    #[staticmethod]
    fn try_from_json(py: Python<'_>, json: &str) -> MdixResult {
        match MdixDatabase::from_json(json) {
            Ok(db)  => MdixResult::ok(py, db),
            Err(e)  => MdixResult::from_py_err(e),
        }
    }

    #[staticmethod]
    fn try_from_toml(py: Python<'_>, toml: &str) -> MdixResult {
        match MdixDatabase::from_toml(toml) {
            Ok(db)  => MdixResult::ok(py, db),
            Err(e)  => MdixResult::from_py_err(e),
        }
    }

    // ── Type inspection ────────────────────────────────────────────────────

    /// Returns `True` if the dotted path exists in the loaded data.
    fn exists(&self, path: &str) -> PyResult<bool> {
        Ok(self.data()?.exists(path))
    }

    /// Returns the value type string at `path`:
    /// `"int"`, `"string"`, `"bool"`, `"float"`, `"double"`, `"array"`,
    /// `"object"`, `"enum"`, `"date"`, `"timestamp"`, `"hex_color"`,
    /// `"blob"`, `"regex"`, `"tuple"`, `"null"`, or `"unknown"`.
    fn get_type(&self, path: &str) -> PyResult<&'static str> {
        let data = self.data()?;
        Ok(match data.get_value(path) {
            None                         => "unknown",
            Some(DixValue::Null)         => "null",
            Some(DixValue::Bool(_))      => "bool",
            Some(DixValue::Int(_))       => "int",
            Some(DixValue::Float(_))     => "float",
            Some(DixValue::Double(_))    => "double",
            Some(DixValue::String(_))    => "string",
            Some(DixValue::Date(_))      => "date",
            Some(DixValue::Timestamp(_)) => "timestamp",
            Some(DixValue::HexColor(_))  => "hex_color",
            Some(DixValue::Blob(_))      => "blob",
            Some(DixValue::Regex(_))     => "regex",
            Some(DixValue::Array(_))     => "array",
            Some(DixValue::Object(_))    => "object",
            Some(DixValue::Tuple(_))     => "tuple",
            Some(DixValue::Enum { .. })  => "enum",
        })
    }

    /// Returns the number of items in the array at `path`.
    /// Returns `-1` if the path is not an array.
    fn get_array_length(&self, path: &str) -> PyResult<i32> {
        let data = self.data()?;
        Ok(match data.get_value(path) {
            Some(DixValue::Array(arr)) => arr.len() as i32,
            _                          => -1,
        })
    }

    /// Returns direct child key names under `prefix`.
    /// Pass `""` for top-level keys.
    #[pyo3(signature = (prefix = ""))]
    fn get_keys(&self, prefix: &str) -> PyResult<Vec<String>> {
        Ok(self.data()?.get_keys(prefix))
    }

    // ── Typed getters — raising ────────────────────────────────────────────

    /// Get a string value. Raises `MdixError` if not found or wrong type.
    /// Pass `default` to return a fallback instead of raising.
    #[pyo3(signature = (path, default = None))]
    fn get_string(&self, path: &str, default: Option<&str>) -> PyResult<String> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get::<String>(path) {
            Ok(v) => Ok(v),
            Err(e) => match default {
                Some(d) => Ok(d.to_string()),
                None    => Err(runtime_err("get_string", e)),
            },
        }
    }

    /// Get an integer value. Raises `MdixError` if not found or wrong type.
    #[pyo3(signature = (path, default = None))]
    fn get_int(&self, path: &str, default: Option<i32>) -> PyResult<i32> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get::<i32>(path) {
            Ok(v) => Ok(v),
            Err(e) => match default {
                Some(d) => Ok(d),
                None    => Err(runtime_err("get_int", e)),
            },
        }
    }

    /// Get a float value. Raises `MdixError` if not found or wrong type.
    #[pyo3(signature = (path, default = None))]
    fn get_float(&self, path: &str, default: Option<f32>) -> PyResult<f32> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get::<f64>(path).map(|v| v as f32) {
            Ok(v) => Ok(v),
            Err(e) => match default {
                Some(d) => Ok(d),
                None    => Err(runtime_err("get_float", e)),
            },
        }
    }

    /// Get a double (Python `float`) value. Raises `MdixError` if not found.
    #[pyo3(signature = (path, default = None))]
    fn get_double(&self, path: &str, default: Option<f64>) -> PyResult<f64> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get::<f64>(path) {
            Ok(v) => Ok(v),
            Err(e) => match default {
                Some(d) => Ok(d),
                None    => Err(runtime_err("get_double", e)),
            },
        }
    }

    /// Get a boolean value. Raises `MdixError` if not found or wrong type.
    #[pyo3(signature = (path, default = None))]
    fn get_bool(&self, path: &str, default: Option<bool>) -> PyResult<bool> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get::<bool>(path) {
            Ok(v) => Ok(v),
            Err(e) => match default {
                Some(d) => Ok(d),
                None    => Err(runtime_err("get_bool", e)),
            },
        }
    }

    /// Get the JSON serialization of any value at `path`.
    /// Useful for arrays, objects, tuples, and blobs.
    fn get_json(&self, path: &str) -> PyResult<String> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        let data = self.data()?;
        match data.get_value(path) {
            None => Err(runtime_err("get_json", format!("path not found: '{}'", path))),
            Some(v) => serde_json::to_string(v)
                .map_err(|e| runtime_err("get_json:serialize", e)),
        }
    }

    /// Get the enum type name at `path` (e.g. `"AIType"`).
    fn get_enum_name(&self, path: &str) -> PyResult<String> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        let data = self.data()?;
        match data.get_value(path) {
            Some(DixValue::Enum { enum_name, .. }) => Ok(enum_name.clone()),
            Some(_) => Err(runtime_err("get_enum_name", format!("'{}' is not an enum", path))),
            None    => Err(runtime_err("get_enum_name", format!("path not found: '{}'", path))),
        }
    }

    /// Get the enum field name at `path` (e.g. `"BOSS"`).
    fn get_enum_field(&self, path: &str) -> PyResult<String> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        let data = self.data()?;
        match data.get_value(path) {
            Some(DixValue::Enum { field_name, .. }) => Ok(field_name.clone()),
            Some(_) => Err(runtime_err("get_enum_field", format!("'{}' is not an enum", path))),
            None    => Err(runtime_err("get_enum_field", format!("path not found: '{}'", path))),
        }
    }

    // ── Typed getters — railway ────────────────────────────────────────────

    fn try_get_string(&self, py: Python<'_>, path: &str) -> MdixResult {
        match self.get_string(path, None) {
            Ok(v)  => MdixResult::ok(py, v),
            Err(e) => MdixResult::from_py_err(e),
        }
    }

    fn try_get_int(&self, py: Python<'_>, path: &str) -> MdixResult {
        match self.get_int(path, None) {
            Ok(v)  => MdixResult::ok(py, v),
            Err(e) => MdixResult::from_py_err(e),
        }
    }

    fn try_get_float(&self, py: Python<'_>, path: &str) -> MdixResult {
        match self.get_float(path, None) {
            Ok(v)  => MdixResult::ok(py, v),
            Err(e) => MdixResult::from_py_err(e),
        }
    }

    fn try_get_double(&self, py: Python<'_>, path: &str) -> MdixResult {
        match self.get_double(path, None) {
            Ok(v)  => MdixResult::ok(py, v),
            Err(e) => MdixResult::from_py_err(e),
        }
    }

    fn try_get_bool(&self, py: Python<'_>, path: &str) -> MdixResult {
        match self.get_bool(path, None) {
            Ok(v)  => MdixResult::ok(py, v),
            Err(e) => MdixResult::from_py_err(e),
        }
    }

    fn try_get_json(&self, py: Python<'_>, path: &str) -> MdixResult {
        match self.get_json(path) {
            Ok(v)  => MdixResult::ok(py, v),
            Err(e) => MdixResult::from_py_err(e),
        }
    }

    // ── Export ─────────────────────────────────────────────────────────────

    /// Export the database as a JSON string.
    #[pyo3(signature = (indented = true))]
    fn to_json(&self, indented: bool) -> PyResult<String> {
        let data = self.data()?;
        let entries = data.to_hashmap();
        let converter = DixConverter::new();
        let ast = converter
            .from_hashmap(entries)
            .map_err(|e| runtime_err("to_json:ast", e))?;
        let map = converter.to_hashmap(&ast);
        if indented {
            serde_json::to_string_pretty(&map)
        } else {
            serde_json::to_string(&map)
        }
        .map_err(|e| runtime_err("to_json:serialize", e))
    }

    /// Export the database as a TOML string.
    fn to_toml(&self) -> PyResult<String> {
        let data = self.data()?;
        let entries = data.to_hashmap();
        let converter = DixConverter::new();
        let ast = converter
            .from_hashmap(entries)
            .map_err(|e| runtime_err("to_toml:ast", e))?;
        converter
            .to_toml(&ast)
            .map_err(|e| runtime_err("to_toml:serialize", e))
    }

    /// Re-serialize the database back to `.mdix` source text.
    fn to_mdix(&self) -> PyResult<String> {
        let data = self.data()?;
        let entries = data.to_hashmap();
        let converter = DixConverter::new();
        let ast = converter
            .from_hashmap(entries)
            .map_err(|e| runtime_err("to_mdix:ast", e))?;
        converter
            .to_mdix(&ast, Some(&DixFormatOptions::pretty()))
            .map_err(|e| runtime_err("to_mdix:serialize", e))
    }
          }
