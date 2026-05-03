// mdix-lsp/src/analyzer.rs
//! Pipeline runner.
//!
//! KEY INVARIANT: The file's @CONFIG `debug_mode` is for the CLI compiler, NOT
//! for the LSP. Running verbose mode inside spawn_blocking floods stderr,
//! causes pipe backpressure, blocks tokio, and prevents shutdown responses —
//! which is why LSP4IJ keeps timing out and restarting the server.
//!
//! We always force `DebugMode::Off` in LSP mode. LSP debug goes to RUST_LOG.

use std::panic;
use std::sync::Mutex;

use dixscript::Compiler::AST::data_types::DebugMode;
use dixscript::Compiler::Core::{
    ConfigSectionHandler, ErrorHandlingStrategy, GeneralAstEnhancer,
    GeneralParser, GeneralSemanticAnalyzer,
};
use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::ErrorManager::DixError;

use crate::document::Document;

static CONFIG_INIT_LOCK: Mutex<()> = Mutex::new(());

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

    // ── Stage 1: extract @CONFIG ──────────────────────────────────────────
    let config_result = {
        let _guard = CONFIG_INIT_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
        handler.process_config_section(&doc.source)
    };

    // Force Continue so all stages run and collect all diagnostics.
    em.force_strategy(ErrorHandlingStrategy::Continue);

    // ── CRITICAL: Override debug_mode for LSP ─────────────────────────────
    //
    // The user's @CONFIG `debug_mode -> "verbose"` is for the CLI compiler.
    // In LSP mode, verbose logging goes to stderr from inside spawn_blocking
    // threads. When stderr pipe fills up, writes block → tokio stalls →
    // shutdown request never gets a response → LSP4IJ timeout → server killed
    // → server restarted → cycle repeats (the 19 "initialized" messages).
    //
    // Solution: always silence the pipeline. LSP debug goes via RUST_LOG.
    let mut operational_settings = config_result.operational_settings.clone();
    operational_settings.debug_mode = DebugMode::Off;

    let cleaned_source = &config_result.cleaned_input_string;

    // Detect and store the @CONFIG line range so hover/completions work
    // even though @CONFIG tokens don't exist in the token stream.
    doc.config_line_range = detect_config_line_range(&doc.source);
    doc.config_line_offset = 0;

    tracing::debug!(
        "Config range: {:?}, settings: {:?}",
        doc.config_line_range,
        operational_settings.error_handling_strategy
    );

    // ── Stage 2: tokenize ─────────────────────────────────────────────────
    let tokenizer = Tokenizer::new_with_error_manager(
        cleaned_source,
        &operational_settings,
        em.clone(),
    );
    let token_result = tokenizer.tokenize();
    doc.tokens = token_result.tokens.clone();

    tracing::debug!("Tokenized: {} tokens", doc.tokens.len());

    if token_result.tokens.is_empty() {
        return em.get_all_errors_flat();
    }

    // ── Stage 3: parse ────────────────────────────────────────────────────
    let parser = match GeneralParser::new_for_lsp(
        token_result.tokens,
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

    // ── Stage 4: semantic analysis ────────────────────────────────────────
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

    // ── Stage 5: AST enhancement ──────────────────────────────────────────
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

/// Detect the 0-based LSP line range of the @CONFIG section by scanning
/// the original source. Returns (start_line, end_line) inclusive.
///
/// This is necessary because @CONFIG is replaced with blank lines before
/// tokenisation, so NO tokens carry SectionId::Config. Position-based
/// detection is the only reliable way to answer hover/completion requests
/// for lines inside the @CONFIG block.
pub fn detect_config_line_range(source: &str) -> Option<(u32, u32)> {
    let mut start_line: Option<u32> = None;
    let mut paren_depth: i32 = 0;
    let mut in_string = false;
    let mut string_char = '\0';
    let mut in_line_comment = false;

    for (line_idx, line) in source.lines().enumerate() {
        let line_upper = line.trim().to_uppercase();

        // Detect @CONFIG start.
        if start_line.is_none()
            && (line_upper.starts_with("@CONFIG") || line_upper.starts_with("@ CONFIG"))
        {
            start_line = Some(line_idx as u32);
        }

        if start_line.is_none() {
            continue;
        }

        // Count parentheses, respecting strings and comments.
        in_line_comment = false;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied().unwrap_or('\0');

            if in_line_comment { break; }

            if !in_string && c == '/' && next == '/' {
                in_line_comment = true;
                break;
            }

            if !in_string && (c == '"' || c == '\'') {
                in_string = true;
                string_char = c;
            } else if in_string && c == '\\' {
                i += 1; // skip escaped char
            } else if in_string && c == string_char {
                in_string = false;
                string_char = '\0';
            } else if !in_string {
                if c == '(' { paren_depth += 1; }
                else if c == ')' {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        return Some((start_line.unwrap(), line_idx as u32));
                    }
                }
            }
            i += 1;
        }
    }

    // If we started but never closed (malformed), return what we have.
    start_line.map(|s| (s, s))
}
