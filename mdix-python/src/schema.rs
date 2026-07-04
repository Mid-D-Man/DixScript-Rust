//! MdixSchemaBuilder / MdixValidationReport — Python binding for schema validation.
//!
//! Wraps `dixscript::Runtime::schema::{SchemaBuilder, ExpectedValueType,
//! ValidationReport}`.
//!
//! NOT bound yet: `require_with`/`optional_with` (custom Rust-closure
//! validators). The underlying closure type is
//! `Fn(&DixData) -> Result<(), String> + Send + Sync`, and an arbitrary
//! Python callable isn't `Sync`. Supporting it needs a wrapper that
//! re-acquires the GIL inside the closure body and marshals `&DixData`
//! into a transient read-only view Python can call back into — real
//! complexity, deliberately deferred rather than rushed into this pass.

use pyo3::prelude::*;

use dixscript::Runtime::schema::{
    ExpectedValueType, SchemaBuilder as CoreSchemaBuilder, ValidationErrorKind, ValidationReport,
};

use crate::database::MdixDatabase;
use crate::error::to_py_err;

fn parse_expected_type(s: &str) -> PyResult<ExpectedValueType> {
    match s {
        "string"    => Ok(ExpectedValueType::String),
        "int"       => Ok(ExpectedValueType::Int),
        "long"      => Ok(ExpectedValueType::Long),
        "float"     => Ok(ExpectedValueType::Float),
        "double"    => Ok(ExpectedValueType::Double),
        "bool"      => Ok(ExpectedValueType::Bool),
        "array"     => Ok(ExpectedValueType::Array),
        "object"    => Ok(ExpectedValueType::Object),
        "date"      => Ok(ExpectedValueType::Date),
        "timestamp" => Ok(ExpectedValueType::Timestamp),
        "hexcolor"  => Ok(ExpectedValueType::HexColor),
        "blob"      => Ok(ExpectedValueType::Blob),
        "regex"     => Ok(ExpectedValueType::Regex),
        "enum"      => Ok(ExpectedValueType::Enum),
        "any"       => Ok(ExpectedValueType::Any),
        other => Err(to_py_err(format!(
            "[mdix] Unknown expected type '{}'. Expected one of: string, int, long, \
             float, double, bool, array, object, date, timestamp, hexcolor, blob, \
             regex, enum, any.",
            other
        ))),
    }
}

fn kind_to_str(kind: &ValidationErrorKind) -> &'static str {
    match kind {
        ValidationErrorKind::Missing      => "missing",
        ValidationErrorKind::WrongType    => "wrong_type",
        ValidationErrorKind::InvalidValue => "invalid_value",
    }
}

// ── MdixValidationError ─────────────────────────────────────────────────────

/// A single field-validation failure.
#[pyclass(module = "midmanstudio.mdix")]
#[derive(Clone)]
pub struct MdixValidationError {
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    expected: String,
    #[pyo3(get)]
    actual: String,
    /// "missing" | "wrong_type" | "invalid_value"
    #[pyo3(get)]
    kind: String,
}

#[pymethods]
impl MdixValidationError {
    fn __repr__(&self) -> String {
        format!(
            "MdixValidationError(path={:?}, kind={:?}, expected={:?}, actual={:?})",
            self.path, self.kind, self.expected, self.actual
        )
    }

    fn __str__(&self) -> String {
        format!(
            "[{}] '{}': expected {}, got {}",
            self.kind, self.path, self.expected, self.actual
        )
    }
}

// ── MdixValidationReport ────────────────────────────────────────────────────

/// The result of a schema validation pass. Never raises — always returned.
#[pyclass(module = "midmanstudio.mdix")]
pub struct MdixValidationReport {
    errors: Vec<MdixValidationError>,
}

impl MdixValidationReport {
    pub(crate) fn from_core(report: ValidationReport) -> Self {
        let errors = report
            .errors
            .into_iter()
            .map(|e| MdixValidationError {
                path:     e.path,
                expected: e.expected,
                actual:   e.actual,
                kind:     kind_to_str(&e.kind).to_string(),
            })
            .collect();
        MdixValidationReport { errors }
    }
}

#[pymethods]
impl MdixValidationReport {
    #[getter]
    fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    #[getter]
    fn error_count(&self) -> usize {
        self.errors.len()
    }

    #[getter]
    fn errors(&self) -> Vec<MdixValidationError> {
        self.errors.clone()
    }

    fn failed_paths(&self) -> Vec<String> {
        self.errors.iter().map(|e| e.path.clone()).collect()
    }

    /// Filter errors by kind: "missing" | "wrong_type" | "invalid_value".
    fn errors_of_kind(&self, kind: &str) -> Vec<MdixValidationError> {
        self.errors.iter().filter(|e| e.kind == kind).cloned().collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "MdixValidationReport(is_valid={}, error_count={})",
            self.is_valid(),
            self.error_count()
        )
    }

    fn __str__(&self) -> String {
        if self.is_valid() {
            "Validation passed.".to_string()
        } else {
            format!(
                "Validation failed with {} error(s):\n{}",
                self.errors.len(),
                self.errors
                    .iter()
                    .map(|e| e.__str__())
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        }
    }

    fn __bool__(&self) -> bool {
        self.is_valid()
    }
}

