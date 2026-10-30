// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/general_ast_enhancer.rs"
// ============================================================================
//! Applies compile-time enhancements to a validated AST.
//!
//! Must run after semantic analysis: Parse → Semantic Analysis → AST Enhancement → Value Resolution.
//!
//! ## Error manager propagation
//! `propagate_error_manager = true`  (LSP / isolated mode): every sub-enhancer
//!   receives a clone of this instance's ErrorManager so all diagnostics land
//!   in the same isolated document store.
//! `propagate_error_manager = false` (CLI / shared mode): sub-enhancers call
//!   their own `new()` which acquires the process-wide shared singleton. This
//!   is the legacy path and produces identical runtime behaviour because the
//!   CLI's top-level ErrorManager IS the shared singleton.
//!
//! ## Reading the arena
//! [`GeneralAstEnhancer::with_document`] gives the enhancer the [`Document`] the owned tree was
//! exported from. The `@QUICKFUNCS` section is then read from the document and enhanced in its
//! arena, and only the enhanced section is copied out into the result's tree. Without a document
//! the owned section is enhanced through a scratch arena. The sections are decided by the tree
//! that is read: with a document attached, the document says whether there is a section.

use crate::Compiler::AST::arena::Document;
use crate::Compiler::AST::DixScript;
use crate::Compiler::Core::{
    EnhancementResult, OperationalSettings, SemanticAnalysisResult, SectionEnhancementInfo,
};
use crate::Compiler::Core::SectionEnhancers::QuickFunctionsAstEnhancer;
use crate::ErrorManager::{DebugConfig, ErrorManager};
use web_time::Instant;

pub struct GeneralAstEnhancer<'a> {
    operational_settings:    &'a OperationalSettings,
    error_manager:           ErrorManager,
    debug_config:            DebugConfig,
    enhancement_result:      EnhancementResult,
    stopwatch:               Instant,
    /// When true, sub-enhancers receive `self.error_manager.clone()` so that
    /// diagnostics from all sub-enhancers aggregate into the same store.
    /// When false (CLI path), sub-enhancers call their own `new()` and use the
    /// process-wide shared singleton independently.
    propagate_error_manager: bool,
    /// The document the owned tree was exported from, when the caller has one.
    document:                Option<&'a mut Document>,
}

impl<'a> GeneralAstEnhancer<'a> {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// LSP / isolated path.
    /// Sub-enhancers will receive `error_manager.clone()`.
    pub fn new_with_error_manager(
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
    ) -> Self {
        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);
        GeneralAstEnhancer {
            operational_settings,
            error_manager,
            debug_config,
            enhancement_result:      EnhancementResult::new(),
            stopwatch:               Instant::now(),
            propagate_error_manager: true,
            document:                None,
        }
    }

    /// CLI / shared path.
    /// Sub-enhancers will call their own `new()` and use the shared singleton.
    pub fn new(operational_settings: &'a OperationalSettings) -> Self {
        Self::new_with_error_manager(operational_settings, ErrorManager::get_shared_instance())
            .with_propagate(false)
    }

    /// Makes the enhancer read the `@QUICKFUNCS` section from `document` instead of from the
    /// owned tree passed to [`enhance`](Self::enhance).
    ///
    /// The enhanced nodes are allocated in the document's arena after the existing ones, so the
    /// document stays valid and means the same as before.
    pub fn with_document(mut self, document: &'a mut Document) -> Self {
        self.document = Some(document);
        self
    }

    /// Builder-style helper to flip the propagation flag.
    fn with_propagate(mut self, propagate: bool) -> Self {
        self.propagate_error_manager = propagate;
        self
    }

    // ── Main entry point ──────────────────────────────────────────────────────

    pub fn enhance(
        mut self,
        ast:             &DixScript,
        semantic_result: Option<&SemanticAnalysisResult>,
    ) -> EnhancementResult {
        self.error_manager.log_info("Starting AST enhancement");

        let mut enhanced_ast = ast.clone();
        self.enhance_quickfunctions(&mut enhanced_ast, semantic_result);

        self.enhancement_result.enhanced_ast = enhanced_ast;
        self.enhancement_result.is_success   = self.enhancement_result.errors.is_empty();

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Enhancement complete — {} enhancements applied, {} warnings",
                self.enhancement_result.total_enhancements,
                self.enhancement_result.warnings.len()
            ));
        }

        self.finalize_result()
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn enhance_quickfunctions(
        &mut self,
        ast:             &mut DixScript,
        semantic_result: Option<&SemanticAnalysisResult>,
    ) {
        // Whether there is a section is decided by the tree that is read.
        let present = match self.document.as_deref() {
            Some(document) => document.quick_functions.is_some(),
            None           => ast.quick_functions.is_some(),
        };
        if !present {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug("No QUICKFUNCS section to enhance");
            }
            return;
        }

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

        let mut enhancer = if self.propagate_error_manager {
            QuickFunctionsAstEnhancer::new_with_error_manager(
                self.operational_settings,
                self.error_manager.clone(),
            )
        } else {
            QuickFunctionsAstEnhancer::new(self.operational_settings)
        };

        let enhanced_section = match self.document.as_deref_mut() {
            Some(document) => {
                let Document { arena, quick_functions, .. } = document;
                let section = quick_functions
                    .as_ref()
                    .expect("presence was read from this document just above");
                let enhanced = enhancer.enhance_doc(section, arena, quickfuncs_analysis);
                enhanced.export(arena)
            }
            None => {
                let section = ast
                    .quick_functions
                    .as_ref()
                    .expect("presence was read from this tree just above");
                enhancer.enhance(section, quickfuncs_analysis)
            }
        };
        let count = enhancer.get_enhancement_count();

        self.enhancement_result.total_enhancements += count;
        self.enhancement_result.section_enhancements.insert(
            "QUICKFUNCS".to_string(),
            SectionEnhancementInfo {
                section_name:         "QUICKFUNCS".to_string(),
                enhancements_applied: count,
                enhancement_types:    vec![
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
