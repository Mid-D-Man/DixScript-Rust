//! MidManStudio.Mdix — Python extension module (PyO3).

mod builder;
mod database;
mod error;
mod merge;
mod result;
mod schema;
mod watch;

use pyo3::prelude::*;

#[pymodule]
fn _mdix(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Exception type — must be added by name, not via add_class.
    m.add("MdixError", py.get_type_bound::<error::MdixError>())?;

    m.add_class::<result::MdixResult>()?;
    m.add_class::<database::MdixDatabase>()?;
    m.add_class::<builder::MdixBuilder>()?;
    m.add_class::<schema::MdixSchemaBuilder>()?;
    m.add_class::<schema::MdixValidationError>()?;
    m.add_class::<schema::MdixValidationReport>()?;
    m.add_class::<watch::MdixWatcher>()?;
    // MdixMerger's merge_files/merge_files_weighted/merge_strings are
    // instance methods on a builder-style class (set strategy, then merge),
    // not free functions — registered as a class, not via wrap_pyfunction.
    m.add_class::<merge::MdixMerger>()?;
    m.add("__version__", "1.0.0")?;
    Ok(())
}
