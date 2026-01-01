//! Parse error types and handling

use super::error_enums::ErrorSeverity;
use crate::Utilities::Token;
use crate::DixCore::List;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
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
    pub metadata: std::collections::HashMap<String, String>,
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
        let error_id = format!("DX{:03}L{}C{}", error_type as u32, line, column);

        let error_indicator = if source_line.is_some() && column > 0 {
            let spaces = " ".repeat(column);
            Some(format!("{}^--", spaces))
        } else {
            None
        };

        let mut error = Self {
            error_id,
            error_type: error_type.clone(),
            message,
            line,
            column,
            suggestion,
            source_line,
            error_indicator,
            severity,
            quick_fixes: List::New(),
            metadata: std::collections::HashMap::new(),
        };

        error.generate_quick_fixes(&error_type);
        error
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
                format!("Unknown static object '{}'", object_name)
            }
            ParseErrorType::UnknownStaticMethod => {
                format!("Unknown method '{}' on object '{}'", method_name, object_name)
            }
            ParseErrorType::UnknownInstanceMethod => {
                format!("Unknown instance method '{}' for expression type", method_name)
            }
            ParseErrorType::InvalidMethodSignature => {
                format!("Invalid signature for {}.{}()", object_name, method_name)
            }
            _ => format!("Registry error with {}.{}", object_name, method_name),
        };

        let suggestion = match error_type {
            ParseErrorType::UnknownStaticObject => {
                Some("Available static objects: Math, DateTime, Array, Random, Enum".to_string())
            }
            ParseErrorType::UnknownStaticMethod => {
                Some(format!("Check available methods for {} object", object_name))
            }
            ParseErrorType::UnknownInstanceMethod => {
                Some("Check methods available for this expression type".to_string())
            }
            ParseErrorType::InvalidMethodSignature => {
                Some(format!("Check parameter count and types for {}.{}()", object_name, method_name))
            }
            _ => Some("Check built-in function documentation".to_string()),
        };

        Self::new(error_type, message, line, column, suggestion, source_line, ErrorSeverity::Error)
    }

    pub fn generate_suggestion(error_type: &ParseErrorType, token: &Token, _context_tokens: Option<&List<Token>>) -> String {
        match error_type {
            ParseErrorType::UnexpectedToken => {
                format!("Unexpected token '{}'. Consider checking the syntax here.", token.GetTokenValue())
            }
            ParseErrorType::MissingToken => {
                format!("Missing expected token before '{}'. Check if required delimiters or keywords are present.", token.GetTokenValue())
            }
            ParseErrorType::InvalidType => {
                format!("Invalid type '{}'. Use a valid DixScript type.", token.GetTokenValue())
            }
            ParseErrorType::DuplicateDefinition => {
                format!("'{}' is already defined. Use a different name.", token.GetTokenValue())
            }
            ParseErrorType::UndefinedReference => {
                format!("Reference to undefined identifier '{}'.", token.GetTokenValue())
            }
            ParseErrorType::TypeMismatch => {
                format!("Type mismatch with '{}'. Check the expected type.", token.GetTokenValue())
            }
            ParseErrorType::InvalidOperation => {
                format!("Invalid operation with '{}'. Check if the operation is allowed for this type.", token.GetTokenValue())
            }
            ParseErrorType::UnsupportedFeature => {
                format!("Feature '{}' is not supported in the current version.", token.GetTokenValue())
            }
            ParseErrorType::UnknownStaticObject => {
                format!("Static object '{}' not found. Available objects: Math, DateTime, Array, Random, Enum.", token.GetTokenValue())
            }
            ParseErrorType::UnknownStaticMethod => {
                format!("Method '{}' not available on this static object. Check the object's available methods.", token.GetTokenValue())
            }
            ParseErrorType::UnknownInstanceMethod => {
                format!("Method '{}' not available for this expression type. Check type-specific methods.", token.GetTokenValue())
            }
            ParseErrorType::InvalidMethodSignature => {
                format!("Method call '{}' has incorrect parameters. Check parameter count and types.", token.GetTokenValue())
            }
            ParseErrorType::InvalidBuiltinCall => {
                format!("Invalid built-in call pattern near '{}'. Check syntax: Object.method() or expression.method().", token.GetTokenValue())
            }
            ParseErrorType::SectionSyntaxError => {
                format!("Section syntax error near '{}'. Check section formatting and delimiters.", token.GetTokenValue())
            }
            _ => format!("Check syntax near line {}, column {}.", token.Line, token.Column),
        }
    }

    fn generate_quick_fixes(&mut self, error_type: &ParseErrorType) {
        match error_type {
            ParseErrorType::MissingToken => {
                self.quick_fixes.Add("Insert missing token".to_string());
                self.quick_fixes.Add("Check surrounding syntax".to_string());
            }
            ParseErrorType::UnexpectedToken => {
                self.quick_fixes.Add("Remove unexpected token".to_string());
                self.quick_fixes.Add("Replace with expected token".to_string());
            }
            ParseErrorType::UnknownStaticObject => {
                self.quick_fixes.Add("Use Math, DateTime, Array, Random, or Enum".to_string());
                self.quick_fixes.Add("Check object name spelling".to_string());
            }
            ParseErrorType::UnknownStaticMethod => {
                self.quick_fixes.Add("Check available methods for this object".to_string());
                self.quick_fixes.Add("Verify method name spelling".to_string());
            }
            ParseErrorType::SectionSyntaxError => {
                self.quick_fixes.Add("Check section syntax in documentation".to_string());
                self.quick_fixes.Add("Verify parentheses and commas".to_string());
            }
            _ => {}
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "[{}] {}: {:?} at line {}, column {}",
            self.error_id, self.severity, self.error_type, self.line, self.column
        )?;
        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref source) = self.source_line {
            writeln!(f, "Source:")?;
            writeln!(f, "{}", source)?;
            if let Some(ref indicator) = self.error_indicator {
                writeln!(f, "{}", indicator)?;
            }
        }

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