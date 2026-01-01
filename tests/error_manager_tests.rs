//! Comprehensive tests for ErrorManager

use std::sync::Arc;
use dixscript::ErrorManager::*;
use dixscript::Utilities::{Token, TokenType};
use dixscript::DixCore::List;

#[test]
fn test_singleton_instance() {
    let instance1 = ErrorManager::get_shared_instance();
    let instance2 = ErrorManager::get_shared_instance();

    // Both should point to the same instance (Arc equality)
    assert!(Arc::ptr_eq(&instance1, &instance2));

    println!("✓ Singleton pattern works correctly");
}

#[test]
fn test_lexical_error_creation() {
    let instance = ErrorManager::get_shared_instance();
    let mut manager = instance.lock().unwrap();

    manager.clear_errors();

    manager.add_lexical_error(
        LexicalErrorType::InvalidCharacter,
        "Invalid character '@' found".to_string(),
        10,
        5,
        Some("Remove invalid character".to_string()),
        Some("let x = @ 42;".to_string()),
    );

    assert!(manager.has_errors());
    assert_eq!(manager.get_lexical_errors().Count(), 1);

    let errors = manager.get_lexical_errors();
    let error = errors.First().unwrap();

    assert_eq!(error.line, 10);
    assert_eq!(error.column, 5);
    assert!(error.error_id.starts_with("DXL"));

    println!("✓ Lexical error creation works");
    println!("  Error: {}", error);
}

#[test]
fn test_parse_error_with_token() {
    let instance = ErrorManager::get_shared_instance();
    let mut manager = instance.lock().unwrap();

    manager.clear_errors();

    let token = Token::New(TokenType::Identifier("testVar".to_string()), 15, 8);

    manager.add_parse_error_from_token(
        ParseErrorType::UndefinedReference,
        &token,
        "Variable 'testVar' is not defined".to_string(),
        None,
        Some("let result = testVar + 5;".to_string()),
    );

    assert!(manager.has_errors());
    let errors = manager.get_parse_errors();
    assert_eq!(errors.Count(), 1);

    let error = errors.First().unwrap();
    assert_eq!(error.line, 15);
    assert_eq!(error.column, 8);

    println!("✓ Parse error from token works");
}

#[test]
fn test_registry_error() {
    let instance = ErrorManager::get_shared_instance();
    let mut manager = instance.lock().unwrap();

    manager.clear_errors();

    manager.add_registry_error(
        ParseErrorType::UnknownStaticObject,
        "Unknown",
        "Method",
        20,
        10,
        Some("Unknown.Method()".to_string()),
    );

    let registry_errors = manager.get_registry_errors();
    assert_eq!(registry_errors.Count(), 1);

    let error = registry_errors.First().unwrap();
    assert_eq!(error.error_type, ParseErrorType::UnknownStaticObject);

    println!("✓ Registry error creation works");
}

#[test]
fn test_semantic_error() {
    let instance = ErrorManager::get_shared_instance();
    let mut manager = instance.lock().unwrap();

    manager.clear_errors();

    manager.add_semantic_error(
        SemanticErrorType::TypeMismatch,
        "Cannot assign string to integer".to_string(),
        25,
        12,
        Some("DATA".to_string()),
        Some("Use correct type or cast".to_string()),
    );

    let errors = manager.get_semantic_errors();
    assert_eq!(errors.Count(), 1);

    let error = errors.First().unwrap();
    assert_eq!(error.section_name, Some("DATA".to_string()));

    println!("✓ Semantic error works");
}

#[test]
fn test_value_resolution_error() {
    let instance = ErrorManager::get_shared_instance();
    let mut manager = instance.lock().unwrap();

    manager.clear_errors();

    manager.add_value_resolution_error(
        ValueResolutionErrorType::FunctionNotFound,
        "Function 'calculate' not found".to_string(),
        30,
        5,
        Some("Check @QUICKFUNCS section".to_string()),
        Some("calculate".to_string()),
        None,
        Some("@DATA".to_string()),
    );

    let errors = manager.get_value_resolution_errors();
    assert_eq!(errors.Count(), 1);

    let error = errors.First().unwrap();
    assert_eq!(error.function_name, Some("calculate".to_string()));

    println!("✓ Value resolution error works");
}

