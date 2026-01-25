//! Core - Lexer, Parser, Semantic Analyzer

pub mod Tokenizer;
pub mod SectionParsers;
pub mod SectionAnalyzers;
pub mod SectionEnhancers;
pub mod ValueResolution;
pub mod BinarySerialization;
pub mod Config;

// Re-export Config types for easier access
pub use Config::{
    ConfigSectionHandler,
    ProcessConfigResult,
    OperationalSettings,
    ErrorHandlingStrategy,
    CompatibilityMode,
    DebugMode,
};

// TODO: Implement GeneralParser, GeneralSemanticsAnalyzer