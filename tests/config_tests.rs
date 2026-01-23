//! Comprehensive tests for Config section handling
//!
//! Tests cover:
//! 1. OperationalSettings creation and defaults
//! 2. Config extraction and removal from input
//! 3. Config parsing and validation
//! 4. Performance benchmarks
//! 5. Edge cases and error handling

#[cfg(test)]
mod config_tests {
    use crate::Compiler::Core::Config::{
        ConfigSectionHandler, ConfigSchema, OperationalSettings,
        ErrorHandlingStrategy, CompatibilityMode, DebugMode,
    };
    use crate::Compiler::AST::{ConfigSection, ConfigValue};
    use std::time::Instant;

    // ==================== OPERATIONAL SETTINGS TESTS ====================

    #[test]
    fn test_operational_settings_defaults() {
        let settings = OperationalSettings::default();

        assert_eq!(settings.error_handling_strategy, ErrorHandlingStrategy::Halt);
        assert_eq!(settings.compatibility_mode, CompatibilityMode::Strict);
        assert_eq!(settings.debug_mode, DebugMode::Off);
        assert_eq!(settings.version, "1.0.0");
        assert_eq!(settings.enabled_features, vec!["advanced"]);
        assert!(!settings.skip_imports_resolution);
        assert!(settings.source_file_path.is_none());
    }

    #[test]
    fn test_operational_settings_advanced_mode() {
        let mut settings = OperationalSettings::default();

        // Advanced mode with explicit "advanced" feature
        settings.enabled_features = vec!["advanced".to_string()];
        assert!(settings.is_advanced_mode());

        // Advanced mode with quickfuncs
        settings.enabled_features = vec!["quickfuncs".to_string()];
        assert!(settings.is_advanced_mode());

        // Advanced mode with enums
        settings.enabled_features = vec!["enums".to_string()];
        assert!(settings.is_advanced_mode());

        // Basic mode
        settings.enabled_features = vec!["basic".to_string()];
        assert!(!settings.is_advanced_mode());
    }

    #[test]
    fn test_operational_settings_feature_enabled() {
        let mut settings = OperationalSettings::default();

        // In advanced mode, all features should be enabled except "basic"
        settings.enabled_features = vec!["advanced".to_string()];
        assert!(settings.is_feature_enabled("quickfuncs"));
        assert!(settings.is_feature_enabled("enums"));
        assert!(settings.is_feature_enabled("data"));
        assert!(!settings.is_feature_enabled("basic"));

        // In basic mode, only basic features
        settings.enabled_features = vec!["basic".to_string()];
        assert!(settings.is_feature_enabled("basic"));
        assert!(settings.is_feature_enabled("data")); // data is in basic
    }

    // ==================== CONFIG EXTRACTION TESTS ====================

    #[test]
    fn test_config_extraction_simple() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8"
)

@DATA(
    name = "test"
)
"#;

        let result = handler.process_config_section(input);

        // Config should be extracted
        assert!(!result.config_section.entries.is_empty());

        // Config should be removed from cleaned input
        assert!(!result.cleaned_input_string.contains("@CONFIG"));
        assert!(result.cleaned_input_string.contains("@DATA"));

        println!("✓ Config extracted and removed successfully");
        println!("Cleaned input:\n{}", result.cleaned_input_string);
    }

    #[test]
    fn test_config_extraction_complex() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
// This is a comment
@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    author -> "Test Author",
    created -> "2024-01-23T12:00:00Z",
    features -> "quickfuncs,enums,data",
    debug_mode -> "verbose",
    error_handling -> "continue",
    compatibility_mode -> "permissive"
)

@ENUMS(
    Status { ACTIVE, INACTIVE }
)

@DATA(
    name = "test",
    count = 42
)
"#;

        let result = handler.process_config_section(input);

        // Verify extraction
        assert!(!result.config_section.entries.is_empty());
        assert!(!result.cleaned_input_string.contains("@CONFIG"));
        assert!(result.cleaned_input_string.contains("@ENUMS"));
        assert!(result.cleaned_input_string.contains("@DATA"));

        // Verify operational settings
        assert_eq!(result.operational_settings.error_handling_strategy, ErrorHandlingStrategy::Continue);
        assert_eq!(result.operational_settings.debug_mode, DebugMode::Verbose);
        assert_eq!(result.operational_settings.compatibility_mode, CompatibilityMode::Permissive);

        println!("✓ Complex config extracted with all settings");
        println!("Features: {:?}", result.operational_settings.enabled_features);
    }

    #[test]
    fn test_config_with_nested_parens() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG(
    version -> "1.0.0",
    description -> "Test (with parentheses) in value"
)

