// src/Compiler/Core/general_semantics_analyzer.rs

use crate::Compiler::AST::*;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::Compiler::Core::SectionAnalyzers::*;
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::VersionControl::{VersionManager, VersionConstraints};
use crate::Compiler::ImportsResolution::ImportsResolver;
use crate::ErrorManager::{ErrorManager, SemanticErrorType};
use crate::Builtins::Static::EnumObject;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Central semantic analysis orchestrator for DixScript v1.0.0
///
/// ANALYSIS PHASES:
/// - Phase 0:    Version Validation
/// - Phase 0.25: IMPORTS Semantic Analysis (validates before resolution)
/// - Phase 0.5:  Imports Resolution (CRITICAL)
/// - Phase 1:    Foundation (ENUMS)
/// - Phase 2:    Functions (QUICKFUNCS)
/// - Phase 3:    Independent (DLM)
/// - Phase 4:    Data-Driven (DATA)
/// - Phase 5:    Generatable (SECURITY)
/// - Phase 4.5:  AST Enhancement (parameter defaults)
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
    /// Returns `SemanticAnalysisResult` with errors, warnings, and enhanced AST
    pub fn analyze(mut self) -> SemanticAnalysisResult {
        self.error_manager.create_scope("General Semantic Analysis");

        self.log_info("Starting semantic analysis v1.0.0");
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
            // PHASE 0: VERSION VALIDATION
            if !self.analyze_phase0_version() {
                return self.finalize_result();
            }
        }

        // PHASE 0.5: IMPORTS RESOLUTION (no separate semantic phase needed)
        if !self.analyze_phase0_5_imports_resolution() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        // PHASE 1: FOUNDATION (ENUMS)
        if !self.analyze_phase1_foundation() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        // Register enums with builtin system
        self.register_enums_with_builtin_system();

        // PHASE 2: FUNCTIONS (QUICKFUNCS)
        if !self.analyze_phase2_functions() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        // PHASE 3: INDEPENDENT (DLM)
        self.analyze_phase3_independent();

        // PHASE 4: DATA-DRIVEN (DATA)
        if !self.analyze_phase4_data_driven() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        // PHASE 5: GENERATABLE (SECURITY)
        self.analyze_phase5_generated();

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

    // ==================== PHASE 0: VERSION VALIDATION ====================

    /// Phase 0: Comprehensive version compatibility check
    ///
    /// Returns `false` if validation fails and should halt
    fn analyze_phase0_version(&mut self) -> bool {
        self.log_info("Phase 0: Performing comprehensive version compatibility check");

        let version_validation = VersionConstraints::instance().validate_script(self.ast);

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
                version_validation.detected_version
            ));
        }

        true
    }

    // ==================== PHASE 0.5: IMPORTS RESOLUTION ====================

    /// Phase 0.5: Resolve imports and populate symbol table
    ///
    /// Returns `false` if resolution fails and should halt
    fn analyze_phase0_5_imports_resolution(&mut self) -> bool {
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

        self.log_info_fmt(|| format!("Phase 0.5: Resolving {} imports", imports.imports.len()));

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

        self.error_manager.create_scope("Imports Resolution Phase");

        // NOTE: ImportsResolver handles ALL parsing internally - we just pass empty map
        // The resolver will parse files on-demand during resolution
        let parsed_imports = HashMap::new();

        let mut imports_resolver = ImportsResolver::new(
            self.symbol_table.clone(),
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
                self.error_manager.exit_scope();
                return false;
            }
        } else {
            let stats = imports_resolver.get_statistics();
            self.log_info_fmt(|| format!("Imports resolved successfully: {}", stats));
        }

        self.error_manager.exit_scope();
        true
    }

    // ==================== PHASE 1: FOUNDATION (ENUMS) ====================

    /// Phase 1: Analyze foundational sections (ENUMS)
    ///
    /// Returns `false` if analysis fails and should halt
    fn analyze_phase1_foundation(&mut self) -> bool {
        self.error_manager.create_scope("Phase 1: Foundation (ENUMS)");

        self.log_info("Phase 1: Analyzing foundational sections");
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
                    self.error_manager.exit_scope();
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
                        self.error_manager.exit_scope();
                        return false;
                    }
                }
            }
        } else {
            self.log_info("ENUMS section not present - skipping analyzer");
        }

        self.log_info("Phase 1 complete");
        self.error_manager.exit_scope();
        true
    }

    // ==================== PHASE 2: FUNCTIONS (QUICKFUNCS) ====================

    /// Phase 2: Analyze function definitions
    ///
    /// Returns `false` if analysis fails and should halt
    fn analyze_phase2_functions(&mut self) -> bool {
        self.error_manager.create_scope("Phase 2: Functions (QUICKFUNCS)");

        self.log_info("Phase 2: Analyzing function definitions");

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
                    self.error_manager.exit_scope();
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

        self.log_info("Phase 2 complete");
        self.error_manager.exit_scope();
        true
    }

    // ==================== PHASE 3: INDEPENDENT (DLM) ====================

    /// Phase 3: Analyze independent sections
    fn analyze_phase3_independent(&mut self) {
        self.error_manager.create_scope("Phase 3: Independent Sections (DLM)");

        self.log_info("Phase 3: Analyzing independent sections");

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

        self.log_info("Phase 3 complete");
        self.error_manager.exit_scope();
    }

    // ==================== PHASE 4: DATA-DRIVEN (DATA) ====================

    /// Phase 4: Analyze data section
    ///
    /// Returns `false` if analysis fails and should halt
    fn analyze_phase4_data_driven(&mut self) -> bool {
        self.error_manager.create_scope("Phase 4: Data-Driven (DATA)");

        self.log_info("Phase 4: Analyzing data section");

        if let Some(ref _data) = self.ast.data {
            self.log_debug("Analyzing DATA section");

            // TODO: Create DataSectionAnalyzer when ported
            self.log_warning("DataSectionAnalyzer not yet ported - skipping analysis");
        } else {
            self.log_warning("DATA section not present - unusual for a data interchange format");
        }

        self.log_info("Phase 4 complete");
        self.error_manager.exit_scope();
        true
    }

    // ==================== PHASE 5: GENERATED (SECURITY) ====================

    /// Phase 5: Analyze compiler-generated sections
    fn analyze_phase5_generated(&mut self) {
        self.error_manager.create_scope("Phase 5: Generated Sections (SECURITY)");

        self.log_info("Phase 5: Analyzing compiler-generated sections");

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

        self.log_info("Phase 5 complete");
        self.error_manager.exit_scope();
    }

    // ==================== ENUM REGISTRATION ====================

    /// CRITICAL: Bridge SymbolTable enums to EnumObject builtin registry
    ///
    /// Must be called after ENUMS analysis (Phase 1) but before value resolution (Phase 4)
    fn register_enums_with_builtin_system(&mut self) {
        self.error_manager.create_scope("RegisterEnumsWithBuiltinSystem");

        // Clear any previous registrations
        EnumObject::clear_enums();

        let enum_count = self.symbol_table.enums.len();
        let mut registered_count = 0;

        self.log_info_fmt(|| format!(
            "Registering {} enums with builtin system",
            enum_count
        ));

        for (enum_name, field_mapping) in &self.symbol_table.enums {
            match EnumObject::register_enum(enum_name.clone(), field_mapping.clone()) {
                Ok(_) => {
                    registered_count += 1;
                    self.log_debug_fmt(|| format!(
                        "  ✓ Registered enum: {} ({} fields)",
                        enum_name,
                        field_mapping.len()
                    ));
                }
                Err(e) => {
                    self.log_error_fmt(|| format!(
                        "  ✗ Failed to register enum '{}': {}",
                        enum_name,
                        e
                    ));
                }
            }
        }

        self.log_info_fmt(|| format!(
            "✓ Enum registration complete: {}/{} enums registered",
            registered_count,
            enum_count
        ));

        // Verify registration
        let registered_enums = EnumObject::get_registered_enums();
        self.log_debug_fmt(|| format!(
            "  EnumObject registry now has: {}",
            registered_enums.join(", ")
        ));

        self.error_manager.exit_scope();
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

        self.error_manager.exit_scope();

        self.analysis_result
    }

    // ==================== LOGGING HELPERS (OPTIMIZED) ====================

    /// Log debug message (only if debug mode is enabled)
    /// OPTIMIZATION: Checks `can_log_debug` before formatting string
    #[inline]
    fn log_debug(&self, message: &str) {
        if self.can_log_debug {
            self.error_manager.log_debug(message);
        }
    }

    /// Log debug with lazy formatting (only formats if debug enabled)
    #[inline]
    fn log_debug_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        if self.can_log_debug {
            self.error_manager.log_debug(&f());
        }
    }

    /// Log verbose message (only if verbose mode is enabled)
    #[inline]
    fn log_verbose(&self, message: &str) {
        if self.can_log_verbose {
            self.error_manager.log_debug(message);
        }
    }

    /// Log verbose with lazy formatting
    #[inline]
    fn log_verbose_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        if self.can_log_verbose {
            self.error_manager.log_debug(&f());
        }
    }

    /// Log info message (always logged)
    #[inline]
    fn log_info(&self, message: &str) {
        self.error_manager.log_info(message);
    }

    /// Log info with lazy formatting
    #[inline]
    fn log_info_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        self.error_manager.log_info(&f());
    }

    /// Log warning message (always logged)
    #[inline]
    fn log_warning(&self, message: &str) {
        self.error_manager.log_Warning(message);
    }

    /// Log warning with lazy formatting
    #[inline]
    fn log_warning_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        self.error_manager.log_Warning(&f());
    }

    /// Log error message (always logged)
    #[inline]
    fn log_error(&self, message: &str) {
        self.error_manager.log_error(message);
    }

    /// Log error with lazy formatting
    #[inline]
    fn log_error_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        self.error_manager.log_error(&f());
    }
}

