// src/Compiler/Core/SectionAnalyzers/mod.rs

//! # Section Analyzers - Semantic Analysis for DixScript
//!
//! This module contains semantic analyzers for each section type.
//! Each analyzer validates AST nodes and populates the symbol table.
//!
//! ## Shared Result Types
//! All analyzers return `SectionAnalysisResult` with standardized error/warning types.
//!
//! ## Ported Analyzers
//! - EnumsSectionAnalyzer - COMPLETE (v1.0.0)
//! - DlmSectionAnalyzer - COMPLETE (v1.0.0)
//! - SecuritySectionAnalyzer - COMPLETE (v1.0.0)
//! - DataSectionAnalyzer - COMPLETE (v1.0.0)
//! - QuickFuncsSectionAnalyzer - COMPLETE (v1.0.0)
//! - ImportsSectionAnalyzer - COMPLETE (v1.0.0)

use crate::Compiler::AST::Position;
use crate::Compiler::Core::SectionEnhancers::{
    QualifiedIdentifierKey, QualifiedIdentifierResolution,
};
use std::collections::HashMap;

pub mod enums_section_analyzer;
pub mod dlm_section_analyzer;
pub mod security_section_analyzer;
pub mod data_section_analyzer;
pub mod quickfuncs_section_analyzer;
pub mod imports_section_analyzer;

// Re-exports for convenience
pub use enums_section_analyzer::EnumsSectionAnalyzer;
pub use dlm_section_analyzer::DlmSectionAnalyzer;
pub use security_section_analyzer::SecuritySectionAnalyzer;
pub use data_section_analyzer::DataSectionAnalyzer;
pub use quickfuncs_section_analyzer::QuickFuncsSectionAnalyzer;
pub use imports_section_analyzer::ImportsSectionAnalyzer;

// ==================== SHARED RESULT TYPES ====================

/// Result of analyzing a section
/// 
/// Used by all section analyzers to return validation results.
/// Contains errors, warnings, and optional qualified identifier resolutions.
#[derive(Debug, Clone)]
pub struct SectionAnalysisResult {
    /// Name of the section analyzed (e.g., "DATA", "QUICKFUNCS")
    pub section_name: String,
    
    /// Whether analysis succeeded (no errors)
    pub is_success: bool,
    
    /// Semantic errors encountered
    pub errors: Vec<SemanticErrorInfo>,
    
    /// Semantic warnings encountered
    pub warnings: Vec<SemanticWarningInfo>,
    
    /// Qualified identifier resolutions (QUICKFUNCS only)
    /// Maps QualifiedIdentifier nodes to their resolved types
    pub qualified_id_resolutions: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution>,
}

impl SectionAnalysisResult {
    /// Create new analysis result for a section
    pub fn new(section_name: impl Into<String>) -> Self {
        SectionAnalysisResult {
            section_name: section_name.into(),
            is_success: false,
            errors: Vec::new(),
            warnings: Vec::new(),
            qualified_id_resolutions: HashMap::new(),
        }
    }
    
    /// Check if analysis has any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    
    /// Check if analysis has any warnings
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
    
    /// Get total count of issues (errors + warnings)
    pub fn total_issues(&self) -> usize {
        self.errors.len() + self.warnings.len()
    }
}

/// Semantic error information
/// 
/// Represents a semantic error encountered during analysis.
/// Contains error ID, type, message, suggestion, and source position.
#[derive(Debug, Clone)]
pub struct SemanticErrorInfo {
    /// Unique error identifier (e.g., "ENUM001", "QFUNC042")
    pub error_id: String,
    
    /// Error type/category (e.g., "DUPLICATE_ENUM_NAME")
    pub error_type: String,
    
    /// Human-readable error message
    pub message: String,
    
    /// Section where error occurred
    pub section_name: String,
    
    /// Suggested fix or guidance
    pub suggestion: String,
    
    /// Source position where error occurred
    pub position: Option<Position>,
}

impl SemanticErrorInfo {
    /// Create new semantic error
    pub fn new(
        error_id: impl Into<String>,
        error_type: impl Into<String>,
        message: impl Into<String>,
        section_name: impl Into<String>,
        suggestion: impl Into<String>,
        position: Option<Position>,
    ) -> Self {
        SemanticErrorInfo {
            error_id: error_id.into(),
            error_type: error_type.into(),
            message: message.into(),
            section_name: section_name.into(),
            suggestion: suggestion.into(),
            position,
        }
    }
}

impl std::fmt::Display for SemanticErrorInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} in @{}: {}",
            self.error_id, self.error_type, self.section_name, self.message
        )?;
        
        if !self.suggestion.is_empty() {
            write!(f, "\n  Suggestion: {}", self.suggestion)?;
        }
        
        if let Some(pos) = self.position {
            write!(f, "\n  at line {}, column {}", pos.line, pos.column)?;
        }
        
        Ok(())
    }
}

/// Semantic warning information
/// 
/// Represents a semantic warning encountered during analysis.
/// Warnings don't prevent compilation but indicate potential issues.
#[derive(Debug, Clone)]
pub struct SemanticWarningInfo {
    /// Unique warning identifier (e.g., "ENUM_WARN001")
    pub warning_id: String,
    
    /// Human-readable warning message
    pub message: String,
    
    /// Section where warning occurred
    pub section_name: String,
    
    /// Source position where warning occurred
    pub position: Option<Position>,
}

