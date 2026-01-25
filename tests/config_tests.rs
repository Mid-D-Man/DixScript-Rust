// tests/config_tests.rs
//! Comprehensive tests for DixScript CONFIG section handling
//!
//! Tests cover:
//! - ConfigSectionHandler extraction and parsing
//! - ConfigSchema validation and defaults
//! - OperationalSettings extraction
//! - Version initialization
//! - Error handling strategies

use dixscript::Compiler::Core::Config::{
    ConfigSectionHandler, ConfigSchema, OperationalSettings,
    ErrorHandlingStrategy, CompatibilityMode, DebugMode,
};
use dixscript::Compiler::AST::{ConfigSection, ConfigValue};
use dixscript::Compiler::VersionControl::VersionManager;

// ==================== HELPER FUNCTIONS ====================

/// Initialize test logger (optional, but helpful for debugging)
fn setup_test() {
    // Reset VersionManager for each test if needed
    // Note: VersionManager uses OnceLock which can't be reset easily
    // So we just ensure it's initialized
}

/// Clean up after test
fn teardown_test() {
    // Any cleanup needed
}

/// Create a basic valid CONFIG string
fn create_basic_config() -> String {
    r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    author -> "Test Suite",
    features -> "advanced",
    debug_mode -> "verbose",
    error_handling -> "halt"
)"#.to_string()
}

/// Create CONFIG with all optional fields
fn create_full_config() -> String {
    r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    author -> "Test Suite",
    created -> "2025-01-25T10:30:00Z",
    features -> "advanced",
    debug_mode -> "verbose",
    error_handling -> "halt",
    compatibility_mode -> "strict"
)"#.to_string()
}

/// Create minimal CONFIG (only required fields)
fn create_minimal_config() -> String {
    r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8"
)"#.to_string()
}

/// Assert config entry exists and has expected value
fn assert_config_entry(config: &ConfigSection, key: &str, expected: &str) {
    let entry = config.entries.iter()
        .find(|e| e.key.eq_ignore_ascii_case(key))
        .unwrap_or_else(|| panic!("Config entry '{}' not found", key));

    match &entry.value {
        ConfigValue::String(s) => assert_eq!(s.as_str(), expected, "Config {} value mismatch", key),
        ConfigValue::Features(features) => {
            let features_str = features.join(",");
            assert_eq!(features_str.as_str(), expected, "Config {} features mismatch", key);
        }
        other => panic!("Unexpected config value type for {}: {:?}", key, other),
    }
}

// ==================== CONFIG EXTRACTION TESTS ====================

#[test]
fn test_extract_basic_config() {
    setup_test();

    let input = create_basic_config();
    let handler = ConfigSectionHandler::new(None);

    let result = handler.process_config_section(&input);

    // Verify config was extracted
    assert!(!result.config_section.entries.is_empty(), "Config entries should not be empty");
    assert_config_entry(&result.config_section, "version", "1.0.0");
    assert_config_entry(&result.config_section, "encoding", "utf-8");
    assert_config_entry(&result.config_section, "author", "Test Suite");

    // Verify warnings
    println!("Warnings: {:#?}", result.warnings);

    // Verify cleaned input (CONFIG should be removed)
    assert!(result.cleaned_input_string.is_empty() || !result.cleaned_input_string.contains("@CONFIG"));

    teardown_test();
}

#[test]
fn test_extract_config_with_data_section() {
    setup_test();

    let input = format!("{}\n\n@DATA(\n    test_value = 42\n)", create_basic_config());
    let handler = ConfigSectionHandler::new(None);

    let result = handler.process_config_section(&input);

    // Verify CONFIG extracted
    assert!(!result.config_section.entries.is_empty());

    // Verify DATA section remains in cleaned input
    assert!(result.cleaned_input_string.contains("@DATA"));
    assert!(result.cleaned_input_string.contains("test_value"));

    // Verify CONFIG removed from cleaned input
    assert!(!result.cleaned_input_string.contains("@CONFIG"));

    println!("Cleaned input:\n{}", result.cleaned_input_string);

    teardown_test();
}