// ==================== RESULT STRUCTURES ====================

/// Result of semantic analysis
#[derive(Debug, Clone)]
pub struct SemanticAnalysisResult {
    pub is_success: bool,
    pub symbol_table: Option<SymbolTable>,
    pub enhanced_ast: Option<DixScript>,
    pub errors: Vec<SemanticErrorInfo>,
    pub warnings: Vec<SemanticWarningInfo>,
    pub section_results: HashMap<String, SectionAnalysisResult>,
    pub analysis_duration: Duration,
    pub short_name_index: Option<HashMap<String, Vec<String>>>,
    pub type_index: Option<HashMap<String, DataType>>,
}

impl SemanticAnalysisResult {
    pub fn new() -> Self {
        SemanticAnalysisResult {
            is_success: false,
            symbol_table: None,
            enhanced_ast: None,
            errors: Vec::new(),
            warnings: Vec::new(),
            section_results: HashMap::new(),
            analysis_duration: Duration::default(),
            short_name_index: None,
            type_index: None,
        }
    }
}

impl Default for SemanticAnalysisResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Semantic error information
#[derive(Debug, Clone)]
pub struct SemanticErrorInfo {
    pub error_id: String,
    pub error_type: String,
    pub message: String,
    pub section_name: String,
    pub suggestion: String,
    pub position: Option<Position>,
}

/// Semantic warning information
#[derive(Debug, Clone)]
pub struct SemanticWarningInfo {
    pub warning_id: String,
    pub message: String,
    pub section_name: String,
    pub position: Option<Position>,
}