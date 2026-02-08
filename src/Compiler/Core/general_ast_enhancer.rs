// src/Compiler/Core/general_ast_enhancer.rs

use crate::Compiler::AST::*;
use crate::Compiler::Core::{
    OperationalSettings,
    SemanticAnalysisResult,
    EnhancementResult,
    SectionEnhancementInfo,
};
use crate::Compiler::VersionControl::VersionManager;
use crate::ErrorManager::ErrorManager;
use std::time::{Duration, Instant};
use std::collections::HashMap;

/// General AST Enhancer - Applies compile-time enhancements to validated AST
///
/// ENHANCEMENT PHASES:
/// - Phase 1: Parameter Default Value Resolution
/// - Phase 2: Qualified Identifier Resolution
/// - Phase 3: Type Inference Refinement
/// - Phase 4: Constant Folding (future)
///
/// NOTE: This is a SEPARATE phase from semantic analysis.
/// Call order: Parse -> Semantic Analysis -> AST Enhancement -> Code Generation
pub struct GeneralAstEnhancer<'a> {
    // Borrowed inputs
    operational_settings: &'a OperationalSettings,

    // Owned state
    error_manager: ErrorManager,
    enhancement_result: EnhancementResult,
    stopwatch: Instant,

    // Cached log checks (optimization)
    can_log_debug: bool,
    can_log_verbose: bool,
}

impl<'a> GeneralAstEnhancer<'a> {
    /// Create new AST enhancer
    ///
    /// # Arguments
    /// * `operational_settings` - Compiler settings (borrowed)
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        let error_manager = ErrorManager::get_shared_instance();

        let can_log_debug = operational_settings.debug_mode != crate::Compiler::Core::DebugMode::Off;
        let can_log_verbose = operational_settings.debug_mode == crate::Compiler::Core::DebugMode::Verbose;

        GeneralAstEnhancer {
            operational_settings,
            error_manager,
            enhancement_result: EnhancementResult::new(),
            stopwatch: Instant::now(),
            can_log_debug,
            can_log_verbose,
        }
    }

    /// Main enhancement entry point
    ///
    /// Takes AST by reference and semantic analysis result
    /// Returns enhanced AST and enhancement metadata
    ///
    /// # Arguments
    /// * `ast` - Validated AST to enhance (borrowed)
    /// * `semantic_result` - Results from semantic analysis (optional, borrowed)
    pub fn enhance(
        mut self,
        ast: &DixScript,
        semantic_result: Option<&SemanticAnalysisResult>,
    ) -> EnhancementResult {
        self.log_info("Starting AST Enhancement");

        let mut enhanced_ast = ast.clone();

        // Phase 1: Parameter Default Value Resolution
        self.enhance_phase1_parameter_defaults(&mut enhanced_ast, semantic_result);

        // Phase 2: Qualified Identifier Resolution
        self.enhance_phase2_qualified_identifiers(&mut enhanced_ast, semantic_result);

        // Phase 3: Type Inference Refinement
        self.enhance_phase3_type_inference(&mut enhanced_ast, semantic_result);

        self.enhancement_result.enhanced_ast = enhanced_ast;
        self.finalize_result()
    }

    // ==================== ENHANCEMENT PHASES ====================

    fn enhance_phase1_parameter_defaults(
        &mut self,
        ast: &mut DixScript,
        _semantic_result: Option<&SemanticAnalysisResult>,
    ) {
        self.log_info("Phase 1: Resolving parameter default values");

        if let Some(ref mut quickfuncs) = ast.quick_functions {
            for func in &mut quickfuncs.functions {
                for param in &mut func.parameters {
                    if param.default_value.is_some() {
                        self.enhancement_result.total_enhancements += 1;
                        self.log_debug_fmt(|| format!(
                            "  Enhanced parameter '{}' in function '{}'",
                            param.name, func.name
                        ));
                    }
                }
            }
        }

        self.log_info("Phase 1 complete");
    }

    fn enhance_phase2_qualified_identifiers(
        &mut self,
        _ast: &mut DixScript,
        _semantic_result: Option<&SemanticAnalysisResult>,
    ) {
        self.log_info("Phase 2: Resolving qualified identifiers");
        // TODO: Implement qualified identifier resolution
        self.log_debug("Qualified identifier resolution not yet implemented");
        self.log_info("Phase 2 complete");
    }

    fn enhance_phase3_type_inference(
        &mut self,
        _ast: &mut DixScript,
        _semantic_result: Option<&SemanticAnalysisResult>,
    ) {
        self.log_info("Phase 3: Refining type inference");
        // TODO: Implement type inference refinement
        self.log_debug("Type inference refinement not yet implemented");
        self.log_info("Phase 3 complete");
    }

    // ==================== HELPER METHODS ====================

    fn finalize_result(mut self) -> EnhancementResult {
        self.enhancement_result.is_success = self.enhancement_result.errors.is_empty();
        self.enhancement_result.enhancement_duration = self.stopwatch.elapsed();

        self.log_info_fmt(|| format!(
            "Enhancement duration: {:.2}ms",
            self.enhancement_result.enhancement_duration.as_secs_f64() * 1000.0
        ));

        self.enhancement_result
    }

    // ==================== LOGGING HELPERS ====================

    #[inline]
    fn log_debug(&self, message: &str) {
        if self.can_log_debug {
            self.error_manager.log_debug(message);
        }
    }

    #[inline]
    fn log_debug_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        if self.can_log_debug {
            self.error_manager.log_debug(&f());
        }
    }

    #[inline]
    fn log_info(&self, message: &str) {
        self.error_manager.log_info(message);
    }

    #[inline]
    fn log_info_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        self.error_manager.log_info(&f());
    }
}