#[test]
fn test_no_config_section() {
    setup_test();

    let input = "@DATA(\n    value = 100\n)".to_string();
    let handler = ConfigSectionHandler::new(None);

    let result = handler.process_config_section(&input);

    // Should use default config
    assert!(!result.config_section.entries.is_empty());
    assert_config_entry(&result.config_section, "version", "1.0.0");

    // Should have warning about missing CONFIG
    assert!(!result.warnings.is_empty());
    assert!(result.warnings.iter().any(|w| w.contains("No CONFIG") || w.contains("default")));

    // Input should remain unchanged
    assert_eq!(result.cleaned_input_string, input);

    println!("Warnings: {:#?}", result.warnings);

    teardown_test();
}

#[test]
fn test_empty_input() {
    setup_test();

    let input = "".to_string();
    let handler = ConfigSectionHandler::new(None);

    let result = handler.process_config_section(&input);

    // Should use cached minimal config
    assert!(!result.config_section.entries.is_empty());
    assert_config_entry(&result.config_section, "version", "1.0.0");

    // Should have warning
    assert!(!result.warnings.is_empty());

    println!("Empty input warnings: {:#?}", result.warnings);

    teardown_test();
}

// ==================== CONFIG PARSING TESTS ====================

#[test]
fn test_parse_full_config() {
    setup_test();

    let input = create_full_config();
    let handler = ConfigSectionHandler::new(None);

    let result = handler.process_config_section(&input);

    // Verify all fields parsed
    assert_config_entry(&result.config_section, "version", "1.0.0");
    assert_config_entry(&result.config_section, "encoding", "utf-8");
    assert_config_entry(&result.config_section, "author", "Test Suite");
    assert_config_entry(&result.config_section, "features", "advanced");

    // Verify operational settings
    assert_eq!(result.operational_settings.version.as_str(), "1.0.0");
    assert_eq!(result.operational_settings.error_handling_strategy, ErrorHandlingStrategy::Halt);
    assert_eq!(result.operational_settings.debug_mode, DebugMode::Verbose);
    assert_eq!(result.operational_settings.compatibility_mode, CompatibilityMode::Strict);

    println!("Operational settings: {:#?}", result.operational_settings);

    teardown_test();
}

#[test]
fn test_parse_minimal_config() {
    setup_test();

    let input = create_minimal_config();
    let handler = ConfigSectionHandler::new(None);

    let result = handler.process_config_section(&input);

    // Required fields
    assert_config_entry(&result.config_section, "version", "1.0.0");
    assert_config_entry(&result.config_section, "encoding", "utf-8");

    // Optional fields should have defaults
    let has_author = result.config_section.entries.iter().any(|e| e.key == "author");
    assert!(has_author, "Author should be added with default");

    teardown_test();
}

#[test]
fn test_config_with_comments() {
    setup_test();

    let input = r#"@CONFIG(
    // Version comment
    version -> "1.0.0",
    /* Multi-line
       comment */
    encoding -> "utf-8",
    author -> "Test" // Inline comment
)"#.to_string();

    let handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_section(&input);

    // Should parse despite comments
    assert_config_entry(&result.config_section, "version", "1.0.0");
    assert_config_entry(&result.config_section, "encoding", "utf-8");

    teardown_test();
}

#[test]
fn test_config_with_nested_parentheses() {
    setup_test();

    let input = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    author -> "Test (with parens)"
)"#.to_string();

    let handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_section(&input);

    assert_config_entry(&result.config_section, "author", "Test (with parens)");

    teardown_test();
}

// ==================== CONFIG SCHEMA VALIDATION TESTS ====================

#[test]
fn test_schema_validate_version() {
    setup_test();

    let mut config = std::collections::HashMap::new();
    config.insert("version".to_string(), "1.0.0".to_string());
    config.insert("encoding".to_string(), "utf-8".to_string());

    let result = ConfigSchema::validate_and_enhance_config(config);
    assert!(result.is_ok());

    let validated = result.unwrap();
    assert_eq!(validated.get("version").unwrap().as_str(), "1.0.0");

    teardown_test();
}

#[test]
fn test_schema_invalid_version() {
    setup_test();

    let mut config = std::collections::HashMap::new();
    config.insert("version".to_string(), "invalid.version".to_string());
    config.insert("encoding".to_string(), "utf-8".to_string());

    let result = ConfigSchema::validate_and_enhance_config(config);

    // Should still succeed but use default version
    assert!(result.is_ok());
    let validated = result.unwrap();

    // Invalid version should be replaced with default
    println!("Validated config: {:#?}", validated);

    teardown_test();
}

