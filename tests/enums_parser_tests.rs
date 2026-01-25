// tests/enums_parser_tests.rs

use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::SectionParsers::EnumsSectionParser;
use dixscript::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use dixscript::ErrorManager::ErrorManager;
use dixscript::Compiler::AST::EnumsSection;
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use std::time::Instant;

// ==================== HELPER FUNCTIONS ====================

fn tokenize_input(input: &str) -> Vec<Token> {
    let tokenizer = Tokenizer::new(input.to_string());
    let result = tokenizer.tokenize();
    result.tokens
}

fn extract_enums_section_tokens(tokens: &[Token]) -> Vec<Token> {
    // Find @ENUMS section start
    let start_pos = tokens.iter()
        .position(|t| matches!(t.token_type, TokenType::SectionEnums))
        .expect("No @ENUMS section found");

    // Find the opening ( after @ENUMS
    let paren_start = tokens[start_pos..].iter()
        .position(|t| matches!(t.token_type, TokenType::Symbol('(')))
        .expect("No opening ( found");

    // Find matching closing ) - need to count depth
    let mut depth = 0;
    let mut end_pos = start_pos + paren_start;

    for (i, token) in tokens[start_pos + paren_start..].iter().enumerate() {
        match &token.token_type {
            TokenType::Symbol('(') => depth += 1,
            TokenType::Symbol(')') => {
                depth -= 1;
                if depth == 0 {
                    end_pos = start_pos + paren_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    // Return section tokens including @ENUMS to closing )
    let mut section_tokens = tokens[start_pos..=end_pos].to_vec();
    section_tokens.push(Token::eof(1, 1)); // Add EOF
    section_tokens
}

fn parse_enums_with_settings(input: &str, settings: OperationalSettings) -> Option<EnumsSection> {
    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let tokens = tokenize_input(input);
    let section_tokens = extract_enums_section_tokens(&tokens);

    let mut parser = EnumsSectionParser::new(&section_tokens, &settings);
    parser.parse_section()
}

fn parse_enums_default(input: &str) -> Option<EnumsSection> {
    parse_enums_with_settings(input, OperationalSettings::default())
}

fn parse_enums_halt_on_error(input: &str) -> Option<EnumsSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Halt;
    parse_enums_with_settings(input, settings)
}

fn parse_enums_recover(input: &str) -> Option<EnumsSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Recover;
    parse_enums_with_settings(input, settings)
}

// ==================== BASIC FUNCTIONALITY TESTS ====================

#[test]
fn test_simple_enum() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2 }
        )
    "#;

    let section = parse_enums_default(input).expect("Failed to parse");

    assert_eq!(section.enums.len(), 1);
    assert_eq!(section.enums[0].name, "Status");
    assert_eq!(section.enums[0].fields.len(), 2);
    assert_eq!(section.enums[0].fields[0].name, "ACTIVE");
    assert_eq!(section.enums[0].fields[0].value, Some(1));
    assert_eq!(section.enums[0].fields[1].name, "INACTIVE");
    assert_eq!(section.enums[0].fields[1].value, Some(2));
}

#[test]
fn test_multiple_enums_no_commas_between() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2 }
            Priority { LOW = 1, MEDIUM = 2, HIGH = 3 }
            UserRole { GUEST = 0, USER = 1, ADMIN = 2 }
        )
    "#;

    let section = parse_enums_default(input).expect("Failed to parse");

    assert_eq!(section.enums.len(), 3);
    assert_eq!(section.enums[0].name, "Status");
    assert_eq!(section.enums[1].name, "Priority");
    assert_eq!(section.enums[2].name, "UserRole");
    assert_eq!(section.enums[1].fields.len(), 3);
}

#[test]
fn test_enum_without_values() {
    let input = r#"
        @ENUMS(
            Color { RED, GREEN, BLUE, YELLOW }
        )
    "#;

    let section = parse_enums_default(input).expect("Failed to parse");

    assert_eq!(section.enums.len(), 1);
    assert_eq!(section.enums[0].fields.len(), 4);
    assert!(section.enums[0].fields.iter().all(|f| f.value.is_none()));
}

