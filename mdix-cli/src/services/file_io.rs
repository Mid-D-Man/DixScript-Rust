// mdix-cli/src/services/file_io.rs
//! File read/write helpers with `CliError` error mapping.

use std::path::{Path, PathBuf};
use crate::commands::CliError;

/// Read a text file, returning `CliError::FileNotFound` if missing and
/// `CliError::IoError` on any other failure.
pub fn read_file(path: &Path) -> Result<String, CliError> {
    if !path.exists() {
        return Err(CliError::FileNotFound(path.to_path_buf()));
    }
    std::fs::read_to_string(path).map_err(CliError::IoError)
}

/// Write text to a file, creating intermediate directories as needed.
pub fn write_file(path: &Path, content: &str) -> Result<(), CliError> {
    ensure_dir(path.parent().unwrap_or(Path::new(".")))?;
    std::fs::write(path, content).map_err(CliError::IoError)
}

/// Create a directory and all parents if they do not already exist.
pub fn ensure_dir(path: &Path) -> Result<(), CliError> {
    std::fs::create_dir_all(path).map_err(CliError::IoError)
}

/// Derive a default output path by replacing `input`'s extension.
///
/// `mdix convert foo.json --to mdix` → `foo.mdix`
pub fn default_output_path(input: &Path, new_ext: &str) -> PathBuf {
    input.with_extension(new_ext)
}

/// Derive an output path with a suffix inserted before the extension.
///
/// `compact("foo.mdix", "compact")` → `foo.compact.mdix`
pub fn suffixed_output_path(input: &Path, suffix: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("mdix");
    let parent = input.parent().unwrap_or(Path::new("."));
    parent.join(format!("{}.{}.{}", stem, suffix, ext))
}

/// Return `CliError::InvalidArgument` if `path` does not end in `.mdix`.
pub fn validate_mdix_extension(path: &Path) -> Result<(), CliError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("mdix") => Ok(()),
        _ => Err(CliError::InvalidArgument(format!(
            "'{}' does not have a .mdix extension",
            path.display()
        ))),
    }
}

/// Return `CliError::InvalidArgument` if `path` does not end in `.mdix.enc`.
pub fn validate_enc_extension(path: &Path) -> Result<(), CliError> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if name.ends_with(".mdix.enc") {
        Ok(())
    } else {
        Err(CliError::InvalidArgument(format!(
            "'{}' does not have a .mdix.enc extension",
            path.display()
        )))
    }
}

/// Human-readable file size string.
pub fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
  }
