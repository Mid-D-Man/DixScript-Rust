// mdix-lsp/src/analyzer.rs
//! Pipeline runner — Approach B (tokenizer-first).
//!
//! SECURITY DIAGNOSTIC:
//!   When @DLM contains DEncryptor and @SECURITY is absent, we always emit
//!   one correctly-positioned diagnostic per encryptor module, anchored to the
//!   @DLM section-keyword token.  All security-missing errors from the core
//!   semantic analyzer are removed first to prevent duplicates.
//!
//! DAuditor:
//!   Informational only — no error is emitted when present without @SECURITY.

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
        debug_mode:              DebugMode::Off,
        ..OperationalSettings::default()
    };

    let tokenizer = Tokenizer::new_with_error_manager(
        &doc.source,
        &tokenizer_settings,
        em.clone(),
    );
    let token_result = tokenizer.tokenize();
    let all_tokens   = token_result.tokens;

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
    operational_settings.debug_mode              = DebugMode::Off;
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

    // Skip cloud imports to prevent tokio-inside-async panic
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

    // ── Stage 6: build the diagnostic list ────────────────────────────────────

    // Start with all errors collected by the ErrorManager during the pipeline.
    let mut all_errors = em.get_all_errors_flat();

    // Remove EVERY security-missing diagnostic the core may have emitted —
    // regardless of line number — so we can re-emit exactly one per encryptor
    // at the correct (token-derived) position.
    all_errors.retain(|e| !is_security_missing_error(e));

    // ── Security validation: DEncryptor present without @SECURITY ─────────────
    //
    // We use the pre-enhancement AST here (DLM/SECURITY sections are never
    // touched by the enhancer, so both ASTs are identical for this check).
    let dlm_encryptors = collect_encryptors(&ast);

    if !dlm_encryptors.is_empty() && ast.security.is_none() {
        // Anchor the diagnostic to the @DLM section-keyword token so the
        // squiggly appears on the right line. Fall back to (1, 1) — the very
        // start of the file — rather than (0, 0) which some editors clip.
        let (diag_line, diag_col) =
            find_section_token_pos(&doc.tokens, TokenType::SectionDLM)
                .unwrap_or((1, 1));

        for algorithm in &dlm_encryptors {
            all_errors.push(DixError::Semantic(SemanticError {
                error_id:     "SEC001".to_string(),
                error_type:   SemanticErrorType::InvalidConfiguration,
                message:      format!(
                    "@SECURITY section is required when DEncryptor.{} is present in @DLM.",
                    algorithm
                ),
                line:         diag_line as i32,
                column:       diag_col  as i32,
                section_name: Some("DLM".to_string()),
                suggestion:   Some(
                    "Add an @SECURITY section: \
                     encryption -> { mode = \"keyfile\", algorithm = \"...\" }"
                        .to_string(),
                ),
                severity:     ErrorSeverity::Error,
                quick_fixes:  Vec::new(),
                metadata:     HashMap::new(),
            }));
        }

        tracing::debug!(
            "Emitted {} SEC001 diagnostics at {}:{}",
            dlm_encryptors.len(),
            diag_line,
            diag_col
        );
    }

    // DAuditor: informational only (no error)
    if let Some(dlm) = &ast.dlm {
        for m in &dlm.modules {
            if matches!(m.module_type, DLMModuleType::DAuditor) {
                let subtype = m.subtype
                    .map(|s| format!("{}", s))
                    .unwrap_or_else(|| "?".to_string());
                tracing::debug!("DAuditor module active: {}", subtype);
            }
        }
    }

    // ── Convert semantic_result errors/warnings ───────────────────────────────
    for err in &semantic_result.errors {
        // Skip security-missing messages — already handled above.
        if is_security_missing_str(&err.message) {
            continue;
        }

        let (line, col) = err
            .position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

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
            suggestion: if err.suggestion.is_empty() {
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
        if is_security_missing_str(&warn.message) {
            continue;
        }

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
    // Store the POST-enhancement AST so all LSP features see resolved nodes.
    let enhanced_ast = enhancement_result.enhanced_ast.clone();
    doc.ast              = Some(enhanced_ast);
    doc.semantic_result  = Some(semantic_result);
    doc.enhancement_result = Some(enhancement_result);

    tracing::debug!("Pipeline complete: {} total diagnostics", all_errors.len());
    all_errors
}

// ── Security helpers ──────────────────────────────────────────────────────────

/// Returns true when the error is a "security section missing" diagnostic
/// regardless of which pipeline phase produced it or what line it sits on.
fn is_security_missing_error(e: &DixError) -> bool {
    is_security_missing_str(e.message())
}

fn is_security_missing_str(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    (lower.contains("security") && lower.contains("missing"))
        || lower.contains("@security section is required")
        || lower.contains("encryptor requires")
        || lower.contains("sec001")
}

/// Returns the algorithm label for every DEncryptor module in @DLM.
/// Returns an empty Vec when @DLM is absent or has no encryptors.
fn collect_encryptors(ast: &dixscript::Compiler::AST::DixScript) -> Vec<String> {
    let dlm = match &ast.dlm {
        Some(d) => d,
        None    => return vec![],
    };
    dlm.modules
        .iter()
        .filter(|m| matches!(m.module_type, DLMModuleType::DEncryptor))
        .map(|m| match m.subtype {
            Some(DLMModuleSubtype::Aes128)  => "aes128".to_string(),
            Some(DLMModuleSubtype::Aes256)  => "aes256".to_string(),
            Some(DLMModuleSubtype::Chacha20) => "chacha20".to_string(),
            Some(DLMModuleSubtype::Xor)      => "xor".to_string(),
            _                                => "aes256".to_string(),
        })
        .collect()
}

/// Find the 1-based (line, column) of a specific section-keyword token.
/// Returns `None` if the token is not present in the stream.
fn find_section_token_pos(
    tokens:       &[dixscript::Compiler::Core::Tokenizer::Token],
    target_type:  TokenType,
) -> Option<(usize, usize)> {
    // TokenType doesn't implement PartialEq for all variants, so we use a
    // discriminant-based comparison for section keywords.
    let is_match = |tt: &TokenType| -> bool {
        matches!(
            (&target_type, tt),
            (TokenType::SectionDLM,        TokenType::SectionDLM)
            | (TokenType::SectionConfig,   TokenType::SectionConfig)
            | (TokenType::SectionData,     TokenType::SectionData)
            | (TokenType::SectionEnums,    TokenType::SectionEnums)
            | (TokenType::SectionImports,  TokenType::SectionImports)
            | (TokenType::SectionQuickFuncs, TokenType::SectionQuickFuncs)
            | (TokenType::SectionSecurity, TokenType::SectionSecurity)
        )
    };

    tokens
        .iter()
        .find(|t| is_match(&t.token_type))
        .map(|t| (t.line, t.column))
}
