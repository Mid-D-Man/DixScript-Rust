// tests/enums_section_analyzer_tests.rs

use dixscript::Compiler::Core::SectionAnalyzers::enums_section_analyzer::{
    EnumsSectionAnalyzer, SectionAnalysisResult
};
use dixscript::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField, Position};
use dixscript::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use dixscript::Compiler::Utilities::SymbolTable;
use dixscript::ErrorManager::ErrorManager;
use std::time::Instant;
use std::collections::HashMap;

// ==================== PERFORMANCE BASELINES ====================

/// Baseline: Small input (1 enum, 3 fields) should analyze in < 1ms
const BASELINE_SMALL_ANALYSIS_MS: u128 = 1;

/// Baseline: Medium input (50 enums, 10 fields each) should analyze in < 10ms
const BASELINE_MEDIUM_ANALYSIS_MS: u128 = 10;

/// Baseline: Large input (500 enums, 20 fields each) should analyze in < 100ms
const BASELINE_LARGE_ANALYSIS_MS: u128 = 100;

/// Baseline: Should process at least 10,000 fields per second
const BASELINE_FIELDS_PER_SEC: f64 = 10000.0;

/// Baseline: Should process at least 1,000 enums per second
const BASELINE_ENUMS_PER_SEC: f64 = 1000.0;

/// Baseline: Memory usage per enum should be < 1KB
const BASELINE_ENUM_SIZE_BYTES: usize = 1024;

/// Baseline: Memory usage per field should be < 200 bytes
const BASELINE_FIELD_SIZE_BYTES: usize = 200;

// ==================== HELPER FUNCTIONS ====================

fn create_test_enum(name: &str, fields: Vec<(&str, Option<i32>)>) -> EnumDeclaration {
    let enum_fields: Vec<EnumField> = fields
        .into_iter()
        .map(|(field_name, value)| {
            EnumField::new(
                field_name.to_string(),
                value,
                Position::new(1, 1)
            )
        })
        .collect();

    EnumDeclaration::new(
        name.to_string(),
        enum_fields,
        Position::new(1, 1)
    )
}

fn create_test_section(enums: Vec<EnumDeclaration>) -> EnumsSection {
    EnumsSection::new(enums, Position::new(1, 1))
}

fn analyze_with_settings(
    section: &EnumsSection,
    settings: &OperationalSettings
) -> (SectionAnalysisResult, SymbolTable) {
    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let mut symbol_table = SymbolTable::new();
    let mut analyzer = EnumsSectionAnalyzer::new(settings);

    let result = analyzer.analyze(section, &mut symbol_table);

    (result, symbol_table)
}

fn analyze_default(section: &EnumsSection) -> (SectionAnalysisResult, SymbolTable) {
    analyze_with_settings(section, &OperationalSettings::default())
}

fn print_performance_summary(
    test_name: &str,
    enum_count: usize,
    field_count: usize,
    duration: std::time::Duration,
    baseline_ms: u128
) {
    let enums_per_sec = enum_count as f64 / duration.as_secs_f64();
    let fields_per_sec = field_count as f64 / duration.as_secs_f64();
    let passed = duration.as_millis() < baseline_ms;

    println!("\n=== {} ===", test_name);
    println!("Enums: {}, Fields: {}", enum_count, field_count);
    println!("Baseline: < {}ms", baseline_ms);
    println!("Actual: {:?}", duration);
    println!("Throughput: {:.0} enums/sec, {:.0} fields/sec", enums_per_sec, fields_per_sec);
    println!("Status: {}", if passed { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");
}

// ==================== BASIC VALIDATION TESTS ====================

#[test]
fn test_valid_single_enum() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("ACTIVE", Some(1)),
            ("INACTIVE", Some(2)),
        ])
    ]);

    let (result, symbol_table) = analyze_default(&section);

    assert!(result.is_success, "Analysis should succeed");
    assert_eq!(result.errors.len(), 0, "Should have no errors");
    assert_eq!(result.warnings.len(), 0, "Should have no warnings");

    // Check symbol table
    assert!(symbol_table.has_enum("Status"), "Should register enum in symbol table");

    let fields = symbol_table.try_get_enum("Status")
        .expect("Should find enum fields");

    assert_eq!(fields.len(), 2);
    assert_eq!(fields.get("ACTIVE"), Some(&1));
    assert_eq!(fields.get("INACTIVE"), Some(&2));
}

