

//! # ErrorManager Module
//!
//! Comprehensive error handling for DixScript compilation and runtime
//! Uses Rust's Result<T, E> pattern with strongly-typed error categories

mod operational_settings;
mod error_manager;
mod ErrorTypes;
// Public re-exports
pub use operational_settings::{CompatibilityMode, DebugMode, ErrorHandlingStrategy, OperationalSettings};
pub use error_manager::ErrorManager;
pub use ErrorTypes::*;
