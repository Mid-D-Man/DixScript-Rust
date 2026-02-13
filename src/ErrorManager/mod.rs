// src/ErrorManager/mod.rs

//! # ErrorManager Module
//!
//! Comprehensive error handling for DixScript compilation and runtime

pub mod ErrorTypes;
pub mod Helpers;

mod error_manager;
mod diagnostic_dumper;

pub use ErrorTypes::*;
pub use Helpers::*;
pub use error_manager::{ErrorManager, DebugConfig};
pub use diagnostic_dumper::DiagnosticDumper;