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
pub struct GeneralSemanticAnalyzer<'a> {
    // References to input data (borrowed, not owned)
    ast: &'a DixScript,
    operational_settings: &'a OperationalSettings,

    // Owned state
    symbol_table: SymbolTable,
    error_manager: ErrorManager,

    // Section analyzers (created on demand)
    enums_analyzer:       Option<EnumsSectionAnalyzer<'a>>,
    dlm_analyzer:         Option<DlmSectionAnalyzer<'a>>,
    security_analyzer:    Option<SecuritySectionAnalyzer<'a>>,
    quickfuncs_analyzer:  Option<QuickFuncsSectionAnalyzer<'a>>,
    data_analyzer:        Option<DataSectionAnalyzer<'a>>,
    // NOTE: ImportsSectionAnalyzer is NOT stored as a field because it requires
    // borrowing symbol_table (owned here) with the struct's 'a lifetime, which
    // the borrow checker cannot satisfy. It is created locally per-call instead.

    // Result accumulator
    analysis_result: SemanticAnalysisResult,

    // Performance tracking
    stopwatch: Instant,

    // OPTIMIZATION: Cache log level checks
    can_log_debug: bool,
    can_log_verbose: bool,

    // Control flags
    skip_validation: bool,
}

impl<'a> GeneralSemanticAnalyzer<'a> {
    pub fn new(
        ast: &'a DixScript,
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let symbol_table = SymbolTable::new();

        let can_log_debug = operational_settings.debug_mode != DebugMode::Off;
        let can_log_verbose = operational_settings.debug_mode == DebugMode::Verbose;

        GeneralSemanticAnalyzer {
            ast,
            operational_settings,
            symbol_table,
            error_manager,
            enums_analyzer:       None,
            dlm_analyzer:         None,
            security_analyzer:    None,
            quickfuncs_analyzer:  None,
            data_analyzer:        None,
            analysis_result: SemanticAnalysisResult::new(),
            stopwatch: Instant::now(),
            can_log_debug,
            can_log_verbose,
            skip_validation: false,
        }
    }

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

        self.initialize_builtin_registries();

        if !self.skip_validation {
            if !self.analyze_phase1_version() {
                return self.finalize_result();
            }
        }

        if !self.analyze_phase2_imports_semantic() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        if !self.analyze_phase3_imports_resolution() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        if !self.analyze_phase4_foundation() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        self.register_enums_with_builtin_system();

        if !self.analyze_phase5_functions() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

        self.analyze_phase6_independent();

        if !self.analyze_phase7_data_driven() {
            if self.should_terminate() {
                return self.finalize_result();
            }
        }

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
        self.log_info_fmt(|| format!(
            "Sections analyzed: {:?}",
            self.analysis_result.section_results.keys().collect::<Vec<_>>()
        ));

