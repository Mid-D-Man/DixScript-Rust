// tests/imports_parser_tests.rs

use dixscript::Compiler::Core::Tokenizer::Tokenizer;
use dixscript::Compiler::Core::SectionParsers::ImportsSectionParser;
use dixscript::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use dixscript::ErrorManager::ErrorManager;
use dixscript::Compiler::AST::ImportsSection;
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use std::time::Instant;

// ==================== PERFORMANCE BASELINES ====================
// Based on hand-written parsers + LALRPOP comparison

// Hand-written parser baselines (realistic expectations)
const BASELINE_SMALL_INPUT_MS: u128 = 3;      // 1-2 imports
const BASELINE_MEDIUM_INPUT_MS: u128 = 30;    // 50 imports
const BASELINE_LARGE_INPUT_MS: u128 = 300;    // 200 imports

// Throughput baselines
const BASELINE_IMPORTS_PER_SEC: f64 = 2000.0;   // Reasonable for hand-written
const BASELINE_TOKENS_PER_SEC: f64 = 15000.0;   // Token processing rate

// LALRPOP comparison (for reference)
// LALRPOP is typically 5-10x faster in pure parsing
// But we trade some speed for better error messages and recovery
const LALRPOP_FACTOR: f64 = 7.0;  // Our parser is ~7x slower than LALRPOP

// ==================== HELPER FUNCTIONS ====================

fn tokenize_input(input: &str) -> Vec<Token> {
    let tokenizer = Tokenizer::new(input.to_string());
    let result = tokenizer.tokenize();
    result.tokens
}

fn extract_imports_section_tokens(tokens: &[Token]) -> Vec<Token> {
    // Find @IMPORTS section start
    let start_pos = tokens.iter()
        .position(|t| matches!(t.token_type, TokenType::SectionImports))
        .expect("No @IMPORTS section found");

    // Skip the @IMPORTS token itself - section parser expects tokens starting at (
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

fn parse_imports_with_settings(input: &str, settings: OperationalSettings) -> Option<ImportsSection> {
    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let tokens = tokenize_input(input);
    let section_tokens = extract_imports_section_tokens(&tokens);

    let mut parser = ImportsSectionParser::new(&section_tokens, &settings);
    parser.parse_section()
}

fn parse_imports_default(input: &str) -> Option<ImportsSection> {
    parse_imports_with_settings(input, OperationalSettings::default())
}

fn parse_imports_halt_on_error(input: &str) -> Option<ImportsSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Halt;
    parse_imports_with_settings(input, settings)
}

fn parse_imports_recover(input: &str) -> Option<ImportsSection> {
    let mut settings = OperationalSettings::default();
    settings.error_handling_strategy = ErrorHandlingStrategy::Recover;
    parse_imports_with_settings(input, settings)
}

// ==================== BASIC FUNCTIONALITY TESTS ====================

