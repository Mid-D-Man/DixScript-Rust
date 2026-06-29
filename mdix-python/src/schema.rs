// mdix-python/src/schema.rs
//! MdixSchema / MdixValidationReport — schema validation for Python.
//!
//! Thin bindings over dixscript::Runtime::schema, the same core module
//! mdix-lua's schema.rs wraps. Custom validators (`require_with` /
//! `optional_with` in the Rust core) are not exposed here, for the same
//! reason as Lua: they take a `Fn(&DixData) -> Result<(), String> + Send
//! + Sync + 'static` closure, and a Python callable crossing that boundary
//! safely (GIL reacquisition, exception propagation) is its own separate
//! piece of work. The named require_*/optional_* convenience methods
//! below cover the overwhelming majority of real schema use.
//!
//! ```python
//! schema = (MdixSchema()
//!     .require_string("app_name")
//!     .require_int("port")
//!     .require_long("created_at_ms")
//!     .optional_bool("debug"))
//!
//! report = db.validate_schema(schema)
//! if not report.is_valid:
//!     print(report)
//!     for path in report.failed_paths():
//!         print("failed:", path)
//! ```

use pyo3::prelude::*;
use pyo3::types::PyDict;
use dixscript::Runtime::{SchemaBuilder, ValidationReport};
use crate::error::to_py_err;

fn consumed_err() -> PyErr {
    to_py_err("[mdix] MdixSchema has been consumed (internal state missing)")
}

// ── MdixSchema ───────────────────────────────────────────────────────────────

/// Wraps `SchemaBuilder`. The core builder's `require_*` / `optional_*` /
/// `with_description` methods consume `self` by value (fluent style), so
/// this stores `Option<SchemaBuilder>` and takes it out on every mutating
/// call — the same "take, mutate, put back" pattern `MdixBuilder` already
/// uses via `PyRefMut` for Python-side method chaining.
#[pyclass(module = "midmanstudio.mdix")]
pub struct MdixSchema {
    inner: Option<SchemaBuilder>,
}

impl MdixSchema {
    fn take(&mut self) -> PyResult<SchemaBuilder> {
        self.inner.take().ok_or_else(consumed_err)
    }

    /// Used by `MdixDatabase.validate_schema` (in database.rs) to call
    /// `SchemaBuilder::validate(&self, data)` directly — deliberately NOT
    /// going through `DixData::validate_schema`, since that one takes the
    /// schema *by value*, and PyO3 only hands us a borrowed `&MdixSchema`
    /// there.
    pub(crate) fn as_builder(&self) -> PyResult<&SchemaBuilder> {
        self.inner.as_ref().ok_or_else(consumed_err)
    }
}

#[pymethods]
impl MdixSchema {
    #[new]
    fn new() -> Self {
        MdixSchema { inner: Some(SchemaBuilder::new()) }
    }

    fn __repr__(&self) -> String {
        format!(
            "MdixSchema(fields={})",
            self.inner.as_ref().map(|b| b.field_count()).unwrap_or(0)
        )
    }

    // ── required ─────────────────────────────────────────────────────────

    fn require_string(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.require_string(path));
        Ok(slf.into())
    }
    fn require_int(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.require_int(path));
        Ok(slf.into())
    }
    /// Requires a 64-bit integer field. Also accepts Int values (an i32
    /// widens into the i64 field with no precision loss).
    fn require_long(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.require_long(path));
        Ok(slf.into())
    }
    fn require_float(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.require_float(path));
        Ok(slf.into())
    }
    fn require_double(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.require_double(path));
        Ok(slf.into())
    }
    fn require_bool(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.require_bool(path));
        Ok(slf.into())
    }
    fn require_array(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.require_array(path));
        Ok(slf.into())
    }
    fn require_object(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.require_object(path));
        Ok(slf.into())
    }
    fn require_enum(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.require_enum(path));
        Ok(slf.into())
    }

    // ── optional ─────────────────────────────────────────────────────────

    fn optional_string(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.optional_string(path));
        Ok(slf.into())
    }
    fn optional_int(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.optional_int(path));
        Ok(slf.into())
    }
    fn optional_long(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.optional_long(path));
        Ok(slf.into())
    }
    fn optional_float(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.optional_float(path));
        Ok(slf.into())
    }
    fn optional_double(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.optional_double(path));
        Ok(slf.into())
    }
    fn optional_bool(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.optional_bool(path));
        Ok(slf.into())
    }
    fn optional_array(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.optional_array(path));
        Ok(slf.into())
    }
    fn optional_object(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.optional_object(path));
        Ok(slf.into())
    }

    // ── metadata ─────────────────────────────────────────────────────────

    /// Annotates the most recently added field with a description.
    fn with_description(mut slf: PyRefMut<'_, Self>, description: &str) -> PyResult<Py<Self>> {
        let b = slf.take()?;
        slf.inner = Some(b.with_description(description));
        Ok(slf.into())
    }

    #[getter]
    fn field_count(&self) -> PyResult<i64> {
        Ok(self.as_builder()?.field_count() as i64)
    }

    fn paths(&self) -> PyResult<Vec<String>> {
        Ok(self.as_builder()?.paths().into_iter().map(String::from).collect())
    }
}

// ── MdixValidationReport ──────────────────────────────────────────────────────

/// Wraps `ValidationReport`, returned by `MdixDatabase.validate_schema`.
#[pyclass(module = "midmanstudio.mdix")]
pub struct MdixValidationReport {
    inner: ValidationReport,
}

impl MdixValidationReport {
    pub(crate) fn new(report: ValidationReport) -> Self {
        MdixValidationReport { inner: report }
    }
}

#[pymethods]
impl MdixValidationReport {
    /// `True` when no validation errors were found.
    #[getter]
    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }

    #[getter]
    fn error_count(&self) -> i64 {
        self.inner.error_count() as i64
    }

    /// Dotted paths that failed validation, in order.
    fn failed_paths(&self) -> Vec<String> {
        self.inner.failed_paths().into_iter().map(String::from).collect()
    }

    /// All errors as a list of dicts:
    /// `{"path": ..., "expected": ..., "actual": ..., "kind": ...}` where
    /// kind is one of "Missing" | "WrongType" | "InvalidValue".
    fn errors(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let mut out = Vec::with_capacity(self.inner.errors.len());
        for e in &self.inner.errors {
            let d = PyDict::new_bound(py);
            d.set_item("path", &e.path)?;
            d.set_item("expected", &e.expected)?;
            d.set_item("actual", &e.actual)?;
            d.set_item("kind", e.kind.to_string())?;
            out.push(d.into_py(py));
        }
        Ok(out)
    }

    /// Human-readable multi-line summary, identical to `str(report)`.
    fn to_string(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        self.inner.to_string()
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }
}
