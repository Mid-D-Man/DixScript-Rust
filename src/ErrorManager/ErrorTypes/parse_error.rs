use crate::Compiler::Core::Tokenizer::Token;
use std::fmt;

/// Parse error types
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
    InvalidIdentifier,
    InvalidLiteral,
}

/// Parse error with location and quick fixes
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
    pub severity: super::ErrorSeverity,
    pub quick_fixes: Vec<String>,
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
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = format!("DX{:03}L{}C{}", error_type as u32, line, column);

        let error_indicator = if let Some(ref src) = source_line {
            if column > 0 {
                let mut indicator = String::new();
                for _ in 0..column {
                    indicator.push(' ');
                }
                indicator.push_str("^--");
                Some(indicator)
            } else {
                None
            }
        } else {
            None
        };

        let quick_fixes = Self::generate_quick_fixes(error_type);

        ParseError {
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
            metadata: std::collections::HashMap::new(),
        }
    }

    fn generate_quick_fixes(error_type: ParseErrorType) -> Vec<String> {
        match error_type {
            ParseErrorType::MissingToken => vec![
                "Insert missing token".to_string(),
                "Check surrounding syntax".to_string(),
            ],
            ParseErrorType::UnexpectedToken => vec![
                "Remove unexpected token".to_string(),
                "Replace with expected token".to_string(),
            ],
            ParseErrorType::UnknownStaticObject => vec![
                "Use Math, DateTime, Array, Random, or Enum".to_string(),
                "Check object name spelling".to_string(),
            ],
            ParseErrorType::UnknownStaticMethod => vec![
                "Check available methods for this object".to_string(),
                "Verify method name spelling".to_string(),
            ],
            ParseErrorType::SectionSyntaxError => vec![
                "Check section syntax in documentation".to_string(),
                "Verify parentheses and commas".to_string(),
            ],
            _ => Vec::new(),
        }
    }

    /// Generate suggestion for parse error
    pub fn generate_suggestion(
        error_type: ParseErrorType,
        token: &Token,
        _context_tokens: Option<&[Token]>,
    ) -> String {
        match error_type {
            ParseErrorType::UnexpectedToken => {
                format!("Unexpected token '{}'. Consider checking the syntax here.", token.get_token_value())
            }
            ParseErrorType::MissingToken => {
                format!("Missing expected token before '{}'. Check if required delimiters or keywords are present.",
                        token.get_token_value())
            }
            ParseErrorType::InvalidType => {
                format!("Invalid type '{}'. Use a valid DixScript type.", token.get_token_value())
            }
            ParseErrorType::DuplicateDefinition => {
                format!("'{}' is already defined. Use a different name.", token.get_token_value())
            }
            ParseErrorType::UndefinedReference => {
                format!("Reference to undefined identifier '{}'.", token.get_token_value())
            }
            ParseErrorType::TypeMismatch => {
                format!("Type mismatch with '{}'. Check the expected type.", token.get_token_value())
            }
            ParseErrorType::InvalidOperation => {
                format!("Invalid operation with '{}'. Check if the operation is allowed for this type.",
                        token.get_token_value())
            }
            ParseErrorType::UnsupportedFeature => {
                format!("Feature '{}' is not supported in the current version.", token.get_token_value())
            }
            ParseErrorType::UnknownStaticObject => {
                format!("Static object '{}' not found. Available objects: Math, DateTime, Array, Random, Enum.",
                        token.get_token_value())
            }
            ParseErrorType::UnknownStaticMethod => {
                format!("Method '{}' not available on this static object. Check the object's available methods.",
                        token.get_token_value())
            }
            ParseErrorType::UnknownInstanceMethod => {
                format!("Method '{}' not available for this expression type. Check type-specific methods.",
                        token.get_token_value())
            }
            ParseErrorType::InvalidMethodSignature => {
                format!("Method call '{}' has incorrect parameters. Check parameter count and types.",
                        token.get_token_value())
            }
            ParseErrorType::InvalidBuiltinCall => {
                format!("Invalid built-in call pattern near '{}'. Check syntax: Object.method() or expression.method().",
                        token.get_token_value())
            }
            ParseErrorType::SectionSyntaxError => {
                format!("Section syntax error near '{}'. Check section formatting and delimiters.",
                        token.get_token_value())
            }
            _ => format!("Check syntax near line {}, column {}.", token.line, token.column),
        }
    }

    /// Create registry error (for built-in validation failures)
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
                "Available static objects: Math, DateTime, Array, Random, Enum".to_string()
            }
            ParseErrorType::UnknownStaticMethod => {
                format!("Check available methods for {} object", object_name)
            }
            ParseErrorType::UnknownInstanceMethod => {
                "Check methods available for this expression type".to_string()
            }
            ParseErrorType::InvalidMethodSignature => {
                format!("Check parameter count and types for {}.{}()", object_name, method_name)
            }
            _ => "Check built-in function documentation".to_string(),
        };

        ParseError::new(
            error_type,
            message,
            line,
            column,
            Some(suggestion),
            source_line,
            super::ErrorSeverity::Error,
        )
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[{}] {:?}: {:?} at line {}, column {}",
                 self.error_id, self.severity, self.error_type, self.line, self.column)?;
        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref src) = self.source_line {
            writeln!(f, "Source:")?;
            writeln!(f, "{}", src)?;
            if let Some(ref indicator) = self.error_indicator {
                writeln!(f, "{}", indicator)?;
            }
        }

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