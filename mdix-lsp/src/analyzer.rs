// mdix-lsp/src/analyzer.rs
//! Pipeline runner — Approach B (tokenizer-first).
//!
//! FIXES:
//!   1. source_file_path set from URI → local imports resolve correctly
//!   2. Cloud imports detected → skip_imports_resolution=true to avoid tokio
//!      runtime-inside-async panic
//!   3. SemanticErrorInfo / SemanticWarningInfo converted to DixError::Semantic
//!      via direct struct construction (preserves original error_id)
//!   4. All ErrorHandlingStrategy forced to Continue in LSP mode

use std::collections::HashMap;
use std::panic;

use dixscript::Compiler::AST::data_types::DebugMode;
use dixscript::Compiler::Core::{
    ConfigSectionHandler, ErrorHandlingStrategy, GeneralAstEnhancer,
    GeneralParser, GeneralSemanticAnalyzer, OperationalSettings,
};
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use dixscript::ErrorManager::{
    DixError, ErrorManager, ErrorSeverity,
    SemanticError, SemanticErrorType,
};

use crate::document::Document;

// ── Public entry point ────────────────────────────────────────────────────────

pub fn run_pipeline(doc: &mut Document) -> Vec<DixError> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        run_pipeline_inner(doc)
    }));
    match result {
        Ok(errors) => errors,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("Pipeline panicked for {}: {}", doc.uri, msg);
            doc.error_manager.get_all_errors_flat()
        }
    }
}

// ── Inner pipeline ────────────────────────────────────────────────────────────

