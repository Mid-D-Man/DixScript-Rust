// mdix-python/src/database.rs
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

// ── Non-pymethods helpers — must be in a plain impl block ─────────────────────
// PyO3 does not allow non-exposed methods inside #[pymethods].
// Placing them here avoids the "trait bound not satisfied" errors that occur
// when PyO3's generated From/Into impls clash with method signatures.
impl MdixDatabase {
    fn from_data(data: DixData) -> Self {
        MdixDatabase { inner: Some(data) }
    }

    /// Called by MdixBuilder and the ML layer.
    pub fn from_data_pub(data: DixData) -> Self {
        MdixDatabase { inner: Some(data) }
    }

    pub(crate) fn data(&self) -> PyResult<&DixData> {
        self.inner.as_ref().ok_or_else(|| disposed_err("MdixDatabase"))
    }

    /// Re-serializes this database back to a .mdix source string.
    /// Plain-Rust helper (not a #[pymethods] fn) shared by the `to_mdix`
    /// pymethod and by `merge::merge_with`, which needs to round-trip two
    /// in-memory databases through source text to merge them at the AST
    /// level.
    pub(crate) fn to_mdix_string(&self) -> PyResult<String> {
        let data    = self.data()?;
        let entries = data.to_hashmap();
        let conv    = DixConverter::new();
        let ast     = conv.from_hashmap(entries)
            .map_err(|e| runtime_err("to_mdix:ast", e))?;
        conv.to_mdix(&ast, Some(&DixFormatOptions::pretty()))
            .map_err(|e| runtime_err("to_mdix:serialize", e))
    }
}

#[pymethods]
impl MdixDatabase {
    // ── Lifecycle ──────────────────────────────────────────────────────────

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __exit__(
        &mut self,
        _exc_type: PyObject,
        _exc_val: PyObject,
        _exc_tb: PyObject,
    ) -> bool {
        self.inner = None;
        false
    }

    fn close(&mut self) {
        self.inner = None;
    }

    fn free(&mut self) {
        self.inner = None;
    }

    #[getter]
    fn is_valid(&self) -> bool {
        self.inner.is_some()
    }

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

    // ── Pickle support ───────────────────────────────────────────────────────
    //
    // Enables `pickle.dumps(db)` / `pickle.loads(data)`, and transitively
    // anything built on pickle — `multiprocessing.Pool`, `copy.deepcopy`,
    // `joblib` caching, etc. Returns `(MdixDatabase.load_str, (source,))`:
    // pickle calls `MdixDatabase.load_str(source)` to reconstruct, reusing
    // the existing, already-tested loader path instead of a separate
    // `__new__`/`__setstate__` construction route. `DixData` (the live
    // Rust handle this class wraps) isn't itself serializable, so the
    // .mdix source text — which `to_mdix_string()` already produces for
    // the `to_mdix()` method — is the natural thing to pickle instead.
    fn __reduce__(&self, py: Python<'_>) -> PyResult<(PyObject, PyObject)> {
        let cls = py.get_type_bound::<MdixDatabase>();
        let load_str = cls.getattr("load_str")?.into_py(py);
        let src = self.to_mdix_string()?;
        let args: PyObject = (src,).into_py(py);
        Ok((load_str, args))
    }

    // ── Loading — raising ──────────────────────────────────────────────────

