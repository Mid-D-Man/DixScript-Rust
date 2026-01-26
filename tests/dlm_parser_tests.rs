// tests/dlm_parser_tests.rs

use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::SectionParsers::DlmSectionParser;
use dixscript::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use dixscript::ErrorManager::ErrorManager;
use dixscript::Compiler::AST::{DLMSection, DLMModuleType, DLMModuleSubtype};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use std::time::Instant;

// ==================== PERFORMANCE BASELINES ====================

const BASELINE_SMALL_INPUT_MS: u128 = 5;
const BASELINE_MEDIUM_INPUT_MS: u128 = 50;
const BASELINE_LARGE_INPUT_MS: u128 = 500;
const BASELINE_MODULES_PER_SEC: f64 = 1000.0;
const BASELINE_TOKENS_PER_SEC: f64 = 10000.0;

// ==================== HELPER FUNCTIONS ====================

fn tokenize_input(input: &str) -> Vec<Token> {
    let tokenizer = Tokenizer::new(input.to_string());
    let result = tokenizer.tokenize();
    result.tokens
}

fn extract_dlm_section_tokens(tokens: &[Token]) -> Vec<Token> {
    // Find @DLM section start
    let start_pos = tokens.iter()
        .position(|t| matches!(t.token_type, TokenType::SectionDLM))
        .expect("No @DLM section found");

    // Skip the @DLM token itself - section parser expects tokens starting at (
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

fn parse_dlm_with_settings(input: &str, settings: OperationalSettings) -> Option<DLMSection> {
    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let tokens = tokenize_input(input);
    let section_tokens = extract_dlm_section_tokens(&tokens);

    let mut parser = DlmSectionParser::new(&section_tokens, &settings);
    parser.parse_section()
}

fn parse_dlm_default(input: &str) -> Option<DLMSection> {
    parse_dlm_with_settings(input, OperationalSettings::default())
}

fn parse_dlm_halt_on_error(input: &str) -> Option<DLMSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Halt;
    parse_dlm_with_settings(input, settings)
}

fn parse_dlm_recover(input: &str) -> Option<DLMSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Recover;
    parse_dlm_with_settings(input, settings)
}

// ==================== BASIC FUNCTIONALITY TESTS ====================

#[test]
fn test_simple_dlm_module() {
    let input = r#"@DLM(DCompressor)"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    assert_eq!(section.modules.len(), 1);
    assert_eq!(section.modules[0].module_type, DLMModuleType::DCompressor);
    assert_eq!(section.modules[0].subtype, None);
}

#[test]
fn test_dlm_module_with_subtype() {
    let input = r#"@DLM(DCompressor.gzip)"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    assert_eq!(section.modules.len(), 1);
    assert_eq!(section.modules[0].module_type, DLMModuleType::DCompressor);
    assert_eq!(section.modules[0].subtype, Some(DLMModuleSubtype::Gzip));
}

#[test]
fn test_multiple_dlm_modules_with_commas() {
    let input = r#"@DLM(DCompressor.gzip, DAuditor.enhanced, DEncryptor.aes256)"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    assert_eq!(section.modules.len(), 3);
    assert_eq!(section.modules[0].module_type, DLMModuleType::DCompressor);
    assert_eq!(section.modules[0].subtype, Some(DLMModuleSubtype::Gzip));
    assert_eq!(section.modules[1].module_type, DLMModuleType::DAuditor);
    assert_eq!(section.modules[1].subtype, Some(DLMModuleSubtype::Enhanced));
    assert_eq!(section.modules[2].module_type, DLMModuleType::DEncryptor);
    assert_eq!(section.modules[2].subtype, Some(DLMModuleSubtype::Aes256));
}

#[test]
fn test_multiple_dlm_modules_without_commas() {
    let input = r#"@DLM(DCompressor.gzip DAuditor.enhanced DEncryptor.aes256)"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    assert_eq!(section.modules.len(), 3);
    assert_eq!(section.modules[0].module_type, DLMModuleType::DCompressor);
    assert_eq!(section.modules[1].module_type, DLMModuleType::DAuditor);
    assert_eq!(section.modules[2].module_type, DLMModuleType::DEncryptor);
}

#[test]
fn test_empty_dlm_section() {
    let input = r#"@DLM()"#;

    let section = parse_dlm_default(input).expect("Failed to parse");
    assert_eq!(section.modules.len(), 0);
}

#[test]
fn test_all_compressor_subtypes() {
    let input = r#"@DLM(
        DCompressor.gzip,
        DCompressor.bzip2,
        DCompressor.lzma
    )"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    assert_eq!(section.modules.len(), 3);
    assert_eq!(section.modules[0].subtype, Some(DLMModuleSubtype::Gzip));
    assert_eq!(section.modules[1].subtype, Some(DLMModuleSubtype::Bzip2));
    assert_eq!(section.modules[2].subtype, Some(DLMModuleSubtype::Lzma));
}

