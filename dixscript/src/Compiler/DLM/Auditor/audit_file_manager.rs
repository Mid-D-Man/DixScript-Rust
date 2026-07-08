//! Centralizes all `.mdix.au` audit-trail I/O: write, rotate, lock, and
//! read-back. Two backends, same split as `CloudFileCache`
//! (ImportsResolution/cloud_file_cache.rs) — same public API either way,
//! so `enhanced_auditor.rs`/`diy_auditor.rs` need zero changes.
//!
//! - **Native**: real filesystem. unlock -> write -> re-lock around every
//!   write, true OS-level append for new entries, `fs::rename` for
//!   rotation to an archive file.
//! - **wasm32**: browser `localStorage`. `AuditorPathUtils::
//!   resolve_audit_file_path` already degrades safely on this target (no
//!   real fs there, `.exists()` always false, so it just returns a
//!   synthetic, never-actually-touched path) — that string is treated
//!   here purely as an *identifier*, not a filesystem path. localStorage
//!   has no native "append", "rename", or file-locking concept: append
//!   becomes a read-modify-write of the whole value (same pattern
//!   `CloudFileCache` already uses), rotation becomes copying the value to
//!   an archive key, and locking is simply a no-op (nothing else in the
//!   same browser tab can race a synchronous Rust call anyway).

use super::audit_file_data::{AuditEntryRecord, AuditFileConfig, AuditFileData};
use super::audit_file_format::{AuditFileParser, AuditFileWriter};

// ─────────────────────────────────────────────────────────────────────────────
// Native backend — local filesystem
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
mod native_audit {
    use super::{AuditEntryRecord, AuditFileConfig, AuditFileData, AuditFileParser, AuditFileWriter};
    use crate::Compiler::Utilities::file_permissions;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::Path;

    pub struct AuditFileManager {
        audit_file_path: String,
        max_entries:     usize,
    }

    impl AuditFileManager {
        pub fn new(audit_file_path: String, max_entries: usize) -> Self {
            AuditFileManager { audit_file_path, max_entries }
        }

        pub fn audit_file_path(&self) -> &str {
            &self.audit_file_path
        }

        // ── Read operations ─────────────────────────────────────────────

        /// Load the full audit data from disk. Returns None if file does not exist.
        pub fn load(&self) -> Option<AuditFileData> {
            let path = Path::new(&self.audit_file_path);
            if !path.exists() { return None; }
            let content = std::fs::read_to_string(path).ok()?;
            AuditFileParser::parse(&content).ok()
        }

        /// Raw file content, unparsed — for callers (like
        /// `EnhancedAuditor::load_previous_audit`) that regex-scan the
        /// text directly rather than going through `AuditFileData`.
        pub fn read_raw(&self) -> Option<String> {
            let path = Path::new(&self.audit_file_path);
            if !path.exists() { return None; }
            std::fs::read_to_string(path).ok()
        }

        pub fn count_entries(&self) -> usize {
            let path = Path::new(&self.audit_file_path);
            if !path.exists() { return 0; }
            std::fs::read_to_string(path)
                .map(|c| AuditFileParser::count_entries(&c))
                .unwrap_or(0)
        }

        pub fn file_exists(&self) -> bool {
            Path::new(&self.audit_file_path).exists()
        }

        // ── Write operations (unlock -> write -> re-lock) ───────────────

        /// Append a new entry to the audit file.
        ///
        /// Creates the file with a header if it does not exist.
        /// Rotates to an archive first if the entry limit is reached.
        pub fn append_entry(
            &self,
            entry:  &AuditEntryRecord,
            config: &AuditFileConfig,
        ) -> Result<(), String> {
            self.rotate_if_needed()?;

            let path        = Path::new(&self.audit_file_path);
            let file_exists = path.exists();

            if file_exists {
                file_permissions::set_writable(path)
                    .map_err(|e| format!("Cannot unlock audit file for writing: {}", e))?;
            }

            let result = self.do_append(entry, config, file_exists);

            if let Err(e) = file_permissions::set_readonly(path) {
                eprintln!("[AuditFileManager] Warning: could not re-lock audit file: {}", e);
            }

            result
        }

        // ── Private ──────────────────────────────────────────────────────

