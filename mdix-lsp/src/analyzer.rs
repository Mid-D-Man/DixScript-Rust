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
    ParseError, ParseErrorType,
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
            // An unparseable document is the normal state while someone is typing, so this
            // is debug, not warn (it used to flood the Unity console on every keystroke).
            // The failure itself is NOT dropped: it is reported as a diagnostic, otherwise
            // the editor says "no problems" about a document that cannot compile.
            let message = e.message().to_string();
            tracing::debug!("Parser construction failed: {}", message);

            let mut errors = em.get_all_errors_flat();
            errors.push(parse_failure_error(&doc.tokens, &message));
            return errors;
        }
    };

    let ast = match parser.parse() {
        Ok(a)  => a,
        Err(e) => {
            let message = e.message().to_string();
            tracing::debug!("Parse failed: {}", message);

            let mut errors = em.get_all_errors_flat();
            errors.push(parse_failure_error(&doc.tokens, &message));
            return errors;
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

// ── Parse-failure diagnostics ─────────────────────────────────────────────────
//
// `GeneralParser::parse()` fails with a bare `ParseException`: a message and no
// position. Reporting that at 1:1 would be no help, so the failure is located from
// the token stream by walking the top level the way the parser does: a section
// keyword opens a body, parentheses nest, and anything else outside a body that is
// not a comment is where the parser gave up.

const SECTION_NAMES: [&str; 9] = [
    "CONFIG", "IMPORTS", "DLM", "ENUMS", "QUICKFUNCS", "DATA", "SECURITY", "RAW", "SCHEMA",
];

fn is_section_token(token_type: &TokenType) -> bool {
    matches!(
        token_type,
        TokenType::SectionConfig
            | TokenType::SectionImports
            | TokenType::SectionDLM
            | TokenType::SectionEnums
            | TokenType::SectionQuickFuncs
            | TokenType::SectionData
            | TokenType::SectionSecurity
            | TokenType::SectionRaw
            | TokenType::SectionSchema
    )
}

/// Index of the first token that sits outside every section body and is not a
/// section keyword or a comment — the token "Expected section keyword" is about.
fn find_stray_top_level_token(
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
) -> Option<usize> {
    let mut depth = 0i32;
    let mut i     = 0usize;

    while i < tokens.len() {
        let token_type = &tokens[i].token_type;

        if matches!(token_type, TokenType::EndOfFile) {
            break;
        }

        // A section keyword always starts a section. The parser also treats one met
        // inside an unclosed section as the end of that section, so this resets.
        if is_section_token(token_type) {
            depth = 1;
            i += 1;

            // The opening parenthesis belongs to the body; the parser tolerates its absence.
            if let Some(next) = tokens.get(i) {
                if matches!(next.token_type, TokenType::Symbol('(')) {
                    i += 1;
                }
            }
            continue;
        }

        if depth == 0 {
            if !matches!(token_type, TokenType::Comment(_)) {
                return Some(i);
            }
        } else {
            match token_type {
                TokenType::Symbol('(') => depth += 1,
                TokenType::Symbol(')') => depth -= 1,
                _ => {}
            }
        }

        i += 1;
    }

    None
}

/// (line, column, message, suggestion) for the stray token at `index`. Positions
/// are 1-based, like every other DixScript position.
fn describe_stray_token(
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
    index:  usize,
) -> (usize, usize, String, Option<String>) {
    let token = &tokens[index];
    let valid = "Valid sections: @CONFIG, @IMPORTS, @DLM, @ENUMS, @QUICKFUNCS, @DATA, @SECURITY."
        .to_string();

    match &token.token_type {
        TokenType::Symbol('@') => {
            if let Some(TokenType::Identifier(name)) =
                tokens.get(index + 1).map(|t| &t.token_type)
            {
                // The lexer matches section names case-insensitively, so a name that is a
                // section in ANY case never arrives here as a stray token: this is a name that
                // is not a section, or one still being typed. Point at the section it may be
                // heading for when exactly one starts with what is there.
                let upper = name.to_uppercase();
                let candidates: Vec<&str> = SECTION_NAMES
                    .iter()
                    .copied()
                    .filter(|section| section.starts_with(upper.as_str()))
                    .collect();

                if candidates.len() == 1 {
                    return (
                        token.line,
                        token.column,
                        format!("Unknown section '@{}'.", name),
                        Some(format!("Did you mean '@{}'?", candidates[0])),
                    );
                }

                return (
                    token.line,
                    token.column,
                    format!("Unknown section '@{}'.", name),
                    Some(valid),
                );
            }

            (
                token.line,
                token.column,
                "Expected a section name after '@'.".to_string(),
                Some(valid),
            )
        }
        TokenType::Identifier(name) => (
            token.line,
            token.column,
            format!("'{}' is outside every section.", name),
            Some("Everything except comments belongs inside a section, e.g. @DATA( ... ).".to_string()),
        ),
        other => (
            token.line,
            token.column,
            format!("Unexpected {} outside every section.", other),
            Some("Everything except comments belongs inside a section, e.g. @DATA( ... ).".to_string()),
        ),
    }
}

/// The diagnostic for a document the parser rejected outright. Falls back to the
/// parser's own message at the top of the file when no stray token can be found
/// (the problem is then inside a section body).
fn parse_failure_error(
    tokens:      &[dixscript::Compiler::Core::Tokenizer::Token],
    raw_message: &str,
) -> DixError {
    let (line, column, message, suggestion) = match find_stray_top_level_token(tokens) {
        Some(i) => describe_stray_token(tokens, i),
        None    => (1, 1, raw_message.to_string(), None),
    };

    DixError::Parse(ParseError::new(
        ParseErrorType::SectionSyntaxError,
        message,
        line,
        column,
        suggestion,
        None,
        ErrorSeverity::Error,
    ))
}

#[cfg(test)]
mod parse_failure_tests {
    use super::*;
    use dixscript::Compiler::Core::Tokenizer::token::SectionId;
    use dixscript::Compiler::Core::Tokenizer::Token;

    fn tok(token_type: TokenType, line: usize, column: usize) -> Token {
        Token::new(token_type, line, column, SectionId::None)
    }

    #[test]
    fn unknown_section_is_located_and_the_valid_ones_are_listed() {
        // @Foo( )
        let tokens = vec![
            tok(TokenType::Symbol('@'), 1, 1),
            tok(TokenType::Identifier("Foo".to_string()), 1, 2),
            tok(TokenType::Symbol('('), 1, 5),
            tok(TokenType::Symbol(')'), 1, 6),
            Token::eof(1, 7),
        ];

        assert_eq!(find_stray_top_level_token(&tokens), Some(0));

        let (line, column, message, suggestion) = describe_stray_token(&tokens, 0);
        assert_eq!((line, column), (1, 1));
        assert_eq!(message, "Unknown section '@Foo'.");
        assert!(suggestion.unwrap_or_default().contains("Valid sections"));
    }

    #[test]
    fn a_section_name_still_being_typed_gets_a_suggestion_in_any_case() {
        // @da   (section names are case-insensitive; only DATA starts with DA)
        let tokens = vec![
            tok(TokenType::Symbol('@'), 1, 1),
            tok(TokenType::Identifier("da".to_string()), 1, 2),
            Token::eof(1, 4),
        ];

        let (_, _, message, suggestion) = describe_stray_token(&tokens, 0);
        assert_eq!(message, "Unknown section '@da'.");
        assert_eq!(suggestion.as_deref(), Some("Did you mean '@DATA'?"));
    }

    #[test]
    fn well_formed_top_level_has_no_stray_token() {
        // @DATA( x ( ) )
        let tokens = vec![
            tok(TokenType::SectionData, 1, 1),
            tok(TokenType::Symbol('('), 1, 6),
            tok(TokenType::Identifier("x".to_string()), 2, 3),
            tok(TokenType::Symbol('('), 2, 5),
            tok(TokenType::Symbol(')'), 2, 6),
            tok(TokenType::Symbol(')'), 3, 1),
            Token::eof(3, 2),
        ];

        assert_eq!(find_stray_top_level_token(&tokens), None);
    }

    #[test]
    fn stray_token_after_a_closed_section_is_found() {
        // @DATA( ) <
        let tokens = vec![
            tok(TokenType::SectionData, 1, 1),
            tok(TokenType::Symbol('('), 1, 6),
            tok(TokenType::Symbol(')'), 1, 7),
            tok(TokenType::Symbol('<'), 2, 1),
            Token::eof(2, 2),
        ];

        assert_eq!(find_stray_top_level_token(&tokens), Some(3));
    }
}