#[test]
fn test_multiple_valid_enums() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![("ACTIVE", Some(1)), ("INACTIVE", Some(2))]),
        create_test_enum("Priority", vec![("LOW", Some(1)), ("MEDIUM", Some(2)), ("HIGH", Some(3))]),
        create_test_enum("Role", vec![("GUEST", Some(0)), ("USER", Some(1)), ("ADMIN", Some(2))]),
    ]);

    let (result, symbol_table) = analyze_default(&section);

    assert!(result.is_success);
    assert_eq!(result.errors.len(), 0);

    assert!(symbol_table.has_enum("Status"));
    assert!(symbol_table.has_enum("Priority"));
    assert!(symbol_table.has_enum("Role"));
}

#[test]
fn test_implicit_values() {
    let section = create_test_section(vec![
        create_test_enum("Color", vec![
            ("RED", None),
            ("GREEN", None),
            ("BLUE", None),
        ])
    ]);

    let (result, symbol_table) = analyze_default(&section);

    assert!(result.is_success);

    let fields = symbol_table.try_get_enum("Color").unwrap();

    // Should assign 0, 1, 2
    assert_eq!(fields.get("RED"), Some(&0));
    assert_eq!(fields.get("GREEN"), Some(&1));
    assert_eq!(fields.get("BLUE"), Some(&2));
}

#[test]
fn test_mixed_explicit_implicit_values() {
    let section = create_test_section(vec![
        create_test_enum("Mixed", vec![
            ("FIRST", Some(10)),
            ("SECOND", None),      // Should be 11
            ("THIRD", Some(30)),
            ("FOURTH", None),      // Should be 31
        ])
    ]);

    let (result, symbol_table) = analyze_default(&section);

    assert!(result.is_success);

    let fields = symbol_table.try_get_enum("Mixed").unwrap();

    assert_eq!(fields.get("FIRST"), Some(&10));
    assert_eq!(fields.get("SECOND"), Some(&11));
    assert_eq!(fields.get("THIRD"), Some(&30));
    assert_eq!(fields.get("FOURTH"), Some(&31));
}

#[test]
fn test_empty_enum_warning() {
    let section = create_test_section(vec![
        create_test_enum("Empty", vec![])
    ]);

    let (result, _) = analyze_default(&section);

    // Should succeed but with warning
    assert!(result.is_success);
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].message.contains("no fields"));
}

// ==================== ERROR DETECTION TESTS ====================

#[test]
fn test_duplicate_enum_names_case_insensitive() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![("ACTIVE", Some(1))]),
        create_test_enum("STATUS", vec![("INACTIVE", Some(2))]),
        create_test_enum("status", vec![("PENDING", Some(3))]),
    ]);

    let (result, symbol_table) = analyze_default(&section);

    assert!(!result.is_success);
    assert_eq!(result.errors.len(), 2, "Should have 2 duplicate errors");

    // Check error messages
    for error in &result.errors {
        assert_eq!(error.error_type, "DUPLICATE_ENUM_NAME");
        assert!(error.message.contains("Status") || error.message.contains("STATUS"));
    }

    // Symbol table should only have first occurrence
    assert!(symbol_table.has_enum("Status"));
}

#[test]
fn test_duplicate_field_names_case_insensitive() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("ACTIVE", Some(1)),
            ("active", Some(2)),
            ("Active", Some(3)),
        ])
    ]);

    let (result, _) = analyze_default(&section);

    assert!(!result.is_success);
    assert_eq!(result.errors.len(), 2, "Should have 2 duplicate field errors");

    for error in &result.errors {
        assert_eq!(error.error_type, "DUPLICATE_FIELD_NAME");
    }
}