@DATA(
    value = 123
)
"#;

        let result = handler.process_config_section(input);

        assert!(!result.cleaned_input_string.contains("@CONFIG"));
        assert!(result.cleaned_input_string.contains("@DATA"));

        println!("✓ Nested parentheses handled correctly");
    }

    #[test]
    fn test_config_with_strings_containing_commas() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG(
    version -> "1.0.0",
    features -> "quickfuncs,enums,data",
    description -> "This has, many, commas"
)

@DATA(x = 1)
"#;

        let result = handler.process_config_section(input);

        // Should parse features as list
        let features = &result.operational_settings.enabled_features;
        assert!(features.contains(&"quickfuncs".to_string()));
        assert!(features.contains(&"enums".to_string()));
        assert!(features.contains(&"data".to_string()));

        println!("✓ String values with commas parsed correctly");
        println!("Features: {:?}", features);
    }

    #[test]
    fn test_no_config_section() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@DATA(
    name = "test",
    value = 42
)
"#;

        let result = handler.process_config_section(input);

        // Should use default config
        assert!(!result.config_section.entries.is_empty());
        assert_eq!(result.operational_settings.version, "1.0.0");
        assert!(result.warnings.iter().any(|w| w.contains("No CONFIG section found")));

        // Input should be unchanged
        assert_eq!(result.cleaned_input_string.trim(), input.trim());

        println!("✓ No config section handled with defaults");
    }

    #[test]
    fn test_empty_input() {
        let handler = ConfigSectionHandler::new(None);
        let result = handler.process_config_section("");

        // Should use cached minimal config
        assert!(!result.config_section.entries.is_empty());
        assert_eq!(result.operational_settings.version, "1.0.0");
        assert!(result.warnings.iter().any(|w| w.contains("Empty input")));

        println!("✓ Empty input handled with minimal config");
    }

    #[test]
    fn test_config_with_comments() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
// Top comment
@CONFIG(
    // Inline comment
    version -> "1.0.0", // End of line comment
    /* Multi-line
       comment */
    encoding -> "utf-8"
)

@DATA(x = 1)
"#;

        let result = handler.process_config_section(input);

        assert!(!result.cleaned_input_string.contains("@CONFIG"));
        assert!(result.cleaned_input_string.contains("@DATA"));

        println!("✓ Comments in config handled correctly");
    }

    // ==================== CONFIG VALIDATION TESTS ====================

    #[test]
    fn test_config_schema_validation_valid() {
        let mut config = std::collections::HashMap::new();
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("encoding".to_string(), "utf-8".to_string());
        config.insert("features".to_string(), "advanced".to_string());

        let result = ConfigSchema::validate_and_enhance_config(config);
        assert!(result.is_ok());

        let validated = result.unwrap();
        assert_eq!(validated.get("version").unwrap(), "1.0.0");
        assert_eq!(validated.get("encoding").unwrap(), "utf-8");

        println!("✓ Valid config passed validation");
    }

    #[test]
    fn test_config_schema_validation_invalid_version() {
        let mut config = std::collections::HashMap::new();
        config.insert("version".to_string(), "invalid.version.format.too.many.parts".to_string());

        let result = ConfigSchema::validate_and_enhance_config(config);
        assert!(result.is_ok()); // Should still succeed with default

        let validated = result.unwrap();
        // Should use default version instead
        assert_eq!(validated.get("version").unwrap(), "1.0.0");

        println!("✓ Invalid version replaced with default");
    }

    #[test]
    fn test_config_schema_validation_invalid_encoding() {
        let mut config = std::collections::HashMap::new();
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("encoding".to_string(), "invalid-encoding".to_string());

        let result = ConfigSchema::validate_and_enhance_config(config);
        assert!(result.is_ok());

        let validated = result.unwrap();
        // Should use default encoding
        assert_eq!(validated.get("encoding").unwrap(), "utf-8");

        println!("✓ Invalid encoding replaced with default");
    }

    #[test]
    fn test_config_schema_required_keys() {
        let config = std::collections::HashMap::new(); // Empty config

        let result = ConfigSchema::validate_and_enhance_config(config);
        assert!(result.is_ok());

        let validated = result.unwrap();
        // Required keys should be added
        assert!(validated.contains_key("version"));
        assert!(validated.contains_key("encoding"));

        println!("✓ Required keys added automatically");
    }

    #[test]
    fn test_config_value_creation() {
        use crate::Compiler::AST::ConfigValue;

        let mut config = std::collections::HashMap::new();
        config.insert("error_handling".to_string(), "continue".to_string());
        config.insert("debug_mode".to_string(), "verbose".to_string());
        config.insert("compatibility_mode".to_string(), "permissive".to_string());
        config.insert("features".to_string(), "quickfuncs,enums".to_string());

        let validated = ConfigSchema::validate_and_enhance_config(config).unwrap();
        let config_section = ConfigSchema::create_config_section(validated);

        // Find and verify error_handling
        let error_handling = config_section.entries.iter()
            .find(|e| e.key == "error_handling")
            .expect("error_handling not found");

        assert!(matches!(error_handling.value, ConfigValue::ErrorHandling(_)));

        println!("✓ ConfigValue types created correctly");
    }

    // ==================== PERFORMANCE TESTS ====================

    #[test]
    fn test_performance_small_config() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8"
)

