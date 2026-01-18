//! AST enhancement phase errors

use super::ErrorSeverity;
use crate::DixCore::List;
use std::collections::HashMap;
use std::fmt;

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

#[derive(Debug, Clone, PartialEq)]
pub struct AstEnhancementError {
    pub error_id: String,
    pub error_type: AstEnhancementErrorType,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
    pub quick_fixes: List<String>,
    pub metadata: HashMap<String, String>,
}

impl AstEnhancementError {
    pub fn new(
        error_type: AstEnhancementErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXENH{:?}L{}C{}", error_type, line, column);
        let quick_fixes = Self::generate_quick_fixes(error_type);

        Self {
            error_id,
            error_type,
            message,
            line,
            column,
            suggestion,
            severity,
            quick_fixes,
            metadata: HashMap::new(),
        }
    }

    #[inline]
    fn generate_quick_fixes(error_type: AstEnhancementErrorType) -> List<String> {
        let mut fixes = List::New();

        match error_type {
            AstEnhancementErrorType::InvalidParameterDefault => {
                fixes.Add("Provide a valid default value".to_string());
            }
            AstEnhancementErrorType::TypeInferenceFailed => {
                fixes.Add("Add explicit type annotations".to_string());
            }
            AstEnhancementErrorType::CircularDependency => {
                fixes.Add("Break circular dependencies by refactoring".to_string());
            }
            _ => {}
        }

        fixes
    }
}

impl fmt::Display for AstEnhancementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} at Line {}, Column {}: {}",
            self.severity, self.error_id, self.line, self.column, self.message
        )?;

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