//! Wraps the dixscript compilation pipeline up to semantic analysis.
//! Uses Approach B (tokenizer-first): tokenize full source → split @CONFIG
//! → process config tokens → parse rest tokens → semantic analysis.

use std::path::Path;
use std::time::Instant;
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use dixscript::Compiler::Core::Config::{ConfigSectionHandler, OperationalSettings};
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer};
use crate::commands::CliError;

/// Result returned to command handlers after validation.
#[derive(Debug)]
pub struct ValidationResult {
    pub file_path:     String,
    pub token_count:   usize,
    pub warning_count: usize,
    pub error_count:   usize,
    pub warnings:      Vec<String>,
    pub elapsed:       std::time::Duration,
}

/// Run the dixscript pipeline up through semantic analysis.
///
/// Returns `Ok(ValidationResult)` on success, `Err(CliError)` on any failure.
pub fn validate_file(path: &Path, strict: bool) -> Result<ValidationResult, CliError> {
    if !path.exists() {
        return Err(CliError::FileNotFound(path.to_path_buf()));
    }

    let source = std::fs::read_to_string(path).map_err(CliError::IoError)?;
    let t = Instant::now();

    // ── Stage 1: tokenize the full source ─────────────────────────────────
    let initial_settings = OperationalSettings {
        source_file_path: Some(path.to_string_lossy().to_string()),
        ..OperationalSettings::default()
    };

    let tokenizer = Tokenizer::new(&source, &initial_settings);
    let tok_result = tokenizer.tokenize();
    // Capture total token count before the move into split_config_tokens.
    let token_count = tok_result.tokens.len();

    // ── Stage 2: split @CONFIG and process it ─────────────────────────────
    let split = split_config_tokens(tok_result.tokens);

    let mut config_handler = ConfigSectionHandler::new(None);
    let config_result = config_handler.process_config_tokens(&split.config_tokens);

    let mut settings = config_result.operational_settings;
    settings.source_file_path = Some(path.to_string_lossy().to_string());

    // ── Stage 3: parse the rest of the token stream ───────────────────────
    let parser =
        GeneralParser::new(split.rest_tokens, &config_result.config_section, &settings)
            .map_err(|e| CliError::ParseError(e.message().to_string()))?;

    let ast = parser
        .parse()
        .map_err(|e| CliError::ParseError(e.message().to_string()))?;

    // ── Stage 4: semantic analysis ────────────────────────────────────────
    let analyzer = GeneralSemanticAnalyzer::new(&ast, &settings);
    let result = analyzer.analyze();

    let elapsed = t.elapsed();

    let warnings: Vec<String> = result.warnings.iter().map(|w| w.message.clone()).collect();

    if !result.is_success {
        let msgs: Vec<String> = result.errors.iter().map(|e| e.message.clone()).collect();
        return Err(CliError::ParseError(msgs.join("\n")));
    }

    if strict && !warnings.is_empty() {
        return Err(CliError::ParseError(format!(
            "{} warning(s) treated as errors (--strict)",
            warnings.len()
        )));
    }

    Ok(ValidationResult {
        file_path: path.to_string_lossy().to_string(),
        token_count,
        warning_count: warnings.len(),
        error_count: result.errors.len(),
        warnings,
        elapsed,
    })
}
