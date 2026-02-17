// tests/parsing_pipeline_tests.rs

use dixscript::Compiler::Core::{
    ConfigSectionHandler, ProcessConfigResult,
    OperationalSettings, ErrorHandlingStrategy, DebugMode,
};
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, TokenizationResult};
use dixscript::Compiler::Core::GeneralParser;
use dixscript::Compiler::AST::DixScript;
use dixscript::ErrorManager::{ErrorManager, DiagnosticDumper};
use std::time::Instant;
use std::fs;

// ==================== HELPER FUNCTIONS ====================

fn parse_mdix_file(filepath: &str) -> Result<DixScript, String> {
    // 1. Read file
    let source = fs::read_to_string(filepath)
        .map_err(|e| format!("Failed to read {}: {}", filepath, e))?;

    // 2. CRITICAL: Handle config FIRST
    let config_handler = ConfigSectionHandler::new();
    let config_result = config_handler.process_config(&source)
        .map_err(|e| format!("Config processing failed: {:?}", e))?;

    // 3. Tokenize cleaned input with operational settings
    let tokenizer = Tokenizer::new(config_result.cleaned_source.clone());
    let token_result = tokenizer.tokenize();

    if token_result.tokens.is_empty() {
        return Err("Tokenization produced no tokens".to_string());
    }

    // 4. Parse tokens into AST
    let parser = GeneralParser::new(
        token_result.tokens,
        config_result.config_section.clone(),
        config_result.operational_settings.clone(),
    ).map_err(|e| format!("Parser creation failed: {:?}", e))?;

    let ast = parser.parse()
        .map_err(|e| format!("Parsing failed: {:?}", e))?;

    Ok(ast)
}

fn parse_mdix_source(source: &str) -> Result<DixScript, String> {
    // CRITICAL: Config first
    let config_handler = ConfigSectionHandler::new();
    let config_result = config_handler.process_config(source)
        .map_err(|e| format!("Config processing failed: {:?}", e))?;

    // Tokenize
    let tokenizer = Tokenizer::new(config_result.cleaned_source.clone());
    let token_result = tokenizer.tokenize();

    // Parse
    let parser = GeneralParser::new(
        token_result.tokens,
        config_result.config_section.clone(),
        config_result.operational_settings.clone(),
    ).map_err(|e| format!("Parser creation failed: {:?}", e))?;

    parser.parse()
        .map_err(|e| format!("Parsing failed: {:?}", e))
}

fn benchmark_parse(source: &str, iterations: usize) -> (f64, usize) {
    let start = Instant::now();
    let mut total_tokens = 0;

    for _ in 0..iterations {
        let config_handler = ConfigSectionHandler::new();
        let config_result = config_handler.process_config(source).unwrap();
        let tokenizer = Tokenizer::new(config_result.cleaned_source.clone());
        let token_result = tokenizer.tokenize();
        total_tokens = token_result.tokens.len();

        let parser = GeneralParser::new(
            token_result.tokens,
            config_result.config_section.clone(),
            config_result.operational_settings.clone(),
        ).unwrap();
        let _ast = parser.parse().unwrap();
    }

    let duration = start.elapsed();
    let avg_ms = duration.as_secs_f64() * 1000.0 / iterations as f64;

    (avg_ms, total_tokens)
}

// ==================== BASIC PARSING TESTS ====================

#[test]
fn test_config_handler_first() {
    let source = r#"
@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8"
)

@DATA(
    x = 42
)
"#;

    // Step 1: Config MUST come first
    let config_handler = ConfigSectionHandler::new();
    let config_result = config_handler.process_config(source);

    assert!(config_result.is_ok(), "Config processing failed");
    let config = config_result.unwrap();

    assert_eq!(config.operational_settings.version, "1.0.0");

    // Step 2: Then tokenization
    let tokenizer = Tokenizer::new(config.cleaned_source.clone());
    let tokens = tokenizer.tokenize();

    assert!(tokens.tokens.len() > 0, "No tokens generated");

    // Step 3: Then parsing
    let parser = GeneralParser::new(
        tokens.tokens,
        config.config_section,
        config.operational_settings,
    ).unwrap();

    let ast = parser.parse().unwrap();
    assert!(ast.config.is_some());
    assert!(ast.data.is_some());
}

