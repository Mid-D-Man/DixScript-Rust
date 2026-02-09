// src/Compiler/Core/SectionEnhancers/quickfuncs_ast_enhancer.rs

//! Enhances QuickFunctions section with compile-time completions
//! Primary tasks:
//! 1. Generate parameter defaults from type annotations
//! 2. Resolve qualified identifiers using semantic analysis results

use crate::Compiler::AST::*;
use crate::Compiler::Core::SectionEnhancers::QualifiedIdentifierResolver;
use crate::Compiler::Core::SectionAnalyzers::SectionAnalysisResult;
use crate::Compiler::Core::OperationalSettings;
use crate::Compiler::Extensions::TypeSystemManager;
use crate::ErrorManager::ErrorManager;
use std::time::{Duration, Instant};

/// Enhances QuickFunctions section with compile-time completions
pub struct QuickFunctionsAstEnhancer {
    operational_settings: OperationalSettings,
    error_manager: ErrorManager,
    enhancement_count: usize,
    start_time: Option<Instant>,
}

impl QuickFunctionsAstEnhancer {
    /// Create new QuickFunctions AST enhancer
    pub fn new(operational_settings: OperationalSettings) -> Self {
        QuickFunctionsAstEnhancer {
            operational_settings,
            error_manager: ErrorManager::get_shared_instance(),
            enhancement_count: 0,
            start_time: None,
        }
    }

    /// Enhance QuickFunctions section with compile-time completions AND qualified identifier resolution
    pub fn enhance(
        &mut self,
        section: &QuickFuncsSection,
        analysis_result: Option<&SectionAnalysisResult>,
    ) -> QuickFuncsSection {
        self.start_time = Some(Instant::now());

        self.error_manager.log_debug(&format!(
            "Processing {} functions",
            section.functions.len()
        ));

        let mut enhanced_functions = Vec::with_capacity(section.functions.len());

        // EXPLICIT ITERATION: Use .iter() to make borrowing crystal clear
        for function in section.functions.iter() {
            let enhanced_function = self.enhance_function(function, analysis_result);
            enhanced_functions.push(enhanced_function);
        }

        let duration = self.start_time.unwrap().elapsed();

        self.error_manager.log_info(&format!(
            "Enhanced {} functions",
            section.functions.len()
        ));
        self.error_manager.log_info(&format!(
            "Applied {} parameter defaults",
            self.enhancement_count
        ));

        // analysis_result is Option<&T> which is Copy, can be used again
        if let Some(ref result) = analysis_result {
            self.error_manager.log_info(&format!(
                "Resolved {} qualified identifiers",
                result.qualified_id_resolutions.len()
            ));
        }

        self.error_manager.log_debug(&format!(
            "Enhancement time: {:.2}ms",
            duration.as_secs_f64() * 1000.0
        ));

        QuickFuncsSection::new(enhanced_functions, section.position)
    }

    /// Enhance a single function
    fn enhance_function(
        &mut self,
        function: &QuickFunction,
        analysis_result: Option<&SectionAnalysisResult>,
    ) -> QuickFunction {
        self.error_manager.log_debug(&format!("Enhancing function: {}", function.name));

        // 1. Apply parameter defaults - clone parameters because apply_defaults_to_parameters takes ownership
        let enhanced_parameters = TypeSystemManager::apply_defaults_to_parameters(
            function.parameters.clone()
        );
        let defaults_applied = self.count_defaults_applied(&function.parameters, &enhanced_parameters);
        self.enhancement_count += defaults_applied;

        // 2. Resolve qualified identifiers in function body (if analysis result available)
        let enhanced_body = if let Some(result) = analysis_result {
            if !result.qualified_id_resolutions.is_empty() {
                self.resolve_qualified_identifiers_in_body(&function.body, result)
            } else {
                function.body.clone()
            }
        } else {
            function.body.clone()
        };

        if defaults_applied > 0 {
            self.error_manager.log_debug(&format!(
                "Applied {} default(s) to function '{}'",
                defaults_applied, function.name
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

    /// Resolve qualified identifiers in function body using analysis results
    fn resolve_qualified_identifiers_in_body(
        &self,
        body: &[QuickFuncStatement],
        analysis_result: &SectionAnalysisResult,
    ) -> Vec<QuickFuncStatement> {
        self.error_manager.log_debug("[QF-Enhancer] Starting qualified identifier resolution");
        self.error_manager.log_debug(&format!(
            "[QF-Enhancer] Total resolutions available: {}",
            analysis_result.qualified_id_resolutions.len()
        ));

        // Log all available resolutions (verbose mode)
        if self.operational_settings.debug_mode == crate::Compiler::Core::DebugMode::Verbose {
            for (key, resolution) in &analysis_result.qualified_id_resolutions {
                self.error_manager.log_debug(&format!(
                    "[QF-Enhancer] Resolution available: {} -> {}",
                    key.parts.join("."), resolution.resolved_type
                ));
            }
        }

        let resolver = QualifiedIdentifierResolver::new(
            analysis_result.qualified_id_resolutions.clone()
        );

        let enhanced_statements: Vec<QuickFuncStatement> = body
            .iter()
            .map(|stmt| resolver.resolve_statement(stmt))
            .collect();

        self.error_manager.log_debug("[QF-Enhancer] Qualified identifier resolution complete");

        enhanced_statements
    }

    /// Count how many defaults were applied (for stats)
    fn count_defaults_applied(
        &self,
        original: &[QuickFuncParam],
        enhanced: &[QuickFuncParam],
    ) -> usize {
        let mut count = 0;

        for i in 0..original.len() {
            if original[i].default_value.is_none() && enhanced[i].default_value.is_some() {
                count += 1;
            }
        }

        count
    }

    /// Get total number of enhancements applied
    pub fn get_enhancement_count(&self) -> usize {
        self.enhancement_count
    }

    /// Get enhancement duration
    pub fn get_enhancement_duration(&self) -> Duration {
        self.start_time.map(|t| t.elapsed()).unwrap_or_default()
    }
}