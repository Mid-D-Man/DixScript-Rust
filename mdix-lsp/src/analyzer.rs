// mdix-lsp/src/analyzer.rs
//! Pipeline runner — Approach B (tokenizer-first).
//!
//! Full source goes to the tokenizer first. Config tokens are split off and
//! routed to ConfigSectionHandler::process_config_tokens, which produces the
//! ConfigSection AST and real OperationalSettings with accurate positions
//! directly from the token stream — no source stripping, no position fixup,
//! no synthetic token generation in the LSP features.
//!
//! doc.tokens stores the FULL token stream (including @CONFIG tokens) so that
//! all LSP features (hover, completions, semantic tokens, folding, etc.) see
//! the complete file with correct positions.
//!
//! KEY INVARIANT: debug_mode from @CONFIG is for the CLI compiler, not the LSP.
//! We always override to DebugMode::Off after processing config. LSP debug
//! output goes via RUST_LOG / tracing.

use std::panic;

use dixscript::Compiler::AST::data_types::DebugMode;
use dixscript::Compiler::Core::{
    ConfigSectionHandler, ErrorHandlingStrategy, GeneralAstEnhancer,
    GeneralParser, GeneralSemanticAnalyzer, OperationalSettings,
};
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use dixscript::ErrorManager::DixError;

use crate::document::Document;

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

fn run_pipeline_inner(doc: &mut Document) -> Vec<DixError> {
    let em = doc.error_manager.clone();

    // Force Continue so all stages run and collect all diagnostics regardless
    // of what the file's @CONFIG specifies for error_handling.
    em.force_strategy(ErrorHandlingStrategy::Continue);

    // ── Stage 1: tokenize the FULL source ────────────────────────────────────
    // Use a safe default OpSettings for the tokenizer pass: Continue + Off.
    // Real OpSettings come from config processing in Stage 2.
    let tokenizer_settings = OperationalSettings {
        error_handling_strategy: ErrorHandlingStrategy::Continue,
        debug_mode:              DebugMode::Off,
        ..OperationalSettings::default()
    };

    let tokenizer = Tokenizer::new_with_error_manager(
        &doc.source,
        &tokenizer_settings,
        em.clone(),
    );
    let token_result = tokenizer.tokenize();

    // Keep the full stream for doc.tokens — LSP features need all tokens
    // including @CONFIG tokens, with their real positions.
    let all_tokens = token_result.tokens;

    tracing::debug!("Tokenized: {} tokens", all_tokens.len());

    if all_tokens.len() <= 1 {
        // Only EOF — nothing to do.
        doc.tokens = all_tokens;
        return em.get_all_errors_flat();
    }

    // ── Stage 2: split @CONFIG and process it ────────────────────────────────
    // split_config_tokens consumes the vec — clone so doc.tokens keeps the full
    // stream. A typical .mdix file has a few hundred tokens; this is cheap.
    let split = split_config_tokens(all_tokens.clone());

    let config_result = {
        let mut handler =
            ConfigSectionHandler::new_with_error_manager(None, em.clone());
        handler.process_config_tokens(&split.config_tokens)
    };

    // ── CRITICAL: override debug_mode for LSP ────────────────────────────────
    // The user's @CONFIG `debug_mode -> "verbose"` is for the CLI compiler.
    // Verbose logging inside spawn_blocking fills stderr, blocks tokio, and
    // causes shutdown timeouts. Always silence the pipeline in LSP mode.
    let mut operational_settings = config_result.operational_settings.clone();
    operational_settings.debug_mode = DebugMode::Off;

    tracing::debug!(
        "Config processed: strategy={:?} version={}",
        operational_settings.error_handling_strategy,
        operational_settings.version,
    );

    // Store the full token stream before we move rest_tokens into the parser.
    doc.tokens = all_tokens;

    // ── Stage 3: parse ────────────────────────────────────────────────────────
    // rest_tokens is everything except the @CONFIG block — exactly what
    // GeneralParser expects (it receives the ConfigSection separately).
    let parser = match GeneralParser::new_for_lsp(
        split.rest_tokens,
        &config_result.config_section,
        &operational_settings,
        em.clone(),
    ) {
        Ok(p)  => p,
        Err(e) => {
            tracing::warn!("Parser construction failed: {}", e.message());
            return em.get_all_errors_flat();
        }
    };

    let ast = match parser.parse() {
        Ok(a)  => a,
        Err(e) => {
            tracing::warn!("Parse failed: {}", e.message());
            return em.get_all_errors_flat();
        }
    };

    tracing::debug!("Parse complete");

    // ── Stage 4: semantic analysis ────────────────────────────────────────────
    let analyzer = GeneralSemanticAnalyzer::new_for_lsp(
        &ast,
        &operational_settings,
        em.clone(),
    );
    let semantic_result = analyzer.analyze();

    tracing::debug!(
        "Semantic analysis: success={}, errors={}",
        semantic_result.is_success,
        semantic_result.errors.len()
    );

    // ── Stage 5: AST enhancement ──────────────────────────────────────────────
    let enhancer = GeneralAstEnhancer::new_with_error_manager(
        &operational_settings,
        em.clone(),
    );
    let enhancement_result = enhancer.enhance(&ast, Some(&semantic_result));

    tracing::debug!(
        "Enhancement complete: {} enhancements",
        enhancement_result.total_enhancements
    );

    doc.ast                = Some(ast);
    doc.semantic_result    = Some(semantic_result);
    doc.enhancement_result = Some(enhancement_result);

    let errors = em.get_all_errors_flat();
    tracing::debug!("Pipeline complete: {} errors/warnings", errors.len());
    errors
}