@DATA(x = 1)
"#;

        let iterations = 1000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = handler.process_config_section(input);
        }

        let elapsed = start.elapsed();
        let avg_time = elapsed / iterations;

        println!("\n=== PERFORMANCE: Small Config ===");
        println!("Iterations: {}", iterations);
        println!("Total time: {:?}", elapsed);
        println!("Average time: {:?}", avg_time);
        println!("Throughput: {:.2} ops/sec", iterations as f64 / elapsed.as_secs_f64());

        // Should be under 0.5ms per operation (target from C#)
        assert!(avg_time.as_micros() < 500, "Performance regression: {:?} > 500µs", avg_time);
    }

    #[test]
    fn test_performance_large_config() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    author -> "Test Author with a longer name",
    created -> "2024-01-23T12:00:00.000Z",
    features -> "quickfuncs,enums,data,security,imports,dlm",
    debug_mode -> "verbose",
    error_handling -> "continue",
    compatibility_mode -> "permissive",
    custom_field_1 -> "custom value 1",
    custom_field_2 -> "custom value 2",
    custom_field_3 -> "custom value 3"
)

@DATA(x = 1, y = 2, z = 3)
@ENUMS(Status { ACTIVE, INACTIVE, PENDING })
@QUICKFUNCS(~test<int>() { return 42; })
"#;

        let iterations = 1000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = handler.process_config_section(input);
        }

        let elapsed = start.elapsed();
        let avg_time = elapsed / iterations;

        println!("\n=== PERFORMANCE: Large Config ===");
        println!("Input size: {} bytes", input.len());
        println!("Iterations: {}", iterations);
        println!("Total time: {:?}", elapsed);
        println!("Average time: {:?}", avg_time);
        println!("Throughput: {:.2} ops/sec", iterations as f64 / elapsed.as_secs_f64());

        // Should still be reasonably fast
        assert!(avg_time.as_micros() < 1000, "Performance regression: {:?} > 1ms", avg_time);
    }

    #[test]
    fn test_performance_9kb_file() {
        let handler = ConfigSectionHandler::new(None);

        // Generate a realistic 9KB DixScript file
        let mut input = String::with_capacity(9 * 1024);
        input.push_str(r#"
@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    author -> "Performance Test Suite",
    created -> "2024-01-23T12:00:00Z",
    features -> "advanced",
    debug_mode -> "off",
    error_handling -> "halt",
    compatibility_mode -> "strict"
)

@ENUMS(
    Status { ACTIVE = 0, INACTIVE = 1, PENDING = 2 },
    Priority { LOW = 1, MEDIUM = 2, HIGH = 3, CRITICAL = 4 }
)

@DATA(
"#);

        // Add lots of data entries to reach ~9KB
        for i in 0..500 {
            input.push_str(&format!("    item_{} = {},\n", i, i));
        }

        input.push_str(")\n");

        // Pad to ensure we're at ~9KB
        while input.len() < 9 * 1024 {
            input.push_str("// Padding comment to reach target size\n");
        }

        let file_size = input.len();
        let iterations = 100; // Fewer iterations for larger file
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = handler.process_config_section(&input);
        }

        let elapsed = start.elapsed();
        let avg_time = elapsed / iterations;

        println!("\n=== PERFORMANCE: 9KB File (Target) ===");
        println!("File size: {} bytes ({:.2} KB)", file_size, file_size as f64 / 1024.0);
        println!("Iterations: {}", iterations);
        println!("Total time: {:?}", elapsed);
        println!("Average time: {:?}", avg_time);
        println!("Throughput: {:.2} files/sec", iterations as f64 / elapsed.as_secs_f64());

        // C# target: < 0.5ms for 9KB file
        assert!(avg_time.as_micros() < 500,
                "Performance target missed: {:?} > 500µs for 9KB file", avg_time);

        println!("✓ Performance target met: {:?} < 500µs", avg_time);
    }

    #[test]
    fn test_throughput_comparison() {
        let handler = ConfigSectionHandler::new(None);

        let test_cases = vec![
            ("Tiny (100B)", generate_test_input(100)),
            ("Small (1KB)", generate_test_input(1024)),
            ("Medium (5KB)", generate_test_input(5 * 1024)),
            ("Large (9KB)", generate_test_input(9 * 1024)),
            ("XLarge (20KB)", generate_test_input(20 * 1024)),
        ];

        println!("\n=== THROUGHPUT COMPARISON ===");
        println!("{:<15} {:>12} {:>15} {:>15}", "Size", "Time (µs)", "Throughput", "MB/sec");
        println!("{:-<60}", "");

        for (name, input) in test_cases {
            let iterations = if input.len() < 1024 { 1000 } else { 100 };
            let start = Instant::now();

            for _ in 0..iterations {
                let _ = handler.process_config_section(&input);
            }

            let elapsed = start.elapsed();
            let avg_time = elapsed / iterations;
            let throughput = iterations as f64 / elapsed.as_secs_f64();
            let mb_per_sec = (input.len() as f64 * throughput) / (1024.0 * 1024.0);

            println!("{:<15} {:>12.2} {:>12.2} ops/s {:>12.2} MB/s",
                     name,
                     avg_time.as_micros(),
                     throughput,
                     mb_per_sec
            );
        }
    }

    // Helper function to generate test input of specific size
    fn generate_test_input(target_size: usize) -> String {
        let mut input = String::with_capacity(target_size);

        input.push_str(r#"@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    features -> "advanced"
)

@DATA(
"#);

        // Add entries to reach target size
        let mut counter = 0;
        while input.len() < target_size - 100 {
            input.push_str(&format!("    field_{} = {},\n", counter, counter));
            counter += 1;
        }

        input.push_str(")\n");
        input
    }

    // ==================== EDGE CASE TESTS ====================

    #[test]
    fn test_malformed_config_missing_closing_paren() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8"
// Missing closing paren

@DATA(x = 1)
"#;

        let result = handler.process_config_section(input);

        // Should handle gracefully (either extract partial or use defaults)
        assert!(!result.config_section.entries.is_empty());

        println!("✓ Malformed config handled gracefully");
    }

    #[test]
    fn test_config_with_escaped_quotes() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG(
    version -> "1.0.0",
    description -> "This has \"escaped\" quotes"
)