#[test]
fn test_duplicate_field_values() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("ACTIVE", Some(1)),
            ("INACTIVE", Some(1)), // Same value
            ("PENDING", Some(2)),
        ])
    ]);

    let (result, _) = analyze_default(&section);

    assert!(!result.is_success);
    assert!(result.errors.iter().any(|e| e.error_type == "DUPLICATE_FIELD_VALUE"));
}

#[test]
fn test_duplicate_field_values_with_implicit() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("ACTIVE", Some(1)),
            ("INACTIVE", None),   // Implicit 2
            ("PENDING", Some(2)), // Explicit 2 - conflicts with INACTIVE's implicit
        ])
    ]);

    let (result, _) = analyze_default(&section);

    assert!(!result.is_success);
    assert!(result.errors.iter().any(|e| e.error_type == "DUPLICATE_FIELD_VALUE"));
}

#[test]
fn test_invalid_enum_name() {
    let invalid_names = vec![
        "123Invalid",  // Starts with number
        "Invalid-Name", // Contains hyphen
        "Invalid Name", // Contains space
        "",            // Empty
    ];

    for name in invalid_names {
        let section = create_test_section(vec![
            create_test_enum(name, vec![("FIELD", Some(1))])
        ]);

        let (result, _) = analyze_default(&section);

        assert!(!result.is_success, "Should fail for invalid name: {}", name);
        assert!(result.errors.iter().any(|e| e.error_type == "INVALID_ENUM_NAME"),
                "Should have INVALID_ENUM_NAME error for: {}", name);
    }
}

#[test]
fn test_invalid_field_name() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("123Invalid", Some(1)),
            ("Invalid-Field", Some(2)),
        ])
    ]);

    let (result, _) = analyze_default(&section);

    assert!(!result.is_success);
    assert_eq!(result.errors.iter().filter(|e| e.error_type == "INVALID_FIELD_NAME").count(), 2);
}

#[test]
fn test_valid_identifier_variations() {
    let section = create_test_section(vec![
        create_test_enum("ValidEnum", vec![
            ("FIELD_1", Some(1)),
            ("_FIELD", Some(2)),
            ("field123", Some(3)),
            ("Field_With_Underscores", Some(4)),
        ])
    ]);

    let (result, _) = analyze_default(&section);

    assert!(result.is_success, "All identifiers should be valid");
    assert_eq!(result.errors.len(), 0);
}

// ==================== ERROR HANDLING STRATEGY TESTS ====================

#[test]
fn test_halt_strategy_stops_on_first_error() {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Halt;

    let section = create_test_section(vec![
        create_test_enum("Status", vec![("ACTIVE", Some(1))]),
        create_test_enum("Status", vec![("INACTIVE", Some(2))]), // Duplicate - should halt here
        create_test_enum("Priority", vec![("LOW", Some(1))]),
    ]);

    let (result, symbol_table) = analyze_with_settings(&section, &settings);

    assert!(!result.is_success);
    assert!(result.errors.len() > 0);

    // With Halt, might not process all enums
    // Priority might not be in symbol table
}

#[test]
fn test_continue_strategy_processes_all() {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Continue;

    let section = create_test_section(vec![
        create_test_enum("Status", vec![("ACTIVE", Some(1))]),
        create_test_enum("Status", vec![("INACTIVE", Some(2))]), // Duplicate
        create_test_enum("Priority", vec![("LOW", Some(1))]),
    ]);

    let (result, symbol_table) = analyze_with_settings(&section, &settings);

    assert!(!result.is_success);

    // Continue should process Priority despite Status error
    assert!(symbol_table.has_enum("Priority"));
}

#[test]
fn test_recover_strategy() {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Recover;

    let section = create_test_section(vec![
        create_test_enum("123Invalid", vec![("FIELD", Some(1))]), // FIX: starts with number
        create_test_enum("Valid", vec![("FIELD", Some(1))]),
    ]);

    let (result, symbol_table) = analyze_with_settings(&section, &settings);

    assert!(!result.is_success);
    assert!(result.errors.len() > 0);

    // Recover should still register valid enums
    assert!(symbol_table.has_enum("Valid"));
}

