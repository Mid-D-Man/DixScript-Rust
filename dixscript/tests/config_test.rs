// dixscript/tests/config_test.rs
// Approach B: tokenise → split_config_tokens → process_config_tokens
// (mirrors DixLoader and the LSP analyser pipeline exactly)

use dixscript::Compiler::Core::Config::{
    ConfigSchema,
    ConfigSectionHandler,
    OperationalSettings,
};
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, split_config_tokens};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::AST::data_types::{
    ErrorHandlingStrategy,
    CompatibilityMode,
    DebugMode,
};

// ==================== ConfigSchema tests ====================

#[test]
fn test_minimal_config_is_valid() {
    let config = ConfigSchema::create_minimal_config();
    assert!(!config.entries.is_empty());
}

#[test]
fn test_extract_defaults_when_no_entries() {
    let config = ConfigSchema::create_minimal_config();
    let settings = ConfigSchema::extract_operational_settings(&config);
    assert_eq!(settings.error_handling_strategy, ErrorHandlingStrategy::Halt);
    assert_eq!(settings.compatibility_mode, CompatibilityMode::Strict);
    assert_eq!(settings.debug_mode, DebugMode::Off);
    assert_eq!(settings.version, "1.0.0");
}

#[test]
fn test_validate_and_enhance_fills_missing_keys() {
    let config = std::collections::HashMap::new();
    let result = ConfigSchema::validate_and_enhance_config(config).unwrap();
    assert!(result.contains_key("version"));
    assert!(result.contains_key("encoding"));
}

#[test]
fn test_validate_rejects_bad_version() {
    let mut config = std::collections::HashMap::new();
    config.insert("version".to_string(), "not_a_version!!".to_string());
    let result = ConfigSchema::validate_and_enhance_config(config).unwrap();
    assert_eq!(result.get("version").unwrap().as_str(), "1.0.0");
}

#[test]
fn test_validate_accepts_good_version() {
    let mut config = std::collections::HashMap::new();
    config.insert("version".to_string(), "1.0.0".to_string());
    let result = ConfigSchema::validate_and_enhance_config(config).unwrap();
    assert_eq!(result.get("version").unwrap().as_str(), "1.0.0");
}

#[test]
fn test_validate_rejects_bad_encoding() {
    let mut config = std::collections::HashMap::new();
    config.insert("encoding".to_string(), "latin-99".to_string());
    let result = ConfigSchema::validate_and_enhance_config(config).unwrap();
    assert_eq!(result.get("encoding").unwrap().as_str(), "utf-8");
}

// ==================== ConfigSectionHandler tests (Approach B) ====================
//
// All handler tests now use the tokeniser-first pipeline:
//   1. Tokenizer::new(source, &settings).tokenize()
//   2. split_config_tokens(tok_result.tokens)  →  config_tokens + rest_tokens
//   3. handler.process_config_tokens(&split.config_tokens)
//
// The old `process_config_section(&str)` and its `cleaned_input_string` field
// no longer exist; separation is done at the token level via split_config_tokens.

#[test]
fn test_empty_input_returns_defaults() {
    let settings   = OperationalSettings::default();
    let tok_result = Tokenizer::new("", &settings).tokenize();
    let split      = split_config_tokens(tok_result.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_tokens(&split.config_tokens);

    // Even with empty input, the handler must populate default config entries.
    assert!(
        !result.config_section.entries.is_empty(),
        "Config section should have default entries when no @CONFIG is provided"
    );
}

#[test]
fn test_no_config_section_returns_defaults() {
    let input = r#"
        @DATA(
            name = "test"
        )
    "#;

    let settings   = OperationalSettings::default();
    let tok_result = Tokenizer::new(input, &settings).tokenize();
    let split      = split_config_tokens(tok_result.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_tokens(&split.config_tokens);

    // Defaults must be populated when @CONFIG is absent.
    assert!(
        !result.config_section.entries.is_empty(),
        "Config section should have defaults when @CONFIG is absent"
    );

    // In Approach B, the @DATA section remains in rest_tokens (not config_tokens).
    let rest_has_data = split.rest_tokens.iter()
        .any(|t| matches!(t.token_type, TokenType::SectionData));
    assert!(rest_has_data, "rest_tokens should contain the @DATA section from the source");
}

#[test]
fn test_valid_config_section_is_parsed() {
    let input = r#"
        @CONFIG(
            version -> "1.0.0",
            encoding -> "utf-8",
            debug_mode -> "verbose"
        )
    "#;

    let settings   = OperationalSettings::default();
    let tok_result = Tokenizer::new(input, &settings).tokenize();
    let split      = split_config_tokens(tok_result.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_tokens(&split.config_tokens);

    assert_eq!(
        result.operational_settings.debug_mode,
        DebugMode::Verbose,
        "debug_mode -> \"verbose\" should produce DebugMode::Verbose"
    );
}

#[test]
fn test_config_section_token_split() {
    // In Approach B, @CONFIG is separated from the rest at the *token* level.
    // Verify that split_config_tokens correctly routes tokens to the right stream.
    let input = r#"@CONFIG(
        version -> "1.0.0"
    )
    @DATA(
        x = 1
    )"#;

    let settings   = OperationalSettings::default();
    let tok_result = Tokenizer::new(input, &settings).tokenize();
    let split      = split_config_tokens(tok_result.tokens);

    // config_tokens must hold the @CONFIG section keyword.
    let config_has_config = split.config_tokens.iter()
        .any(|t| matches!(t.token_type, TokenType::SectionConfig));
    assert!(config_has_config, "config_tokens should contain the @CONFIG section token");

    // rest_tokens must NOT contain any @CONFIG tokens.
    let rest_has_config = split.rest_tokens.iter()
        .any(|t| matches!(t.token_type, TokenType::SectionConfig));
    assert!(!rest_has_config, "rest_tokens must not contain @CONFIG tokens");

    // rest_tokens must contain the @DATA section.
    let rest_has_data = split.rest_tokens.iter()
        .any(|t| matches!(t.token_type, TokenType::SectionData));
    assert!(rest_has_data, "rest_tokens should contain the @DATA section");
}

#[test]
fn test_error_handling_strategy_extracted() {
    let input = r#"@CONFIG(
        version -> "1.0.0",
        error_handling -> "continue"
    )"#;

    let settings   = OperationalSettings::default();
    let tok_result = Tokenizer::new(input, &settings).tokenize();
    let split      = split_config_tokens(tok_result.tokens);
    let mut handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_tokens(&split.config_tokens);

    assert_eq!(
        result.operational_settings.error_handling_strategy,
        ErrorHandlingStrategy::Continue,
        "error_handling -> \"continue\" should produce ErrorHandlingStrategy::Continue"
    );
        }