    #[staticmethod]
    fn load(path: &str) -> PyResult<MdixDatabase> {
        if path.is_empty() {
            return Err(invalid_path_err(path));
        }
        DixLoader::new()
            .load_text(path, &DixLoadOptions::new())
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("load", e))
    }

    #[staticmethod]
    fn load_str(source: &str) -> PyResult<MdixDatabase> {
        if source.is_empty() {
            return Err(invalid_path_err("source string is empty"));
        }
        DixLoader::new()
            .load_from_str(source, &DixLoadOptions::new())
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("load_str", e))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<MdixDatabase> {
        if json.is_empty() {
            return Err(invalid_path_err("json string is empty"));
        }
        let converter = DixConverter::new();
        let ast = converter.from_json(json)
            .map_err(|e| runtime_err("from_json", e))?;
        let src = converter.to_mdix(&ast, None)
            .map_err(|e| runtime_err("from_json:reserialize", e))?;
        DixLoader::new()
            .load_from_str(&src, &DixLoadOptions::new())
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("from_json:load", e))
    }

    #[staticmethod]
    fn from_toml(toml: &str) -> PyResult<MdixDatabase> {
        if toml.is_empty() {
            return Err(invalid_path_err("toml string is empty"));
        }
        let converter = DixConverter::new();
        let ast = converter.from_toml(toml)
            .map_err(|e| runtime_err("from_toml", e))?;
        let src = converter.to_mdix(&ast, None)
            .map_err(|e| runtime_err("from_toml:reserialize", e))?;
        DixLoader::new()
            .load_from_str(&src, &DixLoadOptions::new())
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("from_toml:load", e))
    }

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
        DixLoader::new()
            .load_encrypted(enc_path, &opts)
            .map(MdixDatabase::from_data)
            .map_err(|e| runtime_err("load_encrypted", e))
    }

    #[staticmethod]
    fn load_encrypted_password(enc_path: &str, password: &str) -> PyResult<MdixDatabase> {
        if enc_path.is_empty() { return Err(invalid_path_err(enc_path)); }
        if password.is_empty() {
            return Err(crate::error::to_py_err("[mdix] password cannot be empty"));
        }
        DixLoader::new()
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

    fn exists(&self, path: &str) -> PyResult<bool> {
        Ok(self.data()?.exists(path))
    }

    fn get_type(&self, path: &str) -> PyResult<&'static str> {
        let data = self.data()?;
        Ok(match data.get_value(path) {
            None                         => "unknown",
            Some(DixValue::Null)         => "null",
            Some(DixValue::Bool(_))      => "bool",
            Some(DixValue::Int(_))       => "int",
            Some(DixValue::Long(_))      => "long",
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

    fn get_array_length(&self, path: &str) -> PyResult<i32> {
        let data = self.data()?;
        Ok(match data.get_value(path) {
            Some(DixValue::Array(arr)) => arr.len() as i32,
            _                          => -1,
        })
    }

    #[pyo3(signature = (prefix = ""))]
    fn get_keys(&self, prefix: &str) -> PyResult<Vec<String>> {
        Ok(self.data()?.get_keys(prefix))
    }

    // ── Typed getters — raising ────────────────────────────────────────────

    #[pyo3(signature = (path, default = None))]
    fn get_string(&self, path: &str, default: Option<&str>) -> PyResult<String> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get::<String>(path) {
            Ok(v)  => Ok(v),
            Err(e) => match default {
                Some(d) => Ok(d.to_string()),
                None    => Err(runtime_err("get_string", e)),
            },
        }
    }

    /// Strict: only succeeds on an actual Int (or Enum ordinal) value —
    /// does NOT silently coerce from Long/Float/Double. Matches the
    /// widening rule dixscript's schema.rs type_matches() uses, not the
    /// looser DixData::get::<T>() convenience used elsewhere in this
    /// file, which would happily truncate a Float/Double into this with
    /// no error.
    #[pyo3(signature = (path, default = None))]
    fn get_int(&self, path: &str, default: Option<i32>) -> PyResult<i32> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get_value(path) {
            Some(DixValue::Int(v))             => Ok(*v),
            Some(DixValue::Enum { value, .. }) => Ok(*value),
            Some(other) => match default {
                Some(d) => Ok(d),
                None => Err(runtime_err("get_int", format!(
                    "'{}' is {}, not int", path, other.type_name()
                ))),
            },
            None => match default {
                Some(d) => Ok(d),
                None    => Err(runtime_err("get_int", format!("path not found: '{}'", path))),
            },
        }
    }

    /// Accepts Long (exact) or Int (widened — i32 -> i64 is always
    /// lossless). Rejects Float/Double: those are different numeric
    /// families, and silently truncating one into a Long is exactly
    /// the kind of bug this method exists to prevent.
    #[pyo3(signature = (path, default = None))]
    fn get_long(&self, path: &str, default: Option<i64>) -> PyResult<i64> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get_value(path) {
            Some(DixValue::Long(v)) => Ok(*v),
            Some(DixValue::Int(v))  => Ok(*v as i64),
            Some(other) => match default {
                Some(d) => Ok(d),
                None => Err(runtime_err("get_long", format!(
                    "'{}' is {}, not long", path, other.type_name()
                ))),
            },
            None => match default {
                Some(d) => Ok(d),
                None    => Err(runtime_err("get_long", format!("path not found: '{}'", path))),
            },
        }
    }

    /// Strict: rejects Double, Int, and Long. Was previously implemented
    /// via the lenient f64 path then narrowed to f32, which silently
    /// accepted (and truncated) Int/Long/Double — that defeats the point
    /// of having a typed getter at all.
    #[pyo3(signature = (path, default = None))]
    fn get_float(&self, path: &str, default: Option<f32>) -> PyResult<f32> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get_value(path) {
            Some(DixValue::Float(v)) => Ok(*v),
            Some(other) => match default {
                Some(d) => Ok(d),
                None => Err(runtime_err("get_float", format!(
                    "'{}' is {}, not float", path, other.type_name()
                ))),
            },
            None => match default {
                Some(d) => Ok(d),
                None    => Err(runtime_err("get_float", format!("path not found: '{}'", path))),
            },
        }
    }

    /// Widened from Float/Int/Long if needed — all three are always
    /// exact when promoted to f64 at the magnitudes DixScript configs
    /// realistically use. Matches schema.rs's own Double widening rule
    /// exactly, so this is left on the lenient DixData::get::<T>() path
    /// rather than rewritten like get_int/get_long/get_float above.
    #[pyo3(signature = (path, default = None))]
    fn get_double(&self, path: &str, default: Option<f64>) -> PyResult<f64> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get::<f64>(path) {
            Ok(v)  => Ok(v),
            Err(e) => match default {
                Some(d) => Ok(d),
                None    => Err(runtime_err("get_double", e)),
            },
        }
    }

    #[pyo3(signature = (path, default = None))]
    fn get_bool(&self, path: &str, default: Option<bool>) -> PyResult<bool> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        match self.data()?.get::<bool>(path) {
            Ok(v)  => Ok(v),
            Err(e) => match default {
                Some(d) => Ok(d),
                None    => Err(runtime_err("get_bool", e)),
            },
        }
    }

    fn get_json(&self, path: &str) -> PyResult<String> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        let data = self.data()?;
        match data.get_value(path) {
            None => Err(runtime_err("get_json", format!("path not found: '{}'", path))),
            Some(v) => serde_json::to_string(v)
                .map_err(|e| runtime_err("get_json:serialize", e)),
        }
    }

    fn get_enum_name(&self, path: &str) -> PyResult<String> {
        if path.is_empty() { return Err(invalid_path_err(path)); }
        let data = self.data()?;
        match data.get_value(path) {
            Some(DixValue::Enum { enum_name, .. }) => Ok(enum_name.clone()),
            Some(_) => Err(runtime_err("get_enum_name", format!("'{}' is not an enum", path))),
            None    => Err(runtime_err("get_enum_name", format!("path not found: '{}'", path))),
        }
    }

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

    fn try_get_long(&self, py: Python<'_>, path: &str) -> MdixResult {
        match self.get_long(path, None) {
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

    #[pyo3(signature = (indented = true))]
    fn to_json(&self, indented: bool) -> PyResult<String> {
        let data    = self.data()?;
        let entries = data.to_hashmap();
        let conv    = DixConverter::new();
        let ast     = conv.from_hashmap(entries)
            .map_err(|e| runtime_err("to_json:ast", e))?;
        let map = conv.to_hashmap(&ast);
        if indented {
            serde_json::to_string_pretty(&map)
        } else {
            serde_json::to_string(&map)
        }
        .map_err(|e| runtime_err("to_json:serialize", e))
    }

    fn to_toml(&self) -> PyResult<String> {
        let data    = self.data()?;
        let entries = data.to_hashmap();
        let conv    = DixConverter::new();
        let ast     = conv.from_hashmap(entries)
            .map_err(|e| runtime_err("to_toml:ast", e))?;
        conv.to_toml(&ast)
            .map_err(|e| runtime_err("to_toml:serialize", e))
    }

    fn to_mdix(&self) -> PyResult<String> {
        self.to_mdix_string()
    }

    /// Convert the *entire* database into a native Python dict/list
    /// structure — the dynamic-language equivalent of MidManStudio.Mdix.Core's
    /// reflection-based object serializer in C#. Implemented by reusing
    /// to_json() and handing the string to Python's own `json.loads`,
    /// rather than hand-writing a second recursive DixValue -> PyObject
    /// converter that would have to be kept in sync with the JSON one.
    ///
    /// Pairs with `MdixDatabase.from_table(table)` for a full round trip.
    fn to_table(&self, py: Python<'_>) -> PyResult<PyObject> {
        let json_str = self.to_json(true)?;
        let json_module = py.import_bound("json")
            .map_err(|e| runtime_err("to_table", format!("failed to import json module: {}", e)))?;
        let obj = json_module.call_method1("loads", (json_str,))
            .map_err(|e| runtime_err("to_table", format!("json.loads failed: {}", e)))?;
        Ok(obj.unbind())
    }

    /// Build a database directly from a dict/list structure — the reverse
    /// of to_table(). Routes through json.dumps() + from_json() rather
    /// than a hand-rolled dict walker, for the same reason to_table()
    /// routes through json.loads(): one well-tested conversion path
    /// instead of two that have to agree with each other.
    ///
    /// ```python
    /// db = MdixDatabase.from_table({"app_name": "MyGame", "port": 8080})
    /// ```
    #[staticmethod]
    fn from_table(py: Python<'_>, table: &Bound<'_, PyAny>) -> PyResult<MdixDatabase> {
        let json_module = py.import_bound("json")
            .map_err(|e| runtime_err("from_table", format!("failed to import json module: {}", e)))?;
        let json_str: String = json_module.call_method1("dumps", (table,))
            .map_err(|e| runtime_err("from_table", format!("json.dumps failed: {}", e)))?
            .extract()
            .map_err(|e| runtime_err("from_table", format!("json.dumps did not return a string: {}", e)))?;
        MdixDatabase::from_json(&json_str)
    }

    /// Validates this database against a schema built with `MdixSchema()`.
    /// Returns an `MdixValidationReport` — see schema.rs. Deliberately
    /// calls `SchemaBuilder::validate(&self, ..)` directly rather than
    /// `DixData::validate_schema(self, ..)`, since the latter takes the
    /// schema by value and PyO3 only hands us a borrowed reference here.
    fn validate_schema(&self, schema: &crate::schema::MdixSchema) -> PyResult<crate::schema::MdixValidationReport> {
        let data = self.data()?;
        let report = schema.as_builder()?.validate(data);
        Ok(crate::schema::MdixValidationReport::new(report))
    }

    /// Merges this database with `other`. Returns `(database, conflicts)`
    /// — see merge.rs for strategy names and the conflict dict shape.
    /// Neither `self` nor `other` is mutated.
    ///
    /// `temp_dir` overrides where the round-trip temp files get written —
    /// pass an app-writable cache directory on sandboxed targets where
    /// the default system temp dir may not be writable. See merge.rs.
    #[pyo3(signature = (other, strategy = None, array_strategy = None, temp_dir = None))]
    fn merge_with(
        &self,
        py: Python<'_>,
        other: &MdixDatabase,
        strategy: Option<String>,
        array_strategy: Option<String>,
        temp_dir: Option<String>,
    ) -> PyResult<(MdixDatabase, Vec<PyObject>)> {
        crate::merge::merge_with(py, self, other, strategy, array_strategy, temp_dir)
    }
}