#[test]
fn test_schema_validate_encoding() {
    setup_test();

    for encoding in &["utf-8", "utf-16", "ascii", "iso-8859-1"] {
        let mut config = std::collections::HashMap::new();
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("encoding".to_string(), encoding.to_string());

        let result = ConfigSchema::validate_and_enhance_config(config);
        assert!(result.is_ok(), "Encoding {} should be valid", encoding);
    }

    teardown_test();
}

#[test]
fn test_schema_invalid_encoding() {
    setup_test();

    let mut config = std::collections::HashMap::new();
    config.insert("version".to_string(), "1.0.0".to_string());
    config.insert("encoding".to_string(), "invalid-encoding".to_string());

    let result = ConfigSchema::validate_and_enhance_config(config);
    assert!(result.is_ok()); // Should use default

    let validated = result.unwrap();
    assert_eq!(validated.get("encoding").unwrap().as_str(), "utf-8"); // Default

    teardown_test();
}

#[test]
fn test_schema_validate_features() {
    setup_test();

    for features in &["basic", "advanced", "quickfuncs,enums,dlm"] {
        let mut config = std::collections::HashMap::new();
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("encoding".to_string(), "utf-8".to_string());
        config.insert("features".to_string(), features.to_string());

        let result = ConfigSchema::validate_and_enhance_config(config);
        assert!(result.is_ok(), "Features '{}' should be valid", features);
    }

    teardown_test();
}

#[test]
fn test_schema_validate_error_handling() {
    setup_test();

    for strategy in &["halt", "continue", "recover"] {
        let mut config = std::collections::HashMap::new();
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("encoding".to_string(), "utf-8".to_string());
        config.insert("error_handling".to_string(), strategy.to_string());

        let result = ConfigSchema::validate_and_enhance_config(config);
        assert!(result.is_ok(), "Error handling '{}' should be valid", strategy);
    }

    teardown_test();
}

#[test]
fn test_schema_validate_debug_mode() {
    setup_test();

    for mode in &["off", "regular", "verbose"] {
        let mut config = std::collections::HashMap::new();
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("encoding".to_string(), "utf-8".to_string());
        config.insert("debug_mode".to_string(), mode.to_string());

        let result = ConfigSchema::validate_and_enhance_config(config);
        assert!(result.is_ok(), "Debug mode '{}' should be valid", mode);
    }

    teardown_test();
}

#[test]
fn test_schema_validate_compatibility() {
    setup_test();

    for mode in &["strict", "best_effort", "permissive"] {
        let mut config = std::collections::HashMap::new();
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("encoding".to_string(), "utf-8".to_string());
        config.insert("compatibility_mode".to_string(), mode.to_string());

        let result = ConfigSchema::validate_and_enhance_config(config);
        assert!(result.is_ok(), "Compatibility mode '{}' should be valid", mode);
    }

    teardown_test();
}

// ==================== OPERATIONAL SETTINGS TESTS ====================

#[test]
fn test_operational_settings_extraction() {
    setup_test();

    let input = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    features -> "advanced",
    debug_mode -> "verbose",
    error_handling -> "halt",
    compatibility_mode -> "strict"
)"#.to_string();

    let handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_section(&input);

    let settings = &result.operational_settings;

    assert_eq!(settings.version.as_str(), "1.0.0");
    assert_eq!(settings.error_handling_strategy, ErrorHandlingStrategy::Halt);
    assert_eq!(settings.debug_mode, DebugMode::Verbose);
    assert_eq!(settings.compatibility_mode, CompatibilityMode::Strict);
    assert!(settings.is_advanced_mode());

    println!("Extracted settings: {:#?}", settings);

    teardown_test();
}

#[test]
fn test_operational_settings_basic_mode() {
    setup_test();

    let input = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    features -> "basic"
)"#.to_string();

    let handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_section(&input);

    let settings = &result.operational_settings;
    assert!(!settings.is_advanced_mode());
    assert!(settings.is_feature_enabled("basic"));

    teardown_test();
}

#[test]
fn test_operational_settings_feature_check() {
    setup_test();

    let mut settings = OperationalSettings::default();
    settings.enabled_features = vec!["quickfuncs".to_string(), "enums".to_string()];

    assert!(settings.is_feature_enabled("quickfuncs"));
    assert!(settings.is_feature_enabled("enums"));
    assert!(!settings.is_feature_enabled("dlm"));

    teardown_test();
}

