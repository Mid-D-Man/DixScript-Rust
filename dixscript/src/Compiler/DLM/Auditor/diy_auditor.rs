//! Simple DIY auditor — creates a plain-text log of compilation events.

use super::auditor_trait::{
    IAuditor, AuditorResult, AuditResult, AuditEntry, AuditStep, DecryptionAttempt,
};
use super::auditor_utilities::AuditorPathUtils;
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::Compiler::AST::DixScript;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use sha2::{Sha256, Digest};

/// DIY auditor — appends a human-readable compilation record to a `.dixscript.au`
/// file located in the same directory as the source file.
pub struct DiyAuditor {
    base: DLMModuleBase,
    source_file_path: String,
    output_directory: String,
    audit_file_path: String,
    current_entry: AuditEntry,
    log_lines: Vec<String>,
}

impl DiyAuditor {
    pub fn new(source_file_path: impl AsRef<Path>, output_directory: impl AsRef<Path>) -> Self {
        DiyAuditor {
            base: DLMModuleBase::new("DAuditor.diy", 1),
            source_file_path: source_file_path.as_ref().to_string_lossy().to_string(),
            output_directory: output_directory.as_ref().to_string_lossy().to_string(),
            audit_file_path: String::new(),
            current_entry: AuditEntry::new(),
            log_lines: Vec::new(),
        }
    }

    fn calculate_checksum(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }

    fn count_compilations(&self) -> usize {
        if !Path::new(&self.audit_file_path).exists() {
            return 1;
        }
        match std::fs::read_to_string(&self.audit_file_path) {
            Ok(content) => content.matches("Compilation ID:").count(),
            Err(_) => 1,
        }
    }
}

impl IAuditor for DiyAuditor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn initialize(&mut self, _config: HashMap<String, String>) {
        if self.base.is_debug_enabled() {
            self.base.log_debug("Initialized DIY auditor (plain-text format)");
        }
    }

    fn start_audit(&mut self, _ast: &DixScript, binary_data: &[u8]) -> AuditorResult<AuditResult> {
        self.base.log_info("Starting DIY audit");

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

        self.log_lines.clear();
        self.log_lines.push("=".repeat(80));
        self.log_lines.push("DixScript Compilation Audit (DIY Format)".to_string());
        self.log_lines.push(format!("Compilation ID: {}", self.current_entry.compilation_id));
        self.log_lines.push(format!(
            "Timestamp: {}",
            self.current_entry.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
        ));
        self.log_lines.push(format!(
            "Source File: {}",
            Path::new(&self.source_file_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown"),
        ));
        self.log_lines.push(format!("Source Checksum: {}", self.current_entry.source_checksum));
        self.log_lines.push("=".repeat(80));
        self.log_lines.push(String::new());

        if self.base.is_debug_enabled() {
            self.base.log_debug(&format!("Audit file: {}", self.audit_file_path));
        }
        self.base.log_info(&format!("DIY audit started: {}", self.current_entry.compilation_id));

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

        let now = chrono::Utc::now();
        self.log_lines.push(format!("[{}] {}", now.format("%H:%M:%S%.3f"), step_name));
        self.log_lines.push(format!("  Details: {}", details));
        self.log_lines.push(format!("  Input Size: {} bytes", input_size));
        self.log_lines.push(format!("  Output Size: {} bytes", output_size));
        self.log_lines.push(format!("  Duration: {:.2}ms", duration_ms));

        if input_size > 0 && output_size > 0 && output_size != input_size {
            let ratio = 1.0 - (output_size as f64 / input_size as f64);
            self.log_lines.push(format!("  Size Change: {:.1}%", ratio * 100.0));
        }

        self.log_lines.push(String::new());

        if self.base.is_verbose_enabled() {
            self.base.log_verbose(&format!("Logged step: {}", step_name));
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

        let now = chrono::Utc::now();
        self.log_lines.push(String::new());
        self.log_lines.push("=".repeat(80));
        self.log_lines.push("DECRYPTION ATTEMPT".to_string());
        self.log_lines.push("=".repeat(80));
        self.log_lines.push(format!("Timestamp: {}", now.format("%Y-%m-%d %H:%M:%S UTC")));
        self.log_lines.push(format!("Status: {}", if success { "SUCCESS" } else { "FAILED" }));
        self.log_lines.push(format!("Details: {}", details));
        self.log_lines.push(format!("Encrypted Size: {} bytes", encrypted_size));
        if success {
            self.log_lines.push(format!("Decrypted Size: {} bytes", decrypted_size));
        }
        self.log_lines.push(format!("Duration: {:.2}ms", duration_ms));
        self.log_lines.push("=".repeat(80));
        self.log_lines.push(String::new());

        self.base.log_info(&format!(
            "Logged decryption attempt: {}",
            if success { "SUCCESS" } else { "FAILED" },
        ));
    }

    fn finalize_audit(&mut self) -> AuditorResult<()> {
        self.base.log_info("Finalizing DIY audit");

        self.log_lines.push("=".repeat(80));
        self.log_lines.push("COMPILATION SUMMARY".to_string());
        self.log_lines.push("=".repeat(80));
        self.log_lines.push(format!("Status: {}", self.current_entry.status));
        self.log_lines.push(format!(
            "Total Execution Time: {:.2}ms",
            self.current_entry.execution_time_ms,
        ));
        self.log_lines.push(format!(
            "Modules Executed: {}",
            self.current_entry.modules_executed.len(),
        ));
        for module in &self.current_entry.modules_executed {
            self.log_lines.push(format!("  - {}", module));
        }
        self.log_lines.push(String::new());
        self.log_lines.push(format!(
            "Audit completed at: {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        ));
        self.log_lines.push("=".repeat(80));

        let file_exists = Path::new(&self.audit_file_path).exists();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.audit_file_path)
            .map_err(|e| format!("Failed to open audit file: {}", e))?;

        if file_exists {
            writeln!(file).map_err(|e| format!("Failed to write audit file: {}", e))?;
            writeln!(file).map_err(|e| format!("Failed to write audit file: {}", e))?;
        }

        for line in &self.log_lines {
            writeln!(file, "{}", line)
                .map_err(|e| format!("Failed to write audit file: {}", e))?;
        }

        self.base.log_info(&format!("DIY audit finalized: {}", self.audit_file_path));
        self.base.log_info(&format!(
            "Total compilations logged: {}",
            self.count_compilations(),
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
        metadata.insert("auditor_type".to_string(), "diy".to_string());
        metadata.insert("format".to_string(), "text".to_string());
        metadata.insert("module_name".to_string(), self.module_name().to_string());
        metadata.insert("priority".to_string(), self.priority().to_string());
        metadata.insert("audit_file".to_string(), self.audit_file_path.clone());
        metadata.insert("compilation_id".to_string(), self.current_entry.compilation_id.clone());
        metadata
    }

    fn priority(&self) -> i32 {
        self.base.priority()
    }
}
