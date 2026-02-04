//! Auditor trait and supporting types

use crate::Compiler::AST::{DixScript, Position};
use std::collections::HashMap;

/// Result type for audit operations
pub type AuditorResult<T> = Result<T, String>;

/// Result of audit operation
#[derive(Debug, Clone)]
pub struct AuditResult {
    pub is_success: bool,
    pub audit_file_path: String,
    pub audit_id: String,
    pub errors: Vec<String>,
}

impl AuditResult {
    pub fn success(audit_file_path: String, audit_id: String) -> Self {
        AuditResult {
            is_success: true,
            audit_file_path,
            audit_id,
            errors: Vec::new(),
        }
    }

    pub fn failure(errors: Vec<String>) -> Self {
        AuditResult {
            is_success: false,
            audit_file_path: String::new(),
            audit_id: String::new(),
            errors,
        }
    }
}

/// Represents a single audit entry (one compilation)
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub compilation_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source_checksum: String,
    pub previous_checksum: Option<String>,
    pub status: String, // SUCCESS, FAILED, WARNING
    pub modules_executed: Vec<String>,
    pub execution_time_ms: f64,
    pub changes_detected: Vec<AuditChange>,
    pub changes_summary: Option<String>,
    pub steps: Vec<AuditStep>,
    pub decryption_attempts: Vec<DecryptionAttempt>,
}

impl AuditEntry {
    pub fn new() -> Self {
        AuditEntry {
            compilation_id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            timestamp: chrono::Utc::now(),
            source_checksum: String::new(),
            previous_checksum: None,
            status: "SUCCESS".to_string(),
            modules_executed: Vec::new(),
            execution_time_ms: 0.0,
            changes_detected: Vec::new(),
            changes_summary: None,
            steps: Vec::new(),
            decryption_attempts: Vec::new(),
        }
    }
}

impl Default for AuditEntry {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a decryption attempt for security auditing
#[derive(Debug, Clone)]
pub struct DecryptionAttempt {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub success: bool,
    pub details: String,
    pub encrypted_size: usize,
    pub decrypted_size: usize,
    pub duration_ms: f64,
}

impl DecryptionAttempt {
    pub fn new(
        success: bool,
        details: String,
        encrypted_size: usize,
        decrypted_size: usize,
        duration_ms: f64,
    ) -> Self {
        DecryptionAttempt {
            timestamp: chrono::Utc::now(),
            success,
            details,
            encrypted_size,
            decrypted_size,
            duration_ms,
        }
    }
}

/// Represents a detected change in the AST
#[derive(Debug, Clone)]
pub struct AuditChange {
    pub section: String,
    pub path: String,
    pub change_type: String, // ADDED, MODIFIED, DELETED
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

impl AuditChange {
    pub fn new(
        section: String,
        path: String,
        change_type: String,
        old_value: Option<String>,
        new_value: Option<String>,
    ) -> Self {
        AuditChange {
            section,
            path,
            change_type,
            old_value,
            new_value,
        }
    }
}

/// Represents a single pipeline step in the audit
#[derive(Debug, Clone)]
pub struct AuditStep {
    pub step_name: String,
    pub details: String,
    pub input_size: usize,
    pub output_size: usize,
    pub duration_ms: f64,
}

impl AuditStep {
    pub fn new(
        step_name: String,
        details: String,
        input_size: usize,
        output_size: usize,
        duration_ms: f64,
    ) -> Self {
        AuditStep {
            step_name,
            details,
            input_size,
            output_size,
            duration_ms,
        }
    }
}

/// Trait for auditing modules
/// Auditor wraps the entire pipeline - starts first, ends last
pub trait IAuditor {
    /// Get module name
    fn module_name(&self) -> &str;

    /// Initialize auditor with configuration
    fn initialize(&mut self, config: HashMap<String, String>);

    /// Start audit tracking at beginning of compilation
    fn start_audit(&mut self, ast: &DixScript, binary_data: &[u8]) -> AuditorResult<AuditResult>;

    /// Log a pipeline step (called by DLMPipelineExecutor)
    fn log_step(
        &mut self,
        step_name: &str,
        details: &str,
        input_size: usize,
        output_size: usize,
        duration_ms: f64,
    );

    /// Log a decryption attempt (called by DLMReverseExecutor)
    fn log_decryption_attempt(
        &mut self,
        success: bool,
        details: &str,
        encrypted_size: usize,
        decrypted_size: usize,
        duration_ms: f64,
    );

    /// Finalize audit and write to .mdix.au file
    fn finalize_audit(&mut self) -> AuditorResult<()>;

    /// Validate auditor can execute
    fn validate(&self) -> Result<(), String>;

    /// Get metadata for .mdix.key file
    fn get_metadata(&self) -> HashMap<String, String>;

    /// Get priority (lower = earlier execution)
    fn priority(&self) -> i32;
              }
