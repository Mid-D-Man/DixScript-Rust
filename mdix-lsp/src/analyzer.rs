// mdix-lsp/src/analyzer.rs
//! Pipeline runner — Approach B (tokenizer-first).
//!
//! Cloud import handling:
//!   - If ALL cloud imports are already cached locally → proceed with full
//!     import resolution (fast, no network I/O — just filesystem reads).
//!   - If ANY cloud import is NOT cached → disable import resolution and
//!     emit a single Info diagnostic directing the developer to run
//!     `mdix compile` once to populate the cache.

use std::collections::HashMap;
use std::panic;

use dixscript::Compiler::AST::data_types::DebugMode;
use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::{
    ConfigSectionHandler, ErrorHandlingStrategy, GeneralAstEnhancer,
    GeneralParser, GeneralSemanticAnalyzer, OperationalSettings,
};
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::Core::ValueResolution::ValueResolver;
use dixscript::Compiler::AST::data_types::{DLMModuleType, DLMModuleSubtype};
use dixscript::Compiler::ImportsResolution::CloudFileCache;
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
                .downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("Pipeline panicked for {}: {}", doc.uri, msg);
            doc.error_manager.get_all_errors_flat()
        }
    }
}

// ── Command resolution ────────────────────────────────────────────────────────

pub fn get_resolved_ast(doc: &Document) -> Option<DixScript> {
    let ast = doc.ast.as_ref()?;
    let semantic_result = doc.semantic_result.as_ref()?;

    let has_local_fns = ast.quick_functions.as_ref()
        .map(|q| !q.functions.is_empty()).unwrap_or(false);
    let has_imported_fns = semantic_result.symbol_table.as_ref()
        .map(|st| st.namespaces.values().any(|ns| !ns.functions.is_empty()))
        .unwrap_or(false);

    if (!has_local_fns && !has_imported_fns) || ast.data.is_none() {
        return Some(ast.clone());
    }

    let st = match semantic_result.symbol_table.as_ref() {
        Some(st) => st,
        None => return Some(ast.clone()),
    };

    let ast_clone = ast.clone();
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let resolver = ValueResolver::new(ast_clone, st, DebugMode::Off);
        resolver.resolve()
    }));

    match result {
        Ok(resolution) if resolution.is_success => {
            if let Some(resolved) = resolution.resolved_ast {
                tracing::debug!(
                    "Value resolution for command complete: {} call(s) resolved",
                    resolution.function_calls_resolved
                );
                Some(resolved)
            } else {
                tracing::warn!("Value resolution succeeded but returned None AST");
                Some(ast.clone())
            }
        }
        Ok(resolution) => {
            tracing::warn!(
                "Value resolution for command failed ({} error(s)), using enhanced AST",
                resolution.errors.len()
            );
            Some(ast.clone())
        }
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("Value resolution panicked for command: {}", msg);
            Some(ast.clone())
        }
    }
}

// ── Cloud import cache check ──────────────────────────────────────────────────

/// Returns `true` when every cloud import in `ast` has a cached copy on disk.
/// This is a pure filesystem check — no network I/O.
fn cloud_imports_all_cached(ast: &DixScript) -> bool {
    let imports = match ast.imports.as_ref() {
        Some(i) => i,
        None    => return true,
    };

    // Quick exit: no cloud imports at all
    let cloud_count = imports.imports.iter().filter(|i| i.is_cloud_import).count();
    if cloud_count == 0 {
        return true;
    }

    let cache = CloudFileCache::new(ErrorManager::new_isolated());

    imports.imports.iter()
        .filter(|imp| imp.is_cloud_import)
        .all(|imp| {
            // Strip query params — the cache stores URLs without them
            let url = imp.path.find('?')
                .map(|pos| &imp.path[..pos])
                .unwrap_or(&imp.path);
            cache.is_cached(url)
        })
}

