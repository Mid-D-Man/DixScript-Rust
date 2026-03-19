// dixscript/src/Compiler/DLM/Auditor/enhanced_auditor.rs
//! Enhanced auditor — DixScript-formatted audit trail with smart AST diff.
//! Uses AuditFileManager for permission-safe I/O (unlock → write → re-lock).

use super::audit_file_data::{AuditEntryRecord, AuditFileConfig};
use super::audit_file_manager::AuditFileManager;
use super::auditor_trait::{
    AuditChange, AuditEntry, AuditResult, AuditStep, AuditorResult, DecryptionAttempt, IAuditor,
};
use super::auditor_utilities::AuditorPathUtils;
use crate::Compiler::AST::DixScript;
use crate::Compiler::Core::BinarySerialization::{BinaryPacker, BinaryUnpacker};
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use base64::{engine::general_purpose, Engine as _};
use lazy_static::lazy_static;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

lazy_static! {
    static ref RE_CHECKSUM: Regex =
        Regex::new(r#"source_checksum\s*->\s*"([^"]+)""#)
            .expect("RE_CHECKSUM compile failed");

    static ref RE_AST_SNAPSHOT: Regex =
        Regex::new(r#"ast_snapshot\s*->\s*"([^"]+)""#)
            .expect("RE_AST_SNAPSHOT compile failed");
}

/// Enhanced auditor — writes structured audit entries with an embedded base64
/// AST snapshot and computes a smart diff against the previous compilation.
///
/// Uses [`AuditFileManager`] for all I/O so the `.mdix.au` file is locked
/// read-only between compilations and the unlock → write → re-lock cycle is
/// handled consistently with the key file manager.
pub struct EnhancedAuditor {
    base:             DLMModuleBase,
    source_file_path: String,
    output_directory: String,
    current_ast:      DixScript,
    current_entry:    AuditEntry,
    previous_ast:     Option<DixScript>,
    max_entries:      usize,
    /// Populated in `start_audit` once the resolved path is known.
    manager:          Option<AuditFileManager>,
}

impl EnhancedAuditor {
    pub fn new(
        source_file_path: String,
        output_directory: String,
        current_ast:      DixScript,
    ) -> Self {
        EnhancedAuditor {
            base:             DLMModuleBase::new("DAuditor.enhanced", 1),
            source_file_path,
            output_directory,
            current_ast,
            current_entry:    AuditEntry::new(),
            previous_ast:     None,
            max_entries:      100,
            manager:          None,
        }
    }

    // ── Checksum ──────────────────────────────────────────────────────────────

    fn calculate_checksum(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }

    // ── Previous audit loading ────────────────────────────────────────────────

    /// Read the last-written entry from the existing audit file (if any) to
    /// extract the previous source checksum and AST snapshot for diffing.
    fn load_previous_audit(&mut self) {
        let manager = match self.manager.as_ref() {
            Some(m) => m,
            None    => return,
        };

        if !manager.file_exists() {
            if self.base.is_debug_enabled() {
                self.base.log_debug("No previous audit file found");
            }
            return;
        }

        // Read raw content. Because the file is read-only we can open it
        // normally — read permissions are never revoked.
        let audit_file_path = manager.audit_file_path().to_string();
        let content = match std::fs::read_to_string(&audit_file_path) {
            Ok(c)  => c,
            Err(e) => {
                self.base.log_warning(&format!("Failed to load previous audit: {}", e));
                return;
            }
        };

        // Extract the most recent source checksum for display in the new entry.
        if let Some(caps) = RE_CHECKSUM.captures(&content) {
            // captures() returns the last match; we want the most recent entry.
            // Since entries are appended in order, the last capture IS the most
            // recent compilation's checksum.
            let mut last_checksum = String::new();
            for caps in RE_CHECKSUM.captures_iter(&content) {
                last_checksum = caps[1].to_string();
            }
            if !last_checksum.is_empty() {
                self.current_entry.previous_checksum = Some(last_checksum.clone());
                if self.base.is_debug_enabled() {
                    self.base.log_debug(&format!(
                        "Loaded previous checksum: {}", last_checksum,
                    ));
                }
            }
        }

        // Extract the most recent AST snapshot (last occurrence in file).
        let mut last_snapshot: Option<String> = None;
        for caps in RE_AST_SNAPSHOT.captures_iter(&content) {
            last_snapshot = Some(caps[1].to_string());
        }

        if let Some(snapshot_b64) = last_snapshot {
            match general_purpose::STANDARD.decode(&snapshot_b64) {
                Ok(binary_ast) => {
                    let mut unpacker = BinaryUnpacker::new();
                    let result       = unpacker.unpack(&binary_ast);
                    if result.is_success {
                        if let Some(ast) = result.ast {
                            self.previous_ast = Some(ast);
                            if self.base.is_debug_enabled() {
                                self.base.log_debug("Loaded previous AST snapshot");
                            }
                        }
                    } else {
                        self.base.log_warning(
                            "Failed to deserialize previous AST snapshot",
                        );
                    }
                }
                Err(e) => {
                    self.base.log_warning(&format!(
                        "Failed to decode AST snapshot: {}", e,
                    ));
                }
            }
        }
    }

    // ── Smart diff ────────────────────────────────────────────────────────────

    fn detect_changes(&mut self) {
        self.base.log_info("Detecting changes (smart diff)");

        let mut changes = Vec::new();
        self.compare_config_section(&mut changes);
        self.compare_dlm_section(&mut changes);
        self.compare_enums_section(&mut changes);
        self.compare_data_section(&mut changes);
        self.compare_security_section(&mut changes);

        self.current_entry.changes_detected = changes.clone();

        self.current_entry.changes_summary = if changes.is_empty() {
            Some("No changes detected".to_string())
        } else {
            let added    = changes.iter().filter(|c| c.change_type == "ADDED").count();
            let modified = changes.iter().filter(|c| c.change_type == "MODIFIED").count();
            let deleted  = changes.iter().filter(|c| c.change_type == "DELETED").count();
            let mut parts = Vec::with_capacity(3);
            if added    > 0 { parts.push(format!("{} added",    added));    }
            if modified > 0 { parts.push(format!("{} modified", modified)); }
            if deleted  > 0 { parts.push(format!("{} deleted",  deleted));  }
            Some(parts.join(", "))
        };

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!(
                "Changes detected: {}",
                self.current_entry.changes_summary.as_deref().unwrap_or("none"),
            ));
        }
    }

    fn compare_config_section(&self, changes: &mut Vec<AuditChange>) {
        let current  = self.current_ast.config.as_ref();
        let previous = self.previous_ast.as_ref().and_then(|a| a.config.as_ref());
        match (current, previous) {
            (None, None)       => {}
            (None, Some(_))    => changes.push(AuditChange::new(
                "CONFIG".into(), "section".into(), "DELETED".into(), None, None,
            )),
            (Some(_), None)    => changes.push(AuditChange::new(
                "CONFIG".into(), "section".into(), "ADDED".into(), None, None,
            )),
            (Some(cur), Some(prev)) => {
                if cur.entries.len() != prev.entries.len() {
                    changes.push(AuditChange::new(
                        "CONFIG".into(), "entries".into(), "MODIFIED".into(),
                        Some(format!("{} entries", prev.entries.len())),
                        Some(format!("{} entries", cur.entries.len())),
                    ));
                }
            }
        }
    }

    fn compare_dlm_section(&self, changes: &mut Vec<AuditChange>) {
        let current  = self.current_ast.dlm.as_ref();
        let previous = self.previous_ast.as_ref().and_then(|a| a.dlm.as_ref());
        match (current, previous) {
            (None, None)    => {}
            (None, Some(_)) => changes.push(AuditChange::new(
                "DLM".into(), "section".into(), "DELETED".into(), None, None,
            )),
            (Some(cur), None) => {
                changes.push(AuditChange::new(
                    "DLM".into(), "section".into(), "ADDED".into(), None, None,
                ));
                for module in &cur.modules {
                    changes.push(AuditChange::new(
                        "DLM".into(), "module".into(), "ADDED".into(),
                        None, Some(format!("{:?}", module)),
                    ));
                }
            }
            (Some(cur), Some(prev)) => {
                let cur_mods:  Vec<String> =
                    cur.modules.iter().map(|m| format!("{:?}", m)).collect();
                let prev_mods: Vec<String> =
                    prev.modules.iter().map(|m| format!("{:?}", m)).collect();
                if cur_mods != prev_mods {
                    changes.push(AuditChange::new(
                        "DLM".into(), "modules".into(), "MODIFIED".into(),
                        Some(prev_mods.join(", ")),
                        Some(cur_mods.join(", ")),
                    ));
                }
            }
        }
    }

    fn compare_enums_section(&self, changes: &mut Vec<AuditChange>) {
        let current  = self.current_ast.enums.as_ref();
        let previous = self.previous_ast.as_ref().and_then(|a| a.enums.as_ref());
        match (current, previous) {
            (None, None)    => {}
            (None, Some(_)) => changes.push(AuditChange::new(
                "ENUMS".into(), "section".into(), "DELETED".into(), None, None,
            )),
            (Some(_), None) => changes.push(AuditChange::new(
                "ENUMS".into(), "section".into(), "ADDED".into(), None, None,
            )),
            (Some(cur), Some(prev)) => {
                if cur.enums.len() != prev.enums.len() {
                    changes.push(AuditChange::new(
                        "ENUMS".into(), "count".into(), "MODIFIED".into(),
                        Some(format!("{} enums", prev.enums.len())),
                        Some(format!("{} enums", cur.enums.len())),
                    ));
                }
            }
        }
    }

    fn compare_data_section(&self, changes: &mut Vec<AuditChange>) {
        let current  = self.current_ast.data.as_ref();
        let previous = self.previous_ast.as_ref().and_then(|a| a.data.as_ref());
        match (current, previous) {
            (None, None)    => {}
            (None, Some(_)) => changes.push(AuditChange::new(
                "DATA".into(), "section".into(), "DELETED".into(), None, None,
            )),
            (Some(_), None) => changes.push(AuditChange::new(
                "DATA".into(), "section".into(), "ADDED".into(), None, None,
            )),
            (Some(cur), Some(prev)) => {
                if cur.entries.len() != prev.entries.len() {
                    changes.push(AuditChange::new(
                        "DATA".into(), "entries".into(), "MODIFIED".into(),
                        Some(format!("{} entries", prev.entries.len())),
                        Some(format!("{} entries", cur.entries.len())),
                    ));
                }
            }
        }
    }

    fn compare_security_section(&self, changes: &mut Vec<AuditChange>) {
        let current  = self.current_ast.security.as_ref();
        let previous = self.previous_ast.as_ref().and_then(|a| a.security.as_ref());
        match (current, previous) {
            (None, None)    => {}
            (None, Some(_)) => changes.push(AuditChange::new(
                "SECURITY".into(), "section".into(), "DELETED".into(), None, None,
            )),
            (Some(_), None) => changes.push(AuditChange::new(
                "SECURITY".into(), "section".into(), "ADDED".into(), None, None,
            )),
            (Some(cur), Some(prev)) => {
                if cur.entries.len() != prev.entries.len() {
                    changes.push(AuditChange::new(
                        "SECURITY".into(), "entries".into(), "MODIFIED".into(),
                        Some(format!("{} blocks", prev.entries.len())),
                        Some(format!("{} blocks", cur.entries.len())),
                    ));
                }
            }
        }
    }

    // ── Entry building ────────────────────────────────────────────────────────

    /// Build the `AuditEntryRecord` that will be handed to `AuditFileManager`.
    ///
    /// The enhanced auditor stores two extra fields that don't fit cleanly into
    /// the base record schema: the base64 AST snapshot and the structured
    /// change list. We encode these into `changes_summary` so they survive the
    /// round-trip through the common record format without requiring a separate
    /// file format. Callers who need the full change list should use
    /// `current_entry.changes_detected` directly before `finalize_audit` is
    /// called.
    fn build_record(&self) -> AuditEntryRecord {
        // Serialize the current AST to base64 for embedding in the summary.
        let snapshot_b64 = {
            let mut packer = BinaryPacker::new();
            let result     = packer.pack(&self.current_ast);
            if result.is_success {
                Some(general_purpose::STANDARD.encode(&result.binary_data))
            } else {
                None
            }
        };

        // Build a rich changes_summary string that encodes both the human-readable
        // diff summary and the optional AST snapshot.
        let changes_summary = {
            let diff_text = self.current_entry
                .changes_summary
                .as_deref()
                .unwrap_or("none");

            match snapshot_b64 {
                Some(b64) => Some(format!("diff={} | ast_snapshot=\"{}\"", diff_text, b64)),
                None      => Some(diff_text.to_string()),
            }
        };

        // Flatten the detected changes into the modules_executed list so they
        // appear in the structured entry. The base DiyAuditor already puts
        // module names here; enhanced adds change paths as well.
        let mut modules = self.current_entry.modules_executed.clone();
        for change in &self.current_entry.changes_detected {
            modules.push(format!(
                "{}:{}.{}",
                change.change_type, change.section, change.path,
            ));
        }

        AuditEntryRecord {
            index:             0, // assigned by AuditFileManager
            compilation_id:    self.current_entry.compilation_id.clone(),
            timestamp:         self.current_entry.timestamp,
            source_checksum:   self.current_entry.source_checksum.clone(),
            status:            self.current_entry.status.clone(),
            modules_executed:  modules,
            execution_time_ms: self.current_entry.execution_time_ms,
            changes_summary,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// IAuditor implementation
// ─────────────────────────────────────────────────────────────────────────────

impl IAuditor for EnhancedAuditor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn initialize(&mut self, _config: HashMap<String, String>) {
        if self.base.is_debug_enabled() {
            self.base.log_debug(
                "Initialized Enhanced auditor (structured format, smart diff, AuditFileManager)",
            );
        }
    }

    fn start_audit(
        &mut self,
        _ast:        &DixScript,
        binary_data: &[u8],
    ) -> AuditorResult<AuditResult> {
        self.base.log_info("Starting Enhanced audit with smart diff");

        self.current_entry.source_checksum = self.calculate_checksum(binary_data);
        self.current_entry.timestamp       = chrono::Utc::now();

        let base_name = AuditorPathUtils::base_name(&self.source_file_path)
            .map_err(|e| format!("Failed to resolve audit path: {}", e))?;

        let (path, moved) = AuditorPathUtils::resolve_audit_file_path(
            &self.source_file_path,
            &self.output_directory,
            &base_name,
        ).map_err(|e| format!("Failed to resolve audit path: {}", e))?;

        if moved {
            self.base.log_warning(
                "Audit file relocated from output directory to source directory",
            );
        }

        let audit_file_path = path.to_string_lossy().to_string();

        // Initialise the manager before loading the previous audit so
        // `load_previous_audit` can use `manager.file_exists()`.
        self.manager = Some(AuditFileManager::new(
            audit_file_path.clone(),
            self.max_entries,
        ));

        // Load previous audit for diffing — reads the existing (read-only) file.
        self.load_previous_audit();

        if self.previous_ast.is_some() {
            self.detect_changes();
        } else {
            self.current_entry.changes_summary =
                Some("Initial compilation".to_string());
            self.base.log_info(
                "Initial compilation — no previous audit to compare",
            );
        }

        if self.base.is_debug_enabled() {
            self.base.log_debug(&format!("Audit file: {}", audit_file_path));
        }

        self.base.log_info(&format!(
            "Enhanced audit started: {}", self.current_entry.compilation_id,
        ));

        Ok(AuditResult::success(
            audit_file_path,
            self.current_entry.compilation_id.clone(),
        ))
    }

    fn log_step(
        &mut self,
        step_name:   &str,
        details:     &str,
        input_size:  usize,
        output_size: usize,
        duration_ms: f64,
    ) {
        self.current_entry.steps.push(AuditStep::new(
            step_name.to_string(),
            details.to_string(),
            input_size,
            output_size,
            duration_ms,
        ));
        self.current_entry.modules_executed.push(step_name.to_string());
        self.current_entry.execution_time_ms += duration_ms;

        if self.base.is_verbose_enabled() {
            self.base.log_verbose(&format!(
                "Logged step: {} ({:.2}ms)", step_name, duration_ms,
            ));
        }
    }

    fn log_decryption_attempt(
        &mut self,
        success:        bool,
        details:        &str,
        encrypted_size: usize,
        decrypted_size: usize,
        duration_ms:    f64,
    ) {
        self.current_entry.decryption_attempts.push(DecryptionAttempt::new(
            success,
            details.to_string(),
            encrypted_size,
            decrypted_size,
            duration_ms,
        ));

        self.base.log_info(&format!(
            "Logged decryption attempt: {}",
            if success { "SUCCESS" } else { "FAILED" },
        ));
    }

    fn finalize_audit(&mut self) -> AuditorResult<()> {
        self.base.log_info("Finalizing Enhanced audit");

        let manager = self.manager.as_ref()
            .ok_or_else(|| {
                "Audit not started — call start_audit before finalize_audit".to_string()
            })?;

        let config = AuditFileConfig::new(
            Path::new(&self.source_file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            self.max_entries,
        );

        let record = self.build_record();

        // AuditFileManager handles rotation check + unlock → write → re-lock.
        manager.append_entry(&record, &config)
            .map_err(|e| format!("Failed to write enhanced audit entry: {}", e))?;

        self.base.log_info(&format!(
            "Enhanced audit finalized: {}", manager.audit_file_path(),
        ));
        self.base.log_info(&format!(
            "Compilation ID: {}", self.current_entry.compilation_id,
        ));
        self.base.log_info(&format!(
            "Changes detected: {}", self.current_entry.changes_detected.len(),
        ));
        self.base.log_info(&format!(
            "Total compilations in file: {}", manager.count_entries(),
        ));

        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if self.source_file_path.is_empty() {
            return Err("Source file path not set".to_string());
        }
        if self.output_directory.is_empty() {
            return Err("Output directory not set".to_string());
        }
        Ok(())
    }

    fn get_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::with_capacity(8);
        metadata.insert("auditor_type".to_string(),   "enhanced".to_string());
        metadata.insert("format".to_string(),          "structured".to_string());
        metadata.insert("diff_mode".to_string(),       "smart".to_string());
        metadata.insert("module_name".to_string(),     self.module_name().to_string());
        metadata.insert("priority".to_string(),        self.priority().to_string());
        metadata.insert("max_entries".to_string(),     self.max_entries.to_string());
        metadata.insert("compilation_id".to_string(),  self.current_entry.compilation_id.clone());
        if let Some(ref m) = self.manager {
            metadata.insert("audit_file".to_string(), m.audit_file_path().to_string());
        }
        metadata
    }

    fn priority(&self) -> i32 {
        self.base.priority()
    }
}
