//! Enhanced auditor — DixScript-formatted audit trail with smart AST diff.

use super::auditor_trait::{
    IAuditor, AuditorResult, AuditResult, AuditEntry, AuditStep, DecryptionAttempt, AuditChange,
};
use super::auditor_utilities::AuditorPathUtils;
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::Compiler::AST::DixScript;
use crate::Compiler::Core::BinarySerialization::{BinaryPacker, BinaryUnpacker};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use sha2::{Sha256, Digest};

lazy_static! {
    static ref RE_CHECKSUM: Regex =
        Regex::new(r#"source_checksum\s*=\s*"([^"]+)""#).expect("RE_CHECKSUM compile failed");

    static ref RE_AST_SNAPSHOT: Regex =
        Regex::new(r#"ast_snapshot\s*=\s*"([^"]+)""#).expect("RE_AST_SNAPSHOT compile failed");

    static ref RE_COMPILATION_COUNT: Regex =
        Regex::new(r"compilation_\d+:").expect("RE_COMPILATION_COUNT compile failed");
}

/// Enhanced auditor — writes a DixScript-formatted audit trail and computes a
/// smart diff against the previous compilation's AST snapshot.
pub struct EnhancedAuditor {
    base: DLMModuleBase,
    source_file_path: String,
    output_directory: String,
    current_ast: DixScript,
    audit_file_path: String,
    current_entry: AuditEntry,
    previous_ast: Option<DixScript>,
    max_entries: usize,
}

impl EnhancedAuditor {
    pub fn new(
        source_file_path: String,
        output_directory: String,
        current_ast: DixScript,
    ) -> Self {
        EnhancedAuditor {
            base: DLMModuleBase::new("DAuditor.enhanced", 1),
            source_file_path,
            output_directory,
            current_ast,
            audit_file_path: String::new(),
            current_entry: AuditEntry::new(),
            previous_ast: None,
            max_entries: 100,
        }
    }

    fn calculate_checksum(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }

    fn load_previous_audit(&mut self) {
        use base64::{Engine as _, engine::general_purpose};

        if !Path::new(&self.audit_file_path).exists() {
            if self.base.is_debug_enabled() {
                self.base.log_debug("No previous audit file found");
            }
            return;
        }

        let content = match std::fs::read_to_string(&self.audit_file_path) {
            Ok(c) => c,
            Err(e) => {
                self.base.log_warning(&format!("Failed to load previous audit: {}", e));
                return;
            }
        };

        if let Some(caps) = RE_CHECKSUM.captures(&content) {
            self.current_entry.previous_checksum = Some(caps[1].to_string());
            if self.base.is_debug_enabled() {
                self.base.log_debug(&format!("Loaded previous checksum: {}", &caps[1]));
            }
        }

        if let Some(caps) = RE_AST_SNAPSHOT.captures(&content) {
            match general_purpose::STANDARD.decode(&caps[1]) {
                Ok(binary_ast) => {
                    let mut unpacker = BinaryUnpacker::new();
                    let result = unpacker.unpack(&binary_ast);
                    if result.is_success {
                        if let Some(ast) = result.ast {
                            self.previous_ast = Some(ast);
                            if self.base.is_debug_enabled() {
                                self.base.log_debug("Loaded previous AST snapshot");
                            }
                        }
                    } else {
                        self.base.log_warning("Failed to deserialize previous AST snapshot");
                    }
                }
                Err(e) => {
                    self.base.log_warning(&format!("Failed to decode AST snapshot: {}", e));
                }
            }
        }
    }

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
            let added   = changes.iter().filter(|c| c.change_type == "ADDED").count();
            let modified = changes.iter().filter(|c| c.change_type == "MODIFIED").count();
            let deleted  = changes.iter().filter(|c| c.change_type == "DELETED").count();

            let mut parts = Vec::with_capacity(3);
            if added   > 0 { parts.push(format!("{} added",    added));   }
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
            (None, None) => {}
            (None, Some(_)) => changes.push(AuditChange::new(
                "CONFIG".to_string(), "section".to_string(), "DELETED".to_string(), None, None,
            )),
            (Some(_), None) => changes.push(AuditChange::new(
                "CONFIG".to_string(), "section".to_string(), "ADDED".to_string(), None, None,
            )),
            (Some(cur), Some(prev)) => {
                if cur.entries.len() != prev.entries.len() {
                    changes.push(AuditChange::new(
                        "CONFIG".to_string(),
                        "entries".to_string(),
                        "MODIFIED".to_string(),
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
            (None, None) => {}
            (None, Some(_)) => changes.push(AuditChange::new(
                "DLM".to_string(), "section".to_string(), "DELETED".to_string(), None, None,
            )),
            (Some(cur), None) => {
                changes.push(AuditChange::new(
                    "DLM".to_string(), "section".to_string(), "ADDED".to_string(), None, None,
                ));
                for module in &cur.modules {
                    changes.push(AuditChange::new(
                        "DLM".to_string(),
                        "module".to_string(),
                        "ADDED".to_string(),
                        None,
                        Some(format!("{:?}", module)),
                    ));
                }
            }
            (Some(cur), Some(prev)) => {
                let cur_modules: Vec<String> = cur.modules.iter().map(|m| format!("{:?}", m)).collect();
                let prev_modules: Vec<String> = prev.modules.iter().map(|m| format!("{:?}", m)).collect();
                if cur_modules != prev_modules {
                    changes.push(AuditChange::new(
                        "DLM".to_string(),
                        "modules".to_string(),
                        "MODIFIED".to_string(),
                        Some(prev_modules.join(", ")),
                        Some(cur_modules.join(", ")),
                    ));
                }
            }
        }
    }

    fn compare_enums_section(&self, changes: &mut Vec<AuditChange>) {
        let current  = self.current_ast.enums.as_ref();
        let previous = self.previous_ast.as_ref().and_then(|a| a.enums.as_ref());

        match (current, previous) {
            (None, None) => {}
            (None, Some(_)) => changes.push(AuditChange::new(
                "ENUMS".to_string(), "section".to_string(), "DELETED".to_string(), None, None,
            )),
            (Some(_), None) => changes.push(AuditChange::new(
                "ENUMS".to_string(), "section".to_string(), "ADDED".to_string(), None, None,
            )),
            (Some(cur), Some(prev)) => {
                if cur.enums.len() != prev.enums.len() {
                    changes.push(AuditChange::new(
                        "ENUMS".to_string(),
                        "count".to_string(),
                        "MODIFIED".to_string(),
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
            (None, None) => {}
            (None, Some(_)) => changes.push(AuditChange::new(
                "DATA".to_string(), "section".to_string(), "DELETED".to_string(), None, None,
            )),
            (Some(_), None) => changes.push(AuditChange::new(
                "DATA".to_string(), "section".to_string(), "ADDED".to_string(), None, None,
            )),
            (Some(cur), Some(prev)) => {
                if cur.entries.len() != prev.entries.len() {
                    changes.push(AuditChange::new(
                        "DATA".to_string(),
                        "entries".to_string(),
                        "MODIFIED".to_string(),
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
            (None, None) => {}
            (None, Some(_)) => changes.push(AuditChange::new(
                "SECURITY".to_string(), "section".to_string(), "DELETED".to_string(), None, None,
            )),
            (Some(_), None) => changes.push(AuditChange::new(
                "SECURITY".to_string(), "section".to_string(), "ADDED".to_string(), None, None,
            )),
            (Some(cur), Some(prev)) => {
                if cur.entries.len() != prev.entries.len() {
                    changes.push(AuditChange::new(
                        "SECURITY".to_string(),
                        "entries".to_string(),
                        "MODIFIED".to_string(),
                        Some(format!("{} blocks", prev.entries.len())),
                        Some(format!("{} blocks", cur.entries.len())),
                    ));
                }
            }
        }
    }

    fn check_and_rotate_if_needed(&self) -> Result<(), String> {
        if !Path::new(&self.audit_file_path).exists() {
            return Ok(());
        }

        let count = self.count_compilations();
        if count >= self.max_entries {
            self.base.log_info(&format!(
                "Audit file has {} entries - rotating",
                count,
            ));

            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let archive_path = self.audit_file_path.replace(
                ".mdix.au",
                &format!(".mdix.au.archive_{}", timestamp),
            );

            std::fs::rename(&self.audit_file_path, &archive_path)
                .map_err(|e| format!("Failed to rotate audit file: {}", e))?;

            self.base.log_info(&format!(
                "Audit file rotated to: {}",
                Path::new(&archive_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ));
        }

        Ok(())
    }

    fn write_audit_entry(&self) -> Result<(), String> {
        use base64::{Engine as _, engine::general_purpose};

        let mut content = String::new();

        if !Path::new(&self.audit_file_path).exists() {
            content.push_str("// DixScript Audit Trail - Enhanced Format\n");
            content.push_str(&format!(
                "// Generated: {}\n",
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            ));
            content.push_str("// Format: DixScript v1.0.0\n\n");
            content.push_str("@AUDIT_CONFIG(\n");
            content.push_str(&format!("  max_entries -> {},\n", self.max_entries));
            content.push_str("  rotation_enabled -> true,\n");
            content.push_str(&format!(
                "  archive_to -> \"{}.archive\",\n",
                Path::new(&self.audit_file_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ));
            content.push_str("  diff_mode -> \"smart\"\n)\n\n");
            content.push_str("@AUDIT_HISTORY(\n");
        } else {
            content.push_str("\n  // ----------------------------------------\n");
        }

        let compilation_num = self.count_compilations() + 1;
        content.push_str(&format!("  compilation_{}:\n", compilation_num));
        content.push_str(&format!("    id = \"{}\",\n", self.current_entry.compilation_id));
        content.push_str(&format!(
            "    timestamp = \"{}\",\n",
            self.current_entry.timestamp.format("%Y-%m-%dT%H:%M:%SZ"),
        ));
        content.push_str(&format!(
            "    source_checksum = \"{}\",\n",
            self.current_entry.source_checksum,
        ));

        if let Some(ref prev) = self.current_entry.previous_checksum {
            content.push_str(&format!("    previous_checksum = \"{}\",\n", prev));
        }

        let mut packer = BinaryPacker::new();
        let pack_result = packer.pack(&self.current_ast);
        if pack_result.is_success {
            let snapshot = general_purpose::STANDARD.encode(&pack_result.binary_data);
            content.push_str(&format!("    ast_snapshot = \"{}\",\n", snapshot));
        }

        content.push_str(&format!("    status = \"{}\",\n", self.current_entry.status));
        content.push_str(&format!(
            "    modules:: \"{}\",\n",
            self.current_entry.modules_executed.join("\", \""),
        ));
        content.push_str(&format!(
            "    execution_time_ms = {:.2},\n",
            self.current_entry.execution_time_ms,
        ));

        if !self.current_entry.steps.is_empty() {
            content.push_str("    steps: [\n");
            for step in &self.current_entry.steps {
                content.push_str(&format!(
                    "      {{ name = \"{}\", duration_ms = {:.2} }},\n",
                    step.step_name, step.duration_ms,
                ));
            }
            content.push_str("    ],\n");
        }

        if !self.current_entry.decryption_attempts.is_empty() {
            content.push_str("    decryption_attempts: [\n");
            for attempt in &self.current_entry.decryption_attempts {
                content.push_str(&format!(
                    "      {{ success = {}, details = \"{}\", duration_ms = {:.2} }},\n",
                    attempt.success, attempt.details, attempt.duration_ms,
                ));
            }
            content.push_str("    ],\n");
        }

        if !self.current_entry.changes_detected.is_empty() {
            content.push_str("    changes_detected:\n");
            for change in &self.current_entry.changes_detected {
                content.push_str(&format!("      section = \"{}\",\n", change.section));
                content.push_str(&format!("      path = \"{}\",\n", change.path));
                content.push_str(&format!("      change_type = \"{}\",\n", change.change_type));
                if let Some(ref old_val) = change.old_value {
                    content.push_str(&format!("      old_value = \"{}\",\n", old_val));
                }
                if let Some(ref new_val) = change.new_value {
                    content.push_str(&format!("      new_value = \"{}\",\n", new_val));
                }
                content.push('\n');
            }
        }

        content.push_str(&format!(
            "    changes_summary = \"{}\"\n",
            self.current_entry.changes_summary.as_deref().unwrap_or("none"),
        ));

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.audit_file_path)
            .map_err(|e| format!("Failed to open audit file: {}", e))?;

        file.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write audit file: {}", e))?;

        Ok(())
    }

    fn count_compilations(&self) -> usize {
        if !Path::new(&self.audit_file_path).exists() {
            return 0;
        }
        match std::fs::read_to_string(&self.audit_file_path) {
            Ok(content) => RE_COMPILATION_COUNT.find_iter(&content).count(),
            Err(_) => 0,
        }
    }
}

impl IAuditor for EnhancedAuditor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn initialize(&mut self, _config: HashMap<String, String>) {
        if self.base.is_debug_enabled() {
            self.base.log_debug("Initialized Enhanced auditor (DixScript format, smart diff)");
        }
    }

    fn start_audit(&mut self, _ast: &DixScript, binary_data: &[u8]) -> AuditorResult<AuditResult> {
        self.base.log_info("Starting Enhanced audit with smart diff");

        self.current_entry.source_checksum = self.calculate_checksum(binary_data);
        self.current_entry.timestamp = chrono::Utc::now();

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

        self.audit_file_path = path.to_string_lossy().to_string();

        self.load_previous_audit();

        if self.previous_ast.is_some() {
            self.detect_changes();
        } else {
            self.current_entry.changes_summary = Some("Initial compilation".to_string());
            self.base.log_info("Initial compilation — no previous audit to compare");
        }

        if self.base.is_debug_enabled() {
            self.base.log_debug(&format!("Audit file: {}", self.audit_file_path));
        }
        self.base.log_info(&format!("Enhanced audit started: {}", self.current_entry.compilation_id));

        Ok(AuditResult::success(
            self.audit_file_path.clone(),
            self.current_entry.compilation_id.clone(),
        ))
    }

    fn log_step(
        &mut self,
        step_name: &str,
        details: &str,
        input_size: usize,
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
            self.base.log_verbose(&format!("Logged step: {} ({:.2}ms)", step_name, duration_ms));
        }
    }

    fn log_decryption_attempt(
        &mut self,
        success: bool,
        details: &str,
        encrypted_size: usize,
        decrypted_size: usize,
        duration_ms: f64,
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

        self.check_and_rotate_if_needed()?;
        self.write_audit_entry()?;

        self.base.log_info(&format!("Enhanced audit finalized: {}", self.audit_file_path));
        self.base.log_info(&format!("Compilation ID: {}", self.current_entry.compilation_id));
        self.base.log_info(&format!(
            "Changes detected: {}",
            self.current_entry.changes_detected.len(),
        ));
        self.base.log_info(&format!("Total compilations: {}", self.count_compilations()));

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
        metadata.insert("auditor_type".to_string(), "enhanced".to_string());
        metadata.insert("format".to_string(), "dixscript".to_string());
        metadata.insert("diff_mode".to_string(), "smart".to_string());
        metadata.insert("module_name".to_string(), self.module_name().to_string());
        metadata.insert("priority".to_string(), self.priority().to_string());
        metadata.insert("audit_file".to_string(), self.audit_file_path.clone());
        metadata.insert("compilation_id".to_string(), self.current_entry.compilation_id.clone());
        metadata.insert("max_entries".to_string(), self.max_entries.to_string());
        metadata
    }

    fn priority(&self) -> i32 {
        self.base.priority()
    }
}
