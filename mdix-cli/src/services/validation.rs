
//! Wraps the dixscript compilation pipeline up to semantic analysis.

use std::path::Path;
use std::time::Instant;
use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::Config::ConfigSectionHandler;
use dixscript::Compiler::Core::{GeneralParser, GeneralSemanticAnalyzer};
use dixscript::ErrorManager::ErrorManager;
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

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let t = Instant::now();

    // Config
    let mut config_handler = ConfigSectionHandler::new(None);
    let config_result = config_handler.process_config_section(&source);
    let mut settings = config_result.operational_settings;
    settings.source_file_path = Some(path.to_string_lossy().to_string());
    error_manager.update_settings(settings.clone());

    // Tokenize
    let tokenizer = Tokenizer::new(&config_result.cleaned_input_string, &settings);
    let tok_result = tokenizer.tokenize();
    let token_count = tok_result.tokens.len();

    if error_manager.has_fatal_errors() {
        let report = error_manager.generate_error_report();
        return Err(CliError::ParseError(report));
    }

    // Parse
    let parser = GeneralParser::new(tok_result.tokens, &config_result.config_section, &settings)
        .map_err(|e| CliError::ParseError(e.message().to_string()))?;

    let ast = parser
        .parse()
        .map_err(|e| CliError::ParseError(e.message().to_string()))?;

    // Semantic analysis
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
