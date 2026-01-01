//! Lexical error types and handling

use super::error_enums::ErrorSeverity;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexicalErrorType {
    InvalidCharacter,
    UnterminatedString,
    InvalidNumericFormat,
    InvalidHexColor,
    InvalidRegexLiteral,
    AmbiguousPrefixedConstructor,
    InvalidStaticCallPattern,
}

#[derive(Debug, Clone)]
pub struct LexicalError {
    pub error_id: String,
    pub error_type: LexicalErrorType,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub suggestion: Option<String>,
    pub source_line: Option<String>,
    pub error_indicator: Option<String>,
    pub severity: ErrorSeverity,
}

impl LexicalError {
    pub fn new(
        error_type: LexicalErrorType,
        message: String,
        line: usize,
        column: usize,
        suggestion: Option<String>,
        source_line: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXL{:03}L{}C{}", error_type as u32, line, column);

        let error_indicator = if source_line.is_some() && column > 0 {
            let spaces = " ".repeat(column);
            Some(format!("{}^--", spaces))
        } else {
            None
        };

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
        }
    }
}

impl fmt::Display for LexicalError {
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

        Ok(())
    }
}