// ── MdixSchemaBuilder ────────────────────────────────────────────────────────

/// Fluent schema definition + validation against an `MdixDatabase`.
///
/// ```python
/// from midmanstudio.mdix import MdixSchemaBuilder
///
/// report = (MdixSchemaBuilder()
///           .require_string("app_name")
///           .require_int("port")
///           .optional_bool("debug")
///           .validate(db))
///
/// if not report.is_valid:
///     for err in report.errors:
///         print(err)
/// ```
///
/// Unlike `MdixBuilder`, this builder is NOT single-use — `validate()`
/// borrows rather than consumes, so the same schema can validate multiple
/// databases (matches the underlying Rust `SchemaBuilder::validate(&self)`
/// signature, which the doc comment explicitly calls out as reusable).
#[pyclass(module = "midmanstudio.mdix")]
pub struct MdixSchemaBuilder {
    inner: Option<CoreSchemaBuilder>,
}

impl MdixSchemaBuilder {
    /// Fluent Rust methods (`require_string`, `optional`, etc.) take `self`
    /// by value, not `&mut self` — `Option::take` lets us move the inner
    /// builder out through a `&mut self` PyO3 method, call the consuming
    /// method, then put the result back.
    fn take(&mut self) -> PyResult<CoreSchemaBuilder> {
        self.inner
            .take()
            .ok_or_else(|| to_py_err("[mdix] MdixSchemaBuilder is in an invalid state"))
    }

    pub(crate) fn borrow(&self) -> PyResult<&CoreSchemaBuilder> {
        self.inner
            .as_ref()
            .ok_or_else(|| to_py_err("[mdix] MdixSchemaBuilder is in an invalid state"))
    }
}

#[pymethods]
impl MdixSchemaBuilder {
    #[new]
    fn new() -> Self {
        MdixSchemaBuilder { inner: Some(CoreSchemaBuilder::new()) }
    }

    /// Add a required field with an explicit type string. See the module
    /// docstring for the full list of accepted type names.
    fn require(mut slf: PyRefMut<'_, Self>, path: &str, expected_type: &str) -> PyResult<Py<Self>> {
        let ty = parse_expected_type(expected_type)?;
        let builder = slf.take()?;
        slf.inner = Some(builder.require(path.to_string(), ty));
        Ok(slf.into())
    }

    fn require_string(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.require_string(path.to_string()));
        Ok(slf.into())
    }

    fn require_int(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.require_int(path.to_string()));
        Ok(slf.into())
    }

    fn require_long(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.require_long(path.to_string()));
        Ok(slf.into())
    }

    fn require_float(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.require_float(path.to_string()));
        Ok(slf.into())
    }

    fn require_double(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.require_double(path.to_string()));
        Ok(slf.into())
    }

    fn require_bool(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.require_bool(path.to_string()));
        Ok(slf.into())
    }

    fn require_array(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.require_array(path.to_string()));
        Ok(slf.into())
    }

    fn require_object(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.require_object(path.to_string()));
        Ok(slf.into())
    }

    fn require_enum(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.require_enum(path.to_string()));
        Ok(slf.into())
    }

    /// Add an optional field with an explicit type string.
    fn optional(mut slf: PyRefMut<'_, Self>, path: &str, expected_type: &str) -> PyResult<Py<Self>> {
        let ty = parse_expected_type(expected_type)?;
        let builder = slf.take()?;
        slf.inner = Some(builder.optional(path.to_string(), ty));
        Ok(slf.into())
    }

    fn optional_string(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.optional_string(path.to_string()));
        Ok(slf.into())
    }

    fn optional_int(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.optional_int(path.to_string()));
        Ok(slf.into())
    }

    fn optional_long(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.optional_long(path.to_string()));
        Ok(slf.into())
    }

    fn optional_float(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.optional_float(path.to_string()));
        Ok(slf.into())
    }

    fn optional_double(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.optional_double(path.to_string()));
        Ok(slf.into())
    }

    fn optional_bool(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.optional_bool(path.to_string()));
        Ok(slf.into())
    }

    fn optional_array(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.optional_array(path.to_string()));
        Ok(slf.into())
    }

    fn optional_object(mut slf: PyRefMut<'_, Self>, path: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.optional_object(path.to_string()));
        Ok(slf.into())
    }

    /// Attach a human-readable description to the most recently added field.
    fn with_description(mut slf: PyRefMut<'_, Self>, description: &str) -> PyResult<Py<Self>> {
        let builder = slf.take()?;
        slf.inner = Some(builder.with_description(description.to_string()));
        Ok(slf.into())
    }

    #[getter]
    fn field_count(&self) -> PyResult<usize> {
        Ok(self.borrow()?.field_count())
    }

    #[getter]
    fn paths(&self) -> PyResult<Vec<String>> {
        Ok(self.borrow()?.paths().into_iter().map(|s| s.to_string()).collect())
    }

    /// Validate `db` against this schema. Borrows — the same schema can be
    /// reused to validate multiple databases.
    fn validate(&self, db: &MdixDatabase) -> PyResult<MdixValidationReport> {
        let builder = self.borrow()?;
        let data = db.data()?;
        Ok(MdixValidationReport::from_core(builder.validate(data)))
    }
            }