        /// Rename the audit file to an archive when the entry limit is reached.
        fn rotate_if_needed(&self) -> Result<(), String> {
            let count = self.count_entries();
            if count < self.max_entries { return Ok(()); }

            let path = Path::new(&self.audit_file_path);
            if path.exists() {
                file_permissions::set_writable(path)
                    .map_err(|e| format!("Cannot unlock audit file for rotation: {}", e))?;
            }

            let ts      = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let archive = format!("{}.archive_{}", self.audit_file_path, ts);

            std::fs::rename(&self.audit_file_path, &archive)
                .map_err(|e| format!("Failed to rotate audit file: {}", e))?;

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
                let content = format!(
                    "{}{}",
                    AuditFileWriter::write_header(config),
                    AuditFileWriter::write_entry(&entry),
                );
                std::fs::write(&self.audit_file_path, content)
                    .map_err(|e| format!("Failed to create audit file: {}", e))?;
            } else {
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
}

// ─────────────────────────────────────────────────────────────────────────────
// wasm32 backend — browser localStorage
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod wasm_audit {
    use super::{AuditEntryRecord, AuditFileConfig, AuditFileData, AuditFileParser, AuditFileWriter};

    /// Every key this manager owns in localStorage is prefixed so multiple
    /// audited files (different `audit_file_path` identifiers) never
    /// collide, and so a future "clear all mdix audit data" helper could
    /// enumerate just these entries the same way CloudFileCache's
    /// `clear_cache` does for its own prefix.
    const KEY_PREFIX: &str = "mdix_audit:";

    pub struct AuditFileManager {
        /// Despite the name (kept identical to the native backend's field
        /// for symmetry), this is a localStorage key, not a filesystem
        /// path — built from whatever `AuditorPathUtils::
        /// resolve_audit_file_path` returned, which is a synthetic,
        /// never-touched value on this target already.
        audit_key:   String,
        max_entries: usize,
    }

    impl AuditFileManager {
        pub fn new(audit_file_path: String, max_entries: usize) -> Self {
            AuditFileManager {
                audit_key: format!("{}{}", KEY_PREFIX, audit_file_path),
                max_entries,
            }
        }

        pub fn audit_file_path(&self) -> &str {
            &self.audit_key
        }

        fn storage() -> Option<web_sys::Storage> {
            web_sys::window()?.local_storage().ok()?
        }

        // ── Read operations ─────────────────────────────────────────────

        pub fn load(&self) -> Option<AuditFileData> {
            let storage = Self::storage()?;
            let content = storage.get_item(&self.audit_key).ok()??;
            AuditFileParser::parse(&content).ok()
        }

        /// Raw stored content, unparsed — for callers (like
        /// `EnhancedAuditor::load_previous_audit`) that regex-scan the
        /// text directly rather than going through `AuditFileData`.
        pub fn read_raw(&self) -> Option<String> {
            let storage = Self::storage()?;
            storage.get_item(&self.audit_key).ok()?
        }

        pub fn count_entries(&self) -> usize {
            let Some(storage) = Self::storage() else { return 0; };
            match storage.get_item(&self.audit_key) {
                Ok(Some(content)) => AuditFileParser::count_entries(&content),
                _ => 0,
            }
        }

        pub fn file_exists(&self) -> bool {
            match Self::storage() {
                Some(s) => matches!(s.get_item(&self.audit_key), Ok(Some(_))),
                None    => false,
            }
        }

        // ── Write operations ────────────────────────────────────────────

        /// Append a new entry. Creates the entry (with header) if none
        /// exists yet. Rotates to an archive key first if the entry limit
        /// is reached. There's no real file lock to unlock/re-lock here —
        /// a synchronous wasm call can't race itself within one tab.
        pub fn append_entry(
            &self,
            entry:  &AuditEntryRecord,
            config: &AuditFileConfig,
        ) -> Result<(), String> {
            self.rotate_if_needed()?;

            let Some(storage) = Self::storage() else {
                return Err(
                    "localStorage unavailable — cannot append audit entry for this session"
                        .to_string(),
                );
            };

            let existing = storage.get_item(&self.audit_key).ok().flatten();
            let existing_count = existing
                .as_deref()
                .map(AuditFileParser::count_entries)
                .unwrap_or(0);

            let mut entry = entry.clone();
            entry.index   = existing_count + 1;

            let new_content = match existing {
                Some(content) => format!("{}{}", content, AuditFileWriter::write_entry(&entry)),
                None => format!(
                    "{}{}",
                    AuditFileWriter::write_header(config),
                    AuditFileWriter::write_entry(&entry),
                ),
            };

            storage
                .set_item(&self.audit_key, &new_content)
                .map_err(|_| "Failed to write audit entry to localStorage".to_string())
        }

        // ── Private ──────────────────────────────────────────────────────

        /// Move the current value to an archive key when the entry limit
        /// is reached. No real timestamp source is worth pulling in
        /// js-sys just for this (web-sys here is deliberately kept to only
        /// Window+Storage, see Cargo.toml) — a small rotation counter,
        /// itself stored in localStorage, gives unique-enough archive keys
        /// within a session instead.
        fn rotate_if_needed(&self) -> Result<(), String> {
            let count = self.count_entries();
            if count < self.max_entries { return Ok(()); }

            let Some(storage) = Self::storage() else { return Ok(()); };

            let Ok(Some(content)) = storage.get_item(&self.audit_key) else { return Ok(()); };

            let counter_key = format!("{}.rotation_count", self.audit_key);
            let rotation_n: u32 = storage
                .get_item(&counter_key)
                .ok()
                .flatten()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            let archive_key = format!("{}.archive_{}", self.audit_key, rotation_n);
            let _ = storage.set_item(&archive_key, &content);
            let _ = storage.set_item(&counter_key, &(rotation_n + 1).to_string());
            let _ = storage.remove_item(&self.audit_key);

            Ok(())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native_audit::AuditFileManager;
#[cfg(target_arch = "wasm32")]
pub use wasm_audit::AuditFileManager;
