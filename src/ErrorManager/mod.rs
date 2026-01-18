//! # ErrorManager Module
//!
//! Comprehensive error handling for DixScript compilation and runtime

pub mod ErrorTypes;
mod operational_settings;
mod error_manager;

pub use ErrorTypes::*;
pub use operational_settings::{OperationalSettings, ErrorHandlingStrategy, DebugMode, CompatibilityMode};
pub use error_manager::ErrorManager;