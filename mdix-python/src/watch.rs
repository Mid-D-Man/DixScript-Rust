//! MdixWatcher — hot reload for Python.
//!
//! Thin binding over dixscript::Runtime::HotReloadWatcher — see
//! dixscript/src/Runtime/hot_reload.rs for why this is poll-based rather
//! than OS-event-based.
//!
//! ```python
//! watcher = MdixWatcher("config.mdix")
//!
//! # in your update loop / tick / timer callback:
//! db, changed = watcher.check()
//! if changed:
//!     apply_new_config(db)
//! ```
//!
//! `db` is `None` when nothing changed (`changed == False`) — keep using
//! whatever database instance you already have in that case.

use pyo3::prelude::*;
use dixscript::Runtime::HotReloadWatcher;
use crate::database::MdixDatabase;
use crate::error::runtime_err;

#[pyclass(module = "midmanstudio.mdix")]
pub struct MdixWatcher {
    inner: HotReloadWatcher,
}

#[pymethods]
impl MdixWatcher {
    #[new]
    fn new(path: &str) -> Self {
        MdixWatcher { inner: HotReloadWatcher::new(path) }
    }

    /// Reloads only if the watched file's modified-time has changed
    /// since the last successful check (or since construction, on the
    /// first call). Returns `(database_or_none, changed_bool)`.
    fn check(&mut self) -> PyResult<(Option<MdixDatabase>, bool)> {
        match self.inner.check_and_reload() {
            Ok(Some(data)) => Ok((Some(MdixDatabase::from_data_pub(data)), true)),
            Ok(None)       => Ok((None, false)),
            Err(e)         => Err(runtime_err("watch.check", e)),
        }
    }

    /// Reloads unconditionally, regardless of whether the file changed.
    fn force_reload(&mut self) -> PyResult<MdixDatabase> {
        self.inner.force_reload()
            .map(MdixDatabase::from_data_pub)
            .map_err(|e| runtime_err("watch.force_reload", e))
    }

    /// Checks whether the file has changed without reloading it.
    fn has_changed(&self) -> PyResult<bool> {
        self.inner.has_changed().map_err(|e| runtime_err("watch.has_changed", e))
    }

    /// `True` once a successful reload has happened at least once.
    #[getter]
    fn has_loaded(&self) -> bool {
        self.inner.has_loaded()
    }

    #[getter]
    fn path(&self) -> String {
        self.inner.path().to_string_lossy().to_string()
    }

    fn __repr__(&self) -> String {
        format!("MdixWatcher(path=\"{}\")", self.inner.path().display())
    }
}