#[test]
fn test_operational_settings_update() {
    let instance = ErrorManager::get_shared_instance();
    let mut manager = instance.lock().unwrap();

    let settings = OperationalSettings {
        version: "1.0.0".to_string(),
        error_handling_strategy: ErrorHandlingStrategy::Continue,
        debug_mode: DebugMode::Verbose,
        compatibility_mode: CompatibilityMode::Permissive,
    };

    manager.update_settings(settings.clone());

    assert_eq!(manager.operational_settings.error_handling_strategy, ErrorHandlingStrategy::Continue);
    assert_eq!(manager.operational_settings.debug_mode, DebugMode::Verbose);
    assert!(manager.is_debug_enabled);

    println!("✓ Operational settings update works");
}

#[test]
fn test_error_report_generation() {
    let instance = ErrorManager::get_shared_instance();
    let mut manager = instance.lock().unwrap();

    manager.clear_errors();

    // Add multiple errors
    manager.add_lexical_error(
        LexicalErrorType::UnterminatedString,
        "String not closed".to_string(),
        5,
        10,
        None,
        None,
    );

    manager.add_parse_error(
        ParseErrorType::MissingToken,
        "Expected ';'".to_string(),
        6,
        15,
        None,
        None,
    );

    let report = manager.generate_error_report();

    assert!(report.contains("DixScript Error Report"));
    assert!(report.contains("Lexical Errors"));
    assert!(report.contains("Parse Errors"));
    assert!(report.contains("Total errors: 2"));

    println!("✓ Error report generation works");
    println!("\n{}", report);
}

#[test]
fn test_error_control_flow() {
    let instance = ErrorManager::get_shared_instance();
    let mut manager = instance.lock().unwrap();

    manager.clear_errors();

    // Set to Halt strategy
    let halt_settings = OperationalSettings {
        version: "1.0.0".to_string(),
        error_handling_strategy: ErrorHandlingStrategy::Halt,
        debug_mode: DebugMode::Regular,
        compatibility_mode: CompatibilityMode::Strict,
    };

    manager.update_settings(halt_settings);

    manager.add_lexical_error(
        LexicalErrorType::InvalidCharacter,
        "Test error".to_string(),
        1,
        1,
        None,
        None,
    );

    assert!(manager.should_terminate_parsing());
    assert!(!manager.can_continue());

    // Switch to Continue strategy
    manager.clear_errors();

    let continue_settings = OperationalSettings {
        version: "1.0.0".to_string(),
        error_handling_strategy: ErrorHandlingStrategy::Continue,
        debug_mode: DebugMode::Regular,
        compatibility_mode: CompatibilityMode::Strict,
    };

    manager.update_settings(continue_settings);

    manager.add_lexical_error(
        LexicalErrorType::InvalidCharacter,
        "Test error 2".to_string(),
        1,
        1,
        None,
        None,
    );

    assert!(!manager.should_terminate_parsing());
    assert!(manager.can_continue());

    println!("✓ Error control flow (Halt vs Continue) works");
}

#[test]
fn test_clear_errors() {
    let instance = ErrorManager::get_shared_instance();
    let mut manager = instance.lock().unwrap();

    manager.add_lexical_error(
        LexicalErrorType::InvalidCharacter,
        "Test".to_string(),
        1,
        1,
        None,
        None,
    );

    assert!(manager.has_errors());

    manager.clear_errors();

    assert!(!manager.has_errors());
    assert_eq!(manager.get_lexical_errors().Count(), 0);

    println!("✓ Clear errors works");
}