#[test]
fn test_enum_mixed_values() {
    let input = r#"
        @ENUMS(
            Mixed { FIRST = 10, SECOND, THIRD = 30, FOURTH }
        )
    "#;

    let section = parse_enums_default(input).expect("Failed to parse");

    assert_eq!(section.enums[0].fields[0].value, Some(10));
    assert_eq!(section.enums[0].fields[1].value, None);
    assert_eq!(section.enums[0].fields[2].value, Some(30));
    assert_eq!(section.enums[0].fields[3].value, None);
}

#[test]
fn test_enum_with_trailing_comma() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2, }
        )
    "#;

    let section = parse_enums_default(input).expect("Failed to parse");
    assert_eq!(section.enums[0].fields.len(), 2);
}

#[test]
fn test_empty_enums_section() {
    let input = r#"@ENUMS()"#;

    let section = parse_enums_default(input).expect("Failed to parse");
    assert_eq!(section.enums.len(), 0);
}

#[test]
fn test_positions_are_tracked() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2 }
        )
    "#;

    let section = parse_enums_default(input).expect("Failed to parse");

    assert!(section.position.is_valid());
    assert!(section.enums[0].position.is_valid());
    assert!(section.enums[0].fields[0].position.is_valid());
    assert!(section.enums[0].fields[1].position.is_valid());
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_halt_strategy_stops_on_error() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INVALID SYNTAX, INACTIVE = 2 }
            Priority { LOW = 1 }
        )
    "#;

    let section = parse_enums_halt_on_error(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
    // With Halt strategy, might return None or partial section
    if let Some(s) = section {
        // Should have stopped parsing
        assert!(s.enums.len() < 2);
    }
}

#[test]
fn test_recover_strategy_continues_after_error() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1 MISSING_COMMA INACTIVE = 2 }
            Priority { LOW = 1, MEDIUM = 2 }
        )
    "#;

    let section = parse_enums_recover(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
    // With Recover strategy, should try to parse Priority
    if let Some(s) = section {
        println!("Recovered {} enums", s.enums.len());
    }
}

#[test]
fn test_missing_opening_paren() {
    let input = r#"
        @ENUMS
            Status { ACTIVE = 1 }
        )
    "#;

    let section = parse_enums_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
    let errors = error_manager.get_parse_errors();
    assert!(errors.iter().any(|e| e.message.contains("Expected '('")));
}

#[test]
fn test_missing_closing_brace() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2
        )
    "#;

    let section = parse_enums_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_invalid_comma_between_enums() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1 },
            Priority { LOW = 1 }
        )
    "#;

    let section = parse_enums_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
    let errors = error_manager.get_parse_errors();
    assert!(errors.iter().any(|e| e.message.contains("Commas are not allowed")));
}

// ==================== REAL FILE TESTS ====================

#[test]
fn test_all_datatypes_mdix_file() {
    let file_content = std::fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
        .expect("Failed to read all_datatypes_test.mdix");

    let section = parse_enums_default(&file_content).expect("Failed to parse");

    // File has TestEnum with 3 fields
    assert_eq!(section.enums.len(), 1);
    assert_eq!(section.enums[0].name, "TestEnum");
    assert_eq!(section.enums[0].fields.len(), 3);
    assert_eq!(section.enums[0].fields[0].name, "FIRST");
    assert_eq!(section.enums[0].fields[0].value, Some(1));
}

#[test]
fn test_data_variable_usage_mdix_file() {
    let file_content = std::fs::read_to_string("mdix_files/advanced/enum_test.mdix")
        .expect("Failed to read enum_test.mdix");

    let section = parse_enums_default(&file_content).expect("Failed to parse");

    // File has ServerType enum with 3 fields
    assert_eq!(section.enums.len(), 1);
    assert_eq!(section.enums[0].name, "ServerType");
    assert_eq!(section.enums[0].fields.len(), 3);
    assert_eq!(section.enums[0].fields[0].name, "DEVELOPMENT");
    assert_eq!(section.enums[0].fields[1].name, "STAGING");
    assert_eq!(section.enums[0].fields[2].name, "PRODUCTION");
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_parse_speed_small_input() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2 }
        )
    "#;

    let tokens = tokenize_input(input);
    let section_tokens = extract_enums_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = EnumsSectionParser::new(&section_tokens, &settings);
    let _section = parser.parse_section();
    let duration = start.elapsed();

    println!("Small input: Parsed in {:?}", duration);

    // BASELINE: Small input should parse in < 5ms
    assert!(duration.as_millis() < 5, "Too slow: {:?}", duration);
}

