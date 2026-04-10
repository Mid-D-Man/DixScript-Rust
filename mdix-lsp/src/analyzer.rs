// mdix-lsp/src/analyzer.rs

//! Runs the full DixScript pipeline for one document.
//!
//! ## @CONFIG position offset
//! `ConfigSectionHandler` strips @CONFIG from the source before tokenizing.
//! All token line/column values are therefore relative to the *cleaned* source.
//! We compute the line offset introduced by stripping @CONFIG and store it on
//! the document so feature providers (and future token position helpers) can
//! compensate when mapping back to the original source.
//!
//! Pipeline order:
//!   ConfigSectionHandler → Tokenizer → GeneralParser →
//!   GeneralSemanticAnalyzer → GeneralAstEnhancer

use dixscript::Compiler::Core::{
    ConfigSectionHandler, ErrorHandlingStrategy, GeneralAstEnhancer,
    GeneralParser, GeneralSemanticAnalyzer,
};
use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::ErrorManager::{DixError, ErrorManager};

use crate::document::Document;

/// Runs every compiler stage on `doc` and populates all derived fields.
/// Returns the flat list of errors for the caller to convert to diagnostics.
///
/// Never panics — any stage that fails is logged and subsequent stages are
/// skipped gracefully.
pub fn run_pipeline(doc: &mut Document) -> Vec<DixError> {
    let em = doc.error_manager.clone();

    // ── Stage 1: extract @CONFIG ──────────────────────────────────────────────
    let mut config_handler =
        ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config_result = config_handler.process_config_section(&doc.source);

    // Force Continue so all stages run and every error is collected,
    // regardless of what the file's @CONFIG requested.
    em.force_strategy(ErrorHandlingStrategy::Continue);

    let cleaned_source       = &config_result.cleaned_input_string;
    let operational_settings = &config_result.operational_settings;

    // Compute the line offset introduced by stripping @CONFIG.
    // When @CONFIG is absent the offset is 0 and nothing changes.
    // When @CONFIG spans N lines at the top of the file, all token positions
    // are N lines too low and must be shifted up by N when reporting to the
    // editor.  We store the offset on the document so hover / goto-definition
    // providers can compensate without re-doing the arithmetic.
    doc.config_line_offset = compute_config_line_offset(&doc.source, cleaned_source);

    // ── Stage 2: tokenize ─────────────────────────────────────────────────────
    let tokenizer = Tokenizer::new_with_error_manager(
        cleaned_source,
        operational_settings,
        em.clone(),
    );
    let token_result = tokenizer.tokenize();
    doc.tokens = token_result.tokens.clone();

    if token_result.tokens.is_empty() {
        return em.get_all_errors_flat();
    }

    // ── Stage 3: parse ────────────────────────────────────────────────────────
    let parser = match GeneralParser::new_for_lsp(
        token_result.tokens,
        &config_result.config_section,
        operational_settings,
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

    // ── Stage 4: semantic analysis ────────────────────────────────────────────
    let analyzer        = GeneralSemanticAnalyzer::new_with_error_manager(
        &ast,
        operational_settings,
        em.clone(),
    );
    let semantic_result = analyzer.analyze();

    // ── Stage 5: AST enhancement ──────────────────────────────────────────────
    let enhancer           = GeneralAstEnhancer::new_with_error_manager(
        operational_settings,
        em.clone(),
    );
    let enhancement_result = enhancer.enhance(&ast, Some(&semantic_result));

    doc.ast                = Some(ast);
    doc.semantic_result    = Some(semantic_result);
    doc.enhancement_result = Some(enhancement_result);

    em.get_all_errors_flat()
}

/// Returns the number of lines that were removed from the front of `original`
/// when `ConfigSectionHandler` produced `cleaned`.
///
/// We count from the start of both strings until the content diverges.
/// If @CONFIG was not present, or was at the very end, the offset is 0.
fn compute_config_line_offset(original: &str, cleaned: &str) -> usize {
    let original_lines = original.lines().count();
    let cleaned_lines  = cleaned.lines().count();
    original_lines.saturating_sub(cleaned_lines)
}