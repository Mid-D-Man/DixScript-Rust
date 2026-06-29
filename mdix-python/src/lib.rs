// mdix-python/src/lib.rs
//! MidManStudio.Mdix — Python extension module (PyO3).

mod builder;
mod database;
mod error;
mod merge;
mod result;
mod schema;
mod watch;

use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

#[pymodule]
fn _mdix(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Exception type — must be added by name, not via add_class.
    m.add("MdixError", py.get_type_bound::<error::MdixError>())?;

    m.add_class::<result::MdixResult>()?;
    m.add_class::<database::MdixDatabase>()?;
    m.add_class::<builder::MdixBuilder>()?;
    m.add_class::<schema::MdixSchema>()?;
    m.add_class::<schema::MdixValidationReport>()?;
    m.add_class::<watch::MdixWatcher>()?;
    m.add_function(wrap_pyfunction!(merge::merge_files, m)?)?;
    m.add_function(wrap_pyfunction!(merge::merge_files_weighted, m)?)?;
    m.add("__version__", "1.0.0")?;
    Ok(())
}
