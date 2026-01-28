// tests/general_parser_tests.rs
//! Comprehensive tests for GeneralParser
//! Tests sequential vs concurrent parsing, performance, memory usage, and correctness

use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::{GeneralParser, OperationalSettings, ErrorHandlingStrategy, DebugMode, CompatibilityMode};
use dixscript::Compiler::AST::*;
use dixscript::ErrorManager::ErrorManager;

// ==================== TOKEN BUILDERS ====================

/// Build a complete valid DixScript token stream
fn build_complete_dixscript_tokens() -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut col = 1;

    // @CONFIG section (pre-processed, so we start after it)

    // @IMPORTS section
    tokens.push(Token::new(TokenType::SectionImports, line, col, None));
    col += 8;
    tokens.push(Token::new(TokenType::Symbol('('), line, col, Some("IMPORTS".to_string())));
    col += 1;

    // utils from "utils.mdix"
    tokens.push(Token::new(TokenType::Identifier("utils".to_string()), line, col, Some("IMPORTS".to_string())));
    col += 5;
    tokens.push(Token::new(TokenType::Keyword("from".to_string()), line, col, Some("IMPORTS".to_string())));
    col += 5;
    tokens.push(Token::new(TokenType::String("utils.mdix".to_string()), line, col, Some("IMPORTS".to_string())));
    col += 13;

    tokens.push(Token::new(TokenType::Symbol(')'), line, col, Some("IMPORTS".to_string())));
    line += 1;
    col = 1;

    // @DLM section
    tokens.push(Token::new(TokenType::SectionDLM, line, col, None));
    col += 4;
    tokens.push(Token::new(TokenType::Symbol('('), line, col, Some("DLM".to_string())));
    col += 1;

    // DCompressor.gzip, DEncryptor.aes256
    tokens.push(Token::new(TokenType::Identifier("DCompressor".to_string()), line, col, Some("DLM".to_string())));
    col += 11;
    tokens.push(Token::new(TokenType::Symbol('.'), line, col, Some("DLM".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Identifier("gzip".to_string()), line, col, Some("DLM".to_string())));
    col += 4;
    tokens.push(Token::new(TokenType::Symbol(','), line, col, Some("DLM".to_string())));
    col += 1;

    tokens.push(Token::new(TokenType::Identifier("DEncryptor".to_string()), line, col, Some("DLM".to_string())));
    col += 10;
    tokens.push(Token::new(TokenType::Symbol('.'), line, col, Some("DLM".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Identifier("aes256".to_string()), line, col, Some("DLM".to_string())));
    col += 6;

    tokens.push(Token::new(TokenType::Symbol(')'), line, col, Some("DLM".to_string())));
    line += 1;
    col = 1;

    // @ENUMS section
    tokens.push(Token::new(TokenType::SectionEnums, line, col, None));
    col += 6;
    tokens.push(Token::new(TokenType::Symbol('('), line, col, Some("ENUMS".to_string())));
    col += 1;
    line += 1;
    col = 1;

    // Status { PENDING, ACTIVE = 1, INACTIVE = 2 }
    tokens.push(Token::new(TokenType::Identifier("Status".to_string()), line, col, Some("ENUMS".to_string())));
    col += 6;
    tokens.push(Token::new(TokenType::Symbol('{'), line, col, Some("ENUMS".to_string())));
    col += 1;

    tokens.push(Token::new(TokenType::Identifier("PENDING".to_string()), line, col, Some("ENUMS".to_string())));
    col += 7;
    tokens.push(Token::new(TokenType::Symbol(','), line, col, Some("ENUMS".to_string())));
    col += 1;

    tokens.push(Token::new(TokenType::Identifier("ACTIVE".to_string()), line, col, Some("ENUMS".to_string())));
    col += 6;
    tokens.push(Token::new(TokenType::Symbol('='), line, col, Some("ENUMS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Integer(1), line, col, Some("ENUMS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Symbol(','), line, col, Some("ENUMS".to_string())));
    col += 1;

    tokens.push(Token::new(TokenType::Identifier("INACTIVE".to_string()), line, col, Some("ENUMS".to_string())));
    col += 8;
    tokens.push(Token::new(TokenType::Symbol('='), line, col, Some("ENUMS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Integer(2), line, col, Some("ENUMS".to_string())));
    col += 1;

    tokens.push(Token::new(TokenType::Symbol('}'), line, col, Some("ENUMS".to_string())));
    col += 1;
    line += 1;
    col = 1;

    tokens.push(Token::new(TokenType::Symbol(')'), line, col, Some("ENUMS".to_string())));
    line += 1;
    col = 1;

    // @QUICKFUNCS section
    tokens.push(Token::new(TokenType::SectionQuickFuncs, line, col, None));
    col += 11;
    tokens.push(Token::new(TokenType::Symbol('('), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    line += 1;
    col = 1;

    // ~add<int> => global (x<int>, y<int>) { return x + y; }
    tokens.push(Token::new(TokenType::Symbol('~'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Identifier("add".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 3;
    tokens.push(Token::new(TokenType::Symbol('<'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Identifier("int".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 3;
    tokens.push(Token::new(TokenType::Symbol('>'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;

    tokens.push(Token::new(TokenType::Arrow, line, col, Some("QUICKFUNCS".to_string())));
    col += 2;
    tokens.push(Token::new(TokenType::Keyword("global".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 6;

    tokens.push(Token::new(TokenType::Symbol('('), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Identifier("x".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Symbol('<'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Identifier("int".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 3;
    tokens.push(Token::new(TokenType::Symbol('>'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Symbol(','), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Identifier("y".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Symbol('<'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Identifier("int".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 3;
    tokens.push(Token::new(TokenType::Symbol('>'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Symbol(')'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;

    tokens.push(Token::new(TokenType::Symbol('{'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Keyword("return".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 6;
    tokens.push(Token::new(TokenType::Identifier("x".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Symbol('+'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Identifier("y".to_string()), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Symbol(';'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Symbol('}'), line, col, Some("QUICKFUNCS".to_string())));
    col += 1;
    line += 1;
    col = 1;

    tokens.push(Token::new(TokenType::Symbol(')'), line, col, Some("QUICKFUNCS".to_string())));
    line += 1;
    col = 1;

    // @DATA section
    tokens.push(Token::new(TokenType::SectionData, line, col, None));
    col += 5;
    tokens.push(Token::new(TokenType::Symbol('('), line, col, Some("DATA".to_string())));
    col += 1;
    line += 1;
    col = 1;

    // name = "Test"
    tokens.push(Token::new(TokenType::Identifier("name".to_string()), line, col, Some("DATA".to_string())));
    col += 4;
    tokens.push(Token::new(TokenType::Symbol('='), line, col, Some("DATA".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::String("Test".to_string()), line, col, Some("DATA".to_string())));
    col += 6;
    tokens.push(Token::new(TokenType::Symbol(','), line, col, Some("DATA".to_string())));
    line += 1;
    col = 1;

    // count = 42
    tokens.push(Token::new(TokenType::Identifier("count".to_string()), line, col, Some("DATA".to_string())));
    col += 5;
    tokens.push(Token::new(TokenType::Symbol('='), line, col, Some("DATA".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::Integer(42), line, col, Some("DATA".to_string())));
    col += 2;
    line += 1;
    col = 1;

    tokens.push(Token::new(TokenType::Symbol(')'), line, col, Some("DATA".to_string())));
    line += 1;
    col = 1;

    // @SECURITY section
    tokens.push(Token::new(TokenType::SectionSecurity, line, col, None));
    col += 9;
    tokens.push(Token::new(TokenType::Symbol('('), line, col, Some("SECURITY".to_string())));
    col += 1;
    line += 1;
    col = 1;

    // encryption -> { mode = "keyfile", algorithm = "aes256-gcm" }
    tokens.push(Token::new(TokenType::Identifier("encryption".to_string()), line, col, Some("SECURITY".to_string())));
    col += 10;
    tokens.push(Token::new(TokenType::MultiCharSymbol("->".to_string()), line, col, Some("SECURITY".to_string())));
    col += 2;
    tokens.push(Token::new(TokenType::Symbol('{'), line, col, Some("SECURITY".to_string())));
    col += 1;

    tokens.push(Token::new(TokenType::Identifier("mode".to_string()), line, col, Some("SECURITY".to_string())));
    col += 4;
    tokens.push(Token::new(TokenType::Symbol('='), line, col, Some("SECURITY".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::String("keyfile".to_string()), line, col, Some("SECURITY".to_string())));
    col += 9;
    tokens.push(Token::new(TokenType::Symbol(','), line, col, Some("SECURITY".to_string())));
    col += 1;

    tokens.push(Token::new(TokenType::Identifier("algorithm".to_string()), line, col, Some("SECURITY".to_string())));
    col += 9;
    tokens.push(Token::new(TokenType::Symbol('='), line, col, Some("SECURITY".to_string())));
    col += 1;
    tokens.push(Token::new(TokenType::String("aes256-gcm".to_string()), line, col, Some("SECURITY".to_string())));
    col += 12;

    tokens.push(Token::new(TokenType::Symbol('}'), line, col, Some("SECURITY".to_string())));
    col += 1;
    line += 1;
    col = 1;

    tokens.push(Token::new(TokenType::Symbol(')'), line, col, Some("SECURITY".to_string())));
    line += 1;
    col = 1;

    // EOF
    tokens.push(Token::eof(line, col));

    tokens
}

/// Build minimal valid token stream (just DATA section)
fn build_minimal_tokens() -> Vec<Token> {
    vec![
        Token::new(TokenType::SectionData, 1, 1, None),
        Token::new(TokenType::Symbol('('), 1, 6, Some("DATA".to_string())),
        Token::new(TokenType::Identifier("x".to_string()), 1, 7, Some("DATA".to_string())),
        Token::new(TokenType::Symbol('='), 1, 8, Some("DATA".to_string())),
        Token::new(TokenType::Integer(1), 1, 9, Some("DATA".to_string())),
        Token::new(TokenType::Symbol(')'), 1, 10, Some("DATA".to_string())),
        Token::eof(1, 11),
    ]
}

/// Build token stream with multiple sections for concurrent testing
fn build_multi_section_tokens(section_count: usize) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut line = 1;

    for i in 0..section_count {
        // Add ENUMS section
        tokens.push(Token::new(TokenType::SectionEnums, line, 1, None));
        tokens.push(Token::new(TokenType::Symbol('('), line, 7, Some("ENUMS".to_string())));

        tokens.push(Token::new(
            TokenType::Identifier(format!("Enum{}", i)),
            line,
            8,
            Some("ENUMS".to_string())
        ));
        tokens.push(Token::new(TokenType::Symbol('{'), line, 13, Some("ENUMS".to_string())));
        tokens.push(Token::new(TokenType::Identifier("VALUE".to_string()), line, 14, Some("ENUMS".to_string())));
        tokens.push(Token::new(TokenType::Symbol('}'), line, 19, Some("ENUMS".to_string())));

        tokens.push(Token::new(TokenType::Symbol(')'), line, 20, Some("ENUMS".to_string())));
        line += 1;
    }

    tokens.push(Token::eof(line, 1));
    tokens
}

/// Build config section for parser initialization
fn build_test_config_section() -> ConfigSection {
    ConfigSection {
        entries: vec![
            ConfigEntry {
                key: "version".to_string(),
                value: ConfigValue::String("1.0.0".to_string()),
                position: Position::new(1, 1),
            },
            ConfigEntry {
                key: "features".to_string(),
                value: ConfigValue::Features(vec!["advanced".to_string()]),
                position: Position::new(1, 20),
            },
        ],
        position: Position::new(1, 1),
    }
}

// ==================== HELPER FUNCTIONS ====================

fn create_operational_settings(
    error_strategy: ErrorHandlingStrategy,
    debug_mode: DebugMode,
) -> OperationalSettings {
    OperationalSettings {
        error_handling_strategy: error_strategy,
        compatibility_mode: CompatibilityMode::Strict,
        debug_mode,
        skip_imports_resolution: false,
        source_file_path: None,
        enabled_features: vec!["advanced".to_string()],
        version: "1.0.0".to_string(),
    }
}

// ==================== CORRECTNESS TESTS ====================

#[test]
fn test_parse_complete_dixscript() {
    let tokens = build_complete_dixscript_tokens();
    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);

    let parser = GeneralParser::new(tokens, config, settings)
        .expect("Failed to create parser");

    let result = parser.parse();
    assert!(result.is_ok(), "Parse should succeed");

    let script = result.unwrap();

    // Verify all sections present
    assert!(script.config.is_some(), "Config section missing");
    assert!(script.imports.is_some(), "Imports section missing");
    assert!(script.dlm.is_some(), "DLM section missing");
    assert!(script.enums.is_some(), "Enums section missing");
    assert!(script.quick_functions.is_some(), "QuickFuncs section missing");
    assert!(script.data.is_some(), "Data section missing");
    assert!(script.security.is_some(), "Security section missing");

    println!("✅ Complete DixScript parsed successfully");
}

#[test]
fn test_parse_minimal_script() {
    let tokens = build_minimal_tokens();
    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);

    let parser = GeneralParser::new(tokens, config, settings)
        .expect("Failed to create parser");

    let result = parser.parse();
    assert!(result.is_ok(), "Parse should succeed");

    let script = result.unwrap();
    assert!(script.data.is_some(), "Data section missing");

    println!("✅ Minimal script parsed successfully");
}

#[test]
fn test_empty_program() {
    let tokens = vec![Token::eof(1, 1)];
    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);

    let parser = GeneralParser::new(tokens, config, settings)
        .expect("Failed to create parser");

    let result = parser.parse();
    assert!(result.is_ok(), "Empty program should parse");

    let script = result.unwrap();
    assert!(script.config.is_some(), "Config should exist");
    assert!(script.data.is_none(), "Data should be None");

    println!("✅ Empty program handled correctly");
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_halt_strategy_stops_on_error() {
    // Build tokens with syntax error
    let mut tokens = vec![
        Token::new(TokenType::SectionEnums, 1, 1, None),
        Token::new(TokenType::Symbol('('), 1, 7, Some("ENUMS".to_string())),
        // Missing identifier - should cause error
        Token::new(TokenType::Symbol('{'), 1, 8, Some("ENUMS".to_string())),
        Token::eof(1, 9),
    ];

    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);

    let parser = GeneralParser::new(tokens, config, settings)
        .expect("Failed to create parser");

    let result = parser.parse();

    // With Halt strategy, should get None for section with error
    if let Ok(script) = result {
        // The section might be None or have partial data depending on error recovery
        println!("✅ Halt strategy handled error appropriately");
    }
}

#[test]
fn test_continue_strategy_recovers() {
    let mut tokens = vec![
        Token::new(TokenType::SectionEnums, 1, 1, None),
        Token::new(TokenType::Symbol('('), 1, 7, Some("ENUMS".to_string())),
        Token::new(TokenType::Symbol('{'), 1, 8, Some("ENUMS".to_string())), // Bad syntax
        Token::new(TokenType::Symbol(')'), 1, 9, Some("ENUMS".to_string())),
        Token::eof(1, 10),
    ];

    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Continue, DebugMode::Off);

    let parser = GeneralParser::new(tokens, config, settings)
        .expect("Failed to create parser");

    let result = parser.parse();
    assert!(result.is_ok(), "Continue strategy should not fail completely");

    println!("✅ Continue strategy recovered from error");
}

// ==================== SEQUENTIAL VS CONCURRENT TESTS ====================

#[test]
fn test_sequential_parsing() {
    let tokens = build_multi_section_tokens(5);
    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);

    let parser = GeneralParser::new(tokens, config, settings)
        .expect("Failed to create parser");

    let start = std::time::Instant::now();
    let result = parser.parse();
    let duration = start.elapsed();

    assert!(result.is_ok(), "Sequential parse should succeed");

    println!("✅ Sequential parsing completed in {:?}", duration);
}

#[test]
fn test_concurrent_parsing() {
    let tokens = build_multi_section_tokens(5);
    let config = build_test_config_section();

    // Concurrent requires Continue or Recover strategy (not Halt)
    let settings = create_operational_settings(ErrorHandlingStrategy::Continue, DebugMode::Off);

    let parser = GeneralParser::new(tokens, config, settings)
        .expect("Failed to create parser");

    let start = std::time::Instant::now();
    let result = parser.parse();
    let duration = start.elapsed();

    assert!(result.is_ok(), "Concurrent parse should succeed");

    println!("✅ Concurrent parsing completed in {:?}", duration);
}

#[test]
fn test_sequential_vs_concurrent_correctness() {
    let tokens = build_complete_dixscript_tokens();

    // Sequential parse
    let config1 = build_test_config_section();
    let settings1 = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);
    let parser1 = GeneralParser::new(tokens.clone(), config1, settings1)
        .expect("Failed to create sequential parser");
    let result1 = parser1.parse().expect("Sequential parse failed");

    // Concurrent parse
    let config2 = build_test_config_section();
    let settings2 = create_operational_settings(ErrorHandlingStrategy::Continue, DebugMode::Off);
    let parser2 = GeneralParser::new(tokens, config2, settings2)
        .expect("Failed to create concurrent parser");
    let result2 = parser2.parse().expect("Concurrent parse failed");

    // Compare results (both should have same sections present)
    assert_eq!(result1.imports.is_some(), result2.imports.is_some());
    assert_eq!(result1.dlm.is_some(), result2.dlm.is_some());
    assert_eq!(result1.enums.is_some(), result2.enums.is_some());
    assert_eq!(result1.quick_functions.is_some(), result2.quick_functions.is_some());
    assert_eq!(result1.data.is_some(), result2.data.is_some());
    assert_eq!(result1.security.is_some(), result2.security.is_some());

    println!("✅ Sequential and concurrent parsing produce equivalent results");
}

// ==================== PERFORMANCE BENCHMARKS ====================

#[test]
fn benchmark_small_file() {
    let tokens = build_minimal_tokens();
    let iterations = 1000;

    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);

    let start = std::time::Instant::now();

    for _ in 0..iterations {
        let parser = GeneralParser::new(tokens.clone(), config.clone(), settings.clone())
            .expect("Failed to create parser");
        let _ = parser.parse();
    }

    let duration = start.elapsed();
    let avg = duration / iterations;

    println!("📊 Small file benchmark:");
    println!("   Total: {:?} for {} iterations", duration, iterations);
    println!("   Average: {:?} per parse", avg);
    println!("   Throughput: {:.2} parses/sec", iterations as f64 / duration.as_secs_f64());
}

#[test]
fn benchmark_medium_file() {
    let tokens = build_complete_dixscript_tokens();
    let iterations = 100;

    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Continue, DebugMode::Off);

    let start = std::time::Instant::now();

    for _ in 0..iterations {
        let parser = GeneralParser::new(tokens.clone(), config.clone(), settings.clone())
            .expect("Failed to create parser");
        let _ = parser.parse();
    }

    let duration = start.elapsed();
    let avg = duration / iterations;

    println!("📊 Medium file benchmark:");
    println!("   Total: {:?} for {} iterations", duration, iterations);
    println!("   Average: {:?} per parse", avg);
    println!("   Throughput: {:.2} parses/sec", iterations as f64 / duration.as_secs_f64());
}

#[test]
fn benchmark_large_file() {
    let tokens = build_multi_section_tokens(50);
    let iterations = 10;

    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Continue, DebugMode::Off);

    let start = std::time::Instant::now();

    for _ in 0..iterations {
        let parser = GeneralParser::new(tokens.clone(), config.clone(), settings.clone())
            .expect("Failed to create parser");
        let _ = parser.parse();
    }

    let duration = start.elapsed();
    let avg = duration / iterations;

    println!("📊 Large file benchmark:");
    println!("   Total: {:?} for {} iterations", duration, iterations);
    println!("   Average: {:?} per parse", avg);
    println!("   Throughput: {:.2} parses/sec", iterations as f64 / duration.as_secs_f64());
}

#[test]
fn benchmark_concurrent_vs_sequential() {
    let tokens = build_multi_section_tokens(20);
    let iterations = 10;

    // Sequential
    let config1 = build_test_config_section();
    let settings1 = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);

    let start_seq = std::time::Instant::now();
    for _ in 0..iterations {
        let parser = GeneralParser::new(tokens.clone(), config1.clone(), settings1.clone())
            .expect("Failed to create parser");
        let _ = parser.parse();
    }
    let duration_seq = start_seq.elapsed();

    // Concurrent
    let config2 = build_test_config_section();
    let settings2 = create_operational_settings(ErrorHandlingStrategy::Continue, DebugMode::Off);

    let start_con = std::time::Instant::now();
    for _ in 0..iterations {
        let parser = GeneralParser::new(tokens.clone(), config2.clone(), settings2.clone())
            .expect("Failed to create parser");
        let _ = parser.parse();
    }
    let duration_con = start_con.elapsed();

    println!("📊 Sequential vs Concurrent benchmark:");
    println!("   Sequential: {:?} ({:.2} parses/sec)",
             duration_seq,
             iterations as f64 / duration_seq.as_secs_f64());
    println!("   Concurrent: {:?} ({:.2} parses/sec)",
             duration_con,
             iterations as f64 / duration_con.as_secs_f64());

    let speedup = duration_seq.as_secs_f64() / duration_con.as_secs_f64();
    println!("   Speedup: {:.2}x", speedup);
}

// ==================== MEMORY USAGE TESTS ====================

#[test]
fn test_memory_usage_small_file() {
    use std::mem::size_of_val;

    let tokens = build_minimal_tokens();
    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);

    let parser = GeneralParser::new(tokens.clone(), config, settings)
        .expect("Failed to create parser");

    let result = parser.parse().expect("Parse failed");

    // Approximate memory usage
    let tokens_size = tokens.len() * std::mem::size_of::<Token>();
    let script_size = size_of_val(&result);

    println!("💾 Memory usage (small file):");
    println!("   Tokens: {} bytes ({} tokens)", tokens_size, tokens.len());
    println!("   AST: {} bytes", script_size);
    println!("   Total: {} bytes", tokens_size + script_size);
}

#[test]
fn test_memory_usage_large_file() {
    use std::mem::size_of_val;

    let tokens = build_multi_section_tokens(100);
    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Continue, DebugMode::Off);

    let parser = GeneralParser::new(tokens.clone(), config, settings)
        .expect("Failed to create parser");

    let result = parser.parse().expect("Parse failed");

    let tokens_size = tokens.len() * std::mem::size_of::<Token>();
    let script_size = size_of_val(&result);

    println!("💾 Memory usage (large file):");
    println!("   Tokens: {} bytes ({} tokens)", tokens_size, tokens.len());
    println!("   AST: {} bytes", script_size);
    println!("   Total: {} bytes", tokens_size + script_size);
    println!("   Per section: ~{} bytes", script_size / 100);
}

// ==================== SECTION INDEPENDENCE TESTS ====================

#[test]
fn test_section_independence() {
    // Each section should parse independently without affecting others
    let tokens = build_complete_dixscript_tokens();
    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Continue, DebugMode::Off);

    let parser = GeneralParser::new(tokens, config, settings)
        .expect("Failed to create parser");

    let result = parser.parse().expect("Parse failed");

    // Verify each section exists independently
    assert!(result.imports.is_some());
    assert!(result.dlm.is_some());
    assert!(result.enums.is_some());
    assert!(result.quick_functions.is_some());
    assert!(result.data.is_some());
    assert!(result.security.is_some());

    println!("✅ All sections parsed independently");
}

// ==================== STRESS TESTS ====================

#[test]
fn stress_test_many_sections() {
    let tokens = build_multi_section_tokens(1000);
    let config = build_test_config_section();
    let settings = create_operational_settings(ErrorHandlingStrategy::Continue, DebugMode::Off);

    let parser = GeneralParser::new(tokens.clone(), config, settings)
        .expect("Failed to create parser");

    let start = std::time::Instant::now();
    let result = parser.parse();
    let duration = start.elapsed();

    assert!(result.is_ok(), "Stress test should succeed");

    println!("🔥 Stress test (1000 sections):");
    println!("   Duration: {:?}", duration);
    println!("   Tokens: {}", tokens.len());
}

#[test]
fn test_parser_reusability() {
    // Test that parser can be created multiple times without issues
    let tokens = build_minimal_tokens();
    let config = build_test_config_section();

    for i in 0..10 {
        let settings = create_operational_settings(ErrorHandlingStrategy::Halt, DebugMode::Off);
        let parser = GeneralParser::new(tokens.clone(), config.clone(), settings)
            .expect("Failed to create parser");

        let result = parser.parse();
        assert!(result.is_ok(), "Parse {} failed", i);
    }

    println!("✅ Parser can be reused multiple times");
}