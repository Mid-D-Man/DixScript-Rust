use std::fmt;

/// AST enhancement error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstEnhancementErrorType {
    InvalidParameterDefault,
    TypeInferenceFailed,
    CircularDependency,
    UnsupportedFeature,
    EnhancementFailed,
    InvalidTypeAnnotation,
    ConflictingDefaults,
}

/// AST enhancement error
#[derive(Debug, Clone)]
pub struct AstEnhancementError {
    pub error_id: String,
    pub error_type: AstEnhancementErrorType,
    pub message: String,
    pub line: i32,
    pub column: i32,
    pub section_name: Option<String>,
    pub suggestion: Option<String>,
    pub severity: super::ErrorSeverity,
    pub quick_fixes: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl AstEnhancementError {
    pub fn new(
        error_type: AstEnhancementErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        suggestion: Option<String>,
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = if line > 0 || column > 0 {
            format!("DXENH{:03}L{}C{}", error_type as u32, line, column)
        } else {
            format!("DXENH{:03}", error_type as u32)
        };

        let quick_fixes = Self::generate_quick_fixes(error_type);

        AstEnhancementError {
            error_id,
            error_type,
            message,
            line,
            column,
            section_name,
            suggestion,
            severity,
            quick_fixes,
            metadata: std::collections::HashMap::new(),
        }
    }

    fn generate_quick_fixes(error_type: AstEnhancementErrorType) -> Vec<String> {
        match error_type {
            AstEnhancementErrorType::InvalidParameterDefault => vec![
                "Check type annotation matches default value".to_string(),
                "Verify default value is valid for type".to_string(),
            ],
            AstEnhancementErrorType::TypeInferenceFailed => vec![
                "Add explicit type annotation".to_string(),
                "Provide default value for inference".to_string(),
            ],
            AstEnhancementErrorType::CircularDependency => vec![
                "Break circular reference".to_string(),
                "Restructure parameter defaults".to_string(),
            ],
            AstEnhancementErrorType::UnsupportedFeature => vec![
                "Check version compatibility".to_string(),
                "Update to newer version".to_string(),
            ],
            AstEnhancementErrorType::EnhancementFailed => vec![
                "Check AST structure is valid".to_string(),
                "Report issue if problem persists".to_string(),
            ],
            AstEnhancementErrorType::InvalidTypeAnnotation => vec![
                "Use valid DixScript type".to_string(),
                "Check type name spelling".to_string(),
            ],
            AstEnhancementErrorType::ConflictingDefaults => vec![
                "Remove one default value".to_string(),
                "Ensure consistent default".to_string(),
            ],
        }
    }

    /// Generate suggestion for AST enhancement error
    pub fn generate_suggestion(error_type: AstEnhancementErrorType, context: &str) -> String {
        match error_type {
            AstEnhancementErrorType::InvalidParameterDefault => {
                format!("Invalid default value for parameter '{}'. Check type compatibility.", context)
            }
            AstEnhancementErrorType::TypeInferenceFailed => {
                format!("Cannot infer type for '{}'. Add explicit type annotation.", context)
            }
            AstEnhancementErrorType::CircularDependency => {
                format!("Circular dependency detected in '{}'.", context)
            }
            AstEnhancementErrorType::UnsupportedFeature => {
                format!("Feature '{}' not supported in this version.", context)
            }
            AstEnhancementErrorType::EnhancementFailed => {
                format!("AST enhancement failed: {}", context)
            }
            AstEnhancementErrorType::InvalidTypeAnnotation => {
                format!("Invalid type annotation: {}", context)
            }
            AstEnhancementErrorType::ConflictingDefaults => {
                format!("Conflicting default values for '{}'.", context)
            }
        }
    }
}

impl fmt::Display for AstEnhancementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[{}] {:?}: {:?}", self.error_id, self.severity, self.error_type)?;

        if let Some(ref section) = self.section_name {
            writeln!(f, "Section: {}", section)?;
        }

        if self.line > 0 || self.column > 0 {
            writeln!(f, "Location: Line {}, Column {}", self.line, self.column)?;
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