//! MdixResult — railway-oriented result type for Python callers.

use pyo3::prelude::*;
use pyo3::types::PyType;
use crate::error::to_py_err;

#[pyclass(module = "midmanstudio.mdix")]
#[derive(Clone)]
pub struct MdixResult {
    value:     Option<Py<PyAny>>,
    error_msg: Option<String>,
}

impl MdixResult {
    pub fn ok<V: IntoPy<Py<PyAny>>>(py: Python<'_>, value: V) -> Self {
        MdixResult {
            value:     Some(value.into_py(py)),
            error_msg: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        MdixResult {
            value:     None,
            error_msg: Some(message.into()),
        }
    }

    pub fn from_py_err(e: PyErr) -> Self {
        MdixResult::err(e.to_string())
    }
}

#[pymethods]
impl MdixResult {
    // ── Construction ───────────────────────────────────────────────────────

    #[classmethod]
    #[pyo3(name = "ok")]
    fn py_ok(_cls: &Bound<'_, PyType>, py: Python<'_>, value: PyObject) -> MdixResult {
        MdixResult::ok(py, value)
    }

    #[classmethod]
    #[pyo3(name = "err")]
    fn py_err(_cls: &Bound<'_, PyType>, message: String) -> MdixResult {
        MdixResult::err(message)
    }

    // ── State ──────────────────────────────────────────────────────────────

    #[getter]
    fn is_success(&self) -> bool {
        self.value.is_some()
    }

    #[getter]
    fn is_failure(&self) -> bool {
        self.error_msg.is_some()
    }

    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.value {
            Some(v) => Ok(v.clone_ref(py)),
            None => Err(pyo3::exceptions::PyValueError::new_err(
                "Cannot access .value on a failed MdixResult — check .is_success first.",
            )),
        }
    }

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

    fn or_raise(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.value {
            Some(v) => Ok(v.clone_ref(py)),
            None => Err(to_py_err(
                self.error_msg.as_deref().unwrap_or("unknown error"),
            )),
        }
    }

    fn unwrap(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.or_raise(py)
    }

    fn unwrap_or(&self, py: Python<'_>, fallback: PyObject) -> PyObject {
        match &self.value {
            Some(v) => v.clone_ref(py),
            None    => fallback,
        }
    }

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

    fn and_then(&self, py: Python<'_>, f: PyObject) -> PyResult<MdixResult> {
        match &self.value {
            Some(v) => {
                // Call f(value) — it must return a MdixResult.
                let next: Bound<'_, PyAny> = f.call1(py, (v.clone_ref(py),))?.into_bound(py);
                // Extract via the Bound API which does not require Clone on the
                // outer container (it borrows through the GIL token instead).
                next.extract::<MdixResult>()
            }
            None => Ok(MdixResult::err(
                self.error_msg.clone().unwrap_or_default(),
            )),
        }
    }

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

    fn or_(&self, py: Python<'_>, fallback: PyObject) -> PyResult<MdixResult> {
        if self.value.is_some() {
            Ok(MdixResult {
                value:     self.value.as_ref().map(|v| v.clone_ref(py)),
                error_msg: None,
            })
        } else {
            // Extract via Bound — avoids the Clone constraint on the outer call.
            fallback.into_bound(py).extract::<MdixResult>()
        }
    }

    // ── Branching ──────────────────────────────────────────────────────────

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

    fn tap(&self, py: Python<'_>, f: PyObject) -> PyResult<MdixResult> {
        if let Some(v) = &self.value {
            f.call1(py, (v.clone_ref(py),))?;
        }
        Ok(self.clone())
    }

    fn tap_error(&self, py: Python<'_>, f: PyObject) -> PyResult<MdixResult> {
        if let Some(e) = &self.error_msg {
            f.call1(py, (e.as_str(),))?;
        }
        Ok(self.clone())
    }

    // ── Dunder ─────────────────────────────────────────────────────────────

    fn __bool__(&self) -> bool {
        self.value.is_some()
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        match &self.value {
            Some(v) => {
                let repr = v
                    .bind(py)          // Bound<'_, PyAny> — current non-deprecated API
                    .repr()
                    .map(|r| r.to_string())
                    .unwrap_or_default();
                format!("MdixResult.ok({})", repr)
            }
            None => format!(
                "MdixResult.err('{}')",
                self.error_msg.as_deref().unwrap_or("")
            ),
        }
    }

    #[classmethod]
    fn __class_getitem__(cls: &Bound<'_, PyType>, _item: &Bound<'_, PyAny>) -> PyObject {
        cls.clone().into_any().unbind()
    }
    }
