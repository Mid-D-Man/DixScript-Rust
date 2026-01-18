//! Parser errors (syntax analysis phase)

use super::ErrorSeverity;
use crate::DixCore::List;
use crate::Utilities::Token;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorType {
    UnexpectedToken,
    MissingToken,
    InvalidType,
    DuplicateDefinition,
    UndefinedReference,
    TypeMismatch,
    InvalidOperation,
    UnsupportedFeature,
    UnknownStaticObject,
    UnknownStaticMethod,
    UnknownInstanceMethod,
    InvalidMethodSignature,
    InvalidBuiltinCall,
    SectionSyntaxError,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub error_id: String,
    pub error_type: ParseErrorType,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub suggestion: Option<String>,
    pub source_line: Option<String>,
    pub error_indicator: Option<String>,
    pub severity: ErrorSeverity,
    pub quick_fixes: List<String>,
    pub metadata: HashMap<String, String>,
}

impl ParseError {
    pub fn new(
        error_type: ParseErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        source_line: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DX{:?}L{}C{}", error_type, line, column);
        let error_indicator = source_line.as_ref().map(|sl| {
            let spaces = " ".repeat(column.saturating_sub(1));
            format!("{}\n{}^-- Here", sl, spaces)
        });

        let quick_fixes = Self::generate_quick_fixes(error_type);

        Self {
            error_id,
            error_type,
            message,
            line,
            column,
            suggestion,
            source_line,
            error_indicator,
            severity,
            quick_fixes,
            metadata: HashMap::new(),
        }
    }

    pub fn create_registry_error(
        error_type: ParseErrorType,
        object_name: &str,
        method_name: &str,
        line: usize,
        column: usize,
        source_line: Option<String>,
    ) -> Self {
        let message = match error_type {
            ParseErrorType::UnknownStaticObject => {
                format!("Unknown static object: '{}'", object_name)
            }
            ParseErrorType::UnknownStaticMethod => {
                format!("Unknown static method: '{}.{}'", object_name, method_name)
            }
            ParseErrorType::UnknownInstanceMethod => {
                format!("Unknown instance method: '{}.{}'", object_name, method_name)
            }
            _ => format!("Registry error for '{}.{}'", object_name, method_name),
        };

        let suggestion = format!(
            "Check if '{}' is registered in the built-in registry",
            object_name
        );

        Self::new(
            error_type,
            message,
            line,
            column,
            Some(suggestion),
            source_line,
            ErrorSeverity::Error,
        )
    }

    #[inline]
    fn generate_quick_fixes(error_type: ParseErrorType) -> List<String> {
        let mut fixes = List::New();

        match error_type {
            ParseErrorType::MissingToken => {
                fixes.Add("Add the missing token".to_string());
            }
            ParseErrorType::UnexpectedToken => {
                fixes.Add("Remove or replace the unexpected token".to_string());
            }
            ParseErrorType::UndefinedReference => {
                fixes.Add("Define the referenced variable or function".to_string());
                fixes.Add("Check spelling of the identifier".to_string());
            }
            ParseErrorType::TypeMismatch => {
                fixes.Add("Cast the value to the expected type".to_string());
            }
            ParseErrorType::UnknownStaticObject | ParseErrorType::UnknownStaticMethod => {
                fixes.Add("Check the built-in function registry".to_string());
                fixes.Add("Ensure the static object is properly imported".to_string());
            }
            _ => {}
        }

        fixes
    }

    #[inline]
    pub fn generate_suggestion(
        error_type: &ParseErrorType,
        token: &Token,
        _context_tokens: Option<&List<Token>>,
    ) -> String {
        match error_type {
            ParseErrorType::UnexpectedToken => {
                format!("Unexpected token '{}' at this position", token.Lexeme)
            }
            ParseErrorType::MissingToken => "Expected a token here".to_string(),
            ParseErrorType::UndefinedReference => {
                format!("'{}' is not defined in this scope", token.Lexeme)
            }
            _ => "Check syntax and try again".to_string(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} at Line {}, Column {}: {}",
            self.severity, self.error_id, self.line, self.column, self.message
        )?;

        if let Some(ref indicator) = self.error_indicator {
            write!(f, "\n{}", indicator)?;
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