// mdix-lsp/src/analyzer.rs
//! Pipeline runner — Approach B (tokenizer-first).
//!
//! SECURITY DIAGNOSTIC FIX:
//!   When @DLM contains DEncryptor and @SECURITY is absent, the diagnostic
//!   is emitted with the position of the @DLM token (not 0:0) so the squiggly
//!   appears on the correct line and the light-bulb fires reliably.
//!
//! DAuditor:
//!   DAuditor is informational — no error is emitted when it is present without
//!   a security section. An info-level note is logged only.
//!
//! ENHANCED-AST FIX (2025):
//!   doc.ast now stores the POST-enhancement AST so that all LSP features
//!   (inlay hints, hover, highlights, goto-definition) see resolved
//!   QualifiedIdentifier nodes and accurate type information.

use std::collections::HashMap;
use std::panic;

use dixscript::Compiler::AST::data_types::DebugMode;
use dixscript::Compiler::Core::{
    ConfigSectionHandler, ErrorHandlingStrategy, GeneralAstEnhancer,
    GeneralParser, GeneralSemanticAnalyzer, OperationalSettings,
};
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::AST::data_types::{DLMModuleType, DLMModuleSubtype};
use dixscript::ErrorManager::{
    DixError, ErrorManager, ErrorSeverity,
    SemanticError, SemanticErrorType,
};

use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

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

    em.force_strategy(ErrorHandlingStrategy::Continue);

    // ── Stage 1: tokenize ─────────────────────────────────────────────────────
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

    // ── Stage 2: split @CONFIG ────────────────────────────────────────────────
    let split = split_config_tokens(all_tokens.clone());

    let config_result = {
        let mut handler = ConfigSectionHandler::new_with_error_manager(None, em.clone());
        handler.process_config_tokens(&split.config_tokens)
    };

    let mut operational_settings = config_result.operational_settings.clone();
    operational_settings.debug_mode = DebugMode::Off;
    operational_settings.error_handling_strategy = ErrorHandlingStrategy::Continue;

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
    let has_cloud_imports = ast
        .imports
        .as_ref()
        .map(|imp| imp.imports.iter().any(|i| i.is_cloud_import))
        .unwrap_or(false);

    if has_cloud_imports {
        tracing::debug!("Cloud imports present — disabling import resolution");
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

    // ── Stage 6: DLM security validation (LSP-side, with correct positions) ───
    //
    // The core semantic analyzer may emit the "missing @SECURITY" error at 0:0
    // because it does not have access to the token stream for position lookup.
    // We re-emit it here with the exact @DLM line so the squiggly lands correctly.
    let mut all_errors = em.get_all_errors_flat();

    // Remove any 0:0 "security missing" errors emitted by the core analyzer —
    // we will replace them with correctly-positioned ones below.
    all_errors.retain(|e| {
        let msg = e.message().to_lowercase();
        let is_security_missing = msg.contains("security") && msg.contains("missing")
            || msg.contains("@security section is required")
            || msg.contains("encryptor requires");
        if is_security_missing {
            if let DixError::Semantic(se) = e {
                return se.line > 0;
            }
        }
        true
    });

    // Now emit a correctly-positioned diagnostic for each encryptor without security.
    // Use the original ast for DLM checks (enhancement doesn't touch DLM/SECURITY).
    let dlm_encryptors = collect_encryptors(&ast);
    if !dlm_encryptors.is_empty() && ast.security.is_none() {
        for (enc_line, enc_col, algorithm) in dlm_encryptors {
            let (diag_line, diag_col) = find_dlm_token_pos(&doc.tokens)
                .unwrap_or((enc_line, enc_col));

            all_errors.push(DixError::Semantic(SemanticError {
                error_id:    "SEC001".to_string(),
                error_type:  SemanticErrorType::InvalidConfiguration,
                message:     format!(
                    "@SECURITY section is required when DEncryptor.{} is present in @DLM.",
                    algorithm
                ),
                line:        diag_line as i32,
                column:      diag_col as i32,
                section_name: Some("DLM".to_string()),
                suggestion:  Some(
                    "Add an @SECURITY section with encryption -> { mode = \"keyfile\" or \"password\", algorithm = \"...\" }".to_string()
                ),
                severity:    ErrorSeverity::Error,
                quick_fixes: Vec::new(),
                metadata:    HashMap::new(),
            }));
        }
    }

    // DAuditor: informational only
    if let Some(dlm) = &ast.dlm {
        for m in &dlm.modules {
            if matches!(m.module_type, DLMModuleType::DAuditor) {
                let subtype = m.subtype.map(|s| format!("{}", s)).unwrap_or_else(|| "?".to_string());
                tracing::debug!("DAuditor module active: {}", subtype);
            }
        }
    }

    // ── Convert semantic_result errors/warnings ───────────────────────────────
    for err in &semantic_result.errors {
        let (line, col) = err
            .position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

        let msg_lower = err.message.to_lowercase();
        if line == 0
            && (msg_lower.contains("security") && msg_lower.contains("missing")
                || msg_lower.contains("@security section is required"))
        {
            continue;
        }

        all_errors.push(DixError::Semantic(SemanticError {
            error_id:    err.error_id.clone(),
            error_type:  SemanticErrorType::InvalidConfiguration,
            message:     err.message.clone(),
            line,
            column:      col,
            section_name: if err.section_name.is_empty() {
                None
            } else {
                Some(err.section_name.clone())
            },
            suggestion:  if err.suggestion.is_empty() {
                None
            } else {
                Some(err.suggestion.clone())
            },
            severity:    ErrorSeverity::Error,
            quick_fixes: Vec::new(),
            metadata:    HashMap::new(),
        }));
    }

    for warn in &semantic_result.warnings {
        let (line, col) = warn
            .position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

        all_errors.push(DixError::Semantic(SemanticError {
            error_id:    warn.warning_id.clone(),
            error_type:  SemanticErrorType::InvalidConfiguration,
            message:     warn.message.clone(),
            line,
            column:      col,
            section_name: if warn.section_name.is_empty() {
                None
            } else {
                Some(warn.section_name.clone())
            },
            suggestion:  None,
            severity:    ErrorSeverity::Warning,
            quick_fixes: Vec::new(),
            metadata:    HashMap::new(),
        }));
    }

    // ── Store state ───────────────────────────────────────────────────────────
    //
    // IMPORTANT: store the POST-enhancement AST, not the original.
    // The enhancer resolves QualifiedIdentifier nodes into their concrete
    // forms (EnumAccess, ImportedFunctionCall, PropertyAccess …), so type
    // inference and all LSP features are accurate only on the enhanced tree.
    let enhanced_ast = enhancement_result.enhanced_ast.clone();
    doc.ast = Some(enhanced_ast);
    doc.semantic_result = Some(semantic_result);
    doc.enhancement_result = Some(enhancement_result);

    tracing::debug!("Pipeline complete: {} total diagnostics", all_errors.len());
    all_errors
}