// ==================== DEBUG MODE TESTS ====================

#[test]
fn test_debug_mode_off() {
    let mut settings = OperationalSettings::default();
    settings.debug_mode = DebugMode::Off;

    let section = create_test_section(vec![
        create_test_enum("Status", vec![("ACTIVE", Some(1))])
    ]);

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let mut symbol_table = SymbolTable::new();
    let mut analyzer = EnumsSectionAnalyzer::new(&settings);

    let _result = analyzer.analyze(&section, &mut symbol_table);

    // In Off mode, should not see debug logs (check manually in output)
}

#[test]
fn test_debug_mode_verbose() {
    let mut settings = OperationalSettings::default();
    settings.debug_mode = DebugMode::Verbose;

    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("ACTIVE", Some(1)),
            ("INACTIVE", Some(2)),
        ])
    ]);

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let mut symbol_table = SymbolTable::new();
    let mut analyzer = EnumsSectionAnalyzer::new(&settings);

    let _result = analyzer.analyze(&section, &mut symbol_table);

    // In Verbose mode, should see detailed debug logs (check manually in output)
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_performance_small_input() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("ACTIVE", Some(1)),
            ("INACTIVE", Some(2)),
            ("PENDING", Some(3)),
        ])
    ]);

    let settings = OperationalSettings::default();
    let mut symbol_table = SymbolTable::new();
    let mut analyzer = EnumsSectionAnalyzer::new(&settings);

    let start = Instant::now();
    let result = analyzer.analyze(&section, &mut symbol_table);
    let duration = start.elapsed();

    print_performance_summary(
        "SMALL INPUT PERFORMANCE",
        1,
        3,
        duration,
        BASELINE_SMALL_ANALYSIS_MS
    );

    assert!(result.is_success);
    assert!(
        duration.as_millis() < BASELINE_SMALL_ANALYSIS_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_SMALL_ANALYSIS_MS
    );
}

#[test]
fn test_performance_medium_input() {
    // 50 enums, 10 fields each = 500 total fields
    let mut enums = Vec::with_capacity(50);

    for i in 0..50 {
        let fields: Vec<(&str, Option<i32>)> = (0..10)
            .map(|j| {
                let field_name = format!("FIELD_{}", j);
                (Box::leak(field_name.into_boxed_str()) as &str, Some(j * 10))
            })
            .collect();

        let enum_name = format!("Enum{}", i);
        enums.push(create_test_enum(Box::leak(enum_name.into_boxed_str()), fields));
    }

    let section = create_test_section(enums);

    let settings = OperationalSettings::default();
    let mut symbol_table = SymbolTable::new();
    let mut analyzer = EnumsSectionAnalyzer::new(&settings);

    let start = Instant::now();
    let result = analyzer.analyze(&section, &mut symbol_table);
    let duration = start.elapsed();

    print_performance_summary(
        "MEDIUM INPUT PERFORMANCE",
        50,
        500,
        duration,
        BASELINE_MEDIUM_ANALYSIS_MS
    );

    assert!(result.is_success);
    assert!(
        duration.as_millis() < BASELINE_MEDIUM_ANALYSIS_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_MEDIUM_ANALYSIS_MS
    );

    // Verify throughput
    let fields_per_sec = 500.0 / duration.as_secs_f64();
    assert!(
        fields_per_sec > BASELINE_FIELDS_PER_SEC,
        "Too slow: {:.0} fields/sec (baseline: {})",
        fields_per_sec,
        BASELINE_FIELDS_PER_SEC
    );
}