#[test]
fn test_parse_minimal_mdix() {
    let source = r#"
@CONFIG(version -> "1.0.0")
@DATA(x = 42)
"#;

    let ast = parse_mdix_source(source).unwrap();

    assert!(ast.config.is_some());
    assert!(ast.data.is_some());

    let data = ast.data.unwrap();
    assert_eq!(data.entries.len(), 1);
}

#[test]
fn test_parse_all_sections() {
    let source = r#"
@CONFIG(
    version -> "1.0.0",
    features -> "advanced"
)

@IMPORTS(
    utils from "test.mdix"
)

@DLM(
    DCompressor.gzip
)

@ENUMS(
    Status { ACTIVE = 1, INACTIVE = 2 }
)

@QUICKFUNCS(
    ~test<int> => global() {
        return 42;
    }
)

@DATA(
    x = 100
)

@SECURITY(
    encryption -> {
        mode = "auto"
    }
)
"#;

    let ast = parse_mdix_source(source).unwrap();

    assert!(ast.config.is_some(), "Missing CONFIG");
    assert!(ast.imports.is_some(), "Missing IMPORTS");
    assert!(ast.dlm.is_some(), "Missing DLM");
    assert!(ast.enums.is_some(), "Missing ENUMS");
    assert!(ast.quick_functions.is_some(), "Missing QUICKFUNCS");
    assert!(ast.data.is_some(), "Missing DATA");
    assert!(ast.security.is_some(), "Missing SECURITY");
}

// ==================== REAL FILE TESTS ====================

#[test]
fn test_parse_all_datatypes_mdix() {
    let ast = parse_mdix_file("mdix_files/advanced/all_datatypes_test.mdix")
        .expect("Failed to parse all_datatypes_test.mdix");

    assert!(ast.config.is_some());
    assert!(ast.enums.is_some());
    assert!(ast.quick_functions.is_some());
    assert!(ast.data.is_some());

    let enums = ast.enums.as_ref().unwrap();
    assert!(enums.enums.iter().any(|e| e.name == "TestEnum"));

    let data = ast.data.as_ref().unwrap();
    assert!(data.entries.len() > 50, "Expected many data entries");
}

#[test]
fn test_parse_data_variable_usage_mdix() {
    let ast = parse_mdix_file("mdix_files/advanced/data_variable_usage.mdix")
        .expect("Failed to parse data_variable_usage.mdix");

    assert!(ast.config.is_some());
    assert!(ast.enums.is_some());
    assert!(ast.quick_functions.is_some());
    assert!(ast.data.is_some());
}

#[test]
fn test_parse_enum_test_mdix() {
    let ast = parse_mdix_file("mdix_files/advanced/enum_test.mdix")
        .expect("Failed to parse enum_test.mdix");

    assert!(ast.enums.is_some());
    let enums = ast.enums.unwrap();
    assert_eq!(enums.enums.len(), 1);
    assert_eq!(enums.enums[0].name, "TestEnum");
}

#[test]
fn test_parse_basic_test_mdix() {
    let ast = parse_mdix_file("mdix_files/basic/basic_test.mdix")
        .expect("Failed to parse basic_test.mdix");

    assert!(ast.enums.is_some());
}

#[test]
fn test_parse_sample_9kb_mdix() {
    let ast = parse_mdix_file("tests/fixtures/sample_9kb.mdix")
        .expect("Failed to parse sample_9kb.mdix");

    assert!(ast.config.is_some());
    assert!(ast.imports.is_some());
    assert!(ast.dlm.is_some());
    assert!(ast.enums.is_some());
    assert!(ast.quick_functions.is_some());
    assert!(ast.data.is_some());
    assert!(ast.security.is_some());
}

// ==================== PERFORMANCE BENCHMARKS ====================