#[test]
fn test_parse_speed_medium_input() {
    // Generate medium-sized input (50 enums, 10 fields each)
    let mut input = String::from("@ENUMS(\n");
    for i in 0..50 {
        input.push_str(&format!("    Enum{} {{\n", i));
        for j in 0..10 {
            input.push_str(&format!("        FIELD_{} = {},\n", j, j * 10));
        }
        input.push_str("    }\n");
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_enums_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = EnumsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    println!("Medium input: {} enums in {:?}", section.enums.len(), duration);

    // BASELINE: Medium input (50 enums, 500 fields) should parse in < 50ms
    assert!(duration.as_millis() < 50, "Too slow: {:?}", duration);
    assert_eq!(section.enums.len(), 50);
}

#[test]
fn test_parse_speed_large_input() {
    // Generate large input (500 enums, 20 fields each = 10,000 fields)
    let mut input = String::from("@ENUMS(\n");
    for i in 0..500 {
        input.push_str(&format!("    Enum{} {{\n", i));
        for j in 0..20 {
            input.push_str(&format!("        FIELD_{} = {},\n", j, j * 10));
        }
        input.push_str("    }\n");
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_enums_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = EnumsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let enums_per_sec = section.enums.len() as f64 / duration.as_secs_f64();

    println!("Large input: {} enums in {:?} ({:.0} enums/sec)",
             section.enums.len(), duration, enums_per_sec);

    // BASELINE: Large input (500 enums) should parse in < 500ms
    assert!(duration.as_millis() < 500, "Too slow: {:?}", duration);

    // BASELINE: Should parse at least 1,000 enums per second
    assert!(enums_per_sec > 1000.0, "Too slow: {:.0} enums/sec", enums_per_sec);

    assert_eq!(section.enums.len(), 500);
}

#[test]
fn test_parse_throughput() {
    // Test how many enum fields per second we can parse
    let mut input = String::from("@ENUMS(\n");
    for i in 0..1000 {
        input.push_str(&format!("    Enum{} {{ FIELD = {} }}\n", i, i));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_enums_section_tokens(&tokens);
    let token_count = section_tokens.len();
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = EnumsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let tokens_per_sec = token_count as f64 / duration.as_secs_f64();

    println!("Throughput: {} tokens in {:?} ({:.0} tokens/sec)",
             token_count, duration, tokens_per_sec);

    // BASELINE: Should process at least 10,000 tokens per second
    assert!(tokens_per_sec > 10000.0, "Too slow: {:.0} tokens/sec", tokens_per_sec);
    assert_eq!(section.enums.len(), 1000);
}

#[test]
#[ignore] // Run with: cargo test --release -- --ignored
fn test_release_mode_performance() {
    // Generate very large input (5,000 enums, 10 fields each = 50,000 fields)
    let mut input = String::from("@ENUMS(\n");
    for i in 0..5000 {
        input.push_str(&format!("    Enum{} {{\n", i));
        for j in 0..10 {
            input.push_str(&format!("        FIELD_{} = {},\n", j, j));
        }
        input.push_str("    }\n");
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_enums_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = EnumsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let enums_per_sec = section.enums.len() as f64 / duration.as_secs_f64();
    let fields_per_sec = (section.enums.len() * 10) as f64 / duration.as_secs_f64();

    println!("\n=== RELEASE MODE PERFORMANCE ===");
    println!("Enums: {}", section.enums.len());
    println!("Time: {:?}", duration);
    println!("Enums/sec: {:.0}", enums_per_sec);
    println!("Fields/sec: {:.0}", fields_per_sec);
    println!("================================\n");

    // BASELINE: In release mode, should parse at least 10,000 enums per second
    assert!(enums_per_sec > 10000.0, "Too slow: {:.0} enums/sec", enums_per_sec);
}

// ==================== MEMORY USAGE TESTS ====================

#[test]
fn test_memory_usage_estimate() {
    // Parse a known input and estimate memory usage
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2, PENDING = 3 }
        )
    "#;

    let section = parse_enums_default(input).expect("Failed to parse");

    // Rough estimate of AST size
    let enum_size = std::mem::size_of_val(&section.enums[0]);
    let field_size = std::mem::size_of_val(&section.enums[0].fields[0]);
    let total_estimate = enum_size + (field_size * 3);

    println!("Memory estimate:");
    println!("  Enum struct: {} bytes", enum_size);
    println!("  Field struct: {} bytes", field_size);
    println!("  Total for 1 enum + 3 fields: ~{} bytes", total_estimate);

    // BASELINE: Each enum should be < 1KB, fields should be < 100 bytes each
    assert!(enum_size < 1024, "Enum too large: {} bytes", enum_size);
    assert!(field_size < 100, "Field too large: {} bytes", field_size);
}

#[test]
fn test_no_memory_leaks_repeated_parsing() {
    let input = r#"
        @ENUMS(
            Status { ACTIVE = 1, INACTIVE = 2 }
        )
    "#;

    // Parse same input 1000 times - should not leak memory
    for _ in 0..1000 {
        let _ = parse_enums_default(input);
    }

    // If this completes without panic/OOM, we're good
    println!("Successfully parsed same input 1000 times");
}

// ==================== DEBUG MODE TESTS ====================

#[test]
fn test_debug_mode_off_no_logs() {
    let mut settings = OperationalSettings::default();
    settings.debug_mode = DebugMode::Off;

    let input = r#"@ENUMS( Status { ACTIVE = 1 } )"#;
    let _ = parse_enums_with_settings(input, settings);

    // Just verify it doesn't crash
}

#[test]
fn test_debug_mode_regular() {
    let mut settings = OperationalSettings::default();
    settings.debug_mode = DebugMode::Regular;

    let input = r#"@ENUMS( Status { ACTIVE = 1 } )"#;
    let _ = parse_enums_with_settings(input, settings);

    // Should log debug messages
}

#[test]
fn test_debug_mode_verbose() {
    let mut settings = OperationalSettings::default();
    settings.debug_mode = DebugMode::Verbose;

    let input = r#"@ENUMS( Status { ACTIVE = 1 } )"#;
    let _ = parse_enums_with_settings(input, settings);

    // Should log verbose messages
}

// ==================== EDGE CASES ====================

#[test]
fn test_very_long_enum_name() {
    let long_name = "A".repeat(500);
    let input = format!(r#"
        @ENUMS(
            {} {{ VALUE = 1 }}
        )
    "#, long_name);

    let section = parse_enums_default(&input).expect("Failed to parse");
    assert_eq!(section.enums[0].name.len(), 500);
}

#[test]
fn test_very_long_field_name() {
    let long_field = "FIELD_".to_string() + &"X".repeat(500);
    let input = format!(r#"
        @ENUMS(
            Test {{ {} = 1 }}
        )
    "#, long_field);

    let section = parse_enums_default(&input).expect("Failed to parse");
    assert_eq!(section.enums[0].fields[0].name.len(), 506);
}

#[test]
fn test_large_field_values() {
    let input = r#"
        @ENUMS(
            Test {
                MAX_INT = 2147483647,
                MIN_INT = -2147483648,
                ZERO = 0
            }
        )
    "#;

    let section = parse_enums_default(input).expect("Failed to parse");
    assert_eq!(section.enums[0].fields[0].value, Some(2147483647));
    assert_eq!(section.enums[0].fields[1].value, Some(-2147483648));
}

#[test]
fn test_single_field_enum() {
    let input = r#"@ENUMS( Single { ONLY } )"#;
    let section = parse_enums_default(input).expect("Failed to parse");
    assert_eq!(section.enums[0].fields.len(), 1);
}

#[test]
fn test_enum_with_many_fields() {
    let mut input = String::from("@ENUMS( ManyFields { ");
    for i in 0..1000 {
        input.push_str(&format!("FIELD_{} = {}", i, i));
        if i < 999 {
            input.push_str(", ");
        }
    }
    input.push_str(" } )");

    let section = parse_enums_default(&input).expect("Failed to parse");
    assert_eq!(section.enums[0].fields.len(), 1000);
}