// tests/error_manager_tests.rs

use dixscript::ErrorManager::*;
use dixscript::Utilities::{Token, TokenType};

// ==================== SINGLETON & THREAD SAFETY ====================

#[test]
fn test_singleton_instance() {
    let instance1 = ErrorManager::get_shared_instance();
    let instance2 = ErrorManager::get_shared_instance();

    // Test that both instances behave as the same singleton
    instance1.clear_errors();
    instance1.add_lexical_error(
        LexicalErrorType::InvalidCharacter,
        "Test".to_string(),
        1,
        1,
        None,
        None,
    );

    // Both should see the same error
    assert_eq!(instance2.get_lexical_errors().len(), 1);
    instance1.clear_errors();
}

#[test]
fn test_concurrent_error_additions() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let mgr = manager.clone();
            std::thread::spawn(move || {
                mgr.add_lexical_error(
                    LexicalErrorType::InvalidCharacter,
                    format!("Error from thread {}", i),
                    i,
                    i,
                    None,
                    None,
                );
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let errors = manager.get_lexical_errors();
    assert_eq!(errors.len(), 10);
    manager.clear_errors();
}

// ==================== ERROR ADDITIONS ====================

#[test]
fn test_lexical_error_creation() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    manager.add_lexical_error(
        LexicalErrorType::InvalidCharacter,
        "Invalid character '@'".to_string(),
        10,
        5,
        Some("Remove invalid character".to_string()),
        Some("let x = @ 42;".to_string()),
    );

    let errors = manager.get_lexical_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 10);
    assert_eq!(errors[0].column, 5);
    assert!(errors[0].error_id.starts_with("DXL"));
}

#[test]
fn test_parse_error_creation() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    manager.add_parse_error(
        ParseErrorType::UnexpectedToken,
        "Unexpected token".to_string(),
        15,
        8,
        Some("Check syntax".to_string()),
        Some("let x = ;".to_string()),
    );

    let errors = manager.get_parse_errors();
    assert_eq!(errors.len(), 1);
    assert!(!errors[0].quick_fixes.is_empty());
}

#[test]
fn test_semantic_error_creation() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    manager.add_semantic_error(
        SemanticErrorType::TypeMismatch,
        "Type mismatch".to_string(),
        20,
        10,
        Some("DATA".to_string()),
        Some("Use correct type".to_string()),
    );

    let errors = manager.get_semantic_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].section_name.as_deref(), Some("DATA"));
}

#[test]
fn test_imports_resolution_error() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    manager.add_imports_resolution_error(
        ImportsResolutionErrorType::CircularDependency,
        "Circular import detected".to_string(),
        "utils".to_string(),
        Some("utils.mdix".to_string()),
        Some("/path/to/utils.mdix".to_string()),
        Some(vec!["a.mdix".to_string(), "b.mdix".to_string(), "a.mdix".to_string()]),
        5,
        10,
        Some("Break the circular dependency".to_string()),
    );

    let errors = manager.get_imports_resolution_errors();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].circular_chain.is_some());
}

// ==================== OPERATIONAL SETTINGS ====================

#[test]
fn test_operational_settings_update() {
    let manager = ErrorManager::get_shared_instance();

    let settings = OperationalSettings {
        error_handling_strategy: ErrorHandlingStrategy::Continue,
        compatibility_mode: CompatibilityMode::Permissive,
        debug_mode: DebugMode::Verbose,
        skip_imports_resolution: true,
        source_file_path: Some("test.mdix".to_string()),
        enabled_features: vec!["advanced".to_string()],
        version: "1.0.0".to_string(),
    };

    manager.update_settings(settings);

    assert!(!manager.should_terminate_parsing());
}

#[test]
fn test_feature_flags() {
    let settings = OperationalSettings::default();
    assert!(settings.is_advanced_mode());
    assert!(settings.is_feature_enabled("advanced"));
    assert!(!settings.is_feature_enabled("basic"));

    let mut basic_settings = OperationalSettings::default();
    basic_settings.enabled_features = vec!["basic".to_string()];
    assert!(!basic_settings.is_advanced_mode());
    assert!(basic_settings.is_feature_enabled("basic"));
}

// ==================== ERROR CONTROL FLOW ====================

#[test]
fn test_halt_strategy() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    let settings = OperationalSettings {
        error_handling_strategy: ErrorHandlingStrategy::Halt,
        ..Default::default()
    };
    manager.update_settings(settings);

    manager.add_lexical_error(
        LexicalErrorType::InvalidCharacter,
        "Test error".to_string(),
        1,
        1,
        None,
        None,
    );

    assert!(manager.should_terminate_parsing());
    assert!(manager.has_errors());
}

#[test]
fn test_continue_strategy() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    let settings = OperationalSettings {
        error_handling_strategy: ErrorHandlingStrategy::Continue,
        ..Default::default()
    };
    manager.update_settings(settings);

    manager.add_parse_error(
        ParseErrorType::MissingToken,
        "Missing semicolon".to_string(),
        5,
        10,
        None,
        None,
    );

    assert!(!manager.should_terminate_parsing());
    assert!(manager.has_errors());
}

