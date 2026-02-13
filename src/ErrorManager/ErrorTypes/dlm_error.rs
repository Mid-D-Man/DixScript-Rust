use std::fmt;

/// DLM (Dynamic Library Manager) error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DlmErrorType {
    LibraryNotFound,
    FunctionNotFound,
    InvalidFunctionSignature,
    LoadFailed,
    InvocationFailed,
    UnsupportedPlatform,
    InvalidLibraryPath,
    SecurityViolation,
    VersionMismatch,
    DependencyMissing,
    KeyGenerationFailed,
    KeyFileMissing,
    ModuleExecutionFailed,
}

/// DLM error with library context
#[derive(Debug, Clone)]
pub struct DlmError {
    pub error_id: String,
    pub error_type: DlmErrorType,
    pub message: String,
    pub library_path: Option<String>,
    pub function_name: Option<String>,
    pub suggestion: Option<String>,
    pub severity: super::ErrorSeverity,
    pub quick_fixes: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl DlmError {
    pub fn new(
        error_type: DlmErrorType,
        message: String,
        library_path: Option<String>,
        function_name: Option<String>,
        suggestion: Option<String>,
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXDLM{:03}", error_type as u32);
        let quick_fixes = Self::generate_quick_fixes(error_type);

        DlmError {
            error_id,
            error_type,
            message,
            library_path,
            function_name,
            suggestion,
            severity,
            quick_fixes,
            metadata: std::collections::HashMap::new(),
        }
    }

    fn generate_quick_fixes(error_type: DlmErrorType) -> Vec<String> {
        match error_type {
            DlmErrorType::LibraryNotFound => vec![
                "Check library path is correct".to_string(),
                "Verify library file exists".to_string(),
                "Install missing library".to_string(),
            ],
            DlmErrorType::FunctionNotFound => vec![
                "Check function name spelling".to_string(),
                "Verify function is exported from library".to_string(),
                "Check library documentation".to_string(),
            ],
            DlmErrorType::InvalidFunctionSignature => vec![
                "Check parameter types match".to_string(),
                "Verify return type is correct".to_string(),
                "Review library API documentation".to_string(),
            ],
            DlmErrorType::LoadFailed => vec![
                "Check library is compatible with platform".to_string(),
                "Verify all dependencies are available".to_string(),
                "Check file permissions".to_string(),
            ],
            DlmErrorType::UnsupportedPlatform => vec![
                "Use platform-specific library".to_string(),
                "Check library supports current OS".to_string(),
            ],
            DlmErrorType::InvalidLibraryPath => vec![
                "Use absolute path".to_string(),
                "Check path separators".to_string(),
                "Verify path format".to_string(),
            ],
            DlmErrorType::SecurityViolation => vec![
                "Library blocked by security policy".to_string(),
                "Update security settings".to_string(),
            ],
            _ => Vec::new(),
        }
    }

    /// Generate suggestion for DLM error
    pub fn generate_suggestion(
        error_type: DlmErrorType,
        library_path: Option<&str>,
        function_name: Option<&str>,
    ) -> String {
        match error_type {
            DlmErrorType::LibraryNotFound => {
                format!(
                    "Library '{}' not found. Verify the path and ensure the library is installed.",
                    library_path.unwrap_or("unknown")
                )
            }
            DlmErrorType::FunctionNotFound => {
                format!(
                    "Function '{}' not found in library '{}'. Check the function is exported.",
                    function_name.unwrap_or("unknown"),
                    library_path.unwrap_or("unknown")
                )
            }
            DlmErrorType::InvalidFunctionSignature => {
                format!(
                    "Function signature mismatch for '{}'. Check parameter and return types.",
                    function_name.unwrap_or("unknown")
                )
            }
            DlmErrorType::LoadFailed => {
                format!(
                    "Failed to load library '{}'. Check dependencies and compatibility.",
                    library_path.unwrap_or("unknown")
                )
            }
            DlmErrorType::InvocationFailed => {
                format!(
                    "Failed to invoke function '{}'. Check arguments and library state.",
                    function_name.unwrap_or("unknown")
                )
            }
            DlmErrorType::UnsupportedPlatform => {
                "Dynamic library loading not supported on current platform.".to_string()
            }
            DlmErrorType::InvalidLibraryPath => {
                "Library path format is invalid. Use absolute paths with correct separators.".to_string()
            }
            DlmErrorType::SecurityViolation => {
                "Library loading blocked by security policy.".to_string()
            }
            DlmErrorType::VersionMismatch => {
                "Library version incompatible with current DixScript version.".to_string()
            }
            DlmErrorType::DependencyMissing => {
                "Library dependencies are missing or cannot be resolved.".to_string()
            }
        }
    }
}

impl fmt::Display for DlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[{}] {:?}: {:?}", self.error_id, self.severity, self.error_type)?;

        if let Some(ref path) = self.library_path {
            writeln!(f, "Library Path: {}", path)?;
        }

        if let Some(ref func) = self.function_name {
            writeln!(f, "Function: {}", func)?;
        }

        writeln!(f, "Message: {}", self.message)?;

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