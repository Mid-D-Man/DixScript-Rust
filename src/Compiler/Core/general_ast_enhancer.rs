// src/Compiler/Core/general_ast_enhancer.rs
//! Applies compile-time enhancements to a validated AST.
//!
//! Must run after semantic analysis: Parse → Semantic Analysis → AST Enhancement → Value Resolution.
//! Semantic analysis produces QualifiedIdentifierResolution metadata that this phase consumes.

use crate::Compiler::AST::DixScript;
use crate::Compiler::Core::{
    EnhancementResult, OperationalSettings, SemanticAnalysisResult, SectionEnhancementInfo,
};
use crate::Compiler::Core::SectionEnhancers::QuickFunctionsAstEnhancer;
use crate::ErrorManager::{DebugConfig, ErrorManager};
use std::time::Instant;

pub struct GeneralAstEnhancer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    enhancement_result: EnhancementResult,
    stopwatch: Instant,
}

impl<'a> GeneralAstEnhancer<'a> {
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        GeneralAstEnhancer {
            debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
            error_manager: ErrorManager::get_shared_instance(),
            operational_settings,
            enhancement_result: EnhancementResult::new(),
            stopwatch: Instant::now(),
        }
    }

    pub fn enhance(
        mut self,
        ast: &DixScript,
        semantic_result: Option<&SemanticAnalysisResult>,
    ) -> EnhancementResult {
        self.error_manager.log_info("Starting AST enhancement");

        let mut enhanced_ast = ast.clone();
        self.enhance_quickfunctions(&mut enhanced_ast, semantic_result);

        self.enhancement_result.enhanced_ast = enhanced_ast;
        self.enhancement_result.is_success = self.enhancement_result.errors.is_empty();

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Enhancement complete — {} enhancements applied, {} warnings",
                self.enhancement_result.total_enhancements,
                self.enhancement_result.warnings.len()
            ));
        }

        self.finalize_result()
    }

    fn enhance_quickfunctions(
        &mut self,
        ast: &mut DixScript,
        semantic_result: Option<&SemanticAnalysisResult>,
    ) {
        let quickfuncs_section = match ast.quick_functions.as_ref() {
            Some(s) => s,
            None => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No QUICKFUNCS section to enhance");
                }
                return;
            }
        };

        let quickfuncs_analysis = semantic_result
            .and_then(|sr| sr.section_results.get("QUICKFUNCS"));

        if self.debug_config.is_enabled {
            match quickfuncs_analysis {
                Some(a) => self.error_manager.log_debug(&format!(
                    "QUICKFUNCS: {} qualified identifier resolutions available",
                    a.qualified_id_resolutions.len()
                )),
                None => self.error_manager.log_warning(
                    "No QUICKFUNCS semantic analysis result — enhancement will be limited",
                ),
            }
        }

        let mut enhancer = QuickFunctionsAstEnhancer::new(self.operational_settings);
        let enhanced_section = enhancer.enhance(quickfuncs_section, quickfuncs_analysis);

        let count = enhancer.get_enhancement_count();

        self.enhancement_result.total_enhancements += count;
        self.enhancement_result.section_enhancements.insert(
            "QUICKFUNCS".to_string(),
            SectionEnhancementInfo {
                section_name: "QUICKFUNCS".to_string(),
                enhancements_applied: count,
                enhancement_types: vec![
                    "parameter_defaults".to_string(),
                    "qualified_identifier_resolution".to_string(),
                ],
            },
        );

        ast.quick_functions = Some(enhanced_section);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "QUICKFUNCS enhancement complete — {} parameter defaults applied", count
            ));
        }
    }

    fn finalize_result(mut self) -> EnhancementResult {
        self.enhancement_result.enhancement_duration = self.stopwatch.elapsed();

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Enhancement duration: {:.2}ms",
                self.enhancement_result.enhancement_duration.as_secs_f64() * 1000.0
            ));
        }

        self.enhancement_result
    }
    }
