// dixscript/src/Compiler/DLM/Auditor/audit_file_manager.rs
//! Centralizes all `.mdix.au` file I/O: write, rotate, lock, and read-back.
//!
//! All write operations follow: unlock → write → re-lock.
//! pub(crate) — external consumers use AuditFileParser (read-only).

use super::audit_file_data::{AuditEntryRecord, AuditFileConfig, AuditFileData};
use super::audit_file_format::{AuditFileParser, AuditFileWriter};
use crate::Compiler::Utilities::file_permissions;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub(crate) struct AuditFileManager {
    audit_file_path: String,
    max_entries:     usize,
}

impl AuditFileManager {
    pub(crate) fn new(audit_file_path: String, max_entries: usize) -> Self {
        AuditFileManager { audit_file_path, max_entries }
    }

    pub(crate) fn audit_file_path(&self) -> &str {
        &self.audit_file_path
    }

    // ── Read operations (no unlock needed — read-only files are still readable) ──

    /// Load the full audit data from disk. Returns None if file does not exist.
    pub(crate) fn load(&self) -> Option<AuditFileData> {
        let path = Path::new(&self.audit_file_path);
        if !path.exists() { return None; }
        let content = std::fs::read_to_string(path).ok()?;
        AuditFileParser::parse(&content).ok()
    }

    pub(crate) fn count_entries(&self) -> usize {
        let path = Path::new(&self.audit_file_path);
        if !path.exists() { return 0; }
        std::fs::read_to_string(path)
            .map(|c| AuditFileParser::count_entries(&c))
            .unwrap_or(0)
    }

    pub(crate) fn file_exists(&self) -> bool {
        Path::new(&self.audit_file_path).exists()
    }

    // ── Write operations (unlock → write → re-lock) ───────────────────────────

    /// Append a new entry to the audit file.
    ///
    /// Creates the file with a header if it does not exist.
    /// Rotates to an archive first if the entry limit is reached.
    pub(crate) fn append_entry(
        &self,
        entry:  &AuditEntryRecord,
        config: &AuditFileConfig,
    ) -> Result<(), String> {
        // Rotation may rename the existing file; unlock it first for Windows
        // compatibility (read-only files cannot be renamed on Windows).
        self.rotate_if_needed()?;

        let path        = Path::new(&self.audit_file_path);
        let file_exists = path.exists();

        if file_exists {
            file_permissions::set_writable(path)
                .map_err(|e| format!("Cannot unlock audit file for writing: {}", e))?;
        }

        let result = self.do_append(entry, config, file_exists);

        // Always re-lock, even if the write failed.
        if let Err(e) = file_permissions::set_readonly(path) {
            eprintln!("[AuditFileManager] Warning: could not re-lock audit file: {}", e);
        }

        result
    }

    // ── Private ───────────────────────────────────────────────────────────────

    /// Rename the audit file to an archive when the entry limit is reached.
    ///
    /// Unlocks before rename for Windows compatibility — on Windows a read-only
    /// file cannot be renamed. On Unix the rename only requires write permission
    /// on the parent directory, but we unlock anyway for safety.
    fn rotate_if_needed(&self) -> Result<(), String> {
        let count = self.count_entries();
        if count < self.max_entries { return Ok(()); }

        let path = Path::new(&self.audit_file_path);
        if path.exists() {
            file_permissions::set_writable(path)
                .map_err(|e| format!("Cannot unlock audit file for rotation: {}", e))?;
        }

        let ts      = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        // Append the timestamp suffix — works regardless of file extension.
        let archive = format!("{}.archive_{}", self.audit_file_path, ts);

        std::fs::rename(&self.audit_file_path, &archive)
            .map_err(|e| format!("Failed to rotate audit file: {}", e))?;

        // The archived file is intentionally left without a read-only lock;
        // it has been renamed so is no longer the active write target.
        Ok(())
    }

    fn do_append(
        &self,
        entry:       &AuditEntryRecord,
        config:      &AuditFileConfig,
        file_exists: bool,
    ) -> Result<(), String> {
        let existing_count = if file_exists { self.count_entries() } else { 0 };
        let mut entry      = entry.clone();
        entry.index        = existing_count + 1;

        if !file_exists {
            // New file: write header + first entry in a single atomic write.
            let content = format!(
                "{}{}",
                AuditFileWriter::write_header(config),
                AuditFileWriter::write_entry(&entry),
            );
            std::fs::write(&self.audit_file_path, content)
                .map_err(|e| format!("Failed to create audit file: {}", e))?;
        } else {
            // Existing file: append just the new entry block.
            let mut file = OpenOptions::new()
                .append(true)
                .open(&self.audit_file_path)
                .map_err(|e| format!("Failed to open audit file for append: {}", e))?;
            file.write_all(AuditFileWriter::write_entry(&entry).as_bytes())
                .map_err(|e| format!("Failed to append to audit file: {}", e))?;
        }

        Ok(())
    }
}
