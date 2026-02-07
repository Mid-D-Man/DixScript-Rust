// src/Compiler/Core/general_ast_enhancer.rs


//! Central AST enhancement orchestrator for DixScript v1.0.0
//! Routes to section-specific enhancers
//! Runs AFTER semantic analysis, BEFORE value resolution
//!
//! Purpose: Apply compile-time optimizations and completions:
//! - Generate parameter defaults from type annotations
//! - Resolve qualified identifiers to concrete expression types
//! - Apply constant folding (future)

use crate::Compiler::AST::DixScript;
use crate::Compiler::Core::SectionAnalyzers::SemanticAnalysisResult;
use crate::Compiler::Core::SectionEnhancers::QuickFunctionsAstEnhancer;
use crate::Compiler::Core::{ErrorHandlingStrategy, OperationalSettings};
use crate::Compiler::VersionControl::VersionManager;
use crate::ErrorManager::ErrorManager;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Central AST enhancement orchestrator
pub struct GeneralAstEnhancer {
    operational_settings: OperationalSettings,
    error_manager: ErrorManager,
    start_time: Option<Instant>,
}

impl GeneralAstEnhancer {
    /// Create new AST enhancer
    pub fn new(operational_settings: OperationalSettings) -> Self {
        GeneralAstEnhancer {
            operational_settings,
            error_manager: ErrorManager::get_shared_instance(),
            start_time: None,
        }
    }
    
    /// Main enhancement entry point
    /// Returns enhanced AST with completions applied and qualified identifiers resolved
    pub fn enhance(
        &mut self,
        ast: &DixScript,
        analysis_result: Option<&SemanticAnalysisResult>,
    ) -> EnhancementResult {
        self.start_time = Some(Instant::now());
        
        self.error_manager.log_info("Starting AST enhancement (Phase 4.5)");
        self.error_manager.log_info(&format!(
            "Error Handling Strategy: {:?}",
            self.operational_settings.error_handling_strategy
        ));
        
        let mut result = EnhancementResult::new();
        
        // Clone the input AST for enhancement
        let mut enhanced_ast = ast.clone();
        
        // Route to section enhancers
        enhanced_ast = self.enhance_quick_functions(enhanced_ast, analysis_result, &mut result);
        
        if self.should_terminate(&result) {
            return self.finalize_result(ast.clone(), result);
        }
        
        // Future: Add more section enhancers here
        
        result.enhanced_ast = Some(enhanced_ast);
        result.is_success = true;
        
        let duration = self.start_time.unwrap().elapsed();
        result.enhancement_duration = duration;
        
        self.error_manager.log_info(&format!(
            "AST enhancement complete. Success: {}",
            result.is_success
        ));
        self.error_manager.log_info(&format!(
            "Total warnings: {}",
            result.warnings.len()
        ));
        self.error_manager.log_info(&format!(
            "Enhancement duration: {:.2}ms",
            duration.as_secs_f64() * 1000.0
        ));
        
        self.finalize_result(ast.clone(), result)
    }
    
    /// Enhance QuickFunctions section (parameter defaults + qualified identifier resolution)
    fn enhance_quick_functions(
        &self,
        mut ast: DixScript,
        analysis_result: Option<&SemanticAnalysisResult>,
        result: &mut EnhancementResult,
    ) -> DixScript {
        if ast.quick_functions.is_none() {
            self.error_manager.log_debug("No QuickFunctions section to enhance");
            return ast;
        }
        
        // Check if feature is enabled
        let version_manager = VersionManager::instance().read().unwrap();
        if !version_manager.supports_feature("parameter_defaults") {
            self.error_manager.log_warning(
                "Parameter defaults not supported in this version"
            );
            return ast;
        }
        drop(version_manager);
        
        let section = ast.quick_functions.as_ref().unwrap();
        
        self.error_manager.log_debug(&format!(
            "Enhancing {} functions",
            section.functions.len()
        ));
        
        let mut enhancer = QuickFunctionsAstEnhancer::new(self.operational_settings.clone());
        
        //  Get QuickFunctions analysis result (if available)
        let quickfuncs_analysis = analysis_result
            .and_then(|ar| ar.section_results.get("QUICKFUNCS"));
        
        let enhanced_section = enhancer.enhance(section, quickfuncs_analysis);
        
        // Track enhancements
        result.enhancements_by_section.insert(
            "QUICKFUNCS".to_string(),
            SectionEnhancementInfo {
                section_name: "QUICKFUNCS".to_string(),
                enhancements_applied: enhancer.get_enhancement_count(),
                duration: enhancer.get_enhancement_duration(),
            },
        );
        
        // Update AST with enhanced section
        ast.quick_functions = Some(enhanced_section);
        
        ast
    }
    
    /// Check if enhancement should terminate early
    fn should_terminate(&self, result: &EnhancementResult) -> bool {
        !result.warnings.is_empty()
            && matches!(
                self.operational_settings.error_handling_strategy,
                ErrorHandlingStrategy::Halt
            )
    }
    
    /// Finalize enhancement result
    fn finalize_result(&self, original_ast: DixScript, mut result: EnhancementResult) -> EnhancementResult {
        if result.enhanced_ast.is_none() {
            result.enhanced_ast = Some(original_ast);
        }
        
        result
    }
}

/// Result of AST enhancement phase
#[derive(Debug, Clone)]
pub struct EnhancementResult {
    pub is_success: bool,
    pub enhanced_ast: Option<DixScript>,
    pub enhancement_duration: Duration,
    pub warnings: Vec<String>,
    pub enhancements_by_section: HashMap<String, SectionEnhancementInfo>,
}

impl EnhancementResult {
    pub fn new() -> Self {
        EnhancementResult {
            is_success: false,
            enhanced_ast: None,
            enhancement_duration: Duration::default(),
            warnings: Vec::new(),
            enhancements_by_section: HashMap::new(),
        }
    }
    
    pub fn total_enhancements(&self) -> usize {
        self.enhancements_by_section
            .values()
            .map(|info| info.enhancements_applied)
            .sum()
    }
}

impl Default for EnhancementResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about enhancements applied to a section
#[derive(Debug, Clone)]
pub struct SectionEnhancementInfo {
    pub section_name: String,
    pub enhancements_applied: usize,
    pub duration: Duration,
  }