@DATA(x = 1)
"#;

        let result = handler.process_config_section(input);

        assert!(!result.cleaned_input_string.contains("@CONFIG"));

        println!("✓ Escaped quotes handled correctly");
    }

    #[test]
    fn test_multiple_configs_first_wins() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG(
    version -> "1.0.0"
)

@CONFIG(
    version -> "2.0.0"
)

@DATA(x = 1)
"#;

        let result = handler.process_config_section(input);

        // First config should be extracted
        assert_eq!(result.operational_settings.version, "1.0.0");

        println!("✓ Multiple configs: first one wins");
    }

    #[test]
    fn test_config_case_insensitivity() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@config(
    VERSION -> "1.0.0",
    ENCODING -> "UTF-8"
)

@DATA(x = 1)
"#;

        let result = handler.process_config_section(input);

        // Should handle case-insensitive keywords
        assert!(!result.cleaned_input_string.to_lowercase().contains("@config"));

        println!("✓ Case-insensitive config handled");
    }

    #[test]
    fn test_whitespace_variations() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG   (   version   ->   "1.0.0"   ,   encoding   ->   "utf-8"   )

@DATA(x = 1)
"#;

        let result = handler.process_config_section(input);

        assert!(!result.cleaned_input_string.contains("@CONFIG"));
        assert_eq!(result.operational_settings.version, "1.0.0");

        println!("✓ Whitespace variations handled");
    }

    // ==================== INTEGRATION TESTS ====================

    #[test]
    fn test_full_pipeline_with_all_features() {
        let handler = ConfigSectionHandler::new(None);
        let input = r#"
@CONFIG(
    version -> "1.0.0",
    encoding -> "utf-8",
    author -> "Integration Test",
    created -> "2024-01-23T12:00:00Z",
    features -> "quickfuncs,enums,data,security,imports",
    debug_mode -> "verbose",
    error_handling -> "continue",
    compatibility_mode -> "best_effort"
)

@IMPORTS(
    utils from "shared/utils.mdix"
)

@ENUMS(
    Status { ACTIVE, INACTIVE }
)

@QUICKFUNCS(
    ~test<int>() { return 42; }
)

@DATA(
    name = "test",
    count = 42
)

@SECURITY(
    encryption -> { enabled = true }
)
"#;

        let result = handler.process_config_section(input);

        // Verify extraction
        assert!(!result.cleaned_input_string.contains("@CONFIG"));
        assert!(result.cleaned_input_string.contains("@IMPORTS"));
        assert!(result.cleaned_input_string.contains("@ENUMS"));
        assert!(result.cleaned_input_string.contains("@QUICKFUNCS"));
        assert!(result.cleaned_input_string.contains("@DATA"));
        assert!(result.cleaned_input_string.contains("@SECURITY"));

        // Verify operational settings
        assert_eq!(result.operational_settings.version, "1.0.0");
        assert_eq!(result.operational_settings.error_handling_strategy, ErrorHandlingStrategy::Continue);
        assert_eq!(result.operational_settings.debug_mode, DebugMode::Verbose);
        assert_eq!(result.operational_settings.compatibility_mode, CompatibilityMode::BestEffort);

        // Verify features
        let features = &result.operational_settings.enabled_features;
        assert!(features.contains(&"quickfuncs".to_string()));
        assert!(features.contains(&"enums".to_string()));
        assert!(features.contains(&"data".to_string()));

        println!("\n=== FULL PIPELINE TEST ===");
        println!("✓ Config extracted and removed");
        println!("✓ All sections preserved: @IMPORTS, @ENUMS, @QUICKFUNCS, @DATA, @SECURITY");
        println!("✓ Operational settings created correctly");
        println!("  - Version: {}", result.operational_settings.version);
        println!("  - Error handling: {:?}", result.operational_settings.error_handling_strategy);
        println!("  - Debug mode: {:?}", result.operational_settings.debug_mode);
        println!("  - Compatibility: {:?}", result.operational_settings.compatibility_mode);
        println!("  - Features: {:?}", features);
        println!("✓ Warnings: {}", result.warnings.len());
    }

    #[test]
    fn test_minimal_config_cached_performance() {
        // Test that cached minimal config is fast
        let iterations = 10000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = ConfigSchema::create_minimal_config();
        }

        let elapsed = start.elapsed();
        let avg_time = elapsed / iterations;

        println!("\n=== CACHED MINIMAL CONFIG PERFORMANCE ===");
        println!("Iterations: {}", iterations);
        println!("Average time: {:?}", avg_time);

        // Should be extremely fast (just cloning)
        assert!(avg_time.as_nanos() < 1000, "Cached config too slow: {:?}", avg_time);

        println!("✓ Cached minimal config is blazing fast: {:?}", avg_time);
    }
}

// ==================== BENCHMARK MODULE ====================

#[cfg(test)]
mod config_benchmarks {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore] // Run with: cargo test --release -- --ignored --nocapture
    fn bench_config_extraction_only() {
        use crate::Compiler::Core::Config::ConfigSectionHandler;

        let handler = ConfigSectionHandler::new(None);
        let input = include_str!("../../../tests/fixtures/sample_9kb.mdix");

        let iterations = 10000;
        let start = Instant::now();

        for _ in 0..iterations {
            let _ = handler.process_config_section(input);
        }

        let elapsed = start.elapsed();

        println!("\n=== DETAILED BENCHMARK RESULTS ===");
        println!("Total iterations: {}", iterations);
        println!("Total time: {:?}", elapsed);
        println!("Average time: {:?}", elapsed / iterations);
        println!("Ops/sec: {:.2}", iterations as f64 / elapsed.as_secs_f64());
    }
}