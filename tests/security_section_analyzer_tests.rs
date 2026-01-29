// tests/security_section_analyzer_tests.rs

#[cfg(test)]
mod security_section_analyzer_tests {
    use dixscript::Compiler::AST::*;
    use dixscript::Compiler::Core::SectionAnalyzers::SecuritySectionAnalyzer;
    use dixscript::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
    use dixscript::Compiler::Utilities::SymbolTable;
    use std::time::Instant;

    // ==================== BASELINE TESTS ====================

    #[test]
    fn test_valid_security_section() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let mut analyzer = SecuritySectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_valid_security_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(result.is_success, "Valid security section should succeed");
        assert_eq!(result.errors.len(), 0, "Should have no errors");
    }

    #[test]
    fn test_empty_security_section() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let mut analyzer = SecuritySectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_empty_security_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(result.is_success, "Empty security section should succeed");
        assert!(
            result.warnings.iter().any(|w| w.warning_id == "SEC_WARN001"),
            "Should warn about empty section"
        );
    }

    #[test]
    fn test_missing_encryption_mode() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let mut analyzer = SecuritySectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_missing_mode_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(result.is_success, "Should succeed with defaults");
        assert!(
            result.warnings.iter().any(|w| w.warning_id == "SEC_WARN007"),
            "Should warn about missing mode"
        );
    }

    #[test]
    fn test_manual_mode_warning() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let mut analyzer = SecuritySectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_manual_mode_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(result.is_success, "Manual mode should succeed");
        assert!(
            result.warnings.iter().any(|w| w.warning_id == "SEC_WARN003"),
            "Should warn about manual mode"
        );
    }

    #[test]
    fn test_xor_encryption_warning() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let mut analyzer = SecuritySectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_xor_encryption_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(result.is_success, "XOR encryption should succeed");
        assert!(
            result.warnings.iter().any(|w| w.warning_id == "SEC_WARN004"),
            "Should warn about XOR security"
        );
    }

    #[test]
    fn test_kdf_parameter_validation() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let mut analyzer = SecuritySectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_low_kdf_parameters_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(result.is_success, "Should succeed with warnings");
        assert!(
            result.warnings.iter().any(|w| w.warning_id == "SEC_WARN005"),
            "Should warn about low KDF parameters"
        );
    }

    // ==================== PERFORMANCE TESTS ====================

    #[test]
    fn benchmark_security_analysis_small() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let section = create_valid_security_section();
        
        let start = Instant::now();
        for _ in 0..10000 {
            let mut analyzer = SecuritySectionAnalyzer::new(&settings);
            let mut symbol_table = SymbolTable::new();
            let _result = analyzer.analyze(&section, &mut symbol_table);
        }
        let duration = start.elapsed();
        
        println!("Security analysis (3 blocks): {:?} for 10000 iterations", duration);
        println!("Average: {:?} per iteration", duration / 10000);
        
        // Baseline: should be under 10ms for 10000 iterations
        assert!(duration.as_millis() < 10, "Performance regression: took {:?}", duration);
    }

    #[test]
    fn benchmark_security_analysis_complex() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let section = create_complex_security_section();
        
        let start = Instant::now();
        for _ in 0..5000 {
            let mut analyzer = SecuritySectionAnalyzer::new(&settings);
            let mut symbol_table = SymbolTable::new();
            let _result = analyzer.analyze(&section, &mut symbol_table);
        }
        let duration = start.elapsed();
        
        println!("Security analysis (5 blocks): {:?} for 5000 iterations", duration);
        println!("Average: {:?} per iteration", duration / 5000);
        
        // Baseline: should be under 15ms for 5000 iterations
        assert!(duration.as_millis() < 15, "Performance regression: took {:?}", duration);
    }

    // ==================== MEMORY USAGE TESTS ====================

    #[test]
    fn test_memory_usage_security() {
        let section = create_valid_security_section();
        let section_size = std::mem::size_of_val(&section);
        
        println!("Security section memory: {} bytes", section_size);
        
        // Baseline: should be reasonable
        assert!(section_size < 5_000, "Memory usage too high: {} bytes", section_size);
    }

    // ==================== HELPER FUNCTIONS ====================

    fn create_valid_security_section() -> SecuritySection {
        let encryption_fields = vec![
            SecurityField::new(
                "mode".to_string(),
                Value::String { value: "keyfile".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "algorithm".to_string(),
                Value::String { value: "aes256-gcm".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let validation_fields = vec![
            SecurityField::new(
                "checksum_algorithm".to_string(),
                Value::String { value: "sha256".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let keystore_fields = vec![
            SecurityField::new(
                "auto_generate".to_string(),
                Value::Boolean { value: true, position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let entries = vec![
            SecurityEntry::new("encryption".to_string(), encryption_fields, Position::UNKNOWN),
            SecurityEntry::new("validation".to_string(), validation_fields, Position::UNKNOWN),
            SecurityEntry::new("keystore".to_string(), keystore_fields, Position::UNKNOWN),
        ];

        SecuritySection::new(entries, Position::UNKNOWN)
    }

    fn create_empty_security_section() -> SecuritySection {
        SecuritySection::new(vec![], Position::UNKNOWN)
    }

    fn create_missing_mode_section() -> SecuritySection {
        let encryption_fields = vec![
            SecurityField::new(
                "algorithm".to_string(),
                Value::String { value: "aes256-gcm".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let entries = vec![
            SecurityEntry::new("encryption".to_string(), encryption_fields, Position::UNKNOWN),
        ];

        SecuritySection::new(entries, Position::UNKNOWN)
    }

    fn create_manual_mode_section() -> SecuritySection {
        let encryption_fields = vec![
            SecurityField::new(
                "mode".to_string(),
                Value::String { value: "manual".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "key".to_string(),
                Value::String { value: "0x1234567890ABCDEF".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "iv".to_string(),
                Value::String { value: "0xFEDCBA0987654321".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let entries = vec![
            SecurityEntry::new("encryption".to_string(), encryption_fields, Position::UNKNOWN),
        ];

        SecuritySection::new(entries, Position::UNKNOWN)
    }

    fn create_xor_encryption_section() -> SecuritySection {
        let encryption_fields = vec![
            SecurityField::new(
                "mode".to_string(),
                Value::String { value: "keyfile".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "algorithm".to_string(),
                Value::String { value: "xor".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let entries = vec![
            SecurityEntry::new("encryption".to_string(), encryption_fields, Position::UNKNOWN),
        ];

        SecuritySection::new(entries, Position::UNKNOWN)
    }

    fn create_low_kdf_parameters_section() -> SecuritySection {
        let encryption_fields = vec![
            SecurityField::new(
                "mode".to_string(),
                Value::String { value: "password".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "kdf".to_string(),
                Value::String { value: "argon2id".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "kdf_memory".to_string(),
                Value::Integer { value: 1000, position: Position::UNKNOWN }, // Below minimum
                Position::UNKNOWN,
            ),
        ];

        let entries = vec![
            SecurityEntry::new("encryption".to_string(), encryption_fields, Position::UNKNOWN),
        ];

        SecuritySection::new(entries, Position::UNKNOWN)
    }

    fn create_complex_security_section() -> SecuritySection {
        let encryption_fields = vec![
            SecurityField::new(
                "mode".to_string(),
                Value::String { value: "password".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "algorithm".to_string(),
                Value::String { value: "aes256-gcm".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "kdf".to_string(),
                Value::String { value: "argon2id".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "kdf_memory".to_string(),
                Value::Integer { value: 65536, position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "kdf_iterations".to_string(),
                Value::Integer { value: 3, position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let validation_fields = vec![
            SecurityField::new(
                "checksum_algorithm".to_string(),
                Value::String { value: "sha256".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "auth_tag_length".to_string(),
                Value::Integer { value: 128, position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let keystore_fields = vec![
            SecurityField::new(
                "auto_generate".to_string(),
                Value::Boolean { value: true, position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
            SecurityField::new(
                "backup_count".to_string(),
                Value::Integer { value: 3, position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let override_fields = vec![
            SecurityField::new(
                "manual_key_warning_accepted".to_string(),
                Value::Boolean { value: false, position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let metadata_fields = vec![
            SecurityField::new(
                "version".to_string(),
                Value::String { value: "1.0.0".to_string(), position: Position::UNKNOWN },
                Position::UNKNOWN,
            ),
        ];

        let entries = vec![
            SecurityEntry::new("encryption".to_string(), encryption_fields, Position::UNKNOWN),
            SecurityEntry::new("validation".to_string(), validation_fields, Position::UNKNOWN),
            SecurityEntry::new("keystore".to_string(), keystore_fields, Position::UNKNOWN),
            SecurityEntry::new("override".to_string(), override_fields, Position::UNKNOWN),
            SecurityEntry::new("metadata".to_string(), metadata_fields, Position::UNKNOWN),
        ];

        SecuritySection::new(entries, Position::UNKNOWN)
    }
      }
