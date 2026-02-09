// src/Compiler/Core/general_ast_enhancer.rs

use crate::Compiler::AST::*;
use crate::Compiler::Core::{
    OperationalSettings,
    SemanticAnalysisResult,
    EnhancementResult,
    SectionEnhancementInfo,
};
use crate::Compiler::Core::SectionEnhancers::QuickFunctionsAstEnhancer;
use crate::ErrorManager::ErrorManager;
use std::time::Instant;

/// General AST Enhancer - Applies compile-time enhancements to validated AST
///
/// ENHANCEMENT PHASES:
/// - Phase 1: Parameter Default Value Resolution (from type annotations)
/// - Phase 2: Qualified Identifier Resolution (QualifiedIdentifier -> concrete types)
/// - Phase 3: Type Inference Refinement (future)
/// - Phase 4: Constant Folding (future)
///
/// NOTE: This is a SEPARATE phase from semantic analysis.
/// Call order: Parse -> Semantic Analysis -> AST Enhancement -> Value Resolution
///
/// CRITICAL: Enhancement MUST run after semantic analysis because:
/// - Semantic analysis produces QualifiedIdentifierResolution metadata
/// - Enhancement uses that metadata to transform ambiguous nodes
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
    /// Returns enhanced AST with completions applied and qualified identifiers resolved
    ///
    /// # Arguments
    /// * `ast` - Validated AST to enhance (borrowed)
    /// * `semantic_result` - Results from semantic analysis (optional, borrowed)
    pub fn enhance(
        mut self,
        ast: &DixScript,
        semantic_result: Option<&SemanticAnalysisResult>,
    ) -> EnhancementResult {
        self.log_info("Starting AST Enhancement (Phase 4.5)");
        self.log_info_fmt(|| format!(
            "Error Handling Strategy: {:?}",
            self.operational_settings.error_handling_strategy
        ));

        let mut enhanced_ast = ast.clone();

        // ✅ Phase 1: Enhance QuickFunctions (parameter defaults + qualified identifier resolution)
        self.enhance_phase1_quickfunctions(&mut enhanced_ast, semantic_result);

        // Phase 2: Future enhancements (constant folding, type refinement, etc.)
        // TODO: Add more enhancement phases as needed

        self.enhancement_result.enhanced_ast = enhanced_ast;
        self.enhancement_result.is_success = self.enhancement_result.errors.is_empty();

        self.log_info_fmt(|| format!(
            "AST enhancement complete. Success: {}",
            self.enhancement_result.is_success
        ));
        self.log_info_fmt(|| format!(
            "Total enhancements: {}, Warnings: {}",
            self.enhancement_result.total_enhancements,
            self.enhancement_result.warnings.len()
        ));

        self.finalize_result()
    }

    // ==================== ENHANCEMENT PHASES ====================

    /// Phase 1: Enhance QuickFunctions section
    /// - Applies parameter defaults from type annotations
    /// - Resolves qualified identifiers using semantic analysis metadata
    fn enhance_phase1_quickfunctions(
        &mut self,
        ast: &mut DixScript,
        semantic_result: Option<&SemanticAnalysisResult>,
    ) {
        self.log_info("Phase 1: Enhancing QuickFunctions section");

        // Get a reference to the QuickFunctions section
        let quickfuncs_section = match ast.quick_functions.as_ref() {
            Some(section) => section,
            None => {
                self.log_debug("No QuickFunctions section to enhance");
                return;
            }
        };

        // Extract QUICKFUNCS section analysis result (contains qualified ID resolutions)
        let quickfuncs_analysis = semantic_result
            .and_then(|sr| sr.section_results.get("QUICKFUNCS"));

        if quickfuncs_analysis.is_none() {
            self.log_warning("No semantic analysis result for QUICKFUNCS - enhancement will be limited");
        } else {
            if let Some(analysis) = quickfuncs_analysis {
                self.log_debug_fmt(|| format!(
                    "Found {} qualified identifier resolutions in QUICKFUNCS analysis",
                    analysis.qualified_id_resolutions.len()
                ));
            }
        }

        // Create QuickFunctions enhancer
        let mut enhancer = QuickFunctionsAstEnhancer::new(
            self.operational_settings.clone()
        );

        // Enhance section - passing reference to section
        let enhanced_section = enhancer.enhance(quickfuncs_section, quickfuncs_analysis);

        // Track enhancements
        let enhancement_info = SectionEnhancementInfo {
            section_name: "QUICKFUNCS".to_string(),
            enhancements_applied: enhancer.get_enhancement_count(),
            enhancement_types: vec![
                "parameter_defaults".to_string(),
                "qualified_identifier_resolution".to_string(),
            ],
        };

        self.enhancement_result.total_enhancements += enhancement_info.enhancements_applied;
        self.enhancement_result.section_enhancements.insert(
            "QUICKFUNCS".to_string(),
            enhancement_info,
        );

        // Update AST with enhanced section
        ast.quick_functions = Some(enhanced_section);

        self.log_info_fmt(|| format!(
            "Phase 1 complete: applied {} enhancements to QUICKFUNCS",
            enhancer.get_enhancement_count()
        ));
    }

    // ==================== HELPER METHODS ====================

    fn finalize_result(mut self) -> EnhancementResult {
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

    #[inline]
    fn log_warning(&self, message: &str) {
        self.error_manager.log_Warning(message);
    }
        }
