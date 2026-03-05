// mdix-cli/src/services/mod.rs
//! Services wrap dixscript library calls and return `CliError` on failure.

pub mod compilation;
pub mod conversion;
pub mod file_io;
pub mod key_service;
pub mod validation;