/// Count uncached cloud imports (for the diagnostic message).
fn uncached_cloud_import_aliases(ast: &DixScript) -> Vec<String> {
    let imports = match ast.imports.as_ref() {
        Some(i) => i,
        None    => return vec![],
    };
    let cache = CloudFileCache::new(ErrorManager::new_isolated());
    imports.imports.iter()
        .filter(|imp| {
            if !imp.is_cloud_import { return false; }
            let url = imp.path.find('?')
                .map(|pos| &imp.path[..pos])
                .unwrap_or(&imp.path);
            !cache.is_cached(url)
        })
        .map(|imp| imp.alias.clone())
        .collect()
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
        &doc.source, &tokenizer_settings, em.clone(),
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

    // ── Cloud import handling ─────────────────────────────────────────────────
    //
    // Check the local cache BEFORE deciding whether to run import resolution:
    //   • All cached   → let the resolver use cached files (no network I/O)
    //   • Any missing  → skip resolution + emit Info diagnostic
    //
    // This way the LSP works fully offline once `mdix compile` has been run once
    // to populate the cache, matching the behaviour of Deno LSP / TypeScript
    // path-aliases: cache aggressively, serve offline.

    let has_cloud_imports = ast.imports.as_ref()
        .map(|imp| imp.imports.iter().any(|i| i.is_cloud_import))
        .unwrap_or(false);

    let mut cloud_not_cached_aliases: Vec<String> = vec![];

    if has_cloud_imports {
        if cloud_imports_all_cached(&ast) {
            tracing::debug!(
                "Cloud imports present — all cached, proceeding with import resolution"
            );
        } else {
            cloud_not_cached_aliases = uncached_cloud_import_aliases(&ast);
            tracing::debug!(
                "Cloud imports not cached ({:?}) — disabling import resolution",
                cloud_not_cached_aliases
            );
            operational_settings.skip_imports_resolution = true;
        }
    }

    // ── Stage 4: semantic analysis ────────────────────────────────────────────
    let analyzer = GeneralSemanticAnalyzer::new_for_lsp(
        &ast, &operational_settings, em.clone(),
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
        &operational_settings, em.clone(),
    );
    let enhancement_result = enhancer.enhance(&ast, Some(&semantic_result));

    tracing::debug!(
        "Enhancement complete: {} enhancements applied",
        enhancement_result.total_enhancements
    );

    // ── Stage 6: build diagnostic list ───────────────────────────────────────
    let mut all_errors = em.get_all_errors_flat();

    // Remove auto-generated security-missing diagnostics so we can re-emit
    // them at the correct token position below.
    all_errors.retain(|e| !is_security_missing_error(e));

    // ── Cloud import info diagnostic (emitted after all_errors is built) ──────
    if !cloud_not_cached_aliases.is_empty() {
        let (imp_line, imp_col) =
            find_section_token_pos(&doc.tokens, TokenType::SectionImports)
                .unwrap_or((1, 1));

        let alias_list = cloud_not_cached_aliases.join(", ");
        all_errors.push(DixError::Semantic(SemanticError {
            error_id:    "CLOUD001".to_string(),
            error_type:  SemanticErrorType::InvalidConfiguration,
            message:     format!(
                "Cloud import(s) not cached locally — import resolution disabled in LSP: {}. \
                 Run `mdix compile` once to download and cache remote imports.",
                alias_list
            ),
            line:        imp_line as i32,
            column:      imp_col  as i32,
            section_name: Some("IMPORTS".to_string()),
            suggestion:   Some(
                "Run `mdix compile <your-file.mdix>` to fetch remote imports. \
                 The LSP will use the cache automatically on subsequent edits."
                    .to_string(),
            ),
            severity:    ErrorSeverity::Info,
            quick_fixes: Vec::new(),
            metadata:    HashMap::new(),
        }));
    }

    // ── Security validation ───────────────────────────────────────────────────
    let user_has_security_token = doc.tokens
        .iter()
        .any(|t| matches!(t.token_type, TokenType::SectionSecurity));

    let dlm_encryptors = collect_encryptors(&ast);

    tracing::debug!(
        "Security check: encryptors={:?}, user_has_security_token={}",
        dlm_encryptors, user_has_security_token
    );

    if !dlm_encryptors.is_empty() && !user_has_security_token {
        let (diag_line, diag_col) =
            find_section_token_pos(&doc.tokens, TokenType::SectionDLM)
                .unwrap_or((1, 1));

        for algorithm in &dlm_encryptors {
            all_errors.push(DixError::Semantic(SemanticError {
                error_id:    "SEC001".to_string(),
                error_type:  SemanticErrorType::InvalidConfiguration,
                message:     format!(
                    "@SECURITY section required: DEncryptor.{} is present in @DLM but no @SECURITY block was found.",
                    algorithm
                ),
                line:        diag_line as i32,
                column:      diag_col  as i32,
                section_name: Some("DLM".to_string()),
                suggestion:   Some(
                    "Add an @SECURITY section, e.g.: \
                     @SECURITY( encryption -> { mode = \"keyfile\", algorithm = \"...\" } )"
                        .to_string(),
                ),
                severity:    ErrorSeverity::Error,
                quick_fixes: Vec::new(),
                metadata:    HashMap::new(),
            }));
        }
    }

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

    // ── Convert semantic_result errors / warnings ─────────────────────────────
    for err in &semantic_result.errors {
        if is_security_missing_str(&err.message) { continue; }

        let (line, col) = err.position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

        all_errors.push(DixError::Semantic(SemanticError {
            error_id:    err.error_id.clone(),
            error_type:  SemanticErrorType::InvalidConfiguration,
            message:     err.message.clone(),
            line,
            column:      col,
            section_name: if err.section_name.is_empty() { None } else { Some(err.section_name.clone()) },
            suggestion:   if err.suggestion.is_empty()   { None } else { Some(err.suggestion.clone())   },
            severity:    ErrorSeverity::Error,
            quick_fixes: Vec::new(),
            metadata:    HashMap::new(),
        }));
    }

    for warn in &semantic_result.warnings {
        if is_security_missing_str(&warn.message) { continue; }

        let (line, col) = warn.position
            .map(|p| (p.line as i32, p.column as i32))
            .unwrap_or((0, 0));

        all_errors.push(DixError::Semantic(SemanticError {
            error_id:    warn.warning_id.clone(),
            error_type:  SemanticErrorType::InvalidConfiguration,
            message:     warn.message.clone(),
            line,
            column:      col,
            section_name: if warn.section_name.is_empty() { None } else { Some(warn.section_name.clone()) },
            suggestion:   None,
            severity:    ErrorSeverity::Warning,
            quick_fixes: Vec::new(),
            metadata:    HashMap::new(),
        }));
    }

    // ── Store state ───────────────────────────────────────────────────────────
    let enhanced_ast = enhancement_result.enhanced_ast.clone();
    doc.ast                = Some(enhanced_ast);
    doc.semantic_result    = Some(semantic_result);
    doc.enhancement_result = Some(enhancement_result);

    tracing::debug!("Pipeline complete: {} total diagnostics", all_errors.len());
    all_errors
}

