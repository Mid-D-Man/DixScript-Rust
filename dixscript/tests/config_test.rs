// tests/unit/config_tests.rs

use dixscript::Compiler::Core::Config::{
    ConfigSchema,
    ConfigSectionHandler,
    OperationalSettings,
};
use dixscript::Compiler::AST::data_types::{
    ErrorHandlingStrategy,
    CompatibilityMode,
    DebugMode,
};

// ---- ConfigSchema tests ----

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
    // Should fall back to default rather than hard error
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
    // Bad encoding falls back to default
    assert_eq!(result.get("encoding").unwrap().as_str(), "utf-8");
}

// ---- ConfigSectionHandler tests ----

#[test]
fn test_empty_input_returns_defaults() {
    let mut handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_section("");
    assert!(!result.config_section.entries.is_empty());
    assert!(!result.warnings.is_empty());
}


#[test]
fn test_no_config_section_returns_defaults() {
    let mut handler = ConfigSectionHandler::new(None);
    let input = r#"
        @DATA(
            name = "test"
        )
    "#;
    let result = handler.process_config_section(input);
    assert!(!result.config_section.entries.is_empty());
    assert_eq!(result.cleaned_input_string.trim(), input.trim());
}

#[test]
fn test_valid_config_section_is_parsed() {
    let mut handler = ConfigSectionHandler::new(None);
    let input = r#"
        @CONFIG(
            version -> "1.0.0",
            encoding -> "utf-8",
            debug_mode -> "verbose"
        )
    "#;
    let result = handler.process_config_section(input);
    assert!(result.warnings.is_empty() || result.warnings.iter().all(|w| !w.contains("error")));
    assert_eq!(result.operational_settings.debug_mode, DebugMode::Verbose);
}

#[test]
fn test_config_section_is_stripped_from_cleaned_output() {
    let mut handler = ConfigSectionHandler::new(None);
    let input = r#"@CONFIG(
        version -> "1.0.0"
    )
    @DATA(
        x = 1
    )"#;
    let result = handler.process_config_section(input);
    assert!(!result.cleaned_input_string.contains("@CONFIG"));
    assert!(result.cleaned_input_string.contains("@DATA"));
}

#[test]
fn test_error_handling_strategy_extracted() {
    let mut handler = ConfigSectionHandler::new(None);
    let input = r#"@CONFIG(
        version -> "1.0.0",
        error_handling -> "continue"
    )"#;
    let result = handler.process_config_section(input);
    assert_eq!(
        result.operational_settings.error_handling_strategy,
        ErrorHandlingStrategy::Continue
    );
}