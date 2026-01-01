//! DLM (Data Lifecycle Management) error types and handling

use super::error_enums::ErrorSeverity;
use crate::DixCore::List;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DLMErrorType {
    ModuleExecutionFailed,
    InvalidModuleConfiguration,
    InvalidPipelineOrder,
    MissingDependency,
    FileIOError,
    InvalidBinaryData,
    CorruptedData,
    UnsupportedModule,
    InitializationFailed,
    CompressionFailed,
    DecompressionFailed,
    UnsupportedCompressionFormat,
    EncryptionFailed,
    DecryptionFailed,
    InvalidPassword,
    KeyGenerationFailed,
    KeyFileMissing,
    KeyFileCorrupted,
    AuditWriteFailed,
    AuditReadFailed,
    AuditLogFailed,
    FileReadError,
    FileWriteError,
    InsufficientPermissions,
    DataCorrupted,
    ChecksumMismatch,
    UnexpectedDataFormat,
    ModuleNotFound,
}

#[derive(Debug, Clone)]
pub struct DLMError {
    pub error_id: String,
    pub error_type: DLMErrorType,
    pub message: String,
    pub module_name: Option<String>,
    pub file_path: Option<String>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
    pub quick_fixes: List<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl DLMError {
    pub fn new(
        error_type: DLMErrorType,
        message: String,
        module_name: Option<String>,
        file_path: Option<String>,
        suggestion: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXDLM{:03}", error_type as u32);

        let mut error = Self {
            error_id,
            error_type: error_type.clone(),
            message,
            module_name,
            file_path,
            suggestion,
            severity,
            quick_fixes: List::New(),
            metadata: std::collections::HashMap::new(),
        };

        error.generate_quick_fixes(&error_type);
        error
    }

    pub fn generate_suggestion(error_type: &DLMErrorType, context: &str) -> String {
        match error_type {
            DLMErrorType::ModuleNotFound => {
                format!("Module '{}' not found. Available: DCompressor, DEncryptor, DAuditor", context)
            }
            DLMErrorType::ModuleExecutionFailed => {
                format!("Module '{}' execution failed. Check configuration and input data.", context)
            }
            DLMErrorType::InvalidModuleConfiguration => {
                format!("Invalid configuration for module '{}'. Check @DLM section syntax.", context)
            }
            DLMErrorType::CompressionFailed => {
                "Compression failed. Verify input data and available resources.".to_string()
            }
            DLMErrorType::EncryptionFailed => {
                "Encryption failed. Check @SECURITY section and key configuration.".to_string()
            }
            DLMErrorType::AuditLogFailed => {
                "Audit logging failed. Check output directory permissions.".to_string()
            }
            DLMErrorType::FileIOError => {
                format!("File I/O error: {}. Check paths and permissions.", context)
            }
            DLMErrorType::InvalidBinaryData => {
                "Invalid binary data format. Ensure file is valid .mdix.enc.".to_string()
            }
            DLMErrorType::DecryptionFailed => {
                "Decryption failed. Verify key and data integrity.".to_string()
            }
            DLMErrorType::CorruptedData => {
                format!("Data corruption detected: {}", context)
            }
            _ => format!("DLM error: {}", context),
        }
    }

    fn generate_quick_fixes(&mut self, error_type: &DLMErrorType) {
        match error_type {
            DLMErrorType::ModuleNotFound => {
                self.quick_fixes.Add("Check module name spelling in @DLM section".to_string());
                self.quick_fixes.Add("Verify module is supported (DCompressor, DEncryptor, DAuditor)".to_string());
            }
            DLMErrorType::ModuleExecutionFailed => {
                self.quick_fixes.Add("Check module configuration parameters".to_string());
                self.quick_fixes.Add("Verify input data format".to_string());
                self.quick_fixes.Add("Check system permissions".to_string());
            }
            DLMErrorType::InvalidModuleConfiguration => {
                self.quick_fixes.Add("Review module parameters in @DLM section".to_string());
                self.quick_fixes.Add("Check configuration syntax".to_string());
            }
            DLMErrorType::CompressionFailed => {
                self.quick_fixes.Add("Verify input data is valid".to_string());
                self.quick_fixes.Add("Check available disk space".to_string());
                self.quick_fixes.Add("Try different compression level".to_string());
            }
            DLMErrorType::EncryptionFailed => {
                self.quick_fixes.Add("Verify encryption key is valid".to_string());
                self.quick_fixes.Add("Check @SECURITY section configuration".to_string());
                self.quick_fixes.Add("Ensure required permissions".to_string());
            }
            DLMErrorType::AuditLogFailed => {
                self.quick_fixes.Add("Check output directory permissions".to_string());
                self.quick_fixes.Add("Verify disk space available".to_string());
            }
            DLMErrorType::FileIOError => {
                self.quick_fixes.Add("Check file path and permissions".to_string());
                self.quick_fixes.Add("Verify directory exists".to_string());
                self.quick_fixes.Add("Ensure sufficient disk space".to_string());
            }
            DLMErrorType::InvalidBinaryData => {
                self.quick_fixes.Add("Verify input file is valid .mdix.enc".to_string());
                self.quick_fixes.Add("Check file integrity".to_string());
                self.quick_fixes.Add("Ensure file wasn't corrupted".to_string());
            }
            DLMErrorType::DecryptionFailed => {
                self.quick_fixes.Add("Verify decryption key is correct".to_string());
                self.quick_fixes.Add("Check key file path".to_string());
                self.quick_fixes.Add("Ensure data wasn't tampered with".to_string());
            }
            DLMErrorType::CorruptedData => {
                self.quick_fixes.Add("Verify file integrity".to_string());
                self.quick_fixes.Add("Check for transmission errors".to_string());
                self.quick_fixes.Add("Try regenerating from source".to_string());
            }
            _ => {}
        }
    }
}

impl fmt::Display for DLMError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "[{}] {}: {:?}",
            self.error_id, self.severity, self.error_type
        )?;

        if let Some(ref module) = self.module_name {
            writeln!(f, "Module: {}", module)?;
        }

        if let Some(ref path) = self.file_path {
            writeln!(f, "File: {}", path)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.IsEmpty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in self.quick_fixes.Iter() {
                writeln!(f, "  - {}", fix)?;
            }
        }

        Ok(())
    }
}

/// DLM Pipeline Exception
#[derive(Debug, Clone)]
pub struct DLMPipelineException {
    pub module_name: String,
    pub error_type: DLMErrorType,
    pub message: String,
    pub inner_exception: Option<String>,
}

impl DLMPipelineException {
    pub fn new(module_name: String, error_type: DLMErrorType, message: String) -> Self {
        Self {
            module_name,
            error_type,
            message,
            inner_exception: None,
        }
    }

    pub fn with_inner(
        module_name: String,
        error_type: DLMErrorType,
        message: String,
        inner_exception: String,
    ) -> Self {
        Self {
            module_name,
            error_type,
            message,
            inner_exception: Some(inner_exception),
        }
    }
}

impl fmt::Display for DLMPipelineException {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[{}] {:?}: {}", self.module_name, self.error_type, self.message)
    }
}

impl std::error::Error for DLMPipelineException {}