// ── Security helpers ──────────────────────────────────────────────────────────

fn is_security_missing_error(e: &DixError) -> bool {
    is_security_missing_str(e.message())
}

fn is_security_missing_str(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    (lower.contains("security") && lower.contains("missing"))
        || (lower.contains("security") && lower.contains("required"))
        || lower.contains("encryptor requires")
        || lower.contains("sec001")
}

fn collect_encryptors(ast: &DixScript) -> Vec<String> {
    let dlm = match &ast.dlm { Some(d) => d, None => return vec![] };
    dlm.modules.iter()
        .filter(|m| matches!(m.module_type, DLMModuleType::DEncryptor))
        .map(|m| match m.subtype {
            Some(DLMModuleSubtype::Aes128)   => "aes128".to_string(),
            Some(DLMModuleSubtype::Aes256)   => "aes256".to_string(),
            Some(DLMModuleSubtype::Chacha20) => "chacha20".to_string(),
            Some(DLMModuleSubtype::Xor)      => "xor".to_string(),
            _                                => "aes256".to_string(),
        })
        .collect()
}

fn find_section_token_pos(
    tokens:      &[dixscript::Compiler::Core::Tokenizer::Token],
    target_type: TokenType,
) -> Option<(usize, usize)> {
    let is_match = |tt: &TokenType| -> bool {
        matches!(
            (&target_type, tt),
            (TokenType::SectionDLM,          TokenType::SectionDLM)
            | (TokenType::SectionConfig,     TokenType::SectionConfig)
            | (TokenType::SectionData,       TokenType::SectionData)
            | (TokenType::SectionEnums,      TokenType::SectionEnums)
            | (TokenType::SectionImports,    TokenType::SectionImports)
            | (TokenType::SectionQuickFuncs, TokenType::SectionQuickFuncs)
            | (TokenType::SectionSecurity,   TokenType::SectionSecurity)
        )
    };
    tokens.iter()
        .find(|t| is_match(&t.token_type))
        .map(|t| (t.line, t.column))
        }
