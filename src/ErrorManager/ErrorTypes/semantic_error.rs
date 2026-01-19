use std::fmt;

/// Semantic error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticErrorType {
    UndefinedReference,
    DuplicateDefinition,
    TypeMismatch,
    InvalidScope,
    CircularDependency,
    UnreachableCode,
    MissingReturn,
    InvalidEnumValue,
    ScopeViolation,
    InvalidConfiguration,
    MissingRequiredSection,
    IncompatibleTypes,
    InvalidOperation,
    ImportError,
    CircularImport,
    NamespaceNotFound,
    ImportedFunctionNotFound,
    ImportedEnumNotFound,
    InvalidReference,
    InvalidLiteral,
    NameConflict,
}

/// Semantic error with section context
#[derive(Debug, Clone)]
pub struct SemanticError {
    pub error_id: String,
    pub error_type: SemanticErrorType,
    pub message: String,
    pub line: i32,
    pub column: i32,
    pub section_name: Option<String>,
    pub suggestion: Option<String>,
    pub severity: super::ErrorSeverity,
    pub quick_fixes: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl SemanticError {
    pub fn new(
        error_type: SemanticErrorType,
        message: String,
        line: i32,
        column: i32,
        section_name: Option<String>,
        suggestion: Option<String>,
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXSEM{:03}L{}C{}", error_type as u32, line, column);
        let quick_fixes = Self::generate_quick_fixes(error_type);

        SemanticError {
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

    fn generate_quick_fixes(error_type: SemanticErrorType) -> Vec<String> {
        match error_type {
            SemanticErrorType::UndefinedReference => vec![
                "Check if identifier is defined before use".to_string(),
                "Verify spelling of identifier".to_string(),
                "Check scope visibility".to_string(),
            ],
            SemanticErrorType::DuplicateDefinition => vec![
                "Use a different name".to_string(),
                "Remove duplicate definition".to_string(),
            ],
            SemanticErrorType::TypeMismatch => vec![
                "Check expected type".to_string(),
                "Add type conversion".to_string(),
            ],
            SemanticErrorType::InvalidScope => vec![
                "Check function scope declaration".to_string(),
                "Verify scope matches usage location".to_string(),
            ],
            SemanticErrorType::CircularDependency => vec![
                "Break circular reference".to_string(),
                "Restructure dependencies".to_string(),
            ],
            SemanticErrorType::UnreachableCode => vec![
                "Remove unreachable code".to_string(),
                "Fix control flow logic".to_string(),
            ],
            SemanticErrorType::MissingReturn => vec![
                "Add return statement".to_string(),
                "Ensure all code paths return".to_string(),
            ],
            SemanticErrorType::InvalidEnumValue => vec![
                "Check enum definition in @ENUMS".to_string(),
                "Use valid enum member".to_string(),
            ],
            SemanticErrorType::ScopeViolation => vec![
                "Check function scope matches call location".to_string(),
                "Make function globally accessible".to_string(),
            ],
            _ => Vec::new(),
        }
    }

    /// Generate suggestion for semantic error
    pub fn generate_suggestion(error_type: SemanticErrorType, context: &str) -> String {
        match error_type {
            SemanticErrorType::UndefinedReference => {
                format!("Identifier '{}' is not defined in current scope.", context)
            }
            SemanticErrorType::DuplicateDefinition => {
                format!("Identifier '{}' is already defined.", context)
            }
            SemanticErrorType::TypeMismatch => {
                format!("Type mismatch in '{}'. Check expected type.", context)
            }
            SemanticErrorType::InvalidScope => {
                format!("Invalid scope access for '{}'.", context)
            }
            SemanticErrorType::CircularDependency => {
                format!("Circular dependency detected involving '{}'.", context)
            }
            SemanticErrorType::UnreachableCode => {
                "Code is unreachable and will never execute.".to_string()
            }
            SemanticErrorType::MissingReturn => {
                format!("Function '{}' missing return statement.", context)
            }
            SemanticErrorType::InvalidEnumValue => {
                format!("Invalid enum value '{}'. Check @ENUMS section.", context)
            }
            SemanticErrorType::ScopeViolation => {
                format!("Function '{}' not accessible from current scope.", context)
            }
            _ => format!("Semantic error: {}", context),
        }
    }
}

impl fmt::Display for SemanticError {
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