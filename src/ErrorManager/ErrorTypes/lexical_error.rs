//! Lexical analysis errors (tokenization phase)

use super::ErrorSeverity;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexicalErrorType {
    InvalidCharacter,
    UnterminatedString,
    InvalidNumericFormat,
    InvalidHexColor,
    InvalidRegexLiteral,
    AmbiguousPrefixedConstructor,
    InvalidStaticCallPattern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
        let error_id = format!("DXL{:?}L{}C{}", error_type, line, column);
        let error_indicator = source_line.as_ref().map(|sl| {
            let spaces = " ".repeat(column.saturating_sub(1));
            format!("{}\n{}^-- Here", sl, spaces)
        });

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

        Ok(())
    }
}