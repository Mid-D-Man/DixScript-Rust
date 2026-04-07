
//! Core - Lexer, Parser, Semantic Analyzer, AST Enhancer

use std::collections::HashMap;
use std::time::Duration;
use crate::Compiler::AST::{Position, DataType, DixScript};
use crate::Compiler::Utilities::SymbolTable;

pub mod Tokenizer;
pub mod SectionParsers;
pub mod SectionAnalyzers;
pub mod SectionEnhancers;
pub mod ValueResolution;
pub mod BinarySerialization;
pub mod Config;
pub mod Functions;
mod general_parser;
mod general_ast_enhancer;
mod general_semantics_analyzer;

// Re-export Config types for easier access
pub use Config::{
    ConfigSectionHandler,
    ProcessConfigResult,
    OperationalSettings,
    ErrorHandlingStrategy,
    CompatibilityMode,
    DebugMode,
};

// Re-export parser
pub use general_parser::GeneralParser;

// Re-export AST enhancer and its result types
pub use general_ast_enhancer::GeneralAstEnhancer;

// Re-export semantic analyzer
pub use general_semantics_analyzer::GeneralSemanticAnalyzer;

// Centralized result types for semantic analysis
pub use SectionAnalyzers::{SectionAnalysisResult, SemanticErrorInfo, SemanticWarningInfo};

// ==================== SEMANTIC ANALYSIS RESULT ====================

/// Result of semantic analysis
/// Contains validation results, symbol table, and analysis metadata
#[derive(Debug, Clone)]
pub struct SemanticAnalysisResult {
    /// Whether semantic analysis succeeded (no errors)
    pub is_success: bool,

    /// Populated symbol table after analysis
    pub symbol_table: Option<SymbolTable>,

    /// Errors encountered during analysis
    pub errors: Vec<SemanticErrorInfo>,

    /// Warnings encountered during analysis
    pub warnings: Vec<SemanticWarningInfo>,

    /// Per-section analysis results
    pub section_results: HashMap<String, SectionAnalysisResult>,

    /// Total time spent in semantic analysis
    pub analysis_duration: Duration,

    /// Short name index from DATA section analysis
    pub short_name_index: Option<HashMap<String, Vec<String>>>,

    /// Type index from DATA section analysis
    pub type_index: Option<HashMap<String, DataType>>,
}

impl SemanticAnalysisResult {
    pub fn new() -> Self {
        SemanticAnalysisResult {
            is_success: false,
            symbol_table: None,
            errors: Vec::new(),
            warnings: Vec::new(),
            section_results: HashMap::new(),
            analysis_duration: Duration::default(),
            short_name_index: None,
            type_index: None,
        }
    }
}

impl Default for SemanticAnalysisResult {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== AST ENHANCEMENT RESULT ====================

/// Result of AST enhancement
/// Contains enhanced AST and enhancement metadata
#[derive(Debug, Clone)]
pub struct EnhancementResult {
    /// Whether enhancement succeeded
    pub is_success: bool,

    /// Enhanced AST with resolved identifiers and inferred types
    pub enhanced_ast: DixScript,

    /// Total number of enhancements applied
    pub total_enhancements: usize,

    /// Errors encountered during enhancement
    pub errors: Vec<String>,

    /// Warnings encountered during enhancement
    pub warnings: Vec<String>,

    /// Per-section enhancement information
    pub section_enhancements: HashMap<String, SectionEnhancementInfo>,

    /// Total time spent in enhancement
    pub enhancement_duration: Duration,
}

impl EnhancementResult {
    pub fn new() -> Self {
        EnhancementResult {
            is_success: false,
            enhanced_ast: DixScript::new(),
            total_enhancements: 0,
            errors: Vec::new(),
            warnings: Vec::new(),
            section_enhancements: HashMap::new(),
            enhancement_duration: Duration::default(),
        }
    }
}

impl Default for EnhancementResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Section-specific enhancement information
#[derive(Debug, Clone)]
pub struct SectionEnhancementInfo {
    pub section_name: String,
    pub enhancements_applied: usize,
    pub enhancement_types: Vec<String>,
}

impl SectionEnhancementInfo {
    pub fn new(section_name: impl Into<String>) -> Self {
        SectionEnhancementInfo {
            section_name: section_name.into(),
            enhancements_applied: 0,
            enhancement_types: Vec::new(),
        }
    }
}