// ── DLM helpers ───────────────────────────────────────────────────────────────

/// Returns (line_1based, col_1based, algorithm_name) for every DEncryptor module.
fn collect_encryptors(ast: &dixscript::Compiler::AST::DixScript) -> Vec<(usize, usize, String)> {
    let dlm = match &ast.dlm { Some(d) => d, None => return vec![] };
    dlm.modules
        .iter()
        .filter(|m| matches!(m.module_type, DLMModuleType::DEncryptor))
        .map(|m| {
            let algo = match m.subtype {
                Some(DLMModuleSubtype::Aes128)   => "aes128",
                Some(DLMModuleSubtype::Aes256)   => "aes256",
                Some(DLMModuleSubtype::Chacha20)  => "chacha20",
                Some(DLMModuleSubtype::Xor)       => "xor",
                _                                 => "aes256",
            };
            let line = if m.position.is_valid() { m.position.line } else { 0 };
            let col  = if m.position.is_valid() { m.position.column } else { 0 };
            (line, col, algo.to_string())
        })
        .collect()
}

/// Find the 1-based (line, col) of the @DLM section keyword token.
fn find_dlm_token_pos(tokens: &[dixscript::Compiler::Core::Tokenizer::Token]) -> Option<(usize, usize)> {
    tokens
        .iter()
        .find(|t| matches!(t.token_type, TokenType::SectionDLM))
        .map(|t| (t.line, t.column))
    }
