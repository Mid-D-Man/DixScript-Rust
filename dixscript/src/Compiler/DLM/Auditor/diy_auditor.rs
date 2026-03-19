// dixscript/src/Compiler/DLM/Auditor/diy_auditor.rs
//! Simple DIY auditor — records compilation events using AuditFileManager
//! for permission-safe, structured `.mdix.au` I/O.

use super::audit_file_data::{AuditEntryRecord, AuditFileConfig};
use super::audit_file_manager::AuditFileManager;
use super::auditor_trait::{
    AuditEntry, AuditResult, AuditorResult, DecryptionAttempt, AuditStep, IAuditor,
};
use super::auditor_utilities::AuditorPathUtils;
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::Compiler::AST::DixScript;
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::path::Path;

/// DIY auditor — appends a structured compilation record to a `.mdix.au` file
/// located in the same directory as the source file.
///
/// Uses [`AuditFileManager`] for all I/O so the file is locked read-only
/// between compilations and unlock → write → re-lock is handled consistently.
pub struct DiyAuditor {
    base:             DLMModuleBase,
    source_file_path: String,
    output_directory: String,
    current_entry:    AuditEntry,
    /// Populated in `start_audit` once the resolved path is known.
    manager:          Option<AuditFileManager>,
}

impl DiyAuditor {
    pub fn new(source_file_path: impl AsRef<Path>, output_directory: impl AsRef<Path>) -> Self {
        DiyAuditor {
            base:             DLMModuleBase::new("DAuditor.diy", 1),
            source_file_path: source_file_path.as_ref().to_string_lossy().to_string(),
            output_directory: output_directory.as_ref().to_string_lossy().to_string(),
            current_entry:    AuditEntry::new(),
            manager:          None,
        }
    }

    fn calculate_checksum(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }
}

impl IAuditor for DiyAuditor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn initialize(&mut self, _config: HashMap<String, String>) {
        if self.base.is_debug_enabled() {
            self.base.log_debug("Initialized DIY auditor (structured format via AuditFileManager)");
        }
    }

    fn start_audit(
        &mut self,
        _ast:         &DixScript,
        binary_data:  &[u8],
    ) -> AuditorResult<AuditResult> {
        self.base.log_info("Starting DIY audit");

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

        // Initialise the manager now that the resolved path is known.
        self.manager = Some(AuditFileManager::new(audit_file_path.clone(), 100));

        if self.base.is_debug_enabled() {
            self.base.log_debug(&format!("Audit file: {}", audit_file_path));
        }

        self.base.log_info(&format!(
            "DIY audit started: {}", self.current_entry.compilation_id,
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
        self.base.log_info("Finalizing DIY audit");

        let manager = self.manager.as_ref()
            .ok_or_else(|| {
                "Audit not started — call start_audit before finalize_audit".to_string()
            })?;

        // Build a human-readable summary from the recorded steps and
        // decryption attempts. This is stored in the `changes_summary` field
        // of the entry record so readers don't lose step detail entirely.
        let mut summary_parts: Vec<String> = Vec::new();

        if !self.current_entry.modules_executed.is_empty() {
            summary_parts.push(format!(
                "modules: {}",
                self.current_entry.modules_executed.join(", "),
            ));
        }

        if !self.current_entry.steps.is_empty() {
            let sizes: Vec<String> = self.current_entry.steps.iter()
                .filter(|s| s.input_size > 0 && s.output_size > 0)
                .map(|s| {
                    let ratio = 1.0 - (s.output_size as f64 / s.input_size as f64);
                    format!("{}: {:.1}%", s.step_name, ratio * 100.0)
                })
                .collect();
            if !sizes.is_empty() {
                summary_parts.push(format!("size changes: {}", sizes.join(", ")));
            }
        }

        if !self.current_entry.decryption_attempts.is_empty() {
            let ok = self.current_entry.decryption_attempts
                .iter()
                .filter(|a| a.success)
                .count();
            let total = self.current_entry.decryption_attempts.len();
            summary_parts.push(format!("decryption: {}/{} ok", ok, total));
        }

        let changes_summary = if summary_parts.is_empty() {
            None
        } else {
            Some(summary_parts.join("; "))
        };

        // Build the AuditFileConfig used when writing the header on first creation.
        let config = AuditFileConfig::new(
            Path::new(&self.source_file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            100,
        );

        // Build the entry record from the accumulated runtime data.
        let mut record = AuditEntryRecord::new();
        record.compilation_id    = self.current_entry.compilation_id.clone();
        record.timestamp         = self.current_entry.timestamp;
        record.source_checksum   = self.current_entry.source_checksum.clone();
        record.status            = self.current_entry.status.clone();
        record.modules_executed  = self.current_entry.modules_executed.clone();
        record.execution_time_ms = self.current_entry.execution_time_ms;
        record.changes_summary   = changes_summary;

        // AuditFileManager handles unlock → write → re-lock internally.
        manager.append_entry(&record, &config)
            .map_err(|e| format!("Failed to write audit entry: {}", e))?;

        self.base.log_info(&format!(
            "DIY audit finalized: {}", manager.audit_file_path(),
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
        let mut metadata = HashMap::with_capacity(6);
        metadata.insert("auditor_type".to_string(),   "diy".to_string());
        metadata.insert("format".to_string(),          "structured".to_string());
        metadata.insert("module_name".to_string(),     self.module_name().to_string());
        metadata.insert("priority".to_string(),        self.priority().to_string());
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