impl SemanticWarningInfo {
    /// Create new semantic warning
    pub fn new(
        warning_id: impl Into<String>,
        message: impl Into<String>,
        section_name: impl Into<String>,
        position: Option<Position>,
    ) -> Self {
        SemanticWarningInfo {
            warning_id: warning_id.into(),
            message: message.into(),
            section_name: section_name.into(),
            position,
        }
    }
}

impl std::fmt::Display for SemanticWarningInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} in @{}",
            self.warning_id, self.message, self.section_name
        )?;
        
        if let Some(pos) = self.position {
            write!(f, " at line {}, column {}", pos.line, pos.column)?;
        }
        
        Ok(())
    }
}

// ==================== HELPER FUNCTIONS ====================

/// Create a successful analysis result with no errors
pub fn success_result(section_name: impl Into<String>) -> SectionAnalysisResult {
    let mut result = SectionAnalysisResult::new(section_name);
    result.is_success = true;
    result
}

/// Create a failed analysis result with errors
pub fn failure_result(
    section_name: impl Into<String>,
    errors: Vec<SemanticErrorInfo>,
) -> SectionAnalysisResult {
    let mut result = SectionAnalysisResult::new(section_name);
    result.is_success = false;
    result.errors = errors;
    result
}

/// Create a result with warnings only (success = true)
pub fn warning_result(
    section_name: impl Into<String>,
    warnings: Vec<SemanticWarningInfo>,
) -> SectionAnalysisResult {
    let mut result = SectionAnalysisResult::new(section_name);
    result.is_success = true;
    result.warnings = warnings;
    result
}

// ==================== ANALYZER TRAITS ====================

/// Base trait for all section analyzers
/// 
/// Defines common interface for semantic analysis.
/// All section analyzers should implement this trait.
pub trait SectionAnalyzer {
    /// The AST section type this analyzer processes
    type Section;

    /// Analyze the section and populate symbol table
    /// 
    /// Returns analysis result with errors/warnings and optional
    /// qualified identifier resolutions (for QUICKFUNCS).
    fn analyze(
        &mut self,
        section: &Self::Section,
        symbol_table: &mut crate::Compiler::Utilities::SymbolTable,
    ) -> SectionAnalysisResult;

    /// Get analyzer name for logging/debugging
    fn analyzer_name(&self) -> &'static str;
}

// ==================== COMMON RESULT TYPE ====================

/// Common result type for all section analyzers
/// All analyzers should return this type for consistency
pub type AnalyzerResult = SectionAnalysisResult;

// ==================== COMPLEXITY NOTES ====================

/*
COMPLEXITY RANKING (simplest to hardest):

1. ConfigSectionAnalyzer (SIMPLEST)
   - Just key-value validation
   - No dependencies on other sections
   - No symbol table population
   - No type inference

2. DlmSectionAnalyzer (SIMPLE) - COMPLETE ✅
   - Module type validation
   - Subtype validation
   - Ordering checks
   - Security warnings

3. SecuritySectionAnalyzer (SIMPLE-MEDIUM) - COMPLETE ✅
   - Block key validation
   - Field validation per block type
   - Works with SecurityUtilities for defaults
   - No dependencies

4. EnumsSectionAnalyzer (MEDIUM) - COMPLETE ✅
   - Name/field validation
   - Duplicate detection
   - Value computation
   - Symbol table population

5. ImportsSectionAnalyzer (MEDIUM-COMPLEX) - COMPLETE ✅
   - File path resolution
   - Hash verification
   - Cloud import validation (HTTP/HTTPS)
   - Circular import detection
   - Namespace population

6. QuickFuncsSectionAnalyzer (COMPLEX) - COMPLETE ✅
   - Full type system
   - Expression type inference
   - Statement validation
   - Control flow analysis
   - Local variable tracking
   - Builtin resolution
   - Recursive calls
   - Qualified identifier resolution
   - Depends on: ENUMS, IMPORTS

7. DataSectionAnalyzer (MOST COMPLEX) - COMPLETE ✅
   - Type inference for values
   - Expression evaluation
   - Variable scope resolution
   - Function call validation
   - Enum value validation
   - Index building (short names, types)
   - Depends on: ENUMS, QUICKFUNCS, IMPORTS

ANALYSIS ORDER:
1. CONFIG (first - establishes settings)
2. IMPORTS (early - provides namespaces)
3. ENUMS (early - needed by QuickFuncs and Data)
4. DLM (independent)
5. SECURITY (independent, but checks DLM)
6. QUICKFUNCS (needs Enums and Imports)
7. DATA (last - needs everything)
*/

// ==================== CROSS-ANALYZER DEPENDENCIES ====================

/*
DEPENDENCY GRAPH:

CONFIG
  └─> (no dependencies)

IMPORTS - COMPLETE ✅
  └─> (recursive, may trigger full analysis of imported files)

ENUMS - COMPLETE ✅
  └─> (no dependencies)

DLM - COMPLETE ✅
  └─> (no dependencies)

SECURITY - COMPLETE ✅
  └─> DLM (checks for encryption modules)

QUICKFUNCS - COMPLETE ✅
  ├─> ENUMS (for enum access validation)
  └─> IMPORTS (for namespaced function calls)

DATA - COMPLETE ✅
  ├─> ENUMS (for enum value validation)
  ├─> QUICKFUNCS (for function call validation)
  └─> IMPORTS (for namespaced enum/function access)

ALL ANALYZERS COMPLETE! 🎉
*/
