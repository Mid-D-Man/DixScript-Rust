use std::fmt;

/// General error types for miscellaneous errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneralErrorType {
    Unknown,
    Internal,
    NotSupported,
    NotImplemented,
    InvalidInput,
    InvalidOutput,
    FileSystemError,
    NetworkError,
    IOError,
    EncodingError,
    DecodingError,
    Timeout,
    Cancelled,
    InvalidState,
    ConfigurationError,
}

/// General error for miscellaneous issues
#[derive(Debug, Clone)]
pub struct GeneralError {
    pub error_id: String,
    pub error_type: GeneralErrorType,
    pub message: String,
    pub context: Option<String>,
    pub source_error: Option<String>,
    pub suggestion: Option<String>,
    pub severity: super::ErrorSeverity,
    pub quick_fixes: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl GeneralError {
    pub fn new(
        error_type: GeneralErrorType,
        message: String,
        context: Option<String>,
        source_error: Option<String>,
        suggestion: Option<String>,
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXGEN{:03}", error_type as u32);
        let quick_fixes = Self::generate_quick_fixes(error_type);

        GeneralError {
            error_id,
            error_type,
            message,
            context,
            source_error,
            suggestion,
            severity,
            quick_fixes,
            metadata: std::collections::HashMap::new(),
        }
    }

    fn generate_quick_fixes(error_type: GeneralErrorType) -> Vec<String> {
        match error_type {
            GeneralErrorType::Unknown => vec![
                "Check error message for details".to_string(),
                "Report issue if problem persists".to_string(),
            ],
            GeneralErrorType::Internal => vec![
                "This is an internal error".to_string(),
                "Report to DixScript maintainers".to_string(),
            ],
            GeneralErrorType::NotSupported => vec![
                "Check feature availability".to_string(),
                "Use alternative approach".to_string(),
            ],
            GeneralErrorType::NotImplemented => vec![
                "Feature not yet implemented".to_string(),
                "Check roadmap for planned release".to_string(),
            ],
            GeneralErrorType::InvalidInput => vec![
                "Verify input format".to_string(),
                "Check input constraints".to_string(),
            ],
            GeneralErrorType::FileSystemError => vec![
                "Check file/directory exists".to_string(),
                "Verify permissions".to_string(),
                "Check disk space".to_string(),
            ],
            GeneralErrorType::NetworkError => vec![
                "Check network connection".to_string(),
                "Verify server is accessible".to_string(),
            ],
            GeneralErrorType::IOError => vec![
                "Check file permissions".to_string(),
                "Verify path is correct".to_string(),
            ],
            GeneralErrorType::Timeout => vec![
                "Increase timeout duration".to_string(),
                "Check for blocking operations".to_string(),
            ],
            GeneralErrorType::Cancelled => vec![
                "Operation was cancelled".to_string(),
            ],
            _ => Vec::new(),
        }
    }

    /// Generate suggestion for general error
    pub fn generate_suggestion(
        error_type: GeneralErrorType,
        context: Option<&str>,
    ) -> String {
        match error_type {
            GeneralErrorType::Unknown => {
                "An unknown error occurred. Check error details.".to_string()
            }
            GeneralErrorType::Internal => {
                "An internal error occurred. This should not happen. Please report this issue.".to_string()
            }
            GeneralErrorType::NotSupported => {
                format!(
                    "Feature '{}' is not supported in this configuration.",
                    context.unwrap_or("unknown")
                )
            }
            GeneralErrorType::NotImplemented => {
                format!(
                    "Feature '{}' is not yet implemented.",
                    context.unwrap_or("unknown")
                )
            }
            GeneralErrorType::InvalidInput => {
                format!(
                    "Invalid input provided for '{}'.",
                    context.unwrap_or("unknown")
                )
            }
            GeneralErrorType::InvalidOutput => {
                "Invalid output format or value.".to_string()
            }
            GeneralErrorType::FileSystemError => {
                format!(
                    "File system error: {}",
                    context.unwrap_or("unknown error")
                )
            }
            GeneralErrorType::NetworkError => {
                "Network error occurred. Check connection.".to_string()
            }
            GeneralErrorType::IOError => {
                format!(
                    "I/O error: {}",
                    context.unwrap_or("unknown error")
                )
            }
            GeneralErrorType::EncodingError => {
                "Failed to encode data.".to_string()
            }
            GeneralErrorType::DecodingError => {
                "Failed to decode data.".to_string()
            }
            GeneralErrorType::Timeout => {
                format!(
                    "Operation '{}' timed out.",
                    context.unwrap_or("unknown")
                )
            }
            GeneralErrorType::Cancelled => {
                "Operation was cancelled by user.".to_string()
            }
            GeneralErrorType::InvalidState => {
                "Invalid state for this operation.".to_string()
            }
            GeneralErrorType::ConfigurationError => {
                "Configuration error encountered.".to_string()
            }
        }
    }

    /// Create from a standard error
    pub fn from_error<E: std::error::Error>(error: E, severity: super::ErrorSeverity) -> Self {
        GeneralError::new(
            GeneralErrorType::Unknown,
            error.to_string(),
            None,
            Some(format!("{:?}", error)),
            None,
            severity,
        )
    }
}

impl fmt::Display for GeneralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[{}] {:?}: {:?}", self.error_id, self.severity, self.error_type)?;

        if let Some(ref ctx) = self.context {
            writeln!(f, "Context: {}", ctx)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref source) = self.source_error {
            writeln!(f, "Source Error: {}", source)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.is_empty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in &self.quick_fixes {
                writeln!(f, "  - {}", fix)?;
            }
        }

        Ok(())
    }
}