// ==================== VERSION MANAGER INTEGRATION TESTS ====================

#[test]
fn test_version_manager_initialization() {
    setup_test();

    let input = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8"
)"#.to_string();

    let handler = ConfigSectionHandler::new(None);
    let _result = handler.process_config_section(&input);

    // VersionManager should be initialized
    let version_result = VersionManager::instance().read();
    assert!(version_result.is_ok());

    let vm = version_result.unwrap();
    assert_eq!(vm.current_version(), "1.0.0");

    println!("VersionManager initialized with version: {}", vm.current_version());

    teardown_test();
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_malformed_config_recovery() {
    setup_test();

    let input = r#"@CONFIG(
    version -> "1.0.0"
    // Missing comma - malformed
    encoding -> "utf-8"
)"#.to_string();

    let handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_section(&input);

    // Should recover and use defaults
    assert!(!result.config_section.entries.is_empty());
    assert!(!result.warnings.is_empty());

    println!("Malformed config warnings: {:#?}", result.warnings);

    teardown_test();
}

#[test]
fn test_config_without_closing_paren() {
    setup_test();

    let input = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8"
    // Missing closing parenthesis
@DATA(
    value = 42
)"#.to_string();

    let handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_section(&input);

    // Should handle gracefully
    assert!(!result.config_section.entries.is_empty());

    // DATA section should still be in cleaned input
    assert!(result.cleaned_input_string.contains("@DATA"));

    println!("Unclosed paren warnings: {:#?}", result.warnings);

    teardown_test();
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_large_config_performance() {
    setup_test();

    let mut config_str = String::from("@CONFIG(\n");
    config_str.push_str("    version -> \"1.0.0\",\n");
    config_str.push_str("    encoding -> \"utf-8\",\n");

    // Add many entries
    for i in 0..100 {
        config_str.push_str(&format!("    custom_field_{} -> \"value_{}\",\n", i, i));
    }

    config_str.push_str("    author -> \"Test\"\n)");

    let handler = ConfigSectionHandler::new(None);

    let start = std::time::Instant::now();
    let result = handler.process_config_section(&config_str);
    let duration = start.elapsed();

    println!("Large config processing time: {:?}", duration);
    println!("Config entries: {}", result.config_section.entries.len());

    assert!(!result.config_section.entries.is_empty());
    assert!(duration.as_millis() < 100, "Should process quickly");

    teardown_test();
}

// ==================== INTEGRATION TESTS ====================

#[test]
fn test_full_integration_workflow() {
    setup_test();

    let input = r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    author -> "Integration Test",
    features -> "advanced",
    debug_mode -> "verbose",
    error_handling -> "halt",
    compatibility_mode -> "strict"
)

@DATA(
    test_value<int> = 42,
    test_string<string> = "Hello World"
)"#.to_string();

    let handler = ConfigSectionHandler::new(None);
    let result = handler.process_config_section(&input);

    // Verify CONFIG extracted
    assert!(!result.config_section.entries.is_empty());
    assert_config_entry(&result.config_section, "version", "1.0.0");
    assert_config_entry(&result.config_section, "author", "Integration Test");

    // Verify operational settings
    assert_eq!(result.operational_settings.version.as_str(), "1.0.0");
    assert_eq!(result.operational_settings.debug_mode, DebugMode::Verbose);
    assert!(result.operational_settings.is_advanced_mode());

    // Verify cleaned input contains DATA section
    assert!(result.cleaned_input_string.contains("@DATA"));
    assert!(result.cleaned_input_string.contains("test_value"));
    assert!(!result.cleaned_input_string.contains("@CONFIG"));

    // Verify VersionManager initialized
    let vm_result = VersionManager::instance().read();
    assert!(vm_result.is_ok());
    assert_eq!(vm_result.unwrap().current_version(), "1.0.0");

    println!("\n=== INTEGRATION TEST RESULTS ===");
    println!("Config entries: {}", result.config_section.entries.len());
    println!("Warnings: {}", result.warnings.len());
    println!("Operational settings: {:#?}", result.operational_settings);
    println!("Cleaned input length: {} bytes", result.cleaned_input_string.len());
    println!("================================\n");

    teardown_test();
}