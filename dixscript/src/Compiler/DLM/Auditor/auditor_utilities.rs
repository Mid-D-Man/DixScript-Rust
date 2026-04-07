//! Shared path resolution utilities for all auditor implementations.

use std::path::{Path, PathBuf};

/// Resolves and manages `.mdix.au` file paths, keeping audit history in the
/// source directory for consistent tracking across compilations.
pub struct AuditorPathUtils;

impl AuditorPathUtils {
    /// Resolve the audit file path.
    ///
    /// Prefers the source file's directory. If an existing audit file is found
    /// in the output directory instead, it is relocated. Returns the resolved
    /// path and a bool indicating whether a file was moved.
    pub fn resolve_audit_file_path(
        source_file_path: &str,
        output_directory: &str,
        base_name: &str,
    ) -> Result<(PathBuf, bool), String> {
        let source_dir = Path::new(source_file_path)
            .parent()
            .unwrap_or_else(|| Path::new("../../../../.."));

        let primary = source_dir.join(format!("{}.mdix.au", base_name));

        if primary.exists() {
            return Ok((primary, false));
        }

        let fallback = Path::new(output_directory).join(format!("{}.mdix.au", base_name));

        if fallback.exists() && !Self::same_path(source_dir, Path::new(output_directory)) {
            match std::fs::rename(&fallback, &primary) {
                Ok(_) => return Ok((primary, true)),
                Err(_) => return Ok((fallback, false)),
            }
        }

        Ok((primary, false))
    }

    /// Extract the file stem from a source path.
    pub fn base_name(source_file_path: &str) -> Result<String, String> {
        Path::new(source_file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("Invalid source file path: {}", source_file_path))
    }

    /// Compare two paths after canonicalisation.
    pub fn same_path(a: &Path, b: &Path) -> bool {
        match (a.canonicalize(), b.canonicalize()) {
            (Ok(ca), Ok(cb)) => ca == cb,
            _ => false,
        }
    }
      }
