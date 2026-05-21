// mdix-lsp/src/analyzer.rs
//! Pipeline runner — Approach B (tokenizer-first).
//!
//! Full source goes to the tokenizer first. Config tokens are split off and
//! routed to ConfigSectionHandler::process_config_tokens, which produces the
//! ConfigSection AST and real OperationalSettings with accurate positions
//! directly from the token stream.
//!
//! doc.tokens stores the FULL token stream (including @CONFIG tokens) so that
//! all LSP features see the complete file with correct positions.
//!
//! KEY INVARIANT: debug_mode from @CONFIG is for the CLI compiler, not the LSP.
//! We always override to DebugMode::Off after processing config.
//!
//! FIX (semantic errors/warnings): SemanticErrorInfo / SemanticWarningInfo objects
//! produced by the section analyzers live in SemanticAnalysisResult.errors/warnings
//! but are never forwarded to the ErrorManager as DixError objects.  They are
//! collected here and appended to the final diagnostic list so they appear in the
//! editor as proper squiggles.
//!
//! FIX (imports): operational_settings.source_file_path must be set from the
//! document URI so that ImportsResolver can locate relative .mdix files.

use std::panic;

use dixscript::Compiler::AST::data_types::DebugMode;
use dixscript::Compiler::Core::{
    ConfigSectionHandler, ErrorHandlingStrategy, GeneralAstEnhancer,
    GeneralParser, GeneralSemanticAnalyzer, OperationalSettings,
};
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use dixscript::ErrorManager::{DixError, ErrorSeverity};

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

    // Force Continue so all stages run and every diagnostic is collected.
    em.force_strategy(ErrorHandlingStrategy::Continue);

    // ── Stage 1: tokenize the FULL source ─────────────────────────────────────
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

    // Keep the full stream — LSP features need all tokens including @CONFIG.
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

    // Override debug_mode for LSP — verbose logging blocks tokio.
    let mut operational_settings = config_result.operational_settings.clone();
    operational_settings.debug_mode = DebugMode::Off;

    // ── FIX: set source file path so ImportsResolver can find relative files ──
    // Without this, every @IMPORTS entry fails with "file not found" because
    // the resolver has no base directory to resolve relative paths against.
    if let Ok(file_path) = doc.uri.to_file_path() {
        let path_str = file_path.to_string_lossy().into_owned();
        operational_settings.source_file_path = Some(path_str);
        tracing::debug!(
            "Source file path set: {:?}",
            operational_settings.source_file_path
        );
    } else {
        tracing::debug!(
            "URI is not a file:// URI; import resolution may not work: {}",
            doc.uri
        );
    }

    tracing::debug!(
        "Config processed: strategy={:?} version={}",
        operational_settings.error_handling_strategy,
        operational_settings.version,
    );

    // Store the full token stream before we move rest_tokens into the parser.
    doc.tokens = all_tokens;

    // ── Stage 3: parse ────────────────────────────────────────────────────────
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
        "Enhancement complete: {} enhancements",
        enhancement_result.total_enhancements
    );

    // ── Assemble final diagnostic list ────────────────────────────────────────
    //
    // The error manager holds lexical, parse, and imports-resolution errors.
    // Semantic errors/warnings are tracked separately in SemanticAnalysisResult
    // and are NEVER forwarded to the ErrorManager by the section analyzers —
    // they call error_manager.log_error() (stderr-only) rather than
    // add_semantic_error().  We convert them here so they appear as editor
    // squiggles.
    //
    // We do this BEFORE moving semantic_result into doc so we can borrow it.

    let mut all_errors = em.get_all_errors_flat();

    // ── Convert SemanticErrorInfo → DixError (Error severity) ─────────────────
    for err in &semantic_result.errors {
        let (line, col) = err
            .position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

        all_errors.push(semantic_info_to_dix_error(
            err.error_id.clone(),
            err.message.clone(),
            if err.suggestion.is_empty() {
                None
            } else {
                Some(err.suggestion.clone())
            },
            ErrorSeverity::Error,
            line,
            col,
        ));
    }

    // ── Convert SemanticWarningInfo → DixError (Warning severity) ─────────────
    //
    // This covers ALL warnings:
    //   • SecuritySectionAnalyzer:  xor is weak, missing KDF fields, manual mode, etc.
    //   • DataSectionAnalyzer:      empty group arrays
    //   • DlmSectionAnalyzer:       module ordering issues
    //   • QuickFuncsSectionAnalyzer: unused parameters, etc.
    //   • Phase 8 (missing @SECURITY when DEncryptor is present) — this is an
    //     *error*, already covered in semantic_result.errors above.
    for warn in &semantic_result.warnings {
        let (line, col) = warn
            .position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

        all_errors.push(semantic_info_to_dix_error(
            warn.warning_id.clone(),
            warn.message.clone(),
            None,
            ErrorSeverity::Warning,
            line,
            col,
        ));
    }

    // Store pipeline results.
    doc.ast                = Some(ast);
    doc.semantic_result    = Some(semantic_result);
    doc.enhancement_result = Some(enhancement_result);

    tracing::debug!(
        "Pipeline complete: {} total diagnostics ({} from em, {} semantic)",
        all_errors.len(),
        em.get_all_errors_flat().len(),
        all_errors.len().saturating_sub(em.get_all_errors_flat().len()),
    );

    all_errors
}

// ── Semantic → DixError conversion ───────────────────────────────────────────
//
// SemanticErrorInfo / SemanticWarningInfo are compiler-internal structs.
// DixError::Semantic wraps the ErrorManager's own SemanticError struct.
//
// ASSUMPTION: dixscript::ErrorManager exports `SemanticError` with at least the
// fields accessed by converters.rs: error_id, message, suggestion, severity,
// line, column.  These are all public because converters.rs (in this crate)
// accesses them directly.
//
// If this function fails to compile, inspect dixscript/src/ErrorManager/ for
// the exact struct name and field list, then update the construction below.
// As a fallback you can replace the body with a DixError::General variant,
// which always renders at position 0:0 but at least surfaces the message.
fn semantic_info_to_dix_error(
    error_id:   String,
    message:    String,
    suggestion: Option<String>,
    severity:   ErrorSeverity,
    line:       i32,
    column:     i32,
) -> DixError {
    use dixscript::ErrorManager::SemanticError;

    DixError::Semantic(SemanticError {
        error_id,
        message,
        suggestion,
        severity,
        line,
        column,
    })
        }