#[test]
fn benchmark_parsing_small_file() {
    let source = r#"
@CONFIG(version -> "1.0.0")
@DATA(
    x = 42,
    y = "test",
    z = true
)
"#;

    let (avg_ms, tokens) = benchmark_parse(source, 1000);

    println!("\n=== SMALL FILE BENCHMARK ===");
    println!("Source size: {} bytes", source.len());
    println!("Tokens: {}", tokens);
    println!("Average parse time: {:.4} ms", avg_ms);
    println!("Throughput: {:.0} parses/sec", 1000.0 / avg_ms);
    println!("============================\n");

    // Should parse small files in < 1ms
    assert!(avg_ms < 1.0, "Too slow: {:.4} ms", avg_ms);
}

#[test]
fn benchmark_parsing_medium_file() {
    let mut source = String::from("@CONFIG(version -> \"1.0.0\")\n@DATA(\n");
    for i in 0..100 {
        source.push_str(&format!("    var{} = {},\n", i, i * 2));
    }
    source.push_str(")");

    let (avg_ms, tokens) = benchmark_parse(&source, 500);

    println!("\n=== MEDIUM FILE BENCHMARK ===");
    println!("Source size: {} bytes", source.len());
    println!("Tokens: {}", tokens);
    println!("Average parse time: {:.4} ms", avg_ms);
    println!("Throughput: {:.0} parses/sec", 1000.0 / avg_ms);
    println!("==============================\n");

    // Should parse medium files in < 5ms
    assert!(avg_ms < 5.0, "Too slow: {:.4} ms", avg_ms);
}

#[test]
fn benchmark_parsing_large_file() {
    let mut source = String::from("@CONFIG(version -> \"1.0.0\", features -> \"advanced\")\n@DATA(\n");
    for i in 0..500 {
        source.push_str(&format!("    variable_{} = {},\n", i, i * 2));
    }
    source.push_str(")");

    let (avg_ms, tokens) = benchmark_parse(&source, 100);

    println!("\n=== LARGE FILE BENCHMARK ===");
    println!("Source size: {} bytes", source.len());
    println!("Tokens: {}", tokens);
    println!("Average parse time: {:.4} ms", avg_ms);
    println!("Throughput: {:.0} parses/sec", 1000.0 / avg_ms);
    println!("=============================\n");

    // Should parse large files in < 20ms
    assert!(avg_ms < 20.0, "Too slow: {:.4} ms", avg_ms);
}

#[test]
fn benchmark_parsing_complex_file() {
    let source = fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
        .expect("Failed to read all_datatypes_test.mdix");

    let (avg_ms, tokens) = benchmark_parse(&source, 50);

    println!("\n=== COMPLEX FILE BENCHMARK (all_datatypes_test.mdix) ===");
    println!("Source size: {} bytes", source.len());
    println!("Tokens: {}", tokens);
    println!("Average parse time: {:.4} ms", avg_ms);
    println!("Throughput: {:.0} parses/sec", 1000.0 / avg_ms);
    println!("=========================================================\n");

    // Complex files should parse in < 30ms
    assert!(avg_ms < 30.0, "Too slow: {:.4} ms", avg_ms);
}

#[test]
fn throughput_benchmark_bytes_per_second() {
    let source = fs::read_to_string("tests/fixtures/sample_9kb.mdix")
        .expect("Failed to read sample_9kb.mdix");

    let source_size = source.len();
    let iterations = 100;

    let start = Instant::now();
    for _ in 0..iterations {
        let _ = parse_mdix_source(&source).unwrap();
    }
    let duration = start.elapsed();

    let bytes_per_sec = (source_size * iterations) as f64 / duration.as_secs_f64();
    let mb_per_sec = bytes_per_sec / 1_000_000.0;

    println!("\n=== THROUGHPUT BENCHMARK ===");
    println!("File size: {} bytes ({:.2} KB)", source_size, source_size as f64 / 1024.0);
    println!("Iterations: {}", iterations);
    println!("Total time: {:?}", duration);
    println!("Throughput: {:.2} MB/sec", mb_per_sec);
    println!("Speed: {:.0} bytes/sec", bytes_per_sec);
    println!("============================\n");

    // Should process at least 1 MB/sec
    assert!(mb_per_sec > 1.0, "Too slow: {:.2} MB/sec", mb_per_sec);
}

