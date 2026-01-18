//! Semantic analysis errors (type checking, scope validation)

use super::ErrorSeverity;
use crate::DixCore::List;
use std::collections::HashMap;
use std::fmt;

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
}

#[derive(Debug, Clone, PartialEq)]
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
    pub metadata: HashMap<String, String>,
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
        let error_id = format!("DXSEM{:?}L{}C{}", error_type, line, column);
        let quick_fixes = Self::generate_quick_fixes(error_type);

        Self {
            error_id,
            error_type,
            message,
            line,
            column,
            section_name,
            suggestion,
            severity,
            quick_fixes,
            metadata: HashMap::new(),
        }
    }

    #[inline]
    fn generate_quick_fixes(error_type: SemanticErrorType) -> List<String> {
        let mut fixes = List::New();

        match error_type {
            SemanticErrorType::UndefinedReference => {
                fixes.Add("Define the variable before using it".to_string());
                fixes.Add("Check for typos in the identifier name".to_string());
            }
            SemanticErrorType::DuplicateDefinition => {
                fixes.Add("Rename one of the duplicate identifiers".to_string());
                fixes.Add("Remove the duplicate definition".to_string());
            }
            SemanticErrorType::TypeMismatch => {
                fixes.Add("Cast the value to the expected type".to_string());
                fixes.Add("Change the variable's type annotation".to_string());
            }
            SemanticErrorType::CircularDependency => {
                fixes.Add("Refactor to break the circular dependency".to_string());
            }
            SemanticErrorType::MissingRequiredSection => {
                fixes.Add("Add the required section to your script".to_string());
            }
            _ => {}
        }

        fixes
    }
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} at Line {}, Column {}: {}",
            self.severity, self.error_id, self.line, self.column, self.message
        )?;

        if let Some(ref section) = self.section_name {
            write!(f, "\n📍 Section: {}", section)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n💡 Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.IsEmpty() {
            write!(f, "\n🔧 Quick Fixes:")?;
            for fix in self.quick_fixes.Iter() {
                write!(f, "\n  - {}", fix)?;
            }
        }

        Ok(())
    }
}