#[test]
fn test_fatal_error_detection() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    let settings = OperationalSettings {
        error_handling_strategy: ErrorHandlingStrategy::Halt,
        ..Default::default()
    };
    manager.update_settings(settings);

    manager.add_lexical_error(
        LexicalErrorType::UnterminatedString,
        "Unterminated string".to_string(),
        1,
        1,
        None,
        None,
    );

    assert!(manager.has_fatal_errors());
}

// ==================== ERROR REPORTING ====================

#[test]
fn test_error_report_generation() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    manager.add_lexical_error(
        LexicalErrorType::InvalidCharacter,
        "Test lexical error".to_string(),
        5,
        10,
        None,
        None,
    );

    manager.add_parse_error(
        ParseErrorType::UnexpectedToken,
        "Test parse error".to_string(),
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
}

#[test]
fn test_json_serialization() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    manager.add_config_error(
        ConfigErrorType::InvalidVersion,
        "Invalid version".to_string(),
        Some("CONFIG".to_string()),
        Some("version".to_string()),
        Some("1.0.0".to_string()),
        Some("invalid".to_string()),
        1,
        1,
        Some("Use valid version".to_string()),
    );

    let json = manager.get_all_errors_as_json(false).unwrap();
    assert!(json.contains("\"config\""));
    assert!(json.contains("Invalid version"));

    let pretty_json = manager.get_all_errors_as_json(true).unwrap();
    assert!(pretty_json.len() > json.len()); // Pretty print is longer
}

#[test]
fn test_error_counts_by_severity() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    let halt_settings = OperationalSettings {
        error_handling_strategy: ErrorHandlingStrategy::Halt,
        ..Default::default()
    };
    manager.update_settings(halt_settings);

    manager.add_lexical_error(
        LexicalErrorType::InvalidCharacter,
        "Fatal error".to_string(),
        1,
        1,
        None,
        None,
    );

    let continue_settings = OperationalSettings {
        error_handling_strategy: ErrorHandlingStrategy::Continue,
        ..Default::default()
    };
    manager.update_settings(continue_settings);

    manager.add_semantic_error(
        SemanticErrorType::TypeMismatch,
        "Warning error".to_string(),
        2,
        2,
        None,
        None,
    );

    let counts = manager.get_error_counts_by_severity();

    // Just verify we have some counts
    let total: usize = counts.values().sum();
    assert!(total >= 2);
}

// ==================== DIAGNOSTIC DUMPER ====================

#[test]
fn test_diagnostic_dump_generation() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    manager.add_config_error(
        ConfigErrorType::MissingRequiredField,
        "Missing required field".to_string(),
        Some("CONFIG".to_string()),
        Some("version".to_string()),
        None,
        None,
        1,
        1,
        Some("Add required field".to_string()),
    );

    let dumper = DiagnosticDumper::new();
    let dump = dumper.generate_dump();

    assert!(dump.contains("DIXSCRIPT DIAGNOSTIC DUMP"));
    assert!(dump.contains("CONFIG ERRORS"));
}

#[test]
fn test_diagnostic_dump_to_file() {
    let dumper = DiagnosticDumper::new();

    let result = dumper.dump_to_file("test_diagnostic_dump.txt");

    match result {
        Ok(filepath) => {
            assert!(filepath.ends_with("test_diagnostic_dump.txt"));
            // Clean up
            let _ = std::fs::remove_file(&filepath);
        }
        Err(e) => panic!("Failed to dump to file: {}", e),
    }
}

// ==================== EDGE CASES ====================

#[test]
fn test_clear_errors() {
    let manager = ErrorManager::get_shared_instance();

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
    assert_eq!(manager.get_lexical_errors().len(), 0);
}

#[test]
fn test_empty_error_manager() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    assert!(!manager.has_errors());
    assert!(!manager.has_fatal_errors());

    let report = manager.generate_error_report();
    assert!(report.contains("No errors detected"));
}

#[test]
fn test_registry_errors() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    manager.add_parse_error(
        ParseErrorType::UnknownStaticObject,
        "Unknown object".to_string(),
        10,
        5,
        None,
        None,
    );

    manager.add_parse_error(
        ParseErrorType::UnknownStaticMethod,
        "Unknown method".to_string(),
        11,
        6,
        None,
        None,
    );

    let registry_errors = manager.get_registry_errors();
    assert_eq!(registry_errors.len(), 2);
}

#[test]
fn test_log_delegation() {
    let manager = ErrorManager::get_shared_instance();

    manager.log_debug("Debug message");
    manager.log_info("Info message");
    manager.log_Warning("Warning message");
    manager.log_error("Error message");

    let log_contents = manager.get_log_contents();
    assert!(log_contents.contains("Debug message"));
    assert!(log_contents.contains("Error message"));
}

#[test]
fn test_debug_info() {
    let manager = ErrorManager::get_shared_instance();
    manager.clear_errors();

    let debug_info = manager.get_debug_info();

    assert!(debug_info.contains_key("version"));
    assert!(debug_info.contains_key("has_errors"));
    assert!(debug_info.contains_key("total_errors"));
    assert_eq!(debug_info.get("has_errors").unwrap().as_str(), "false");
}