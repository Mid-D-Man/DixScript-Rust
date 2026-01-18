//! Core error enumerations shared across all error types

/// Severity level for errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Fatal,
}

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorSeverity::Info => write!(f, "INFO"),
            ErrorSeverity::Warning => write!(f, "WARNING"),
            ErrorSeverity::Error => write!(f, "ERROR"),
            ErrorSeverity::Fatal => write!(f, "FATAL"),
        }
    }
}

/// Source of the error in the compilation pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSource {
    Configuration,
    Lexer,
    Parser,
    SemanticAnalyzer,
    AstEnhancement,
    ValueResolution,
    BinarySerialization,
    DLM,
    Runtime,
    General,
}