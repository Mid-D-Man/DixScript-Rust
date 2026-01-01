//! AST Enhancement error types and handling

use super::error_enums::ErrorSeverity;
use crate::DixCore::List;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstEnhancementErrorType {
    InvalidParameterDefault,
    TypeInferenceFailed,
    CircularDependency,
    UnsupportedFeature,
    EnhancementFailed,
    InvalidTypeAnnotation,
    ConflictingDefaults,
}

#[derive(Debug, Clone)]
pub struct AstEnhancementError {
    pub error_id: String,
    pub error_type: AstEnhancementErrorType,
    pub message: String,
    pub line: i32,
    pub column: i32,
    pub section_name: Option<String>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
    pub quick_fixes: List<String>,
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
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = if line > 0 || column > 0 {
            format!("DXENH{:03}L{}C{}", error_type as u32, line, column)
        } else {
            format!("DXENH{:03}", error_type as u32)
        };

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

    pub fn generate_suggestion(error_type: &AstEnhancementErrorType, context: &str) -> String {
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

    fn generate_quick_fixes(&mut self, error_type: &AstEnhancementErrorType) {
        match error_type {
            AstEnhancementErrorType::InvalidParameterDefault => {
                self.quick_fixes.Add("Check type annotation matches default value".to_string());
                self.quick_fixes.Add("Verify default value is valid for type".to_string());
            }
            AstEnhancementErrorType::TypeInferenceFailed => {
                self.quick_fixes.Add("Add explicit type annotation".to_string());
                self.quick_fixes.Add("Provide default value for inference".to_string());
            }
            AstEnhancementErrorType::CircularDependency => {
                self.quick_fixes.Add("Break circular reference".to_string());
                self.quick_fixes.Add("Restructure parameter defaults".to_string());
            }
            AstEnhancementErrorType::UnsupportedFeature => {
                self.quick_fixes.Add("Check version compatibility".to_string());
                self.quick_fixes.Add("Update to newer version".to_string());
            }
            AstEnhancementErrorType::EnhancementFailed => {
                self.quick_fixes.Add("Check AST structure is valid".to_string());
                self.quick_fixes.Add("Report issue if problem persists".to_string());
            }
            AstEnhancementErrorType::InvalidTypeAnnotation => {
                self.quick_fixes.Add("Use valid DixScript type".to_string());
                self.quick_fixes.Add("Check type name spelling".to_string());
            }
            AstEnhancementErrorType::ConflictingDefaults => {
                self.quick_fixes.Add("Remove one default value".to_string());
                self.quick_fixes.Add("Ensure consistent default".to_string());
            }
        }
    }
}

impl fmt::Display for AstEnhancementError {
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