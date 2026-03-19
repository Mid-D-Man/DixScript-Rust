//! OS-level read/write permission helpers for compiler-managed output files.
//!
//! All three DLM output types (.mdix.enc, .mdix.key, .mdix.au) are locked
//! read-only immediately after the compiler writes them. The compiler unlocks,
//! writes, then re-locks in a single critical section.
//!
//! On WASM there is no persistent filesystem — all functions compile away.

use std::path::Path;

/// Set the file at `path` to read-only at the OS level.
///
/// Called immediately after every compiler write to a managed output file.
pub fn set_readonly(path: &Path) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if !path.exists() {
            return Ok(());
        }
        let mut perms = std::fs::metadata(path)
            .map_err(|e| format!("Cannot read metadata for '{}': {}", path.display(), e))?
            .permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(path, perms)
            .map_err(|e| format!("Cannot lock '{}' read-only: {}", path.display(), e))?;
    }
    Ok(())
}

/// Temporarily make the file at `path` writable so the compiler can overwrite it.
///
/// Only called during recompilation when the file already exists.
/// Always followed immediately by `set_readonly`.
pub fn set_writable(path: &Path) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if !path.exists() {
            return Ok(());
        }
        let mut perms = std::fs::metadata(path)
            .map_err(|e| format!("Cannot read metadata for '{}': {}", path.display(), e))?
            .permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(path, perms)
            .map_err(|e| format!("Cannot unlock '{}' for writing: {}", path.display(), e))?;
    }
    Ok(())
}

/// Returns true if the file is currently read-only. Always false on WASM.
pub fn is_readonly(path: &Path) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        return std::fs::metadata(path)
            .map(|m| m.permissions().readonly())
            .unwrap_or(false);
    }
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
          }
