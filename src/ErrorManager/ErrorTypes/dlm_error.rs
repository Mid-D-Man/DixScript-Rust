//! DLM (Data Lifecycle Management) errors

use super::ErrorSeverity;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DLMError {
    pub error_id: String,
    pub error_type: DLMErrorType,
    pub message: String,
    pub module_name: Option<String>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
}

impl DLMError {
    pub fn new(
        error_type: DLMErrorType,
        message: String,
        module_name: Option<String>,
        suggestion: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXDLM{:?}", error_type);

        Self {
            error_id,
            error_type,
            message,
            module_name,
            suggestion,
            severity,
        }
    }
}

impl fmt::Display for DLMError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.error_id, self.message)?;

        if let Some(ref module) = self.module_name {
            write!(f, "\n📍 Module: {}", module)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n💡 Suggestion: {}", suggestion)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DLMPipelineException {
    pub module_name: String,
    pub error_type: DLMErrorType,
    pub message: String,
    pub inner_exception: Option<String>,
}

impl DLMPipelineException {
    pub fn new(
        module_name: String,
        error_type: DLMErrorType,
        message: String,
        inner_exception: Option<String>,
    ) -> Self {
        Self {
            module_name,
            error_type,
            message,
            inner_exception,
        }
    }
}

impl fmt::Display for DLMPipelineException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DLM Pipeline Exception in '{}': {}",
            self.module_name, self.message
        )?;

        if let Some(ref inner) = self.inner_exception {
            write!(f, "\n  Inner Exception: {}", inner)?;
        }

        Ok(())
    }
}