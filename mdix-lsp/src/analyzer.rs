// mdix-lsp/src/analyzer.rs
use std::panic;
use std::sync::Mutex;

use dixscript::Compiler::Core::{
    ConfigSectionHandler, ErrorHandlingStrategy, GeneralAstEnhancer,
    GeneralParser, GeneralSemanticAnalyzer,
};
use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::ErrorManager::{DixError, ErrorManager};

use crate::document::Document;

/// Serializes the config-section phase across all concurrent analyses.
///
/// `ConfigSectionHandler::initialize_singletons` calls
/// `VersionManager::initialize()`.  Although that function is now idempotent
/// (read-first, write only on version change), we still serialize this phase
/// to avoid contention on the very first document open when multiple files
/// open simultaneously before the singleton has been seeded.
///
/// Only the config phase is serialized; the rest of the pipeline
/// (tokenize, parse, semantic, enhance) runs freely in parallel.
static CONFIG_INIT_LOCK: Mutex<()> = Mutex::new(());

/// Runs every compiler stage on `doc` and populates all derived fields.
///
/// Wrapped in `catch_unwind` so a pipeline panic never kills the server process.
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
                .unwrap_or_else(|| "unknown panic payload".to_string());
            tracing::error!("Pipeline panicked for {}: {}", doc.uri, msg);
            // Return whatever errors were collected before the panic.
            doc.error_manager.get_all_errors_flat()
        }
    }
}

fn run_pipeline_inner(doc: &mut Document) -> Vec<DixError> {
    let em = doc.error_manager.clone();

    // ── Stage 1: extract @CONFIG (serialized — seeds VersionManager) ──────
    //
    // ConfigSectionHandler replaces the @CONFIG block in the source with an
    // equal number of blank lines.  This means:
    //   • No SectionConfig tokens are ever produced by the tokenizer.
    //   • Every token retains its original 1-based line number.
    //   • No position offset is required anywhere downstream.
    let config_result = {
        let _guard = CONFIG_INIT_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut config_handler =
            ConfigSectionHandler::new_with_error_manager(None, em.clone());
        config_handler.process_config_section(&doc.source)
    };

    // Force Continue so every subsequent stage runs and collects all diagnostics.
    em.force_strategy(ErrorHandlingStrategy::Continue);

    // cleaned_input_string is the source with @CONFIG replaced by blank lines
    // (or the original source if no @CONFIG was present).  Token positions
    // from this source already match the original file — no offset needed.
    let cleaned_source       = &config_result.cleaned_input_string;
    let operational_settings = &config_result.operational_settings;

    // config_line_offset is 0 because line numbers are already correct.
    doc.config_line_offset = 0;

    // ── Stage 2: tokenize ─────────────────────────────────────────────────
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

    // ── Stage 3: parse ────────────────────────────────────────────────────
    //
    // The token stream contains no SectionConfig tokens (blank-line
    // replacement ensures this).  GeneralParser receives the pre-parsed
    // ConfigSection directly and populates script.config from it.
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

    // ── Stage 4: semantic analysis ────────────────────────────────────────
    let analyzer        = GeneralSemanticAnalyzer::new_with_error_manager(
        &ast,
        operational_settings,
        em.clone(),
    );
    let semantic_result = analyzer.analyze();

    // ── Stage 5: AST enhancement ──────────────────────────────────────────
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