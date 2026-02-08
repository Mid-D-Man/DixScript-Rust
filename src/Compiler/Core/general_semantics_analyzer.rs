// src/Compiler/Core/general_semantics_analyzer.rs

use crate::Compiler::AST::*;
use crate::Compiler::Core::{
    OperationalSettings,
    ErrorHandlingStrategy,
    DebugMode,
    SemanticAnalysisResult,
    SemanticErrorInfo,
    SemanticWarningInfo,
};
use crate::Compiler::Core::SectionAnalyzers::*;
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::VersionControl::{VersionConstraints, ValidationResult};
use crate::Compiler::ImportsResolution::ImportsResolver;
use crate::ErrorManager::{ErrorManager, SemanticErrorType};
use crate::Builtins::Static::enum_object;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Central semantic analysis orchestrator for DixScript v1.0.0
///
/// ANALYSIS PHASES (RENUMBERED):
/// - Phase 1: Version Validation
/// - Phase 2: IMPORTS Semantic Analysis (validates import declarations)
/// - Phase 3: Imports Resolution (CRITICAL - parses and resolves imports)
/// - Phase 4: Foundation (ENUMS)
/// - Phase 5: Functions (QUICKFUNCS)
/// - Phase 6: Independent (DLM)
/// - Phase 7: Data-Driven (DATA)
/// - Phase 8: Generatable (SECURITY)
///
/// NOTE: AST Enhancement is a separate phase run by the main compiler flow
pub struct GeneralSemanticAnalyzer<'a> {
    // References to input data (borrowed, not owned)
    ast: &'a DixScript,
    operational_settings: &'a OperationalSettings,

    // Owned state
    symbol_table: SymbolTable,
    error_manager: ErrorManager,

    // Section analyzers (created on demand)
    enums_analyzer: Option<EnumsSectionAnalyzer<'a>>,
    dlm_analyzer: Option<DlmSectionAnalyzer<'a>>,
    security_analyzer: Option<SecuritySectionAnalyzer<'a>>,

    // Result accumulator
    analysis_result: SemanticAnalysisResult,

    // Performance tracking
    stopwatch: Instant,

    // OPTIMIZATION: Cache log level checks (avoids repeated enum comparisons)
    can_log_debug: bool,
    can_log_verbose: bool,

    // Control flags
    skip_validation: bool,
}

impl<'a> GeneralSemanticAnalyzer<'a> {
    /// Create new semantic analyzer
    ///
    /// # Arguments
    /// * `ast` - AST to analyze (borrowed)
    /// * `operational_settings` - Compiler settings (borrowed)
    pub fn new(
        ast: &'a DixScript,
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let symbol_table = SymbolTable::new();

        // OPTIMIZATION: Cache log level for O(1) checks
        let can_log_debug = operational_settings.debug_mode != DebugMode::Off;
        let can_log_verbose = operational_settings.debug_mode == DebugMode::Verbose;

        GeneralSemanticAnalyzer {
            ast,
            operational_settings,
            symbol_table,
            error_manager,
            enums_analyzer: None,
            dlm_analyzer: None,
            security_analyzer: None,
            analysis_result: SemanticAnalysisResult::new(),
            stopwatch: Instant::now(),
            can_log_debug,
            can_log_verbose,
            skip_validation: false,
        }
    }

    /// Main analysis entry point
    ///
    /// Returns SemanticAnalysisResult with errors, warnings, and populated symbol table.
    /// NOTE: AST enhancement is NOT performed here - it's a separate phase.
    pub fn analyze(mut self) -> SemanticAnalysisResult {
        self.log_info("Starting General Semantic Analysis v1.0.0");
        self.log_info_fmt(|| format!(
            "Error Handling Strategy: {:?}",
            self.operational_settings.error_handling_strategy
        ));
        self.log_info_fmt(|| format!(
            "Compatibility Mode: {:?}",
            self.operational_settings.compatibility_mode
        ));
        self.log_info_fmt(|| format!(
            "Advanced Mode: {}",
            self.operational_settings.is_advanced_mode()
        ));

        // Initialize builtin registries
        self.initialize_builtin_registries();

        if !self.skip_validation {
            // PHASE 1: VERSION VALIDATION
            if !self.analyze_phase1_version() {
                return self.finalize_result();
            }
        }

        // PHASE 2: IMPORTS SEMANTIC ANALYSIS (validate declarations)
        if !self.analyze_phase2_imports_semantic() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        // PHASE 3: IMPORTS RESOLUTION (parse and resolve)
        if !self.analyze_phase3_imports_resolution() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        // PHASE 4: FOUNDATION (ENUMS)
        if !self.analyze_phase4_foundation() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        // Register enums with builtin system
        self.register_enums_with_builtin_system();

        // PHASE 5: FUNCTIONS (QUICKFUNCS)
        if !self.analyze_phase5_functions() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        // PHASE 6: INDEPENDENT (DLM)
        self.analyze_phase6_independent();

        // PHASE 7: DATA-DRIVEN (DATA)
        if !self.analyze_phase7_data_driven() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        // PHASE 8: GENERATABLE (SECURITY)
        self.analyze_phase8_generated();

        self.log_info_fmt(|| format!(
            "Semantic analysis complete. Success: {}",
            self.analysis_result.is_success
        ));
        self.log_info_fmt(|| format!(
            "Total errors: {}, Warnings: {}",
            self.analysis_result.errors.len(),
            self.analysis_result.warnings.len()
        ));

        // Finalize and return result
        self.finalize_result()
    }

