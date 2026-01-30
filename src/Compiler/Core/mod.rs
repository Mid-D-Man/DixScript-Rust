// src/Compiler/Core/mod.rs
//! Core - Lexer, Parser, Semantic Analyzer, AST Enhancer

pub mod Tokenizer;
pub mod SectionParsers;
pub mod SectionAnalyzers;
pub mod SectionEnhancers;
pub mod ValueResolution;
pub mod BinarySerialization;
pub mod Config;
pub mod Functions;
mod general_parser;
mod general_ast_enhancer;  // ADD THIS

// Re-export Config types for easier access
pub use Config::{
    ConfigSectionHandler,
    ProcessConfigResult,
    OperationalSettings,
    ErrorHandlingStrategy,
    CompatibilityMode,
    DebugMode,
};
pub use general_parser::GeneralParser;
pub use general_ast_enhancer::{GeneralAstEnhancer, EnhancementResult, SectionEnhancementInfo};  // ADD THIS
