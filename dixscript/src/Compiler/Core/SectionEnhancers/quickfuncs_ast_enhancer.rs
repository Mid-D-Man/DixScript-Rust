// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/SectionEnhancers/quickfuncs_ast_enhancer.rs"
// ============================================================================
//! Enhances the @QUICKFUNCS section with compile-time completions.
//!
//! Applies parameter default values from type annotations and resolves
//! QualifiedIdentifier nodes using semantic analysis metadata.
//! The resolver is constructed once per section, not once per function,
//! keeping cost O(q) rather than O(f*q).
//!
//! The work is done on the arena by [`QuickFunctionsAstEnhancer::enhance_doc`]. The owned
//! [`QuickFunctionsAstEnhancer::enhance`] copies the section into a scratch arena, calls it and
//! copies the result back out.

use crate::Compiler::AST::arena::{Arena, DocQuickFuncParam, DocQuickFunction, DocQuickFuncs, ExprNode};
use crate::Compiler::AST::*;
use crate::Compiler::Core::SectionAnalyzers::SectionAnalysisResult;
use crate::Compiler::Core::SectionEnhancers::QualifiedIdentifierResolver;
use crate::Compiler::Core::OperationalSettings;
use crate::Compiler::Extensions::TypeSystemManager;
use crate::ErrorManager::{DebugConfig, ErrorManager};

/// Applies parameter defaults and resolves qualified identifiers in a `@QUICKFUNCS` section.
pub struct QuickFunctionsAstEnhancer<'a> {
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    enhancement_count: usize,
}

impl<'a> QuickFunctionsAstEnhancer<'a> {
    /// An enhancer that logs to the process-wide shared error manager.
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        Self::new_with_error_manager(operational_settings,ErrorManager::get_shared_instance())

    }
    /// An enhancer that logs to `error_manager`.
    pub fn new_with_error_manager(operational_settings: &'a OperationalSettings,error_manager:ErrorManager) -> Self {
        QuickFunctionsAstEnhancer {
            debug_config: DebugConfig::from_debug_mode(operational_settings.debug_mode),
            error_manager,
            operational_settings,
            enhancement_count: 0,
        }
    }

    /// Enhances an owned section. Copies it into a scratch arena, runs
    /// [`enhance_doc`](Self::enhance_doc) and copies the result back out.
    pub fn enhance(
        &mut self,
        section: &QuickFuncsSection,
        analysis_result: Option<&SectionAnalysisResult>,
    ) -> QuickFuncsSection {
        let mut arena = Arena::new();
        let doc = DocQuickFuncs::from_owned(&mut arena, section);
        let enhanced = self.enhance_doc(&doc, &mut arena, analysis_result);
        enhanced.export(&arena)
    }

    /// Enhances a section that lives in `arena`.
    ///
    /// Returns a new section whose parameters carry their defaults and whose bodies have their
    /// qualified identifiers resolved. The nodes that did not change are shared with `section`;
    /// the ones that did are allocated in `arena` after the existing ones, so `section` is still
    /// valid and unchanged afterwards.
    pub fn enhance_doc(
        &mut self,
        section: &DocQuickFuncs,
        arena: &mut Arena,
        analysis_result: Option<&SectionAnalysisResult>,
    ) -> DocQuickFuncs {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Enhancing {} functions", section.functions.len()
            ));
        }

        // Build the resolver once for the entire section. Creating it per-function
        // would clone the resolution map f times, making cost O(f^2 * q) rather than O(f*q).
        let resolver = analysis_result.as_ref()
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
            enhanced_functions.push(self.enhance_function(arena, function, resolver.as_ref()));
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Enhanced {} functions, {} parameter defaults applied",
                section.functions.len(),
                self.enhancement_count
            ));
            if  let  Some(r) = analysis_result {
                self.error_manager.log_info(&format!(
                    "Resolved {} qualified identifiers",
                    r.qualified_id_resolutions.len()
                ));
            }
        }

        DocQuickFuncs { functions: enhanced_functions, position: section.position }
    }

    fn enhance_function(
        &mut self,
        arena: &mut Arena,
        function: &DocQuickFunction,
        resolver: Option<&QualifiedIdentifierResolver>,
    ) -> DocQuickFunction {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!("Enhancing function: {}", function.name));
        }

        let (enhanced_parameters, defaults_applied) = Self::apply_defaults(arena, &function.parameters);
        self.enhancement_count += defaults_applied;

        let enhanced_body = match resolver {
            Some(r) => r.resolve_statement_list(arena, function.body),
            None => function.body,
        };

        if defaults_applied > 0 && self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Applied {} default(s) to '{}'", defaults_applied, function.name
            ));
        }

        DocQuickFunction {
            name: function.name.clone(),
            return_type: function.return_type,
            scope_list: function.scope_list.clone(),
            parameters: enhanced_parameters,
            body: enhanced_body,
            position: function.position,
        }
    }

    /// Gives every parameter that has a type annotation and no default the synthetic default of
    /// its type. Returns the parameters and how many got one.
    ///
    /// Same rule as [`TypeSystemManager::apply_defaults_to_parameters`], over the arena: a
    /// parameter that has a default of its own, or no type, is left as it is.
    fn apply_defaults(arena: &mut Arena, parameters: &[DocQuickFuncParam]) -> (Vec<DocQuickFuncParam>, usize) {
        let mut applied = 0;
        let mut result = Vec::with_capacity(parameters.len());
        for param in parameters {
            match (param.default_value, param.data_type) {
                (None, Some(data_type)) => {
                    let value = TypeSystemManager::synthetic_parameter_default(&param.name, data_type);
                    let value = arena.import_value(&value);
                    let default = arena.alloc_expr(ExprNode::Value { value, position: param.position });
                    applied += 1;
                    result.push(DocQuickFuncParam {
                        name: param.name.clone(),
                        data_type: param.data_type,
                        default_value: Some(default),
                        position: param.position,
                    });
                }
                _ => result.push(param.clone()),
            }
        }
        (result, applied)
    }

    /// How many parameter defaults the enhancer has applied so far.
    pub fn get_enhancement_count(&self) -> usize {
        self.enhancement_count
    }
}