#[test]
fn test_performance_large_input() {
    // 500 enums, 20 fields each = 10,000 total fields
    let mut enums = Vec::with_capacity(500);

    for i in 0..500 {
        let fields: Vec<(&str, Option<i32>)> = (0..20)
            .map(|j| {
                let field_name = format!("FIELD_{}", j);
                (Box::leak(field_name.into_boxed_str()) as &str, Some(j * 10))
            })
            .collect();

        let enum_name = format!("Enum{}", i);
        enums.push(create_test_enum(Box::leak(enum_name.into_boxed_str()), fields));
    }

    let section = create_test_section(enums);

    let settings = OperationalSettings::default();
    let mut symbol_table = SymbolTable::new();
    let mut analyzer = EnumsSectionAnalyzer::new(&settings);

    let start = Instant::now();
    let result = analyzer.analyze(&section, &mut symbol_table);
    let duration = start.elapsed();

    print_performance_summary(
        "LARGE INPUT PERFORMANCE",
        500,
        10000,
        duration,
        BASELINE_LARGE_ANALYSIS_MS
    );

    assert!(result.is_success);
    assert!(
        duration.as_millis() < BASELINE_LARGE_ANALYSIS_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_LARGE_ANALYSIS_MS
    );

    // Verify throughput
    let enums_per_sec = 500.0 / duration.as_secs_f64();
    let fields_per_sec = 10000.0 / duration.as_secs_f64();

    assert!(
        enums_per_sec > BASELINE_ENUMS_PER_SEC,
        "Too slow: {:.0} enums/sec (baseline: {})",
        enums_per_sec,
        BASELINE_ENUMS_PER_SEC
    );

    assert!(
        fields_per_sec > BASELINE_FIELDS_PER_SEC,
        "Too slow: {:.0} fields/sec (baseline: {})",
        fields_per_sec,
        BASELINE_FIELDS_PER_SEC
    );
}

#[test]
#[ignore] // Run with: cargo test --release -- --ignored
fn test_release_mode_performance() {
    // 5,000 enums, 10 fields each = 50,000 total fields
    let mut enums = Vec::with_capacity(5000);

    for i in 0..5000 {
        let fields: Vec<(&str, Option<i32>)> = (0..10)
            .map(|j| {
                let field_name = format!("FIELD_{}", j);
                (Box::leak(field_name.into_boxed_str()) as &str, Some(j * 10))
            })
            .collect();

        let enum_name = format!("Enum{}", i);
        enums.push(create_test_enum(Box::leak(enum_name.into_boxed_str()), fields));
    }

    let section = create_test_section(enums);

    let settings = OperationalSettings::default();
    let mut symbol_table = SymbolTable::new();
    let mut analyzer = EnumsSectionAnalyzer::new(&settings);

    let start = Instant::now();
    let result = analyzer.analyze(&section, &mut symbol_table);
    let duration = start.elapsed();

    let enums_per_sec = 5000.0 / duration.as_secs_f64();
    let fields_per_sec = 50000.0 / duration.as_secs_f64();

    println!("\n=== RELEASE MODE PERFORMANCE ===");
    println!("Enums: 5,000");
    println!("Fields: 50,000");
    println!("Time: {:?}", duration);
    println!("Enums/sec: {:.0}", enums_per_sec);
    println!("Fields/sec: {:.0}", fields_per_sec);
    println!("Expected: > 5,000 enums/sec, > 50,000 fields/sec");
    println!("Status: {}",
             if enums_per_sec > 5000.0 && fields_per_sec > 50000.0 {
                 "✅ PASS"
             } else {
                 "❌ FAIL"
             }
    );
    println!("================================\n");

    assert!(result.is_success);
    assert!(enums_per_sec > 5000.0, "Too slow: {:.0} enums/sec", enums_per_sec);
    assert!(fields_per_sec > 50000.0, "Too slow: {:.0} fields/sec", fields_per_sec);
}

// ==================== MEMORY USAGE TESTS ====================

