//! MdixResult — railway-oriented result type for Python callers.

use pyo3::prelude::*;
use pyo3::types::PyType;
use crate::error::to_py_err;

/// Railway-oriented result type.
///
/// Every `try_*` method on `MdixDatabase` and `MdixBuilder` returns a
/// `MdixResult[T]` instead of raising. Use `MdixResult` to chain
/// transformations without try/except at every call site.
///
/// ```python
/// from midmanstudio.mdix import MdixDatabase
///
/// port = (MdixDatabase.try_load_str("@DATA( port = 8080 )")
///         .and_then(lambda db: db.try_get_int("port"))
///         .map(lambda p: p * 2)
///         .unwrap_or(0))
/// ```
#[pyclass(module = "midmanstudio.mdix")]
pub struct MdixResult {
    value:     Option<Py<PyAny>>,
    error_msg: Option<String>,
}

impl MdixResult {
    /// Construct a successful result wrapping any Python-compatible value.
    pub fn ok<V: IntoPy<Py<PyAny>>>(py: Python<'_>, value: V) -> Self {
        MdixResult {
            value:     Some(value.into_py(py)),
            error_msg: None,
        }
    }

    /// Construct a failed result from a message string.
    pub fn err(message: impl Into<String>) -> Self {
        MdixResult {
            value:     None,
            error_msg: Some(message.into()),
        }
    }

    /// Construct a failed result from an existing `PyErr` (extracts message).
    pub fn from_py_err(e: PyErr) -> Self {
        MdixResult::err(e.to_string())
    }
}

#[pymethods]
impl MdixResult {
    // ── Construction (callable from Python) ───────────────────────────────

    /// Create a successful result: `MdixResult.ok(42)`.
    #[classmethod]
    #[pyo3(name = "ok")]
    fn py_ok(_cls: &Bound<'_, PyType>, py: Python<'_>, value: PyObject) -> MdixResult {
        MdixResult::ok(py, value)
    }

    /// Create a failed result: `MdixResult.err("not found")`.
    #[classmethod]
    #[pyo3(name = "err")]
    fn py_err(_cls: &Bound<'_, PyType>, message: String) -> MdixResult {
        MdixResult::err(message)
    }

    // ── State ──────────────────────────────────────────────────────────────

    /// `True` when the operation succeeded.
    #[getter]
    fn is_success(&self) -> bool {
        self.value.is_some()
    }

    /// `True` when the operation failed.
    #[getter]
    fn is_failure(&self) -> bool {
        self.error_msg.is_some()
    }

