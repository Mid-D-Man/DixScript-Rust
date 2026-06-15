// mdix-cli/src/commands/mod.rs
//! Shared command types and the CliError enum.

pub mod compact;
pub mod compile;
pub mod config;
pub mod convert;
pub mod create;
pub mod debug_ast;
pub mod debug_symbols;
pub mod debug_tokens;
pub mod decrypt;
pub mod format;
pub mod inspect;
pub mod key;
pub mod merge;
pub mod validate;

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct GlobalOpts {
    pub verbose: bool,
    pub quiet:   bool,
    pub json:    bool,
}

#[derive(Debug)]
pub enum CliError {
    FileNotFound(PathBuf),
    ParseError(String),
    CompileError(String),
    ConversionError(String),
    KeyError(String),
    ConfigError(String),
    IoError(std::io::Error),
    UnsupportedFormat(String),
    InvalidArgument(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::FileNotFound(_)      => 2,
            CliError::UnsupportedFormat(_) => 4,
            CliError::InvalidArgument(_)   => 3,
            _                              => 1,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::FileNotFound(p)      => write!(f, "File not found: {}", p.display()),
            CliError::ParseError(msg)      => write!(f, "Parse error: {}", msg),
            CliError::CompileError(msg)    => write!(f, "Compile error: {}", msg),
            CliError::ConversionError(msg) => write!(f, "Conversion error: {}", msg),
            CliError::KeyError(msg)        => write!(f, "Key error: {}", msg),
            CliError::ConfigError(msg)     => write!(f, "Config error: {}", msg),
            CliError::IoError(e)           => write!(f, "IO error: {}", e),
            CliError::UnsupportedFormat(s) => write!(f, "Unsupported format: {}", s),
            CliError::InvalidArgument(msg) => write!(f, "Invalid argument: {}", msg),
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self { CliError::IoError(e) }
}

pub fn require_file(path: &std::path::Path) -> Result<(), CliError> {
    if !path.exists() {
        Err(CliError::FileNotFound(path.to_path_buf()))
    } else {
        Ok(())
    }
}

pub fn handle_error(error: &CliError, json: bool) -> i32 {
    if json {
        eprintln!("{}", serde_json::json!({ "success": false, "error": error.to_string() }));
    } else {
        crate::output::printer::error(&error.to_string());
    }
    error.exit_code()
                }