        self.finalize_result()
    }

    // ==================== PHASE 1: VERSION VALIDATION ====================

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
                    error_id:     "SEM_VERSION".to_string(),
                    error_type:   "VersionCompatibility".to_string(),
                    message:      error.clone(),
                    section_name: "VERSION_CHECK".to_string(),
                    suggestion:   "Upgrade compiler version or adjust CONFIG section".to_string(),
                    position:     None,
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

    /// Phase 2: Validate import declarations using ImportsSectionAnalyzer.
    ///
    /// IMPORTANT: ImportsSectionAnalyzer::analyze() returns () and reports errors
    /// directly to the shared ErrorManager rather than returning a SectionAnalysisResult.
    /// We create the analyzer locally (not stored as a field) to avoid lifetime
    /// conflicts with the owned symbol_table, and we build a synthetic
    /// SectionAnalysisResult from the ErrorManager state after analysis.
    fn analyze_phase2_imports_semantic(&mut self) -> bool {
        let imports = match &self.ast.imports {
            Some(section) if !section.imports.is_empty() => section,
            _ => {
                self.log_info("No imports section — skipping imports semantic analysis");
                return true;
            }
        };

        self.log_info("Phase 2: Analyzing IMPORTS section semantically");

        if !self.operational_settings.is_feature_enabled("imports")
            && !self.operational_settings.is_advanced_mode()
        {
            self.log_error("IMPORTS section found but imports feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "IMPORTS section requires 'imports' feature or advanced mode to be enabled".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion:   "Add 'imports' to features list or enable advanced mode in CONFIG".to_string(),
                position:     None,
            });
            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
            return true;
        }

        // Derive the current file path from operational settings; fall back to empty string
        // if not set (e.g. when analysing an in-memory script without a backing file).
        let current_file_path = self.operational_settings
            .source_file_path
            .as_deref()
            .unwrap_or("");

        // Track whether ErrorManager had errors before this phase so we can detect
        // any new errors added by ImportsSectionAnalyzer.
        let had_errors_before = self.error_manager.has_errors();

        // Create the analyzer locally — it borrows &self.symbol_table for the duration
        // of the call only, avoiding any long-lived borrow conflict.
        let mut imports_analyzer = ImportsSectionAnalyzer::new(
            &self.symbol_table,
            self.operational_settings,
            current_file_path,
        );

        // analyze() takes Option<&ImportsSection> and returns ().
        // Errors are reported directly into the shared ErrorManager.
        imports_analyzer.analyze(Some(imports));

        // Drop the analyzer immediately so the immutable borrow on symbol_table ends,
        // allowing mutable borrows again in subsequent phases.
        drop(imports_analyzer);

        // Build a synthetic SectionAnalysisResult reflecting the ErrorManager state.
        // We cannot enumerate the specific new errors from ErrorManager without a
        // get_semantic_errors() API, so we record is_success only and leave the
        // per-error detail in the shared ErrorManager.
        let phase_ok = !self.error_manager.has_errors();
        let mut result = SectionAnalysisResult::new("IMPORTS_SEMANTIC");
        result.is_success = phase_ok;

        self.log_info_fmt(|| format!(
            "Phase 2 complete: IMPORTS semantic — success={}",
            result.is_success,
        ));

        // If new errors appeared, surface a summary entry so analysis_result.errors
        // reflects the failure (individual errors are in ErrorManager).
        if !phase_ok && !had_errors_before {
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_IMPORTS_SEM".to_string(),
                error_type:   "ImportsSemantic".to_string(),
                message:      "IMPORTS section semantic validation failed — see ErrorManager for details".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion:   String::new(),
                position:     None,
            });
        }

        self.add_section_result("IMPORTS_SEMANTIC", result);

        if !phase_ok && self.should_terminate() {
            self.analysis_result.is_success = false;
            return false;
        }

        true
    }

    // ==================== PHASE 3: IMPORTS RESOLUTION ====================

    fn analyze_phase3_imports_resolution(&mut self) -> bool {
        if self.operational_settings.skip_imports_resolution {
            self.log_debug("Skipping imports resolution (imported file — parent is resolving)");
            return true;
        }

        let imports = match &self.ast.imports {
            Some(section) if !section.imports.is_empty() => section,
            _ => {
                self.log_info("No imports section — skipping import resolution");
                return true;
            }
        };

        self.log_info_fmt(|| format!("Phase 3: Resolving {} imports", imports.imports.len()));

        if !self.operational_settings.is_feature_enabled("imports")
            && !self.operational_settings.is_advanced_mode()
        {
            self.log_error("IMPORTS section found but imports feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "IMPORTS section requires 'imports' feature or advanced mode to be enabled".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion:   "Add 'imports' to features list or enable advanced mode in CONFIG".to_string(),
                position:     None,
            });
            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
            return true;
        }

        self.log_debug("Starting imports resolution phase");

        let parsed_imports = HashMap::new();

        let mut imports_resolver = ImportsResolver::new(
            &mut self.symbol_table,
            self.operational_settings,
        );

        let resolve_success = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async {
                imports_resolver.resolve_imports(&parsed_imports).await
            });

        if !resolve_success {
            self.log_error("Import resolution failed");

            let import_errors = self.error_manager.get_imports_resolution_errors();
            if !import_errors.is_empty() {
                self.log_warning_fmt(|| format!(
                    "Import resolution completed with {} errors",
                    import_errors.len()
                ));

                for error in import_errors {
                    self.analysis_result.errors.push(SemanticErrorInfo {
                        error_id:     error.error_id.clone(),
                        error_type:   format!("{:?}", error.error_type),
                        message:      error.message.clone(),
                        section_name: "IMPORTS".to_string(),
                        suggestion:   error.suggestion.clone().unwrap_or_default(),
                        position:     Some(Position::new(error.line as usize, error.column as usize)),
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

    fn analyze_phase4_foundation(&mut self) -> bool {
        self.log_info("Phase 4: Analyzing foundational sections");
        self.log_info("CONFIG already processed by ConfigSectionHandler — skipping validation");

        let enums = match &self.ast.enums {
            Some(section) => section,
            None => {
                self.log_info("ENUMS section not present — skipping analyzer");
                return true;
            }
        };

        if !self.operational_settings.is_feature_enabled("enums")
            && !self.operational_settings.is_advanced_mode()
        {
            self.log_error("ENUMS section found but enums feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "ENUMS section requires 'enums' feature or advanced mode to be enabled".to_string(),
                section_name: "ENUMS".to_string(),
                suggestion:   "Add 'enums' to features list or enable advanced mode in CONFIG".to_string(),
                position:     None,
            });
            if self.should_terminate() {
                return false;
            }
            return true;
        }

        self.log_debug("Analyzing ENUMS section with EnumsSectionAnalyzer");

        if self.enums_analyzer.is_none() {
            self.enums_analyzer = Some(EnumsSectionAnalyzer::new(self.operational_settings));
        }

        let result = self.enums_analyzer.as_mut().unwrap()
            .analyze(enums, &mut self.symbol_table);

        self.log_info_fmt(|| format!(
            "Phase 4 complete: ENUMS — success={} errors={} warnings={}",
            result.is_success, result.errors.len(), result.warnings.len()
        ));

        if !result.errors.is_empty() {
            for e in &result.errors {
                self.log_error_fmt(|| format!(
                    "  ENUMS ERR [{}] {}: {}",
                    e.error_id, e.error_type, e.message
                ));
            }
        }

        let phase_ok = result.is_success;
        self.add_section_result("ENUMS", result);

        if !phase_ok && self.should_terminate() {
            return false;
        }

        true
    }

    // ==================== PHASE 5: FUNCTIONS (QUICKFUNCS) ====================

    fn analyze_phase5_functions(&mut self) -> bool {
        self.log_info("Phase 5: Analyzing function definitions (QUICKFUNCS)");

        let quickfuncs = match &self.ast.quick_functions {
            Some(section) => section,
            None => {
                self.log_info("QUICKFUNCS section not present — skipping analyzer");
                return true;
            }
        };

        if !self.operational_settings.is_feature_enabled("quickfuncs")
            && !self.operational_settings.is_advanced_mode()
        {
            self.log_error("QUICKFUNCS section found but quickfuncs feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "QUICKFUNCS section requires 'quickfuncs' feature or advanced mode to be enabled".to_string(),
                section_name: "QUICKFUNCS".to_string(),
                suggestion:   "Add 'quickfuncs' to features list or enable advanced mode in CONFIG".to_string(),
                position:     None,
            });
            if self.should_terminate() {
                return false;
            }
            return true;
        }

        self.log_debug("Analyzing QUICKFUNCS section with QuickFuncsSectionAnalyzer");

        if self.quickfuncs_analyzer.is_none() {
            self.quickfuncs_analyzer = Some(QuickFuncsSectionAnalyzer::new(self.operational_settings));
        }

        let result = self.quickfuncs_analyzer.as_mut().unwrap()
            .analyze(quickfuncs, &mut self.symbol_table);

        self.log_info_fmt(|| format!(
            "Phase 5 complete: QUICKFUNCS — success={} errors={} warnings={} qi_resolutions={}",
            result.is_success,
            result.errors.len(),
            result.warnings.len(),
            result.qualified_id_resolutions.len(),
        ));

        if !result.errors.is_empty() {
            for e in &result.errors {
                self.log_error_fmt(|| format!(
                    "  QUICKFUNCS ERR [{}] {}: {}",
                    e.error_id, e.error_type, e.message
                ));
                if !e.suggestion.is_empty() {
                    self.log_error_fmt(|| format!("    → {}", e.suggestion));
                }
            }
        }

        let phase_ok = result.is_success;
        self.add_section_result("QUICKFUNCS", result);

        if !phase_ok && self.should_terminate() {
            return false;
        }

        true
    }

    // ==================== PHASE 6: INDEPENDENT (DLM) ====================

    fn analyze_phase6_independent(&mut self) {
        self.log_info("Phase 6: Analyzing independent sections (DLM)");

        let dlm = match &self.ast.dlm {
            Some(section) => section,
            None => {
                self.log_debug("DLM section not present — skipping analyzer");
                return;
            }
        };

        self.log_debug("Analyzing DLM section with DlmSectionAnalyzer");

        if self.dlm_analyzer.is_none() {
            self.dlm_analyzer = Some(DlmSectionAnalyzer::new(self.operational_settings));
        }

        let result = self.dlm_analyzer.as_mut().unwrap()
            .analyze(dlm, &mut self.symbol_table);

        self.log_info_fmt(|| format!(
            "Phase 6 complete: DLM — success={} errors={} warnings={}",
            result.is_success, result.errors.len(), result.warnings.len()
        ));

        self.add_section_result("DLM", result);
    }

    // ==================== PHASE 7: DATA-DRIVEN (DATA) ====================

    /// Phase 7: Analyze DATA section using DataSectionAnalyzer.
    ///
    /// The short_name_index and type_index are built internally by DataSectionAnalyzer
    /// and retrieved via get_indexes() — they are NOT fields on SymbolTable.
    fn analyze_phase7_data_driven(&mut self) -> bool {
        self.log_info("Phase 7: Analyzing data section (DATA)");

        let data = match &self.ast.data {
            Some(section) => section,
            None => {
                self.log_warning("DATA section not present — unusual for a data interchange format");
                return true;
            }
        };

        self.log_debug("Analyzing DATA section with DataSectionAnalyzer");

        if self.data_analyzer.is_none() {
            self.data_analyzer = Some(DataSectionAnalyzer::new(self.operational_settings));
        }

        let result = self.data_analyzer.as_mut().unwrap()
            .analyze(data, &mut self.symbol_table);

        self.log_info_fmt(|| format!(
            "Phase 7 complete: DATA — success={} errors={} warnings={}",
            result.is_success, result.errors.len(), result.warnings.len()
        ));

        if !result.errors.is_empty() {
            for e in &result.errors {
                self.log_error_fmt(|| format!(
                    "  DATA ERR [{}] {} in @{}: {}",
                    e.error_id, e.error_type, e.section_name, e.message
                ));
                if !e.suggestion.is_empty() {
                    self.log_error_fmt(|| format!("    → {}", e.suggestion));
                }
            }
        }

        if !result.warnings.is_empty() {
            for w in &result.warnings {
                self.log_warning_fmt(|| format!(
                    "  DATA WARN [{}] in @{}: {}",
                    w.warning_id, w.section_name, w.message
                ));
            }
        }

        let phase_ok = result.is_success;
        self.add_section_result("DATA", result);

        // Retrieve the indexes that DataSectionAnalyzer built internally.
        // These live on DataSectionAnalyzer, NOT on SymbolTable — hence get_indexes().
        if let Some(ref data_analyzer) = self.data_analyzer {
            let (short_name_idx, type_idx) = data_analyzer.get_indexes();

            if !short_name_idx.is_empty() {
                self.analysis_result.short_name_index = Some(
                    short_name_idx
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                );
            }

            if !type_idx.is_empty() {
                self.analysis_result.type_index = Some(
                    type_idx
                        .iter()
                        .map(|(k, v)| (k.clone(), *v))
                        .collect(),
                );
            }
        }

        if !phase_ok && self.should_terminate() {
            self.analysis_result.is_success = false;
            return false;
        }

        true
    }

    // ==================== PHASE 8: GENERATED (SECURITY) ====================

    fn analyze_phase8_generated(&mut self) {
        self.log_info("Phase 8: Analyzing compiler-generated sections (SECURITY)");

        let requires_security = self.ast.dlm.as_ref()
            .map(|dlm| dlm.modules.iter().any(|m| {
                matches!(m.module_type, DLMModuleType::DEncryptor)
            }))
            .unwrap_or(false);

        let security = match &self.ast.security {
            Some(section) => section,
            None => {
                if requires_security {
                    self.log_error(
                        "SECURITY section is required when using DEncryptor module but not present"
                    );
                    self.analysis_result.errors.push(SemanticErrorInfo {
                        error_id:     "SEM0002".to_string(),
                        error_type:   "MissingSection".to_string(),
                        message:      "SECURITY section is required when using DEncryptor module in @DLM".to_string(),
                        section_name: "SECURITY".to_string(),
                        suggestion:   "Add @SECURITY section with encryption configuration".to_string(),
                        position:     None,
                    });
                } else {
                    self.log_debug("SECURITY section not present — skipping (not required without DEncryptor)");
                }
                return;
            }
        };

        self.log_debug("Analyzing SECURITY section with SecuritySectionAnalyzer");

        if self.security_analyzer.is_none() {
            self.security_analyzer = Some(SecuritySectionAnalyzer::new(self.operational_settings));
        }

        let result = self.security_analyzer.as_mut().unwrap()
            .analyze(security, &mut self.symbol_table);

        self.log_info_fmt(|| format!(
            "Phase 8 complete: SECURITY — success={} errors={} warnings={}",
            result.is_success, result.errors.len(), result.warnings.len()
        ));

        self.add_section_result("SECURITY", result);
    }

    // ==================== ENUM REGISTRATION ====================

    fn register_enums_with_builtin_system(&mut self) {
        self.log_info("Registering enums with builtin system");

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

        if self.can_log_verbose {
            let registered_enums = enum_object::get_registered_enums();
            self.log_debug_fmt(|| format!(
                "  EnumObject registry now contains: {}",
                registered_enums.join(", ")
            ));
        }
    }

    // ==================== HELPER METHODS ====================

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

    fn add_section_result(&mut self, section_name: &str, result: SectionAnalysisResult) {
        self.analysis_result.errors.extend(result.errors.clone());
        self.analysis_result.warnings.extend(result.warnings.clone());

        if !result.is_success {
            self.log_warning_fmt(|| format!(
                "Section {} analysis completed with errors",
                section_name
            ));
        }

        self.analysis_result.section_results.insert(
            section_name.to_string(),
            result,
        );
    }

    #[inline]
    fn should_terminate(&self) -> bool {
        !self.analysis_result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    fn finalize_result(mut self) -> SemanticAnalysisResult {
        self.analysis_result.is_success = self.analysis_result.errors.is_empty();
        self.analysis_result.analysis_duration = self.stopwatch.elapsed();

        let duration_ms = self.analysis_result.analysis_duration.as_secs_f64() * 1000.0;
        self.log_info(&format!("Analysis duration: {:.2}ms", duration_ms));

        self.analysis_result.symbol_table = Some(self.symbol_table);
        self.analysis_result
    }

    // ==================== LOGGING HELPERS ====================

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
        self.error_manager.log_warning(message);
    }

    #[inline]
    fn log_warning_fmt<F>(&self, f: F)
    where
        F: FnOnce() -> String,
    {
        self.error_manager.log_warning(&f());
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