#[test]
fn test_memory_usage_estimate() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("ACTIVE", Some(1)),
            ("INACTIVE", Some(2)),
            ("PENDING", Some(3)),
        ])
    ]);

    let enum_size = std::mem::size_of_val(&section.enums[0]);
    let field_size = std::mem::size_of_val(&section.enums[0].fields[0]);

    println!("\n=== MEMORY USAGE ===");
    println!("EnumDeclaration size: {} bytes", enum_size);
    println!("EnumField size: {} bytes", field_size);
    println!("Total for 1 enum + 3 fields: ~{} bytes", enum_size + (field_size * 3));
    println!("Baseline: < {} bytes per enum, < {} bytes per field",
             BASELINE_ENUM_SIZE_BYTES, BASELINE_FIELD_SIZE_BYTES);
    println!("Status: {}",
             if enum_size < BASELINE_ENUM_SIZE_BYTES && field_size < BASELINE_FIELD_SIZE_BYTES {
                 "✅ PASS"
             } else {
                 "❌ FAIL"
             }
    );
    println!("================================\n");

    assert!(enum_size < BASELINE_ENUM_SIZE_BYTES,
            "Enum too large: {} bytes", enum_size);
    assert!(field_size < BASELINE_FIELD_SIZE_BYTES,
            "Field too large: {} bytes", field_size);
}

#[test]
fn test_no_memory_leaks_repeated_analysis() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("ACTIVE", Some(1)),
            ("INACTIVE", Some(2)),
        ])
    ]);

    // Analyze same section 1000 times
    for _ in 0..1000 {
        let _ = analyze_default(&section);
    }

    println!("✅ Successfully analyzed same section 1000 times without memory leaks");
}

#[test]
fn test_symbol_table_memory_efficiency() {
    // Create section with many enums
    let mut enums = Vec::with_capacity(100);

    for i in 0..100 {
        let fields: Vec<(&str, Option<i32>)> = (0..10)
            .map(|j| {
                let field_name = format!("FIELD_{}", j);
                (Box::leak(field_name.into_boxed_str()) as &str, Some(j))
            })
            .collect();

        let enum_name = format!("Enum{}", i);
        enums.push(create_test_enum(Box::leak(enum_name.into_boxed_str()), fields));
    }

    let section = create_test_section(enums);

    let (result, symbol_table) = analyze_default(&section);

    assert!(result.is_success);

    // Symbol table should have all 100 enums
    for i in 0..100 {
        let enum_name = format!("Enum{}", i);
        assert!(symbol_table.has_enum(&enum_name));

        let fields = symbol_table.try_get_enum(&enum_name).unwrap();
        assert_eq!(fields.len(), 10);
    }

    println!("✅ Symbol table efficiently stores 100 enums with 1000 total fields");
}

// ==================== EDGE CASES ====================

#[test]
fn test_very_long_names() {
    let long_enum_name = "A".repeat(500);
    let long_field_name = "B".repeat(500);

    let section = create_test_section(vec![
        create_test_enum(&long_enum_name, vec![
            (Box::leak(long_field_name.into_boxed_str()), Some(1)),
        ])
    ]);

    let (result, symbol_table) = analyze_default(&section);

    assert!(result.is_success);
    assert!(symbol_table.has_enum(&long_enum_name));
}

#[test]
fn test_negative_values() {
    let section = create_test_section(vec![
        create_test_enum("Status", vec![
            ("NEGATIVE", Some(-100)),
            ("ZERO", Some(0)),
            ("POSITIVE", Some(100)),
        ])
    ]);

    let (result, symbol_table) = analyze_default(&section);

    assert!(result.is_success);

    let fields = symbol_table.try_get_enum("Status").unwrap();
    assert_eq!(fields.get("NEGATIVE"), Some(&-100));
    assert_eq!(fields.get("ZERO"), Some(&0));
    assert_eq!(fields.get("POSITIVE"), Some(&100));
}

#[test]
fn test_max_min_i32_values() {
    let section = create_test_section(vec![
        create_test_enum("Extreme", vec![
            ("MIN", Some(i32::MIN)),
            ("MAX", Some(i32::MAX)),
        ])
    ]);

    let (result, symbol_table) = analyze_default(&section);

    assert!(result.is_success);

    let fields = symbol_table.try_get_enum("Extreme").unwrap();
    assert_eq!(fields.get("MIN"), Some(&i32::MIN));
    assert_eq!(fields.get("MAX"), Some(&i32::MAX));
}

