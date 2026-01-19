use std::fmt;

/// Imports resolution error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportsResolutionErrorType {
    /// The imported file was not found at the specified path
    FileNotFound = 1,
    /// A circular dependency was detected in the import chain
    CircularDependency = 2,
    /// The import path is invalid (wrong format, extension, etc.)
    InvalidPath = 3,
    /// A parse error occurred in the imported file
    ParseError = 4,
    /// A semantic error occurred in the imported file
    SemanticError = 5,
    /// The import alias is already used
    DuplicateAlias = 6,
    /// The import alias conflicts with a built-in or existing symbol
    AliasConflict = 7,
    /// The import path does not have .mdix extension
    InvalidExtension = 8,
    /// Cloud imports are not yet supported
    CloudImportNotSupported = 9,
    /// Hash verification failed for the imported file
    HashVerificationFailed = 10,
    /// A file is trying to import itself
    SelfImport = 11,
    /// AST enhancement failed for the imported file
    EnhancementError = 12,
    /// General import resolution error
    GeneralError = 99,
}

/// Imports resolution error with detailed context
#[derive(Debug, Clone)]
pub struct ImportsResolutionError {
    pub error_id: String,
    pub error_type: ImportsResolutionErrorType,
    pub message: String,
    pub import_alias: String,
    pub import_path: Option<String>,
    pub resolved_path: Option<String>,
    pub circular_chain: Option<Vec<String>>,
    pub suggestion: Option<String>,
    pub severity: super::ErrorSeverity,
    pub line: i32,
    pub column: i32,
    pub quick_fixes: Vec<String>,
}

impl ImportsResolutionError {
    pub fn new(
        error_type: ImportsResolutionErrorType,
        message: String,
        import_alias: String,
        import_path: Option<String>,
        resolved_path: Option<String>,
        circular_chain: Option<Vec<String>>,
        line: i32,
        column: i32,
        suggestion: Option<String>,
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = format!("IMP{:03}", error_type as u32);

        let suggestion = suggestion.unwrap_or_else(|| {
            Self::generate_suggestion(error_type, &import_alias, import_path.as_deref())
        });

        let quick_fixes = Self::generate_quick_fixes(error_type);

        ImportsResolutionError {
            error_id,
            error_type,
            message,
            import_alias,
            import_path,
            resolved_path,
            circular_chain,
            suggestion: Some(suggestion),
            severity,
            line,
            column,
            quick_fixes,
        }
    }

    fn generate_suggestion(
        error_type: ImportsResolutionErrorType,
        import_alias: &str,
        import_path: Option<&str>,
    ) -> String {
        match error_type {
            ImportsResolutionErrorType::FileNotFound => {
                format!(
                    "Check that the file path '{}' is correct and the file exists. \
                     Paths are resolved relative to the importing file's directory.",
                    import_path.unwrap_or("unknown")
                )
            }
            ImportsResolutionErrorType::CircularDependency => {
                "Remove the circular import chain to break the cycle. \
                 Consider extracting shared code into a separate utility file.".to_string()
            }
            ImportsResolutionErrorType::InvalidPath => {
                "Import paths must use forward slashes (/) and end with .mdix extension. \
                 Use relative paths (e.g., '../shared/utils.mdix') instead of absolute paths.".to_string()
            }
            ImportsResolutionErrorType::ParseError => {
                format!("Fix syntax errors in the imported file '{}' before importing it.",
                        import_path.unwrap_or("unknown"))
            }
            ImportsResolutionErrorType::SemanticError => {
                format!("Fix semantic errors in the imported file '{}' before importing it.",
                        import_path.unwrap_or("unknown"))
            }
            ImportsResolutionErrorType::DuplicateAlias => {
                format!("Import alias '{}' is already used. Choose a different alias or remove the duplicate import.",
                        import_alias)
            }
            ImportsResolutionErrorType::AliasConflict => {
                format!("Import alias '{}' conflicts with a built-in name or existing symbol. Choose a different alias that doesn't conflict.",
                        import_alias)
            }
            ImportsResolutionErrorType::InvalidExtension => {
                "Import path must end with .mdix extension. DixScript only supports importing .mdix files.".to_string()
            }
            ImportsResolutionErrorType::CloudImportNotSupported => {
                "Cloud imports (from_cloud) are not yet implemented in v1.0.0. Use local file imports (from) instead.".to_string()
            }
            ImportsResolutionErrorType::HashVerificationFailed => {
                "The imported file's hash does not match the expected hash. The file may have been modified or corrupted.".to_string()
            }
            ImportsResolutionErrorType::SelfImport => {
                "A file cannot import itself. Remove this import declaration.".to_string()
            }
            ImportsResolutionErrorType::EnhancementError => {
                format!("AST enhancement failed for imported file '{}'. Check for issues with parameter defaults in the imported file.",
                        import_path.unwrap_or("unknown"))
            }
            _ => format!("Import resolution failed for '{}' from '{}'.",
                         import_alias, import_path.unwrap_or("unknown")),
        }
    }

    fn generate_quick_fixes(error_type: ImportsResolutionErrorType) -> Vec<String> {
        match error_type {
            ImportsResolutionErrorType::FileNotFound => vec![
                "Check file path spelling".to_string(),
                "Verify file exists in expected location".to_string(),
                "Use relative paths from importing file".to_string(),
            ],
            ImportsResolutionErrorType::CircularDependency => vec![
                "Remove one of the imports in the cycle".to_string(),
                "Extract shared code into a new file".to_string(),
                "Restructure dependencies to avoid cycle".to_string(),
            ],
            ImportsResolutionErrorType::DuplicateAlias => vec![
                "Rename one of the import aliases".to_string(),
                "Remove duplicate import if not needed".to_string(),
            ],
            ImportsResolutionErrorType::AliasConflict => vec![
                "Choose a different alias name".to_string(),
                "Avoid names that conflict with built-ins".to_string(),
            ],
            ImportsResolutionErrorType::InvalidPath => vec![
                "Use forward slashes (/) in paths".to_string(),
                "Ensure path ends with .mdix".to_string(),
                "Use relative paths instead of absolute".to_string(),
            ],
            ImportsResolutionErrorType::InvalidExtension => vec![
                "Add .mdix extension to path".to_string(),
            ],
            ImportsResolutionErrorType::SelfImport => vec![
                "Remove this import declaration".to_string(),
            ],
            _ => Vec::new(),
        }
    }
}

impl fmt::Display for ImportsResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[{}] {:?}: {:?}", self.error_id, self.severity, self.error_type)?;

        if self.line > 0 || self.column > 0 {
            writeln!(f, "Location: Line {}, Column {}", self.line, self.column)?;
        }

        writeln!(f, "Import Alias: {}", self.import_alias)?;

        if let Some(ref path) = self.import_path {
            writeln!(f, "Import Path: {}", path)?;
        }

        if let Some(ref resolved) = self.resolved_path {
            writeln!(f, "Resolved Path: {}", resolved)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref chain) = self.circular_chain {
            if !chain.is_empty() {
                writeln!(f, "Circular Import Chain:")?;
                for file in chain {
                    writeln!(f, "  → {}", file)?;
                }
            }
        }

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.is_empty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in &self.quick_fixes {
                writeln!(f, "  • {}", fix)?;
            }
        }

        Ok(())
    }
}