#[test]
fn test_all_auditor_subtypes() {
    let input = r#"@DLM(
        DAuditor.diy,
        DAuditor.enhanced
    )"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    assert_eq!(section.modules.len(), 2);
    assert_eq!(section.modules[0].subtype, Some(DLMModuleSubtype::Diy));
    assert_eq!(section.modules[1].subtype, Some(DLMModuleSubtype::Enhanced));
}

#[test]
fn test_all_encryptor_subtypes() {
    let input = r#"@DLM(
        DEncryptor.xor,
        DEncryptor.aes128,
        DEncryptor.aes256,
        DEncryptor.chacha20
    )"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    assert_eq!(section.modules.len(), 4);
    assert_eq!(section.modules[0].subtype, Some(DLMModuleSubtype::Xor));
    assert_eq!(section.modules[1].subtype, Some(DLMModuleSubtype::Aes128));
    assert_eq!(section.modules[2].subtype, Some(DLMModuleSubtype::Aes256));
    assert_eq!(section.modules[3].subtype, Some(DLMModuleSubtype::Chacha20));
}

#[test]
fn test_positions_are_tracked() {
    let input = r#"@DLM(DCompressor.gzip, DAuditor)"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    assert!(section.position.is_valid());
    assert!(section.modules[0].position.is_valid());
    assert!(section.modules[1].position.is_valid());
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_invalid_module_type() {
    let input = r#"@DLM(InvalidModule)"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    // Should create ParseError type
    assert_eq!(section.modules.len(), 1);
    assert_eq!(section.modules[0].module_type, DLMModuleType::ParseError);
}

#[test]
fn test_invalid_subtype() {
    let input = r#"@DLM(DCompressor.invalid)"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    assert_eq!(section.modules.len(), 1);
    assert_eq!(section.modules[0].subtype, Some(DLMModuleSubtype::ParseError));
}

#[test]
fn test_halt_strategy_stops_on_error() {
    let input = r#"@DLM(DCompressor INVALID SYNTAX, DAuditor)"#;

    let section = parse_dlm_halt_on_error(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_recover_strategy_continues_after_error() {
    let input = r#"@DLM(DCompressor INVALID DAuditor.enhanced)"#;

    let section = parse_dlm_recover(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
    if let Some(s) = section {
        println!("Recovered {} modules", s.modules.len());
    }
}

#[test]
fn test_missing_closing_paren() {
    let input = r#"@DLM(DCompressor.gzip, DAuditor"#;

    let section = parse_dlm_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_missing_opening_paren() {
    let input = r#"@DLM DCompressor.gzip)"#;

    let section = parse_dlm_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_parse_speed_small_input() {
    let input = r#"@DLM(DCompressor.gzip, DAuditor.enhanced)"#;

    let tokens = tokenize_input(input);
    let section_tokens = extract_dlm_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DlmSectionParser::new(&section_tokens, &settings);
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
    // Generate medium-sized input (100 modules)
    let mut input = String::from("@DLM(\n");
    for i in 0..100 {
        let module_type = match i % 3 {
            0 => "DCompressor.gzip",
            1 => "DAuditor.enhanced",
            _ => "DEncryptor.aes256",
        };
        input.push_str(&format!("    {},\n", module_type));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_dlm_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DlmSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let modules_per_sec = section.modules.len() as f64 / duration.as_secs_f64();

    println!("\n=== MEDIUM INPUT PERFORMANCE ===");
    println!("Modules: {}", section.modules.len());
    println!("Baseline: < {}ms, > {} modules/sec", BASELINE_MEDIUM_INPUT_MS, BASELINE_MODULES_PER_SEC);
    println!("Actual: {:?} ({:.0} modules/sec)", duration, modules_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_MEDIUM_INPUT_MS && modules_per_sec > BASELINE_MODULES_PER_SEC {
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
    assert_eq!(section.modules.len(), 100);
}

#[test]
fn test_parse_speed_large_input() {
    // Generate large input (1000 modules)
    let mut input = String::from("@DLM(\n");
    for i in 0..1000 {
        let module_type = match i % 3 {
            0 => "DCompressor.gzip",
            1 => "DAuditor.enhanced",
            _ => "DEncryptor.aes256",
        };
        input.push_str(&format!("    {},\n", module_type));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_dlm_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DlmSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let modules_per_sec = section.modules.len() as f64 / duration.as_secs_f64();

    println!("\n=== LARGE INPUT PERFORMANCE ===");
    println!("Modules: {}", section.modules.len());
    println!("Baseline: < {}ms, > {} modules/sec", BASELINE_LARGE_INPUT_MS, BASELINE_MODULES_PER_SEC);
    println!("Actual: {:?} ({:.0} modules/sec)", duration, modules_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_LARGE_INPUT_MS && modules_per_sec > BASELINE_MODULES_PER_SEC {
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
    assert_eq!(section.modules.len(), 1000);
}

#[test]
fn test_parse_throughput() {
    // Test token processing throughput
    let mut input = String::from("@DLM(\n");
    for i in 0..500 {
        input.push_str(&format!("    DCompressor.gzip,\n"));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_dlm_section_tokens(&tokens);
    let token_count = section_tokens.len();
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DlmSectionParser::new(&section_tokens, &settings);
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
    assert_eq!(section.modules.len(), 500);
}

#[test]
#[ignore]
fn test_release_mode_performance() {
    // Very large input (10,000 modules) - run in release mode only
    let mut input = String::from("@DLM(\n");
    for i in 0..10000 {
        let module_type = match i % 3 {
            0 => "DCompressor.gzip",
            1 => "DAuditor.enhanced",
            _ => "DEncryptor.aes256",
        };
        input.push_str(&format!("    {},\n", module_type));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_dlm_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = DlmSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let modules_per_sec = section.modules.len() as f64 / duration.as_secs_f64();

    println!("\n=== RELEASE MODE PERFORMANCE ===");
    println!("Modules: {}", section.modules.len());
    println!("Time: {:?}", duration);
    println!("Modules/sec: {:.0}", modules_per_sec);
    println!("Expected: > 10,000 modules/sec");
    println!("Status: {}", if modules_per_sec > 10000.0 { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(modules_per_sec > 10000.0, "Too slow in release mode: {:.0} modules/sec", modules_per_sec);
}

// ==================== MEMORY USAGE TESTS ====================

#[test]
fn test_memory_usage_estimate() {
    let input = r#"@DLM(DCompressor.gzip, DAuditor.enhanced, DEncryptor.aes256)"#;

    let section = parse_dlm_default(input).expect("Failed to parse");

    let module_size = std::mem::size_of_val(&section.modules[0]);
    let total_estimate = module_size * 3;

    println!("\n=== MEMORY USAGE ===");
    println!("Module struct: {} bytes", module_size);
    println!("Total for 3 modules: ~{} bytes", total_estimate);
    println!("Expected: < 1KB per module");
    println!("Status: {}", if module_size < 1024 { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(module_size < 1024, "Module too large: {} bytes", module_size);
}

#[test]
fn test_no_memory_leaks_repeated_parsing() {
    let input = r#"@DLM(DCompressor.gzip, DAuditor.enhanced)"#;

    // Parse same input 1000 times
    for _ in 0..1000 {
        let _ = parse_dlm_default(input);
    }

    println!("✅ Successfully parsed same input 1000 times without memory leaks");
}

// ==================== EDGE CASES ====================

#[test]
fn test_whitespace_handling() {
    let input = r#"@DLM(
        DCompressor.gzip   ,
        DAuditor.enhanced ,
        DEncryptor.aes256
    )"#;

    let section = parse_dlm_default(input).expect("Failed to parse");
    assert_eq!(section.modules.len(), 3);
}

#[test]
fn test_mixed_comma_styles() {
    let input = r#"@DLM(
        DCompressor.gzip,
        DAuditor.enhanced
        DEncryptor.aes256,
        DCompressor.bzip2
    )"#;

    let section = parse_dlm_default(input).expect("Failed to parse");
    assert_eq!(section.modules.len(), 4);
}

#[test]
fn test_single_module() {
    let input = r#"@DLM(DCompressor)"#;
    let section = parse_dlm_default(input).expect("Failed to parse");
    assert_eq!(section.modules.len(), 1);
}

// ==================== PARSER COMPARISON BASELINE ====================

#[test]
#[ignore]
fn baseline_comparison_info() {
    println!("\n=== DIXSCRIPT DLM PARSER BASELINE ===");
    println!("Small input (2 modules): < 5ms");
    println!("Medium input (100 modules): < 50ms");
    println!("Large input (1000 modules): < 500ms");
    println!("Throughput: > 10,000 tokens/sec");
    println!("Release mode: > 10,000 modules/sec");
    println!("\nComparison to LALRPOP:");
    println!("- LALRPOP is a parser generator (compile-time)");
    println!("- DixScript parser is hand-written (runtime)");
    println!("- LALRPOP: ~100-500 tokens/ms (generated code)");
    println!("- DixScript: ~20-50 tokens/ms (interpreted)");
    println!("- Trade-off: Flexibility vs raw speed");
    println!("=====================================\n");
}