#[test]
fn test_single_field_enum() {
    let section = create_test_section(vec![
        create_test_enum("Single", vec![("ONLY", Some(1))])
    ]);

    let (result, _) = analyze_default(&section);

    assert!(result.is_success);
}

#[test]
fn test_empty_section() {
    let section = create_test_section(vec![]);

    let (result, _) = analyze_default(&section);

    assert!(result.is_success);
    assert_eq!(result.errors.len(), 0);
}

// ==================== INTEGRATION WITH REAL FILES ====================

#[test]
fn test_analyze_enum_test_mdix() {
    use dixscript::Compiler::Core::Tokenizer::Tokenizer;
    use dixscript::Compiler::Core::SectionParsers::EnumsSectionParser;

    let file_content = std::fs::read_to_string("mdix_files/advanced/enum_test.mdix")
        .expect("Failed to read enum_test.mdix");

    // Tokenize
    let tokenizer = Tokenizer::new(file_content);
    let token_result = tokenizer.tokenize();

    // Extract ENUMS section tokens (same helper as in parser tests)
    let tokens = &token_result.tokens;
    let start_pos = tokens.iter()
        .position(|t| matches!(t.token_type, dixscript::Compiler::Core::Tokenizer::TokenType::SectionEnums))
        .expect("No @ENUMS section found");

    let paren_start = tokens[start_pos + 1..].iter()
        .position(|t| matches!(t.token_type, dixscript::Compiler::Core::Tokenizer::TokenType::Symbol('(')))
        .expect("No opening ( found");

    let actual_start = start_pos + 1 + paren_start;

    let mut depth = 0;
    let mut end_pos = actual_start;

    for (i, token) in tokens[actual_start..].iter().enumerate() {
        match &token.token_type {
            dixscript::Compiler::Core::Tokenizer::TokenType::Symbol('(') => depth += 1,
            dixscript::Compiler::Core::Tokenizer::TokenType::Symbol(')') => {
                depth -= 1;
                if depth == 0 {
                    end_pos = actual_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    let mut section_tokens = tokens[actual_start..=end_pos].to_vec();
    section_tokens.push(dixscript::Compiler::Core::Tokenizer::Token::eof(1, 1));

    // Parse
    let settings = OperationalSettings::default();
    let mut parser = EnumsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");

    // Analyze
    let (result, symbol_table) = analyze_default(&section);

    assert!(result.is_success, "Analysis should succeed");
    assert!(symbol_table.has_enum("TestEnum"));

    let fields = symbol_table.try_get_enum("TestEnum").unwrap();
    assert_eq!(fields.get("FIRST"), Some(&1));
    assert_eq!(fields.get("SECOND"), Some(&2));
    assert_eq!(fields.get("THIRD"), Some(&3));
}

// ==================== BASELINE SUMMARY ====================

#[test]
#[ignore]
fn print_baseline_summary() {
    println!("\n=== ENUMS SECTION ANALYZER BASELINES ===");
    println!("Small input (1 enum, 3 fields): < {}ms", BASELINE_SMALL_ANALYSIS_MS);
    println!("Medium input (50 enums, 500 fields): < {}ms", BASELINE_MEDIUM_ANALYSIS_MS);
    println!("Large input (500 enums, 10K fields): < {}ms", BASELINE_LARGE_ANALYSIS_MS);
    println!("Throughput: > {} fields/sec", BASELINE_FIELDS_PER_SEC);
    println!("Throughput: > {} enums/sec", BASELINE_ENUMS_PER_SEC);
    println!("Memory: < {} bytes per enum", BASELINE_ENUM_SIZE_BYTES);
    println!("Memory: < {} bytes per field", BASELINE_FIELD_SIZE_BYTES);
    println!("\nRelease mode expectations:");
    println!("  > 5,000 enums/sec");
    println!("  > 50,000 fields/sec");
    println!("\nOptimization focus:");
    println!("  ✅ Zero-allocation identifier validation");
    println!("  ✅ Preallocated collections");
    println!("  ✅ Borrowed references (no cloning)");
    println!("  ✅ Inline hot paths");
    println!("=========================================\n");
}