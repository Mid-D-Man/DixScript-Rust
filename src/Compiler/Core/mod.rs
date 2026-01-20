//! Core - Lexer, Parser, Semantic Analyzer

pub mod Tokenizer;
pub mod SectionParsers;
pub mod SectionAnalyzers;
pub mod SectionEnhancers;
pub mod ValueResolution;
pub mod BinarySerialization;

// TODO: Implement GeneralParser, GeneralSemanticsAnalyzer