    // ==================== PHASE 1: VERSION VALIDATION ====================

    /// Phase 1: Comprehensive version compatibility check
    ///
    /// Returns false if validation fails and should halt
    fn analyze_phase1_version(&mut self) -> bool {
        self.log_info("Phase 1: Performing comprehensive version compatibility check");

        let version_constraints = VersionConstraints::new();
        let version_validation = version_constraints.validate_script(self.ast);

        if !version_validation.is_valid {
            self.log_error_fmt(|| format!(
                "Version validation failed with {} errors",
                version_validation.errors.len()
            ));

            for error in &version_validation.errors {
                self.analysis_result.errors.push(SemanticErrorInfo {
                    error_id: "SEM_VERSION".to_string(),
                    error_type: "VersionCompatibility".to_string(),
                    message: error.clone(),
                    section_name: "VERSION_CHECK".to_string(),
                    suggestion: "Upgrade compiler version or adjust CONFIG section".to_string(),
                    position: None,
                });
            }

            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
                self.log_error("Halting analysis due to version incompatibility");
                self.analysis_result.is_success = false;
                return false;
            }
        } else {
            self.log_info_fmt(|| format!(
                "Version validation passed (Script v{})",
                version_validation.detected_version.as_ref().unwrap_or(&"1.0.0".to_string())
            ));
        }

