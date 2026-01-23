//! Core - Lexer, Parser, Semantic Analyzer

pub mod Tokenizer;
pub mod SectionParsers;
pub mod SectionAnalyzers;
pub mod SectionEnhancers;
pub mod ValueResolution;
pub mod BinarySerialization;
mod Config;
// TODO: Implement GeneralParser, GeneralSemanticsAnalyzer