fn run_pipeline_inner(doc: &mut Document) -> Vec<DixError> {
    let em = doc.error_manager.clone();

    // Collect all diagnostics regardless of what @CONFIG says.
    em.force_strategy(ErrorHandlingStrategy::Continue);

    // ── Stage 1: tokenize the FULL source ─────────────────────────────────────
    let tokenizer_settings = OperationalSettings {
        error_handling_strategy: ErrorHandlingStrategy::Continue,
        debug_mode: DebugMode::Off,
        ..OperationalSettings::default()
    };

    let tokenizer = Tokenizer::new_with_error_manager(
        &doc.source,
        &tokenizer_settings,
        em.clone(),
    );
    let token_result = tokenizer.tokenize();
    let all_tokens = token_result.tokens;

    tracing::debug!("Tokenized: {} tokens", all_tokens.len());

    if all_tokens.len() <= 1 {
        doc.tokens = all_tokens;
        return em.get_all_errors_flat();
    }

    // ── Stage 2: split @CONFIG and process it ────────────────────────────────
    let split = split_config_tokens(all_tokens.clone());

    let config_result = {
        let mut handler =
            ConfigSectionHandler::new_with_error_manager(None, em.clone());
        handler.process_config_tokens(&split.config_tokens)
    };

    // Override debug_mode — verbose logging blocks tokio in LSP.
    let mut operational_settings = config_result.operational_settings.clone();
    operational_settings.debug_mode = DebugMode::Off;
    // Always continue so every diagnostic is surfaced.
    operational_settings.error_handling_strategy = ErrorHandlingStrategy::Continue;

    // ── FIX: set source_file_path so ImportsResolver finds relative files ─────
    //
    // Without this the resolver has no base directory and every local import
    // fails with "file not found".  Non-file:// URIs (e.g. untitled:) cannot be
    // resolved, so we skip import resolution for them entirely.
    match doc.uri.to_file_path() {
        Ok(file_path) => {
            let path_str = file_path.to_string_lossy().into_owned();
            tracing::debug!("Source file path: {}", path_str);
            operational_settings.source_file_path = Some(path_str);
        }
        Err(_) => {
            tracing::debug!(
                "URI is not a file:// path — skipping import resolution: {}",
                doc.uri
            );
            operational_settings.skip_imports_resolution = true;
        }
    }

    // Store full token stream — all LSP features use this (including @CONFIG).
    doc.tokens = all_tokens;

    // ── Stage 3: parse ────────────────────────────────────────────────────────
    let parser = match GeneralParser::new_for_lsp(
        split.rest_tokens,
        &config_result.config_section,
        &operational_settings,
        em.clone(),
    ) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Parser construction failed: {}", e.message());
            return em.get_all_errors_flat();
        }
    };

    let ast = match parser.parse() {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("Parse failed: {}", e.message());
            return em.get_all_errors_flat();
        }
    };

    tracing::debug!("Parse complete");

    // ── FIX: skip cloud imports to prevent tokio-inside-async panic ───────────
    //
    // `ImportsResolver::download_cloud_file_sync` calls
    // `tokio::runtime::Runtime::new().block_on(...)`.  Inside a tokio worker
    // (spawn_blocking) this panics with "Cannot start a runtime from within a
    // runtime."  We detect cloud imports here and disable resolution for them.
    // Local imports are still resolved when source_file_path is set above.
    let has_cloud_imports = ast
        .imports
        .as_ref()
        .map(|imp| imp.imports.iter().any(|i| i.is_cloud_import))
        .unwrap_or(false);

    if has_cloud_imports {
        tracing::debug!(
            "Cloud imports present — disabling import resolution to prevent tokio panic"
        );
        operational_settings.skip_imports_resolution = true;
    }

    // ── Stage 4: semantic analysis ────────────────────────────────────────────
    let analyzer = GeneralSemanticAnalyzer::new_for_lsp(
        &ast,
        &operational_settings,
        em.clone(),
    );
    let semantic_result = analyzer.analyze();

    tracing::debug!(
        "Semantic analysis: success={}, errors={}, warnings={}",
        semantic_result.is_success,
        semantic_result.errors.len(),
        semantic_result.warnings.len(),
    );

    // ── Stage 5: AST enhancement ──────────────────────────────────────────────
    let enhancer = GeneralAstEnhancer::new_with_error_manager(
        &operational_settings,
        em.clone(),
    );
    let enhancement_result = enhancer.enhance(&ast, Some(&semantic_result));

    tracing::debug!(
        "Enhancement complete: {} enhancements applied",
        enhancement_result.total_enhancements
    );

    // ── Assemble diagnostics ──────────────────────────────────────────────────
    //
    // em holds: lexical, parse, imports-resolution errors added via add_*_error().
    // semantic_result holds: SemanticErrorInfo / SemanticWarningInfo produced by
    // section analyzers — these are NEVER forwarded to the ErrorManager by the
    // analyzers themselves (they call em.log_error / log_warning, which is
    // stderr-only, not add_semantic_error).  Convert them here.

    let mut all_errors = em.get_all_errors_flat();

    // Convert SemanticErrorInfo → DixError::Semantic (Error severity).
    // Direct struct construction preserves the analyzer's original error_id
    // (e.g. "DATA2C1" from DataSectionAnalyzer) rather than overwriting it.
    for err in &semantic_result.errors {
        let (line, col) = err
            .position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

        all_errors.push(DixError::Semantic(SemanticError {
            error_id: err.error_id.clone(),
            error_type: SemanticErrorType::InvalidConfiguration,
            message: err.message.clone(),
            line,
            column,
            section_name: if err.section_name.is_empty() {
                None
            } else {
                Some(err.section_name.clone())
            },
            suggestion: if err.suggestion.is_empty() {
                None
            } else {
                Some(err.suggestion.clone())
            },
            severity: ErrorSeverity::Error,
            quick_fixes: Vec::new(),
            metadata: HashMap::new(),
        }));
    }

    // Convert SemanticWarningInfo → DixError::Semantic (Warning severity).
    //
    // This is where ALL section-level warnings land:
    //   • SecuritySectionAnalyzer: xor weak, missing KDF, manual mode, etc.
    //   • DataSectionAnalyzer:     empty group arrays
    //   • DlmSectionAnalyzer:      ordering issues
    //   • QuickFuncsSectionAnalyzer: unused parameters, etc.
    //   • Missing @SECURITY when DEncryptor present (emitted as an *error* in
    //     semantic_result.errors above, not as a warning).
    for warn in &semantic_result.warnings {
        let (line, col) = warn
            .position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

        all_errors.push(DixError::Semantic(SemanticError {
            error_id: warn.warning_id.clone(),
            error_type: SemanticErrorType::InvalidConfiguration,
            message: warn.message.clone(),
            line,
            column,
            section_name: if warn.section_name.is_empty() {
                None
            } else {
                Some(warn.section_name.clone())
            },
            suggestion: None,
            severity: ErrorSeverity::Warning,
            quick_fixes: Vec::new(),
            metadata: HashMap::new(),
        }));
    }

    doc.ast = Some(ast);
    doc.semantic_result = Some(semantic_result);
    doc.enhancement_result = Some(enhancement_result);

    tracing::debug!("Pipeline complete: {} total diagnostics", all_errors.len());
    all_errors
    }
