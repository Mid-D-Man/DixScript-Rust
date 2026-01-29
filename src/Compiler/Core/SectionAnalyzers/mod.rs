// src/Compiler/Core/SectionAnalyzers/mod.rs

//! # Section Analyzers - Semantic Analysis for DixScript
//!
//! This module contains semantic analyzers for each section type.
//! Each analyzer validates AST nodes and populates the symbol table.
//!
//! ## Ported Analyzers
//! - ✅ EnumsSectionAnalyzer - COMPLETE (v1.0.0)
//! - ✅ DlmSectionAnalyzer - COMPLETE (v1.0.0)
//! - ✅ SecuritySectionAnalyzer - COMPLETE (v1.0.0)
//!
//! ## TODO: Analyzers to Port
//! - ⏳ ConfigSectionAnalyzer - Simple validation, no symbol table
//! - ⏳ ImportsSectionAnalyzer - Path resolution, hash verification
//! - ⏳ DataSectionAnalyzer - Complex: expressions, type inference, scoping
//! - ⏳ QuickFuncsSectionAnalyzer - Most complex: full type checking, control flow
//!
//! ## Porting Order (by complexity)
//! 1. ConfigSectionAnalyzer (simplest - just validation)
//! 2. ImportsSectionAnalyzer (medium - file I/O, hashing)
//! 3. DataSectionAnalyzer (complex - expressions, type inference)
//! 4. QuickFuncsSectionAnalyzer (most complex - full type system)

pub mod enums_section_analyzer;
pub mod dlm_section_analyzer;
pub mod security_section_analyzer;

// TODO: Port these analyzers
// pub mod config_section_analyzer;
// pub mod imports_section_analyzer;
// pub mod data_section_analyzer;
// pub mod quickfuncs_section_analyzer;

// Re-exports for convenience
pub use enums_section_analyzer::EnumsSectionAnalyzer;
pub use dlm_section_analyzer::DlmSectionAnalyzer;
pub use security_section_analyzer::SecuritySectionAnalyzer;

// Shared types used by all analyzers
use crate::Compiler::AST::Position;

/// Result of analyzing a section
#[derive(Debug, Clone)]
pub struct SectionAnalysisResult {
    pub section_name: String,
    pub is_success: bool,
    pub errors: Vec<SemanticErrorInfo>,
    pub warnings: Vec<SemanticWarningInfo>,
}

impl SectionAnalysisResult {
    pub fn new(section_name: impl Into<String>) -> Self {
        SectionAnalysisResult {
            section_name: section_name.into(),
            is_success: false,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// Semantic error information
#[derive(Debug, Clone)]
pub struct SemanticErrorInfo {
    pub error_id: String,
    pub error_type: String,
    pub message: String,
    pub section_name: String,
    pub suggestion: String,
    pub position: Option<Position>,
}

/// Semantic warning information
#[derive(Debug, Clone)]
pub struct SemanticWarningInfo {
    pub warning_id: String,
    pub message: String,
    pub section_name: String,
    pub position: Option<Position>,
}

// TODO: Re-export these when ported
// pub use config_section_analyzer::ConfigSectionAnalyzer;
// pub use imports_section_analyzer::ImportsSectionAnalyzer;
// pub use data_section_analyzer::DataSectionAnalyzer;
// pub use quickfuncs_section_analyzer::QuickFuncsSectionAnalyzer;

/// Common result type for all section analyzers
/// All analyzers should return this type for consistency
pub type AnalyzerResult = SectionAnalysisResult;

// ==================== ANALYZER TRAITS ====================

/// Base trait for all section analyzers
/// Defines common interface for semantic analysis
pub trait SectionAnalyzer {
    /// The AST section type this analyzer processes
    type Section;

    /// Analyze the section and populate symbol table
    /// Returns analysis result with errors/warnings
    fn analyze(
        &mut self,
        section: &Self::Section,
        symbol_table: &mut crate::Compiler::Utilities::SymbolTable,
    ) -> SectionAnalysisResult;

    /// Get analyzer name for logging/debugging
    fn analyzer_name(&self) -> &'static str;
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

// ==================== ANALYZER COMPLEXITY NOTES ====================

/*
COMPLEXITY RANKING (simplest → hardest):

1. ConfigSectionAnalyzer (SIMPLEST)
   - Just key-value validation
   - No dependencies on other sections
   - No symbol table population
   - No type inference

2. DlmSectionAnalyzer (SIMPLE) ✅ COMPLETE
   - Module type validation
   - Subtype validation
   - Ordering checks
   - Security warnings

3. SecuritySectionAnalyzer (SIMPLE-MEDIUM) ✅ COMPLETE
   - Block key validation
   - Field validation per block type
   - Works with SecurityUtilities for defaults
   - No dependencies

4. EnumsSectionAnalyzer (MEDIUM) ✅ COMPLETE
   - Name/field validation
   - Duplicate detection
   - Value computation
   - Symbol table population

5. ImportsSectionAnalyzer (MEDIUM-COMPLEX)
   - File path resolution
   - Hash verification
   - Recursive file parsing
   - Circular import detection
   - Namespace population

6. DataSectionAnalyzer (COMPLEX)
   - Type inference for values
   - Expression evaluation
   - Variable scope resolution
   - Function call validation
   - Enum value validation
   - Depends on: ENUMS, QUICKFUNCS, IMPORTS

7. QuickFuncsSectionAnalyzer (MOST COMPLEX)
   - Full type system
   - Expression type inference
   - Statement validation
   - Control flow analysis
   - Local variable tracking
   - Builtin resolution
   - Recursive calls
   - Depends on: ENUMS, IMPORTS
   - Used by: DATA

PORTING STRATEGY:
- ✅ Start with EnumsSectionAnalyzer (establish patterns)
- ✅ Move to DlmSectionAnalyzer (validation complexity)
- ✅ Port SecuritySectionAnalyzer (similar patterns)
- ⏳ Port ConfigSectionAnalyzer (warm-up)
- ⏳ Port ImportsSectionAnalyzer (file I/O, introduces dependencies)
- ⏳ Port QuickFuncsSectionAnalyzer (complex but independent)
- ⏳ Finally port DataSectionAnalyzer (depends on QuickFuncs)

TESTING STRATEGY:
- Each analyzer has comprehensive tests
- Performance baselines for each
- Memory usage tests
- Error handling tests (Halt/Continue/Recover)
- Integration tests with real .mdix files
*/

// ==================== CROSS-ANALYZER DEPENDENCIES ====================

/*
DEPENDENCY GRAPH:

CONFIG
  └─> (no dependencies)

IMPORTS
  └─> (recursive, may trigger full analysis of imported files)

ENUMS ✅ COMPLETE
  └─> (no dependencies)

DLM ✅ COMPLETE
  └─> (no dependencies)

SECURITY ✅ COMPLETE
  └─> DLM (checks for encryption modules)

QUICKFUNCS
  ├─> ENUMS (for enum access validation)
  └─> IMPORTS (for namespaced function calls)

DATA (analyzed last)
  ├─> ENUMS (for enum value validation)
  ├─> QUICKFUNCS (for function call validation)
  └─> IMPORTS (for namespaced enum/function access)

ANALYSIS ORDER:
1. CONFIG (first - establishes settings)
2. IMPORTS (early - provides namespaces)
3. ENUMS (early - needed by QuickFuncs and Data)
4. DLM (independent)
5. SECURITY (independent, but checks DLM)
6. QUICKFUNCS (needs Enums and Imports)
7. DATA (last - needs everything)
*/
