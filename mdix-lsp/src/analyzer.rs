//! Runs the full DixScript pipeline for one document.
//!
//! Pipeline order (from lsp_call_order_contract in the project spec):
//!   ConfigSectionHandler -> Tokenizer -> GeneralParser ->
//!   GeneralSemanticAnalyzer -> GeneralAstEnhancer
//!
//! The ErrorManager is isolated per document.
//! force_strategy(Continue) is called immediately after ConfigSectionHandler
//! so that ALL errors are collected rather than stopping at the first one.

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
/// This function never panics — any stage that fails is logged and the
/// remaining stages are skipped gracefully.
pub fn run_pipeline(doc: &mut Document) -> Vec<DixError> {
    let em = doc.error_manager.clone();

    // Stage 1: extract @CONFIG, derive OperationalSettings.
    let mut config_handler =
        ConfigSectionHandler::new_with_error_manager(None, em.clone());
    let config_result = config_handler.process_config_section(&doc.source);

    // Force Continue so all stages run and all errors are collected,
    // regardless of what the file's @CONFIG requested.
    em.force_strategy(ErrorHandlingStrategy::Continue);

    let cleaned_source        = &config_result.cleaned_input_string;
    let operational_settings  = &config_result.operational_settings;

    // Stage 2: tokenize.
    let mut tokenizer = Tokenizer::new_with_error_manager(
        cleaned_source,
        operational_settings,
        em.clone(),
    );
    let token_result = tokenizer.tokenize();
    doc.tokens = token_result.tokens.clone();

    if token_result.tokens.is_empty() {
        return em.get_all_errors_flat();
    }

    // Stage 3: parse.
    //
    // new_for_lsp() disables rayon concurrent section parsing:
    // documents are small and sequential parsing avoids spawning rayon
    // work inside the tokio executor.
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

    // Stage 4: semantic analysis.
    let analyzer        = GeneralSemanticAnalyzer::new_with_error_manager(
        &ast,
        operational_settings,
        em.clone(),
    );
    let semantic_result = analyzer.analyze();

    // Stage 5: AST enhancement.
    let enhancer           = GeneralAstEnhancer::new_with_error_manager(
        operational_settings,
        em.clone(),
    );
    let enhancement_result = enhancer.enhance(&ast, Some(&semantic_result));

    // Persist results on the document for feature providers to read.
    doc.ast                = Some(ast);
    doc.semantic_result    = Some(semantic_result);
    doc.enhancement_result = Some(enhancement_result);

    em.get_all_errors_flat()
                    }
