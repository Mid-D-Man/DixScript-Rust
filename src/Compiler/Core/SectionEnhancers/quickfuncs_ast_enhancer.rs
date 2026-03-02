// src/Compiler/Core/SectionEnhancers/quickfuncs_ast_enhancer.rs
//! Enhances the @QUICKFUNCS section with compile-time completions.
//!
//! Applies parameter default values from type annotations and resolves
//! QualifiedIdentifier nodes using semantic analysis metadata.
//! The resolver is constructed once per section, not once per function,
//! keeping cost O(q) rather than O(f·q).

use crate::Compiler::AST::*;
use crate::Compiler::Core::SectionAnalyzers::SectionAnalysisResult;
use crate::Compiler::Core::SectionEnhancers::QualifiedIdentifierResolver;
use crate::Compiler::Core::OperationalSettings;
use crate::Compiler::Extensions::TypeSystemManager;
use crate::ErrorManager::{DebugConfig, ErrorManager};

pub struct QuickFunctionsAstEnhancer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    enhancement_count: usize,
}

impl<'a> QuickFunctionsAstEnhancer<'a> {
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        QuickFunctionsAstEnhancer {
            debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
            error_manager: ErrorManager::get_shared_instance(),
            operational_settings,
            enhancement_count: 0,
        }
    }

    pub fn enhance(
        &mut self,
        section: &QuickFuncsSection,
        analysis_result: Option<&SectionAnalysisResult>,
    ) -> QuickFuncsSection {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Enhancing {} functions", section.functions.len()
            ));
        }

        // Build the resolver once for the entire section. Creating it per-function
        // would clone the resolution map f times, making cost O(f²·q) rather than O(f·q).
        let resolver = analysis_result
            .filter(|r| !r.qualified_id_resolutions.is_empty())
            .map(|r| {
                if self.debug_config.is_verbose {
                    for (key, resolution) in &r.qualified_id_resolutions {
                        self.error_manager.log_debug(&format!(
                            "Resolution available: {} -> {}",
                            key.parts.join("."),
                            resolution.resolved_type
                        ));
                    }
                }
                QualifiedIdentifierResolver::new(
                    r.qualified_id_resolutions.clone(),
                    self.debug_config,
                )
            });

        let mut enhanced_functions = Vec::with_capacity(section.functions.len());
        for function in &section.functions {
            enhanced_functions.push(self.enhance_function(function, resolver.as_ref()));
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Enhanced {} functions, {} parameter defaults applied",
                section.functions.len(),
                self.enhancement_count
            ));
            if let Some(r) = analysis_result {
                self.error_manager.log_info(&format!(
                    "Resolved {} qualified identifiers",
                    r.qualified_id_resolutions.len()
                ));
            }
        }

        QuickFuncsSection::new(enhanced_functions, section.position)
    }

    fn enhance_function(
        &mut self,
        function: &QuickFunction,
        resolver: Option<&QualifiedIdentifierResolver>,
    ) -> QuickFunction {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!("Enhancing function: {}", function.name));
        }

        let enhanced_parameters =
            TypeSystemManager::apply_defaults_to_parameters(function.parameters.clone());

        let defaults_applied =
            self.count_defaults_applied(&function.parameters, &enhanced_parameters);
        self.enhancement_count += defaults_applied;

        let enhanced_body = match resolver {
            Some(r) => r.resolve_statements(&function.body),
            None => function.body.clone(),
        };

        if defaults_applied > 0 && self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Applied {} default(s) to '{}'", defaults_applied, function.name
            ));
        }

        QuickFunction::new(
            function.name.clone(),
            function.return_type,
            function.scope_list.clone(),
            enhanced_parameters,
            enhanced_body,
            function.position,
        )
    }

    fn count_defaults_applied(
        &self,
        original: &[QuickFuncParam],
        enhanced: &[QuickFuncParam],
    ) -> usize {
        original
            .iter()
            .zip(enhanced.iter())
            .filter(|(o, e)| o.default_value.is_none() && e.default_value.is_some())
            .count()
    }

    pub fn get_enhancement_count(&self) -> usize {
        self.enhancement_count
    }
                                                     }