        true
    }

    // ==================== PHASE 2: IMPORTS SEMANTIC ANALYSIS ====================

    /// Phase 2: IMPORTS Semantic Analysis
    /// Validates import declarations BEFORE attempting to parse/resolve them
    ///
    /// Returns false if validation fails and should halt
    fn analyze_phase2_imports_semantic(&mut self) -> bool {
        let imports = match &self.ast.imports {
            Some(section) if !section.imports.is_empty() => section,
            _ => {
                self.log_info("No imports section - skipping imports semantic analysis");
                return true;
            }
        };

        self.log_info("Phase 2: Analyzing IMPORTS section semantically");

        // Check if imports feature is enabled
        if !self.operational_settings.is_feature_enabled("imports")
            && !self.operational_settings.is_advanced_mode() {
            self.log_error("IMPORTS section found but imports feature not enabled");

            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id: "SEM_FEATURE".to_string(),
                error_type: "FeatureNotEnabled".to_string(),
                message: "IMPORTS section requires 'imports' feature or advanced mode to be enabled".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion: "Add 'imports' to features list or enable advanced mode in CONFIG".to_string(),
                position: None,
            });

            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
        }

        // TODO: Create ImportsSectionAnalyzer when ported
        self.log_debug("ImportsSectionAnalyzer not yet ported - skipping semantic validation");
        self.log_info("Phase 2 complete");

        true
    }

    // ==================== PHASE 3: IMPORTS RESOLUTION ====================

    /// Phase 3: Resolve imports and populate symbol table
    ///
    /// Returns false if resolution fails and should halt
    fn analyze_phase3_imports_resolution(&mut self) -> bool {
        // Skip if this is an imported file (parent is resolving)
        if self.operational_settings.skip_imports_resolution {
            self.log_debug("Skipping imports resolution (imported file - parent is resolving)");
            return true;
        }

        let imports = match &self.ast.imports {
            Some(section) if !section.imports.is_empty() => section,
            _ => {
                self.log_info("No imports section - skipping import resolution");
                return true;
            }
        };

        self.log_info_fmt(|| format!("Phase 3: Resolving {} imports", imports.imports.len()));

        // Check if imports feature is enabled
        if !self.operational_settings.is_feature_enabled("imports")
            && !self.operational_settings.is_advanced_mode() {
            self.log_error("IMPORTS section found but imports feature not enabled");

            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id: "SEM_FEATURE".to_string(),
                error_type: "FeatureNotEnabled".to_string(),
                message: "IMPORTS section requires 'imports' feature or advanced mode to be enabled".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion: "Add 'imports' to features list or enable advanced mode in CONFIG".to_string(),
                position: None,
            });

            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
        }

        self.log_debug("Starting imports resolution phase");

        // NOTE: ImportsResolver handles ALL parsing internally - we just pass empty map
        // The resolver will parse files on-demand during resolution
        let parsed_imports = HashMap::new();

        // CRITICAL: Pass mutable reference to symbol table
        // ImportsResolver will populate it with imported namespaces
        let mut imports_resolver = ImportsResolver::new(
            &mut self.symbol_table,
            self.operational_settings.clone(),
        );

        let resolve_success = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async {
                imports_resolver.resolve_imports(&parsed_imports).await
            });

        if !resolve_success {
            self.log_error("Import resolution failed");

            // Get import errors from error manager
            let import_errors = self.error_manager.get_imports_resolution_errors();
            if !import_errors.is_empty() {
                self.log_warning_fmt(|| format!(
                    "Import resolution completed with {} errors",
                    import_errors.len()
                ));

                // Add to analysis result
                for error in import_errors {
                    self.analysis_result.errors.push(SemanticErrorInfo {
                        error_id: error.error_id.clone(),
                        error_type: format!("{:?}", error.error_type),
                        message: error.message.clone(),
                        section_name: "IMPORTS".to_string(),
                        suggestion: error.suggestion.clone().unwrap_or_default(),
                        position: Some(Position::new(error.line as usize, error.column as usize)),
                    });
                }
            }

            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
        } else {
            let stats = imports_resolver.get_statistics();
            self.log_info_fmt(|| format!("Imports resolved successfully: {}", stats));
        }

        self.log_info("Phase 3 complete");
        true
    }

    // ==================== PHASE 4: FOUNDATION (ENUMS) ====================

    /// Phase 4: Analyze foundational sections (ENUMS)
    ///
    /// Returns false if analysis fails and should halt
    fn analyze_phase4_foundation(&mut self) -> bool {
        self.log_info("Phase 4: Analyzing foundational sections");
        self.log_info("CONFIG already processed by ConfigSectionHandler - skipping validation");

        // ENUMS Section
        if let Some(ref enums) = self.ast.enums {
            // Check if enums feature is enabled
            if !self.operational_settings.is_feature_enabled("enums")
                && !self.operational_settings.is_advanced_mode() {
                self.log_error("ENUMS section found but enums feature not enabled");

                self.analysis_result.errors.push(SemanticErrorInfo {
                    error_id: "SEM_FEATURE".to_string(),
                    error_type: "FeatureNotEnabled".to_string(),
                    message: "ENUMS section requires 'enums' feature or advanced mode to be enabled".to_string(),
                    section_name: "ENUMS".to_string(),
                    suggestion: "Add 'enums' to features list or enable advanced mode in CONFIG".to_string(),
                    position: None,
                });

                if self.should_terminate() {
                    return false;
                }
            } else {
                self.log_debug("Analyzing ENUMS section");

                if self.enums_analyzer.is_none() {
                    self.enums_analyzer = Some(EnumsSectionAnalyzer::new(self.operational_settings));
                }

                let result = self.enums_analyzer.as_mut().unwrap()
                    .analyze(enums, &mut self.symbol_table);

                self.add_section_result("ENUMS", result);

                if !self.analysis_result.section_results.get("ENUMS").unwrap().is_success {
                    self.log_error_fmt(|| format!(
                        "ENUMS analysis failed with {} errors",
                        self.analysis_result.section_results.get("ENUMS").unwrap().errors.len()
                    ));

                    if self.should_terminate() {
                        return false;
                    }
                }
            }
        } else {
            self.log_info("ENUMS section not present - skipping analyzer");
        }

        self.log_info("Phase 4 complete");
        true
    }

    // ==================== PHASE 5: FUNCTIONS (QUICKFUNCS) ====================

    /// Phase 5: Analyze function definitions
    ///
    /// Returns false if analysis fails and should halt
    fn analyze_phase5_functions(&mut self) -> bool {
        self.log_info("Phase 5: Analyzing function definitions");

        if let Some(ref _quickfuncs) = self.ast.quick_functions {
            // Check if quickfuncs feature is enabled
            if !self.operational_settings.is_feature_enabled("quickfuncs")
                && !self.operational_settings.is_advanced_mode() {
                self.log_error("QUICKFUNCS section found but quickfuncs feature not enabled");

                self.analysis_result.errors.push(SemanticErrorInfo {
                    error_id: "SEM_FEATURE".to_string(),
                    error_type: "FeatureNotEnabled".to_string(),
                    message: "QUICKFUNCS section requires 'quickfuncs' feature or advanced mode to be enabled".to_string(),
                    section_name: "QUICKFUNCS".to_string(),
                    suggestion: "Add 'quickfuncs' to features list or enable advanced mode in CONFIG".to_string(),
                    position: None,
                });

                if self.should_terminate() {
                    return false;
                }
            } else {
                self.log_debug("Analyzing QUICKFUNCS section");

                // TODO: Create QuickFuncsSectionAnalyzer when ported
                self.log_warning("QuickFuncsSectionAnalyzer not yet ported - skipping analysis");
            }
        } else {
            self.log_info("QUICKFUNCS section not present - skipping analyzer");
        }

        self.log_info("Phase 5 complete");
        true
    }

    // ==================== PHASE 6: INDEPENDENT (DLM) ====================

    /// Phase 6: Analyze independent sections
    fn analyze_phase6_independent(&mut self) {
        self.log_info("Phase 6: Analyzing independent sections");

        if let Some(ref dlm) = self.ast.dlm {
            self.log_debug("Analyzing DLM section");

            if self.dlm_analyzer.is_none() {
                self.dlm_analyzer = Some(DlmSectionAnalyzer::new(self.operational_settings));
            }

            let result = self.dlm_analyzer.as_mut().unwrap()
                .analyze(dlm, &mut self.symbol_table);

            self.add_section_result("DLM", result);
        } else {
            self.log_debug("DLM section not present - skipping analyzer");
        }

        self.log_info("Phase 6 complete");
    }

    // ==================== PHASE 7: DATA-DRIVEN (DATA) ====================

    /// Phase 7: Analyze data section
    ///
    /// Returns false if analysis fails and should halt
    fn analyze_phase7_data_driven(&mut self) -> bool {
        self.log_info("Phase 7: Analyzing data section");

        if let Some(ref _data) = self.ast.data {
            self.log_debug("Analyzing DATA section");

            // TODO: Create DataSectionAnalyzer when ported
            self.log_warning("DataSectionAnalyzer not yet ported - skipping analysis");
        } else {
            self.log_warning("DATA section not present - unusual for a data interchange format");
        }

        self.log_info("Phase 7 complete");
        true
    }

    // ==================== PHASE 8: GENERATED (SECURITY) ====================

    /// Phase 8: Analyze compiler-generated sections
    fn analyze_phase8_generated(&mut self) {
        self.log_info("Phase 8: Analyzing compiler-generated sections");

        // Check if SECURITY is required
        let requires_security = self.ast.dlm.as_ref()
            .map(|dlm| dlm.modules.iter().any(|m| {
                matches!(m.module_type, DLMModuleType::DEncryptor)
            }))
            .unwrap_or(false);

        if let Some(ref security) = self.ast.security {
            self.log_debug("Analyzing SECURITY section");

            if self.security_analyzer.is_none() {
                self.security_analyzer = Some(SecuritySectionAnalyzer::new(self.operational_settings));
            }

            let result = self.security_analyzer.as_mut().unwrap()
                .analyze(security, &mut self.symbol_table);

            self.add_section_result("SECURITY", result);
        } else if requires_security {
            self.log_error("SECURITY section is required when using DEncryptor module but not present");

            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id: "SEM0002".to_string(),
                error_type: "MissingSection".to_string(),
                message: "SECURITY section is required when using DEncryptor module in @DLM".to_string(),
                section_name: "SECURITY".to_string(),
                suggestion: "Add @SECURITY section with encryption configuration".to_string(),
                position: None,
            });
        } else {
            self.log_debug("SECURITY section not present - skipping analyzer (not required)");
        }

        self.log_info("Phase 8 complete");
    }

    // ==================== ENUM REGISTRATION ====================

    /// CRITICAL: Bridge SymbolTable enums to EnumObject builtin registry
    ///
    /// Must be called after ENUMS analysis (Phase 4) but before value resolution (Phase 7)
    fn register_enums_with_builtin_system(&mut self) {
        self.log_info("Registering enums with builtin system");

        // Clear any previous registrations
        enum_object::clear_enums();

        let enum_count = self.symbol_table.enums.len();
        let mut registered_count = 0;

        self.log_info_fmt(|| format!(
            "Registering {} enums with builtin system",
            enum_count
        ));

        for (enum_name, field_mapping) in &self.symbol_table.enums {
            enum_object::register_enum(enum_name.clone(), field_mapping.clone());
            registered_count += 1;
            self.log_debug_fmt(|| format!(
                "  Registered enum: {} ({} fields)",
                enum_name,
                field_mapping.len()
            ));
        }

        self.log_info_fmt(|| format!(
            "Enum registration complete: {}/{} enums registered",
            registered_count,
            enum_count
        ));

        // Verify registration
        let registered_enums = enum_object::get_registered_enums();
        self.log_debug_fmt(|| format!(
            "  EnumObject registry now has: {}",
            registered_enums.join(", ")
        ));
    }

    // ==================== HELPER METHODS ====================

    /// Initialize builtin registries
    fn initialize_builtin_registries(&mut self) {
        self.symbol_table.add_builtin_static_object("Math".to_string());
        self.symbol_table.add_builtin_static_object("Dix".to_string());
        self.symbol_table.add_builtin_static_object("DateTime".to_string());
        self.symbol_table.add_builtin_static_object("Array".to_string());
        self.symbol_table.add_builtin_static_object("Random".to_string());
        self.symbol_table.add_builtin_static_object("Enum".to_string());
        self.symbol_table.add_builtin_static_object("Guid".to_string());
        self.symbol_table.add_builtin_static_object("Ip".to_string());

        self.symbol_table.add_dix_function(
            "logEvent".to_string(),
            "void".to_string(),
            vec!["string".to_string()],
        );
        self.symbol_table.add_dix_function(
            "getSystemInfo".to_string(),
            "object".to_string(),
            vec![],
        );
        self.symbol_table.add_dix_function(
            "validateConfig".to_string(),
            "bool".to_string(),
            vec!["string".to_string()],
        );

        self.log_debug("Built-in registries initialized");
    }

    /// Add section analysis result to overall result
    fn add_section_result(&mut self, section_name: &str, result: SectionAnalysisResult) {
        self.analysis_result.errors.extend(result.errors.clone());
        self.analysis_result.warnings.extend(result.warnings.clone());

        if !result.is_success {
            self.log_warning_fmt(|| format!("Section {} analysis failed", section_name));
        }

        self.analysis_result.section_results.insert(
            section_name.to_string(),
            result,
        );
    }

    /// Check if analysis should terminate early
    #[inline]
    fn should_terminate(&self) -> bool {
        !self.analysis_result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    /// Finalize analysis result
    fn finalize_result(mut self) -> SemanticAnalysisResult {
        self.analysis_result.is_success = self.analysis_result.errors.is_empty();
        self.analysis_result.symbol_table = Some(self.symbol_table);
        self.analysis_result.analysis_duration = self.stopwatch.elapsed();

        self.log_info_fmt(|| format!(
            "Analysis duration: {:.2}ms",
            self.analysis_result.analysis_duration.as_secs_f64() * 1000.0
        ));

        self.analysis_result
    }

    // ==================== LOGGING HELPERS (OPTIMIZED) ====================

    #[inline]
    fn log_debug(&self, message: &str) {
        if self.can_log_debug {
            self.error_manager.log_debug(message);
        }
    }

    #[inline]
    fn log_debug_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        if self.can_log_debug {
            self.error_manager.log_debug(&f());
        }
    }

    #[inline]
    fn log_verbose(&self, message: &str) {
        if self.can_log_verbose {
            self.error_manager.log_debug(message);
        }
    }

    #[inline]
    fn log_verbose_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        if self.can_log_verbose {
            self.error_manager.log_debug(&f());
        }
    }

    #[inline]
    fn log_info(&self, message: &str) {
        self.error_manager.log_info(message);
    }

    #[inline]
    fn log_info_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        self.error_manager.log_info(&f());
    }

    #[inline]
    fn log_warning(&self, message: &str) {
        self.error_manager.log_Warning(message);
    }

    #[inline]
    fn log_warning_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        self.error_manager.log_Warning(&f());
    }

    #[inline]
    fn log_error(&self, message: &str) {
        self.error_manager.log_error(message);
    }

    #[inline]
    fn log_error_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        self.error_manager.log_error(&f());
    }
}