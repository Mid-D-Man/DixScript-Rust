/// Error severity levels for DixScript compilation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Fatal,
}

/// Error source categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSource {
    Configuration,
    Lexer,
    Parser,
    ImportsResolution,
    SemanticAnalyzer,
    AstEnhancement,
    ValueResolution,
    BinarySerialization,
    DLM,
    Runtime,
    General,
}