//! Semantic error types and handling

use super::error_enums::ErrorSeverity;
use crate::DixCore::List;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

#[derive(Debug, Clone)]
pub struct SemanticError {
    pub error_id: String,
    pub error_type: SemanticErrorType,
    pub message: String,
    pub line: i32,
    pub column: i32,
    pub section_name: Option<String>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
    pub quick_fixes: List<String>,
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
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXSEM{:03}L{}C{}", error_type as u32, line, column);

        let mut error = Self {
            error_id,
            error_type: error_type.clone(),
            message,
            line,
            column,
            section_name,
            suggestion,
            severity,
            quick_fixes: List::New(),
            metadata: std::collections::HashMap::new(),
        };

        error.generate_quick_fixes(&error_type);
        error
    }

    pub fn generate_suggestion(error_type: &SemanticErrorType, context: &str) -> String {
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

    fn generate_quick_fixes(&mut self, error_type: &SemanticErrorType) {
        match error_type {
            SemanticErrorType::UndefinedReference => {
                self.quick_fixes.Add("Check if identifier is defined before use".to_string());
                self.quick_fixes.Add("Verify spelling of identifier".to_string());
                self.quick_fixes.Add("Check scope visibility".to_string());
            }
            SemanticErrorType::DuplicateDefinition => {
                self.quick_fixes.Add("Use a different name".to_string());
                self.quick_fixes.Add("Remove duplicate definition".to_string());
            }
            SemanticErrorType::TypeMismatch => {
                self.quick_fixes.Add("Check expected type".to_string());
                self.quick_fixes.Add("Add type conversion".to_string());
            }
            SemanticErrorType::InvalidScope => {
                self.quick_fixes.Add("Check function scope declaration".to_string());
                self.quick_fixes.Add("Verify scope matches usage location".to_string());
            }
            SemanticErrorType::CircularDependency => {
                self.quick_fixes.Add("Break circular reference".to_string());
                self.quick_fixes.Add("Restructure dependencies".to_string());
            }
            SemanticErrorType::UnreachableCode => {
                self.quick_fixes.Add("Remove unreachable code".to_string());
                self.quick_fixes.Add("Fix control flow logic".to_string());
            }
            SemanticErrorType::MissingReturn => {
                self.quick_fixes.Add("Add return statement".to_string());
                self.quick_fixes.Add("Ensure all code paths return".to_string());
            }
            SemanticErrorType::InvalidEnumValue => {
                self.quick_fixes.Add("Check enum definition in @ENUMS".to_string());
                self.quick_fixes.Add("Use valid enum member".to_string());
            }
            SemanticErrorType::ScopeViolation => {
                self.quick_fixes.Add("Check function scope matches call location".to_string());
                self.quick_fixes.Add("Make function globally accessible".to_string());
            }
            _ => {}
        }
    }
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "[{}] {}: {:?}",
            self.error_id, self.severity, self.error_type
        )?;

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

        if !self.quick_fixes.IsEmpty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in self.quick_fixes.Iter() {
                writeln!(f, "  - {}", fix)?;
            }
        }

        Ok(())
    }
}