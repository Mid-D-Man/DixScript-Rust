//! MidManStudio.Mdix — Python extension module (PyO3).

mod builder;
mod database;
mod error;
mod result;

use pyo3::prelude::*;

#[pymodule]
fn _mdix(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Exception type — must be added by name, not via add_class.
    m.add("MdixError", py.get_type_bound::<error::MdixError>())?;

    m.add_class::<result::MdixResult>()?;
    m.add_class::<database::MdixDatabase>()?;
    m.add_class::<builder::MdixBuilder>()?;
    m.add("__version__", "1.0.0")?;
    Ok(())
  }
