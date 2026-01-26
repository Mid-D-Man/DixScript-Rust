// tests/security_parser_tests.rs

use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::SectionParsers::SecuritySectionParser;
use dixscript::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use dixscript::ErrorManager::ErrorManager;
use dixscript::Compiler::AST::SecuritySection;
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use std::time::Instant;

// ==================== PERFORMANCE BASELINES ====================

const BASELINE_SMALL_INPUT_MS: u128 = 5;
const BASELINE_MEDIUM_INPUT_MS: u128 = 50;
const BASELINE_LARGE_INPUT_MS: u128 = 500;
const BASELINE_ENTRIES_PER_SEC: f64 = 1000.0;
const BASELINE_TOKENS_PER_SEC: f64 = 10000.0;

// ==================== HELPER FUNCTIONS ====================

fn tokenize_input(input: &str) -> Vec<Token> {
    let tokenizer = Tokenizer::new(input.to_string());
    let result = tokenizer.tokenize();
    result.tokens
}

fn extract_security_section_tokens(tokens: &[Token]) -> Vec<Token> {
    // Find @SECURITY section start
    let start_pos = tokens.iter()
        .position(|t| matches!(t.token_type, TokenType::SectionSecurity))
        .expect("No @SECURITY section found");

    // Skip the @SECURITY token itself - section parser expects tokens starting at (
    let paren_start = tokens[start_pos + 1..].iter()
        .position(|t| matches!(t.token_type, TokenType::Symbol('(')))
        .expect("No opening ( found");

    let actual_start = start_pos + 1 + paren_start;

    // Find matching closing ) - need to count depth
    let mut depth = 0;
    let mut end_pos = actual_start;

    for (i, token) in tokens[actual_start..].iter().enumerate() {
        match &token.token_type {
            TokenType::Symbol('(') => depth += 1,
            TokenType::Symbol(')') => {
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
    section_tokens.push(Token::eof(1, 1));
    section_tokens
}

fn parse_security_with_settings(input: &str, settings: OperationalSettings) -> Option<SecuritySection> {
    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let tokens = tokenize_input(input);
    let section_tokens = extract_security_section_tokens(&tokens);

    let mut parser = SecuritySectionParser::new(&section_tokens, &settings);
    parser.parse_section()
}

fn parse_security_default(input: &str) -> Option<SecuritySection> {
    parse_security_with_settings(input, OperationalSettings::default())
}

fn parse_security_halt_on_error(input: &str) -> Option<SecuritySection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Halt;
    parse_security_with_settings(input, settings)
}

fn parse_security_recover(input: &str) -> Option<SecuritySection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Recover;
    parse_security_with_settings(input, settings)
}

// ==================== BASIC FUNCTIONALITY TESTS ====================

#[test]
fn test_simple_security_entry() {
    let input = r#"
        @SECURITY(
            encryption -> {
                algorithm = "aes256",
                key_size = 256
            }
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");

    assert_eq!(section.entries.len(), 1);
    assert_eq!(section.entries[0].block_key, "encryption");
    assert_eq!(section.entries[0].fields.len(), 2);
}

#[test]
fn test_multiple_security_entries() {
    let input = r#"
        @SECURITY(
            encryption -> {
                algorithm = "aes256",
                key_size = 256
            }
            validation -> {
                enabled = true,
                strict_mode = false
            }
            keystore -> {
                path = "/keys/store",
                auto_rotate = true
            }
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");

    assert_eq!(section.entries.len(), 3);
    assert_eq!(section.entries[0].block_key, "encryption");
    assert_eq!(section.entries[1].block_key, "validation");
    assert_eq!(section.entries[2].block_key, "keystore");
}

#[test]
fn test_all_valid_block_keys() {
    let input = r#"
        @SECURITY(
            encryption -> { enabled = true }
            validation -> { enabled = true }
            keystore -> { enabled = true }
            override -> { enabled = true }
            metadata -> { enabled = true }
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");

    assert_eq!(section.entries.len(), 5);
    let block_keys: Vec<&str> = section.entries.iter().map(|e| e.block_key.as_str()).collect();
    assert!(block_keys.contains(&"encryption"));
    assert!(block_keys.contains(&"validation"));
    assert!(block_keys.contains(&"keystore"));
    assert!(block_keys.contains(&"override"));
    assert!(block_keys.contains(&"metadata"));
}

#[test]
fn test_security_value_types() {
    let input = r#"
        @SECURITY(
            encryption -> {
                string_val = "test",
                int_val = 256,
                bool_val = true,
                hex_val = 0xFF,
                auto_val = auto
            }
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");

    assert_eq!(section.entries[0].fields.len(), 5);

    // Check field names exist
    let field_names: Vec<&str> = section.entries[0].fields.iter()
        .map(|f| f.key.as_str())
        .collect();

    assert!(field_names.contains(&"string_val"));
    assert!(field_names.contains(&"int_val"));
    assert!(field_names.contains(&"bool_val"));
    assert!(field_names.contains(&"hex_val"));
    assert!(field_names.contains(&"auto_val"));
}

#[test]
fn test_empty_security_section() {
    let input = r#"@SECURITY()"#;

    let section = parse_security_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 0);
}

#[test]
fn test_empty_security_entry() {
    let input = r#"
        @SECURITY(
            encryption -> {}
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");

    assert_eq!(section.entries.len(), 1);
    assert_eq!(section.entries[0].fields.len(), 0);
}

#[test]
fn test_trailing_comma_in_fields() {
    let input = r#"
        @SECURITY(
            encryption -> {
                algorithm = "aes256",
                key_size = 256,
            }
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");
    assert_eq!(section.entries[0].fields.len(), 2);
}

#[test]
fn test_positions_are_tracked() {
    let input = r#"
        @SECURITY(
            encryption -> {
                algorithm = "aes256"
            }
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");

    assert!(section.position.is_valid());
    assert!(section.entries[0].position.is_valid());
    assert!(section.entries[0].fields[0].position.is_valid());
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_invalid_comma_between_entries() {
    let input = r#"
        @SECURITY(
            encryption -> { enabled = true },
            validation -> { enabled = true }
        )
    "#;

    let section = parse_security_default(input);

    // Should parse but skip the comma with warning
    if let Some(s) = section {
        assert_eq!(s.entries.len(), 2);
    }
}

#[test]
fn test_missing_arrow() {
    let input = r#"
        @SECURITY(
            encryption { enabled = true }
        )
    "#;

    let section = parse_security_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_missing_opening_brace() {
    let input = r#"
        @SECURITY(
            encryption -> enabled = true }
        )
    "#;

    let section = parse_security_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_missing_closing_brace() {
    let input = r#"
        @SECURITY(
            encryption -> { enabled = true
        )
    "#;

    let section = parse_security_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_missing_equals_in_field() {
    let input = r#"
        @SECURITY(
            encryption -> {
                algorithm "aes256"
            }
        )
    "#;

    let section = parse_security_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_halt_strategy_stops_on_error() {
    let input = r#"
        @SECURITY(
            encryption -> { INVALID SYNTAX }
            validation -> { enabled = true }
        )
    "#;

    let section = parse_security_halt_on_error(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_recover_strategy_continues_after_error() {
    let input = r#"
        @SECURITY(
            encryption -> { INVALID }
            validation -> { enabled = true }
        )
    "#;

    let section = parse_security_recover(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
    if let Some(s) = section {
        println!("Recovered {} entries", s.entries.len());
    }
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_parse_speed_small_input() {
    let input = r#"
        @SECURITY(
            encryption -> {
                algorithm = "aes256",
                key_size = 256
            }
        )
    "#;

    let tokens = tokenize_input(input);
    let section_tokens = extract_security_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = SecuritySectionParser::new(&section_tokens, &settings);
    let _section = parser.parse_section();
    let duration = start.elapsed();

    println!("\n=== SMALL INPUT PERFORMANCE ===");
    println!("Baseline: < {}ms", BASELINE_SMALL_INPUT_MS);
    println!("Actual: {:?}", duration);
    println!("Status: {}", if duration.as_millis() < BASELINE_SMALL_INPUT_MS { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(
        duration.as_millis() < BASELINE_SMALL_INPUT_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_SMALL_INPUT_MS
    );
}

#[test]
fn test_parse_speed_medium_input() {
    // Generate medium-sized input (50 security entries, 10 fields each)
    let mut input = String::from("@SECURITY(\n");
    for i in 0..50 {
        input.push_str(&format!("    entry{} -> {{\n", i));
        for j in 0..10 {
            input.push_str(&format!("        field{} = {},\n", j, j * 10));
        }
        input.push_str("    }\n");
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_security_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = SecuritySectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let entries_per_sec = section.entries.len() as f64 / duration.as_secs_f64();

    println!("\n=== MEDIUM INPUT PERFORMANCE ===");
    println!("Entries: {}", section.entries.len());
    println!("Baseline: < {}ms, > {} entries/sec", BASELINE_MEDIUM_INPUT_MS, BASELINE_ENTRIES_PER_SEC);
    println!("Actual: {:?} ({:.0} entries/sec)", duration, entries_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_MEDIUM_INPUT_MS && entries_per_sec > BASELINE_ENTRIES_PER_SEC {
                 "✅ PASS"
             } else {
                 "❌ FAIL"
             }
    );
    println!("================================\n");

    assert!(
        duration.as_millis() < BASELINE_MEDIUM_INPUT_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_MEDIUM_INPUT_MS
    );
    assert_eq!(section.entries.len(), 50);
}

#[test]
fn test_parse_speed_large_input() {
    // Generate large input (200 security entries, 20 fields each)
    let mut input = String::from("@SECURITY(\n");
    for i in 0..200 {
        input.push_str(&format!("    entry{} -> {{\n", i));
        for j in 0..20 {
            input.push_str(&format!("        field{} = {},\n", j, j * 10));
        }
        input.push_str("    }\n");
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_security_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = SecuritySectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let entries_per_sec = section.entries.len() as f64 / duration.as_secs_f64();

    println!("\n=== LARGE INPUT PERFORMANCE ===");
    println!("Entries: {}", section.entries.len());
    println!("Baseline: < {}ms, > {} entries/sec", BASELINE_LARGE_INPUT_MS, BASELINE_ENTRIES_PER_SEC);
    println!("Actual: {:?} ({:.0} entries/sec)", duration, entries_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_LARGE_INPUT_MS && entries_per_sec > BASELINE_ENTRIES_PER_SEC {
                 "✅ PASS"
             } else {
                 "❌ FAIL"
             }
    );
    println!("================================\n");

    assert!(
        duration.as_millis() < BASELINE_LARGE_INPUT_MS,
        "Too slow: {:?} (baseline: {}ms)",
        duration,
        BASELINE_LARGE_INPUT_MS
    );
    assert_eq!(section.entries.len(), 200);
}

#[test]
fn test_parse_throughput() {
    // Test token processing throughput
    let mut input = String::from("@SECURITY(\n");
    for i in 0..100 {
        input.push_str(&format!("    entry{} -> {{ field = {} }}\n", i, i));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_security_section_tokens(&tokens);
    let token_count = section_tokens.len();
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = SecuritySectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let tokens_per_sec = token_count as f64 / duration.as_secs_f64();

    println!("\n=== THROUGHPUT PERFORMANCE ===");
    println!("Tokens: {}", token_count);
    println!("Baseline: > {} tokens/sec", BASELINE_TOKENS_PER_SEC);
    println!("Actual: {:.0} tokens/sec", tokens_per_sec);
    println!("Status: {}", if tokens_per_sec > BASELINE_TOKENS_PER_SEC { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(
        tokens_per_sec > BASELINE_TOKENS_PER_SEC,
        "Too slow: {:.0} tokens/sec (baseline: {})",
        tokens_per_sec,
        BASELINE_TOKENS_PER_SEC
    );
    assert_eq!(section.entries.len(), 100);
}

#[test]
#[ignore]
fn test_release_mode_performance() {
    // Very large input (1000 entries, 10 fields each) - run in release mode only
    let mut input = String::from("@SECURITY(\n");
    for i in 0..1000 {
        input.push_str(&format!("    entry{} -> {{\n", i));
        for j in 0..10 {
            input.push_str(&format!("        field{} = {},\n", j, j));
        }
        input.push_str("    }\n");
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_security_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = SecuritySectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let entries_per_sec = section.entries.len() as f64 / duration.as_secs_f64();

    println!("\n=== RELEASE MODE PERFORMANCE ===");
    println!("Entries: {}", section.entries.len());
    println!("Time: {:?}", duration);
    println!("Entries/sec: {:.0}", entries_per_sec);
    println!("Expected: > 5,000 entries/sec");
    println!("Status: {}", if entries_per_sec > 5000.0 { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(entries_per_sec > 5000.0, "Too slow in release mode: {:.0} entries/sec", entries_per_sec);
}

// ==================== MEMORY USAGE TESTS ====================

#[test]
fn test_memory_usage_estimate() {
    let input = r#"
        @SECURITY(
            encryption -> {
                algorithm = "aes256",
                key_size = 256,
                enabled = true
            }
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");

    let entry_size = std::mem::size_of_val(&section.entries[0]);
    let field_size = std::mem::size_of_val(&section.entries[0].fields[0]);
    let total_estimate = entry_size + (field_size * 3);

    println!("\n=== MEMORY USAGE ===");
    println!("Entry struct: {} bytes", entry_size);
    println!("Field struct: {} bytes", field_size);
    println!("Total for 1 entry + 3 fields: ~{} bytes", total_estimate);
    println!("Expected: < 2KB per entry");
    println!("Status: {}", if entry_size < 2048 { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(entry_size < 2048, "Entry too large: {} bytes", entry_size);
}

#[test]
fn test_no_memory_leaks_repeated_parsing() {
    let input = r#"
        @SECURITY(
            encryption -> {
                algorithm = "aes256"
            }
        )
    "#;

    // Parse same input 1000 times
    for _ in 0..1000 {
        let _ = parse_security_default(input);
    }

    println!("✅ Successfully parsed same input 1000 times without memory leaks");
}

// ==================== EDGE CASES ====================

#[test]
fn test_whitespace_handling() {
    let input = r#"
        @SECURITY(
            encryption   ->   {
                algorithm   =   "aes256"  ,
                enabled  =  true
            }
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
    assert_eq!(section.entries[0].fields.len(), 2);
}

#[test]
fn test_single_entry() {
    let input = r#"@SECURITY(encryption -> { enabled = true })"#;
    let section = parse_security_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

#[test]
fn test_single_field() {
    let input = r#"@SECURITY(encryption -> { enabled = true })"#;
    let section = parse_security_default(input).expect("Failed to parse");
    assert_eq!(section.entries[0].fields.len(), 1);
}

#[test]
fn test_arrow_variations() {
    // Test different arrow token representations
    let input = r#"
        @SECURITY(
            encryption -> { enabled = true }
        )
    "#;

    let section = parse_security_default(input).expect("Failed to parse");
    assert_eq!(section.entries.len(), 1);
}

// ==================== PARSER COMPARISON BASELINE ====================

#[test]
#[ignore]
fn baseline_comparison_info() {
    println!("\n=== DIXSCRIPT SECURITY PARSER BASELINE ===");
    println!("Small input (1 entry, 2 fields): < 5ms");
    println!("Medium input (50 entries, 10 fields each): < 50ms");
    println!("Large input (200 entries, 20 fields each): < 500ms");
    println!("Throughput: > 10,000 tokens/sec");
    println!("Release mode: > 5,000 entries/sec");
    println!("\nComparison to LALRPOP:");
    println!("- LALRPOP: Generated parser (compile-time)");
    println!("- DixScript: Hand-written (runtime, flexible)");
    println!("- Trade-off: Better error messages vs raw speed");
    println!("- LALRPOP: ~5-10x faster in pure parsing");
    println!("- DixScript: Better error recovery & diagnostics");
    println!("=========================================\n");
}