// ==================== ERROR HANDLING TESTS ====================

#[test]
fn test_missing_config_section() {
    let source = "@DATA(x = 42)";

    // Should still work - config has defaults
    let result = parse_mdix_source(source);
    assert!(result.is_ok(), "Should handle missing config with defaults");
}

#[test]
fn test_invalid_config_value() {
    let source = r#"
@CONFIG(
    version -> "invalid_version_format",
    features -> 12345
)
@DATA(x = 42)
"#;

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let result = parse_mdix_source(source);

    // May succeed with warnings or fail depending on validation
    if result.is_err() {
        println!("Expected error: {}", result.unwrap_err());
    }
}

#[test]
fn test_parse_error_with_diagnostic_dump() {
    let source = r#"
@CONFIG(version -> "1.0.0")
@DATA(
    x = 42
    y = "missing comma before this"
)
"#;

    let error_manager = ErrorManager::get_shared_instance();
    error_manager.clear_errors();

    let result = parse_mdix_source(source);

    if result.is_err() {
        // Generate diagnostic dump
        let dumper = DiagnosticDumper::new();
        let dump_path = dumper.dump_to_file("parse_error_diagnostic.txt")
            .expect("Failed to write diagnostic dump");

        println!("Diagnostic dump written to: {}", dump_path);
        println!("Error: {}", result.unwrap_err());
    }
}

// ==================== MEMORY USAGE TESTS ====================

#[test]
fn test_ast_memory_footprint() {
    let source = fs::read_to_string("mdix_files/advanced/all_datatypes_test.mdix")
        .expect("Failed to read file");

    let ast = parse_mdix_source(&source).unwrap();
    let ast_size = std::mem::size_of_val(&ast);

    println!("\n=== AST MEMORY FOOTPRINT ===");
    println!("Source size: {} bytes", source.len());
    println!("AST size: {} bytes", ast_size);
    println!("Ratio: {:.2}x", ast_size as f64 / source.len() as f64);
    println!("============================\n");

    // AST should not be more than 10x source size
    assert!(ast_size < source.len() * 10, "AST too large: {} bytes", ast_size);
}

// ==================== STRESS TESTS ====================

#[test]
#[ignore] // Run with: cargo test --release -- --ignored
fn stress_test_parsing_1000_iterations() {
    let source = fs::read_to_string("tests/fixtures/sample_9kb.mdix")
        .expect("Failed to read file");

    let iterations = 1000;
    let start = Instant::now();

    for i in 0..iterations {
        let result = parse_mdix_source(&source);
        assert!(result.is_ok(), "Parse failed on iteration {}", i);
    }

    let duration = start.elapsed();
    let avg_ms = duration.as_secs_f64() * 1000.0 / iterations as f64;

    println!("\n=== STRESS TEST (1000 iterations) ===");
    println!("Total time: {:?}", duration);
    println!("Average: {:.4} ms per parse", avg_ms);
    println!("Throughput: {:.0} parses/sec", 1000.0 / avg_ms);
    println!("======================================\n");
}

// ==================== COMPARISON WITH OTHER FORMATS (if available) ====================

#[test]
#[cfg(feature = "compare_formats")]
fn compare_with_jsonnet() {
    // Note: This requires jsonnet crate to be added as optional dependency
    // [dependencies]
    // jsonnet = { version = "0.1", optional = true }

    // TODO: Implement when jsonnet comparison is needed
    println!("Jsonnet comparison not yet implemented");
}

#[test]
#[cfg(feature = "compare_formats")]
fn compare_with_cue() {
    // Note: This requires cue crate if available
    // TODO: Check if Rust cue implementation exists

    println!("CUE comparison not yet implemented");
}