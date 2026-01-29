// tests/dlm_section_analyzer_tests.rs

#[cfg(test)]
mod dlm_section_analyzer_tests {
    use dixscript::Compiler::AST::*;
    use dixscript::Compiler::Core::SectionAnalyzers::DlmSectionAnalyzer;
    use dixscript::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
    use dixscript::Compiler::Utilities::SymbolTable;
    use std::time::Instant;

    // ==================== BASELINE TESTS ====================

    #[test]
    fn test_valid_dlm_section() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let mut analyzer = DlmSectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_valid_dlm_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(result.is_success, "Valid DLM section should succeed");
        assert_eq!(result.errors.len(), 0, "Should have no errors");
    }

    #[test]
    fn test_duplicate_modules() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        settings.error_handling_strategy = ErrorHandlingStrategy::Continue;
        
        let mut analyzer = DlmSectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_duplicate_module_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(!result.is_success, "Duplicate modules should fail");
        assert!(
            result.errors.iter().any(|e| e.error_type == "DUPLICATE_MODULE"),
            "Should report duplicate module"
        );
    }

    #[test]
    fn test_invalid_module_subtype() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        settings.error_handling_strategy = ErrorHandlingStrategy::Continue;
        
        let mut analyzer = DlmSectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_invalid_subtype_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(!result.is_success, "Invalid subtype should fail");
        assert!(
            result.errors.iter().any(|e| e.error_type == "INVALID_MODULE_SUBTYPE"),
            "Should report invalid subtype"
        );
    }

    #[test]
    fn test_xor_security_warning() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let mut analyzer = DlmSectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_xor_encryption_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(result.is_success, "XOR encryption should succeed");
        assert!(
            result.warnings.iter().any(|w| w.warning_id == "DLM_WARN003"),
            "Should warn about XOR low security"
        );
    }

    #[test]
    fn test_suboptimal_ordering() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let mut analyzer = DlmSectionAnalyzer::new(&settings);
        let mut symbol_table = SymbolTable::new();

        let section = create_suboptimal_ordering_section();
        let result = analyzer.analyze(&section, &mut symbol_table);

        assert!(result.is_success, "Should succeed with warning");
        assert!(
            result.warnings.iter().any(|w| w.warning_id == "DLM_WARN002"),
            "Should warn about suboptimal ordering"
        );
    }

    // ==================== PERFORMANCE TESTS ====================

    #[test]
    fn benchmark_dlm_analysis_small() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let section = create_valid_dlm_section();
        
        let start = Instant::now();
        for _ in 0..10000 {
            let mut analyzer = DlmSectionAnalyzer::new(&settings);
            let mut symbol_table = SymbolTable::new();
            let _result = analyzer.analyze(&section, &mut symbol_table);
        }
        let duration = start.elapsed();
        
        println!("DLM analysis (3 modules): {:?} for 10000 iterations", duration);
        println!("Average: {:?} per iteration", duration / 10000);
        
        // Baseline: should be under 5ms for 10000 iterations
        assert!(duration.as_millis() < 5, "Performance regression: took {:?}", duration);
    }

    #[test]
    fn benchmark_dlm_analysis_medium() {
        let mut settings = OperationalSettings::default();
        settings.debug_mode = DebugMode::Off;
        
        let section = create_medium_dlm_section();
        
        let start = Instant::now();
        for _ in 0..10000 {
            let mut analyzer = DlmSectionAnalyzer::new(&settings);
            let mut symbol_table = SymbolTable::new();
            let _result = analyzer.analyze(&section, &mut symbol_table);
        }
        let duration = start.elapsed();
        
        println!("DLM analysis (10 modules): {:?} for 10000 iterations", duration);
        println!("Average: {:?} per iteration", duration / 10000);
        
        // Baseline: should be under 10ms for 10000 iterations
        assert!(duration.as_millis() < 10, "Performance regression: took {:?}", duration);
    }

    // ==================== MEMORY USAGE TESTS ====================

    #[test]
    fn test_memory_usage_dlm() {
        let section = create_valid_dlm_section();
        let section_size = std::mem::size_of_val(&section);
        
        println!("DLM section memory: {} bytes", section_size);
        
        // Baseline: should be small
        assert!(section_size < 1_000, "Memory usage too high: {} bytes", section_size);
    }

    // ==================== HELPER FUNCTIONS ====================

    fn create_valid_dlm_section() -> DLMSection {
        let modules = vec![
            DLMModule::new(
                DLMModuleType::DCompressor,
                Some(DLMModuleSubtype::Gzip),
                Position::UNKNOWN,
            ),
            DLMModule::new(
                DLMModuleType::DAuditor,
                Some(DLMModuleSubtype::Enhanced),
                Position::UNKNOWN,
            ),
            DLMModule::new(
                DLMModuleType::DEncryptor,
                Some(DLMModuleSubtype::Aes256),
                Position::UNKNOWN,
            ),
        ];
        
        DLMSection::new(modules, Position::UNKNOWN)
    }

    fn create_duplicate_module_section() -> DLMSection {
        let modules = vec![
            DLMModule::new(
                DLMModuleType::DCompressor,
                Some(DLMModuleSubtype::Gzip),
                Position::UNKNOWN,
            ),
            DLMModule::new(
                DLMModuleType::DCompressor,
                Some(DLMModuleSubtype::Gzip),
                Position::UNKNOWN,
            ),
        ];
        
        DLMSection::new(modules, Position::UNKNOWN)
    }

    fn create_invalid_subtype_section() -> DLMSection {
        let modules = vec![
            DLMModule::new(
                DLMModuleType::DCompressor,
                Some(DLMModuleSubtype::Aes256), // Wrong subtype for compressor
                Position::UNKNOWN,
            ),
        ];
        
        DLMSection::new(modules, Position::UNKNOWN)
    }

    fn create_xor_encryption_section() -> DLMSection {
        let modules = vec![
            DLMModule::new(
                DLMModuleType::DEncryptor,
                Some(DLMModuleSubtype::Xor),
                Position::UNKNOWN,
            ),
        ];
        
        DLMSection::new(modules, Position::UNKNOWN)
    }

    fn create_suboptimal_ordering_section() -> DLMSection {
        let modules = vec![
            DLMModule::new(
                DLMModuleType::DEncryptor,
                Some(DLMModuleSubtype::Aes256),
                Position::UNKNOWN,
            ),
            DLMModule::new(
                DLMModuleType::DCompressor,
                Some(DLMModuleSubtype::Gzip),
                Position::UNKNOWN,
            ),
        ];
        
        DLMSection::new(modules, Position::UNKNOWN)
    }

    fn create_medium_dlm_section() -> DLMSection {
        let modules = vec![
            DLMModule::new(DLMModuleType::DCompressor, Some(DLMModuleSubtype::Gzip), Position::UNKNOWN),
            DLMModule::new(DLMModuleType::DCompressor, Some(DLMModuleSubtype::Bzip2), Position::UNKNOWN),
            DLMModule::new(DLMModuleType::DCompressor, Some(DLMModuleSubtype::Lzma), Position::UNKNOWN),
            DLMModule::new(DLMModuleType::DAuditor, Some(DLMModuleSubtype::Diy), Position::UNKNOWN),
            DLMModule::new(DLMModuleType::DAuditor, Some(DLMModuleSubtype::Enhanced), Position::UNKNOWN),
            DLMModule::new(DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Xor), Position::UNKNOWN),
            DLMModule::new(DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Aes128), Position::UNKNOWN),
            DLMModule::new(DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Aes256), Position::UNKNOWN),
            DLMModule::new(DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Chacha20), Position::UNKNOWN),
        ];
        
        DLMSection::new(modules, Position::UNKNOWN)
    }
      }