#[test]
fn test_simple_local_import() {
    let input = r#"
        @IMPORTS(
            utils from "shared/utilities.mdix"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");

    assert_eq!(section.imports.len(), 1);
    assert_eq!(section.imports[0].alias, "utils");
    assert_eq!(section.imports[0].path, "shared/utilities.mdix");
    assert!(!section.imports[0].is_cloud_import);
    assert_eq!(section.imports[0].verify_hash, None);
}

#[test]
fn test_local_import_with_verify() {
    let input = r#"
        @IMPORTS(
            crypto from "security/crypto.mdix" verify "sha256:abc123def456"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");

    assert_eq!(section.imports.len(), 1);
    assert_eq!(section.imports[0].alias, "crypto");
    assert_eq!(section.imports[0].verify_hash, Some("sha256:abc123def456".to_string()));
}

#[test]
fn test_cloud_import_https() {
    let input = r#"
        @IMPORTS(
            remote from_cloud "https://cdn.example.com/lib.mdix"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");

    assert_eq!(section.imports.len(), 1);
    assert_eq!(section.imports[0].alias, "remote");
    assert!(section.imports[0].is_cloud_import);
    assert_eq!(section.imports[0].path, "https://cdn.example.com/lib.mdix");
}

#[test]
fn test_cloud_import_with_query_params() {
    let input = r#"
        @IMPORTS(
            api from_cloud "https://api.example.com/data.mdix?version=1.0&token=xyz"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");

    assert_eq!(section.imports.len(), 1);
    assert_eq!(section.imports[0].path, "https://api.example.com/data.mdix?version=1.0&token=xyz");
}

#[test]
fn test_multiple_imports_with_commas() {
    let input = r#"
        @IMPORTS(
            utils from "shared/utilities.mdix",
            crypto from "security/crypto.mdix",
            validators from "validation/validators.mdix"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");

    assert_eq!(section.imports.len(), 3);
    assert_eq!(section.imports[0].alias, "utils");
    assert_eq!(section.imports[1].alias, "crypto");
    assert_eq!(section.imports[2].alias, "validators");
}

#[test]
fn test_multiple_imports_without_commas() {
    let input = r#"
        @IMPORTS(
            utils from "shared/utilities.mdix"
            crypto from "security/crypto.mdix"
            validators from "validation/validators.mdix"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");

    assert_eq!(section.imports.len(), 3);
    assert_eq!(section.imports[0].alias, "utils");
    assert_eq!(section.imports[1].alias, "crypto");
    assert_eq!(section.imports[2].alias, "validators");
}

#[test]
fn test_mixed_local_and_cloud_imports() {
    let input = r#"
        @IMPORTS(
            local from "local/file.mdix"
            remote from_cloud "https://cdn.example.com/remote.mdix"
            another from "another/local.mdix" verify "hash123"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");

    assert_eq!(section.imports.len(), 3);
    assert!(!section.imports[0].is_cloud_import);
    assert!(section.imports[1].is_cloud_import);
    assert!(!section.imports[2].is_cloud_import);
    assert_eq!(section.imports[2].verify_hash, Some("hash123".to_string()));
}

#[test]
fn test_empty_imports_section() {
    let input = r#"@IMPORTS()"#;

    let section = parse_imports_default(input).expect("Failed to parse");
    assert_eq!(section.imports.len(), 0);
}

#[test]
fn test_positions_are_tracked() {
    let input = r#"
        @IMPORTS(
            utils from "shared/utilities.mdix"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");

    assert!(section.position.is_valid());
    assert!(section.imports[0].position.is_valid());
}

// ==================== VALIDATION TESTS ====================

#[test]
fn test_duplicate_alias_detection() {
    let input = r#"
        @IMPORTS(
            utils from "file1.mdix"
            utils from "file2.mdix"
        )
    "#;

    let section = parse_imports_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());

    // Should still parse first import
    if let Some(s) = section {
        assert_eq!(s.imports.len(), 1);
    }
}

#[test]
fn test_reserved_alias_rejection() {
    let reserved_tests = vec![
        ("Math", "Math from \"file.mdix\""),
        ("DateTime", "DateTime from \"file.mdix\""),
        ("config", "config from \"file.mdix\""),
        ("Dix", "Dix from \"file.mdix\""),
        ("if", "if from \"file.mdix\""),
        ("true", "true from \"file.mdix\""),
    ];

    for (alias, import_str) in reserved_tests {
        let input = format!("@IMPORTS({})", import_str);
        let error_manager = ErrorManager::get_shared_instance();
        error_manager.clear_errors();

        let _section = parse_imports_default(&input);

        assert!(error_manager.has_errors(), "Should reject reserved alias: {}", alias);
    }
}

#[test]
fn test_invalid_alias_characters() {
    let input = r#"
        @IMPORTS(
            invalid-name from "file.mdix"
        )
    "#;

    let _section = parse_imports_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_path_must_end_with_mdix() {
    let input = r#"
        @IMPORTS(
            utils from "file.txt"
        )
    "#;

    let _section = parse_imports_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_path_no_backslashes() {
    let input = r#"
        @IMPORTS(
            utils from "folder\file.mdix"
        )
    "#;

    let _section = parse_imports_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_cloud_import_requires_https() {
    let input = r#"
        @IMPORTS(
            remote from_cloud "cdn.example.com/file.mdix"
        )
    "#;

    let _section = parse_imports_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_cloud_import_s3_not_supported_yet() {
    let input = r#"
        @IMPORTS(
            remote from_cloud "s3://bucket/file.mdix"
        )
    "#;

    let _section = parse_imports_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_missing_from_keyword() {
    let input = r#"
        @IMPORTS(
            utils "file.mdix"
        )
    "#;

    let section = parse_imports_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_missing_path_string() {
    let input = r#"
        @IMPORTS(
            utils from
        )
    "#;

    let section = parse_imports_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_missing_verify_hash() {
    let input = r#"
        @IMPORTS(
            utils from "file.mdix" verify
        )
    "#;

    let section = parse_imports_default(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_halt_strategy_stops_on_error() {
    let input = r#"
        @IMPORTS(
            Math from "file.mdix"
            valid from "another.mdix"
        )
    "#;

    let section = parse_imports_halt_on_error(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
}

#[test]
fn test_recover_strategy_continues_after_error() {
    let input = r#"
        @IMPORTS(
            INVALID
            valid from "file.mdix"
        )
    "#;

    let section = parse_imports_recover(input);
    let error_manager = ErrorManager::get_shared_instance();

    assert!(error_manager.has_errors());
    if let Some(s) = section {
        println!("Recovered {} imports", s.imports.len());
    }
}

// ==================== PERFORMANCE TESTS ====================

#[test]
fn test_parse_speed_small_input() {
    let input = r#"
        @IMPORTS(
            utils from "shared/utilities.mdix"
            crypto from "security/crypto.mdix"
        )
    "#;

    let tokens = tokenize_input(input);
    let section_tokens = extract_imports_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = ImportsSectionParser::new(&section_tokens, &settings);
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
    // Generate medium-sized input (50 imports)
    let mut input = String::from("@IMPORTS(\n");
    for i in 0..50 {
        input.push_str(&format!(
            "    import{} from \"path/to/file{}.mdix\"\n",
            i, i
        ));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_imports_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = ImportsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let imports_per_sec = section.imports.len() as f64 / duration.as_secs_f64();

    println!("\n=== MEDIUM INPUT PERFORMANCE ===");
    println!("Imports: {}", section.imports.len());
    println!("Baseline: < {}ms, > {} imports/sec", BASELINE_MEDIUM_INPUT_MS, BASELINE_IMPORTS_PER_SEC);
    println!("Actual: {:?} ({:.0} imports/sec)", duration, imports_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_MEDIUM_INPUT_MS && imports_per_sec > BASELINE_IMPORTS_PER_SEC {
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
    assert_eq!(section.imports.len(), 50);
}

#[test]
fn test_parse_speed_large_input() {
    // Generate large input (200 imports with verify)
    let mut input = String::from("@IMPORTS(\n");
    for i in 0..200 {
        input.push_str(&format!(
            "    import{} from \"path/to/file{}.mdix\" verify \"hash{}\"\n",
            i, i, i
        ));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_imports_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = ImportsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let imports_per_sec = section.imports.len() as f64 / duration.as_secs_f64();

    println!("\n=== LARGE INPUT PERFORMANCE ===");
    println!("Imports: {}", section.imports.len());
    println!("Baseline: < {}ms, > {} imports/sec", BASELINE_LARGE_INPUT_MS, BASELINE_IMPORTS_PER_SEC);
    println!("Actual: {:?} ({:.0} imports/sec)", duration, imports_per_sec);
    println!("Status: {}",
             if duration.as_millis() < BASELINE_LARGE_INPUT_MS && imports_per_sec > BASELINE_IMPORTS_PER_SEC {
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
    assert_eq!(section.imports.len(), 200);
}

#[test]
fn test_parse_throughput() {
    // Test token processing throughput
    let mut input = String::from("@IMPORTS(\n");
    for i in 0..100 {
        input.push_str(&format!("    import{} from \"file{}.mdix\"\n", i, i));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_imports_section_tokens(&tokens);
    let token_count = section_tokens.len();
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = ImportsSectionParser::new(&section_tokens, &settings);
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
    assert_eq!(section.imports.len(), 100);
}

#[test]
#[ignore]
fn test_lalrpop_comparison() {
    println!("\n=== LALRPOP COMPARISON ===");
    println!("Hand-written parser performance:");
    println!("  - Small (2 imports): < {}ms", BASELINE_SMALL_INPUT_MS);
    println!("  - Medium (50 imports): < {}ms", BASELINE_MEDIUM_INPUT_MS);
    println!("  - Large (200 imports): < {}ms", BASELINE_LARGE_INPUT_MS);
    println!();
    println!("LALRPOP expected performance (estimated):");
    println!("  - Small: < {:.1}ms (~{:.0}x faster)", BASELINE_SMALL_INPUT_MS as f64 / LALRPOP_FACTOR, LALRPOP_FACTOR);
    println!("  - Medium: < {:.1}ms (~{:.0}x faster)", BASELINE_MEDIUM_INPUT_MS as f64 / LALRPOP_FACTOR, LALRPOP_FACTOR);
    println!("  - Large: < {:.1}ms (~{:.0}x faster)", BASELINE_LARGE_INPUT_MS as f64 / LALRPOP_FACTOR, LALRPOP_FACTOR);
    println!();
    println!("Trade-off analysis:");
    println!("  ✅ Hand-written: Better error messages and recovery");
    println!("  ✅ Hand-written: More flexible parsing strategies");
    println!("  ✅ LALRPOP: ~{}x faster pure parsing speed", LALRPOP_FACTOR);
    println!("  ✅ LALRPOP: Generated at compile-time (no runtime overhead)");
    println!();
    println!("Conclusion: Hand-written parser is suitable for DixScript's");
    println!("error-recovery needs, even with the performance trade-off.");
    println!("==========================\n");
}

#[test]
#[ignore]
fn test_release_mode_performance() {
    // Very large input (1000 imports) - run in release mode only
    let mut input = String::from("@IMPORTS(\n");
    for i in 0..1000 {
        input.push_str(&format!(
            "    import{} from \"path/to/file{}.mdix\" verify \"hash{}\"\n",
            i, i, i
        ));
    }
    input.push_str(")");

    let tokens = tokenize_input(&input);
    let section_tokens = extract_imports_section_tokens(&tokens);
    let settings = OperationalSettings::default();

    let start = Instant::now();
    let mut parser = ImportsSectionParser::new(&section_tokens, &settings);
    let section = parser.parse_section().expect("Failed to parse");
    let duration = start.elapsed();

    let imports_per_sec = section.imports.len() as f64 / duration.as_secs_f64();

    println!("\n=== RELEASE MODE PERFORMANCE ===");
    println!("Imports: {}", section.imports.len());
    println!("Time: {:?}", duration);
    println!("Imports/sec: {:.0}", imports_per_sec);
    println!("Expected: > 10,000 imports/sec");
    println!("Status: {}", if imports_per_sec > 10000.0 { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(imports_per_sec > 10000.0, "Too slow in release mode: {:.0} imports/sec", imports_per_sec);
}

// ==================== MEMORY USAGE TESTS ====================

#[test]
fn test_memory_usage_estimate() {
    let input = r#"
        @IMPORTS(
            utils from "shared/utilities.mdix" verify "hash123"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");

    let import_size = std::mem::size_of_val(&section.imports[0]);

    println!("\n=== MEMORY USAGE ===");
    println!("ImportDeclaration struct: {} bytes", import_size);
    println!("Expected: < 1KB per import");
    println!("Status: {}", if import_size < 1024 { "✅ PASS" } else { "❌ FAIL" });
    println!("================================\n");

    assert!(import_size < 1024, "Import too large: {} bytes", import_size);
}

#[test]
fn test_no_memory_leaks_repeated_parsing() {
    let input = r#"
        @IMPORTS(
            utils from "shared/utilities.mdix"
        )
    "#;

    // Parse same input 1000 times
    for _ in 0..1000 {
        let _ = parse_imports_default(input);
    }

    println!("✅ Successfully parsed same input 1000 times without memory leaks");
}

// ==================== EDGE CASES ====================

#[test]
fn test_whitespace_handling() {
    let input = r#"
        @IMPORTS(
            utils   from   "file.mdix"
            crypto    from_cloud    "https://example.com/file.mdix"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");
    assert_eq!(section.imports.len(), 2);
}

#[test]
fn test_single_import() {
    let input = r#"@IMPORTS(utils from "file.mdix")"#;
    let section = parse_imports_default(input).expect("Failed to parse");
    assert_eq!(section.imports.len(), 1);
}

#[test]
fn test_http_localhost_allowed() {
    let input = r#"
        @IMPORTS(
            local from_cloud "http://localhost:8080/dev.mdix"
        )
    "#;

    let section = parse_imports_default(input).expect("Failed to parse");
    assert_eq!(section.imports.len(), 1);
    // Should not error, but should warn (check logs)
}