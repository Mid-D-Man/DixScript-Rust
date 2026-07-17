//! Services wrap dixscript library calls and return `CliError` on failure.

pub mod audit_service;
pub mod compilation;
pub mod conversion;
pub mod diff_service;
pub mod file_io;
pub mod key_service;
pub mod merge_service;
pub mod validation;