    /// The success value. Raises `ValueError` if accessed on a failure.
    /// Prefer `unwrap_or` or `or_raise` for safe access.
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.value {
            Some(v) => Ok(v.clone_ref(py)),
            None => Err(pyo3::exceptions::PyValueError::new_err(
                "Cannot access .value on a failed MdixResult — check .is_success first.",
            )),
        }
    }

    /// The error message string. Raises `ValueError` if accessed on a success.
    #[getter]
    fn error(&self) -> PyResult<&str> {
        match &self.error_msg {
            Some(e) => Ok(e.as_str()),
            None => Err(pyo3::exceptions::PyValueError::new_err(
                "Cannot access .error on a successful MdixResult.",
            )),
        }
    }

    // ── Unwrapping ─────────────────────────────────────────────────────────

    /// Returns the success value or raises `MdixError`.
    fn or_raise(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.value {
            Some(v) => Ok(v.clone_ref(py)),
            None => Err(to_py_err(
                self.error_msg.as_deref().unwrap_or("unknown error"),
            )),
        }
    }

    /// Alias for `or_raise` — familiar to Rust users.
    fn unwrap(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.or_raise(py)
    }

    /// Returns the success value or `fallback` if this is a failure.
    fn unwrap_or(&self, py: Python<'_>, fallback: PyObject) -> PyObject {
        match &self.value {
            Some(v) => v.clone_ref(py),
            None    => fallback,
        }
    }

    /// Returns the success value or the result of calling `factory(error_message)`.
    fn unwrap_or_else(&self, py: Python<'_>, factory: PyObject) -> PyResult<PyObject> {
        match &self.value {
            Some(v) => Ok(v.clone_ref(py)),
            None => factory.call1(
                py,
                (self.error_msg.as_deref().unwrap_or(""),),
            ),
        }
    }

    // ── Transformation ─────────────────────────────────────────────────────

    /// Maps the success value with `f(value) -> new_value`.
    /// Failures are forwarded unchanged.
    ///
    /// ```python
    /// result = db.try_get_int("port").map(lambda p: p * 2)
    /// ```
    fn map(&self, py: Python<'_>, f: PyObject) -> PyResult<MdixResult> {
        match &self.value {
            Some(v) => {
                let new_val = f.call1(py, (v.clone_ref(py),))?;
                Ok(MdixResult::ok(py, new_val))
            }
            None => Ok(MdixResult::err(
                self.error_msg.clone().unwrap_or_default(),
            )),
        }
    }

    /// Chains a function that returns a `MdixResult`.
    /// Failures short-circuit.
    ///
    /// ```python
    /// result = (MdixDatabase.try_load_str(source)
    ///           .and_then(lambda db: db.try_get_int("port")))
    /// ```
    fn and_then(&self, py: Python<'_>, f: PyObject) -> PyResult<MdixResult> {
        match &self.value {
            Some(v) => {
                let next = f.call1(py, (v.clone_ref(py),))?;
                next.extract::<MdixResult>(py)
            }
            None => Ok(MdixResult::err(
                self.error_msg.clone().unwrap_or_default(),
            )),
        }
    }

    /// Validates the success value with a predicate.
    /// Returns `MdixResult.err(error_message)` if the predicate returns `False`.
    ///
    /// ```python
    /// result = db.try_get_int("port").ensure(lambda p: p > 1024, "port must be > 1024")
    /// ```
    fn ensure(
        &self,
        py: Python<'_>,
        predicate: PyObject,
        error_message: String,
    ) -> PyResult<MdixResult> {
        match &self.value {
            Some(v) => {
                let passes: bool = predicate
                    .call1(py, (v.clone_ref(py),))?
                    .extract(py)?;
                if passes {
                    Ok(MdixResult {
                        value:     Some(v.clone_ref(py)),
                        error_msg: None,
                    })
                } else {
                    Ok(MdixResult::err(error_message))
                }
            }
            None => Ok(MdixResult::err(
                self.error_msg.clone().unwrap_or_default(),
            )),
        }
    }

    /// Returns this result on success, or `fallback` on failure.
    fn or_(&self, py: Python<'_>, fallback: PyObject) -> PyResult<MdixResult> {
        if self.value.is_some() {
            Ok(MdixResult {
                value:     self.value.as_ref().map(|v| v.clone_ref(py)),
                error_msg: None,
            })
        } else {
            fallback.extract::<MdixResult>(py)
        }
    }

    // ── Branching ──────────────────────────────────────────────────────────

    /// Calls `on_success(value)` or `on_failure(error)` and returns the result.
    /// Analogous to `match` / `fold` in functional programming.
    ///
    /// ```python
    /// message = result.fold(
    ///     on_success=lambda v: f"Got {v}",
    ///     on_failure=lambda e: f"Error: {e}"
    /// )
    /// ```
    #[pyo3(signature = (on_success, on_failure))]
    fn fold(
        &self,
        py: Python<'_>,
        on_success: PyObject,
        on_failure: PyObject,
    ) -> PyResult<PyObject> {
        match &self.value {
            Some(v) => on_success.call1(py, (v.clone_ref(py),)),
            None    => on_failure.call1(py, (self.error_msg.as_deref().unwrap_or(""),)),
        }
    }

    // ── Side effects ───────────────────────────────────────────────────────

    /// Calls `f(value)` on success without transforming. Returns `self`.
    fn tap(&self, py: Python<'_>, f: PyObject) -> PyResult<MdixResult> {
        if let Some(v) = &self.value {
            f.call1(py, (v.clone_ref(py),))?;
        }
        Ok(MdixResult {
            value:     self.value.as_ref().map(|v| v.clone_ref(py)),
            error_msg: self.error_msg.clone(),
        })
    }

    /// Calls `f(error_message)` on failure without transforming. Returns `self`.
    fn tap_error(&self, py: Python<'_>, f: PyObject) -> PyResult<MdixResult> {
        if let Some(e) = &self.error_msg {
            f.call1(py, (e.as_str(),))?;
        }
        Ok(MdixResult {
            value:     self.value.as_ref().map(|v| v.clone_ref(py)),
            error_msg: self.error_msg.clone(),
        })
    }

    // ── Dunder ─────────────────────────────────────────────────────────────

    /// `bool(result)` is `True` for success, `False` for failure.
    fn __bool__(&self) -> bool {
        self.value.is_some()
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        match &self.value {
            Some(v) => format!(
                "MdixResult.ok({})",
                v.as_ref(py).repr().map(|r| r.to_string()).unwrap_or_default()
            ),
            None => format!(
                "MdixResult.err('{}')",
                self.error_msg.as_deref().unwrap_or("")
            ),
        }
    }

    /// Enables `MdixResult[int]` subscript syntax for type hints.
    #[classmethod]
    fn __class_getitem__(cls: &Bound<'_, PyType>, _item: &Bound<'_, PyAny>) -> PyObject {
        cls.clone().into_any().unbind()
    }
  }
