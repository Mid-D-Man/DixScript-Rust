//! Exception type and error construction helpers.

use pyo3::prelude::*;

pyo3::create_exception!(
    midmanstudio.mdix,
    MdixError,
    pyo3::exceptions::PyException,
    "Raised when a DixScript operation fails.\n\
     Catch this to handle any mdix error uniformly."
);

/// Raise a `MdixError` with a plain message.
pub fn to_py_err(message: impl Into<String>) -> PyErr {
    MdixError::new_err(message.into())
}

/// Raise a `MdixError` with context prefix.
pub fn runtime_err(context: &str, detail: impl std::fmt::Display) -> PyErr {
    MdixError::new_err(format!("[mdix:{}] {}", context, detail))
}

/// Raised when a method is called on a freed database or builder.
pub fn disposed_err(type_name: &str) -> PyErr {
    MdixError::new_err(format!(
        "[mdix] {} has been freed and cannot be used.", type_name
    ))
}

/// Raised when a path argument is null or empty.
pub fn invalid_path_err(path: &str) -> PyErr {
    MdixError::new_err(format!("[mdix] Path is null or empty: '{}'", path))
}

/// Raised when a two-tier builder ordering rule is violated.
pub fn two_tier_err(property_name: &str) -> PyErr {
    MdixError::new_err(format!(
        "[mdix] Cannot add flat property '{}' after table properties or group arrays. \
         Flat properties must come first (two-tier rule).",
        property_name
    ))
}
