//! Core error enums and severity levels

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Info = 0,
    Warning = 1,
    Error = 2,
    Fatal = 3,
}

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ErrorSeverity::Info => write!(f, "INFO"),
            ErrorSeverity::Warning => write!(f, "WARNING"),
            ErrorSeverity::Error => write!(f, "ERROR"),
            ErrorSeverity::Fatal => write!(f, "FATAL"),
        }
    }
}

/// Error source modules
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

impl std::fmt::Display for ErrorSource {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ErrorSource::Configuration => write!(f, "Configuration"),
            ErrorSource::Lexer => write!(f, "Lexer"),
            ErrorSource::Parser => write!(f, "Parser"),
            ErrorSource::SemanticAnalyzer => write!(f, "SemanticAnalyzer"),
            ErrorSource::AstEnhancement => write!(f, "AstEnhancement"),
            ErrorSource::ValueResolution => write!(f, "ValueResolution"),
            ErrorSource::BinarySerialization => write!(f, "BinarySerialization"),
            ErrorSource::DLM => write!(f, "DLM"),
            ErrorSource::Runtime => write!(f, "Runtime"),
            ErrorSource::General => write!(f, "General"),
        }
    }
}