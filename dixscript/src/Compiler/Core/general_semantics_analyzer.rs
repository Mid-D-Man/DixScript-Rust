//! Central semantic analysis orchestrator — runs all section analyzers in dependency order.
//!
//! ## Error manager propagation
//! `propagate_error_manager = true` (LSP / `new_for_lsp` / `new_with_error_manager`):
//!   every sub-analyzer receives `self.error_manager.clone()` so diagnostics from
//!   all sub-analyzers aggregate in the same isolated per-document store.
//! `propagate_error_manager = false` (CLI / `new`):
//!   sub-analyzers use `ErrorManager::get_shared_instance()` independently.
//!   Behaviour is functionally identical to the propagating path when the top-level
//!   error manager IS the shared singleton.

use std::collections::HashMap;
use std::time::Instant;
use crate::Compiler::AST::*;
use crate::Compiler::Core::{
    ErrorHandlingStrategy,
    OperationalSettings,
    SemanticAnalysisResult,
    SemanticErrorInfo,
    SectionAnalysisResult,
};
use crate::Compiler::Core::SectionAnalyzers::*;
use crate::Compiler::Utilities::SymbolTable;
use crate::Compiler::Utilities::symbol_table::ImportedNamespace;
use crate::Compiler::VersionControl::VersionConstraints;
use crate::Compiler::ImportsResolution::ImportsResolver;
use crate::ErrorManager::{DebugConfig, ErrorManager};
use crate::Builtins::Static::enum_object;

pub struct GeneralSemanticAnalyzer<'a> {
    ast:                  &'a DixScript,
    operational_settings: &'a OperationalSettings,
    symbol_table:         SymbolTable,
    error_manager:        ErrorManager,
    debug_config:         DebugConfig,

    analysis_result:  SemanticAnalysisResult,
    stopwatch:        Instant,
    skip_validation:  bool,

    has_imports_enabled:    bool,
    has_enums_enabled:      bool,
    has_quickfuncs_enabled: bool,
    has_dlm_enabled:        bool,

    /// When true, sub-analyzers receive `self.error_manager.clone()` so all
    /// diagnostics aggregate in the same isolated store (LSP path).
    /// When false, sub-analyzers acquire the shared process-wide singleton (CLI path).
    propagate_error_manager: bool,
}

impl<'a> GeneralSemanticAnalyzer<'a> {

    // ── Constructors ──────────────────────────────────────────────────────────

    /// CLI path — sub-analyzers use shared singleton.
    pub fn new(
        ast:                  &'a DixScript,
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let mut s = Self::build(ast, operational_settings, ErrorManager::get_shared_instance());
        s.propagate_error_manager = false;
        s
    }

    /// Full constructor — caller supplies the ErrorManager instance.
    /// Sub-analyzers receive `error_manager.clone()` (propagate = true).
    pub fn new_with_error_manager(
        ast:                  &'a DixScript,
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
    ) -> Self {
        let mut s = Self::build(ast, operational_settings, error_manager);
        s.propagate_error_manager = true;
        s
    }

    /// LSP path — explicit constructor for language-server use.
    /// Identical to `new_with_error_manager` but semantically documents the call site.
    pub fn new_for_lsp(
        ast:                  &'a DixScript,
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
    ) -> Self {
        Self::new_with_error_manager(ast, operational_settings, error_manager)
    }

    /// Constructor used when analyzing an imported file that has transitive
    /// dependencies already resolved by the outer ImportsResolver.
    pub fn new_with_seed_namespaces(
        ast:                  &'a DixScript,
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
        seed_namespaces:      &HashMap<String, ImportedNamespace>,
    ) -> Self {
        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

        let is_advanced            = operational_settings.is_advanced_mode();
        let has_imports_enabled    = is_advanced || operational_settings.is_feature_enabled("imports");
        let has_enums_enabled      = is_advanced || operational_settings.is_feature_enabled("enums");
        let has_quickfuncs_enabled = is_advanced || operational_settings.is_feature_enabled("quickfuncs");
        let has_dlm_enabled        = is_advanced || operational_settings.is_feature_enabled("dlm");

        let mut symbol_table = SymbolTable::new();
        symbol_table.seed_namespaces_from_map(seed_namespaces);

        if debug_config.is_enabled {
            let em = ErrorManager::get_shared_instance();
            em.log_debug(&format!(
                "[GeneralSemanticAnalyzer] Seeded symbol table with {} namespace(s): [{}]",
                seed_namespaces.len(),
                seed_namespaces.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        }

        GeneralSemanticAnalyzer {
            ast,
            operational_settings,
            symbol_table,
            error_manager,
            debug_config,
            analysis_result: SemanticAnalysisResult::new(),
            stopwatch: Instant::now(),
            skip_validation: false,
            has_imports_enabled,
            has_enums_enabled,
            has_quickfuncs_enabled,
            has_dlm_enabled,
            propagate_error_manager: true,
        }
    }

    // ── Internal builder ──────────────────────────────────────────────────────

    fn build(
        ast:                  &'a DixScript,
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
    ) -> Self {
        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

        let is_advanced            = operational_settings.is_advanced_mode();
        let has_imports_enabled    = is_advanced || operational_settings.is_feature_enabled("imports");
        let has_enums_enabled      = is_advanced || operational_settings.is_feature_enabled("enums");
        let has_quickfuncs_enabled = is_advanced || operational_settings.is_feature_enabled("quickfuncs");
        let has_dlm_enabled        = is_advanced || operational_settings.is_feature_enabled("dlm");

        GeneralSemanticAnalyzer {
            ast,
            operational_settings,
            symbol_table: SymbolTable::new(),
            error_manager,
            debug_config,
            analysis_result: SemanticAnalysisResult::new(),
            stopwatch: Instant::now(),
            skip_validation: false,
            has_imports_enabled,
            has_enums_enabled,
            has_quickfuncs_enabled,
            has_dlm_enabled,
            propagate_error_manager: true, // overridden by constructors
        }
    }

    // ── EM helper — resolves to isolated clone or shared singleton ────────────

    fn make_error_manager(&self) -> ErrorManager {
        if self.propagate_error_manager {
            self.error_manager.clone()
        } else {
            ErrorManager::get_shared_instance()
        }
    }

    // ── Main entry point ──────────────────────────────────────────────────────

    pub fn analyze(mut self) -> SemanticAnalysisResult {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("Starting General Semantic Analysis v1.0.0");
            self.error_manager.log_debug(&format!(
                "Error Handling: {:?} | Compat: {:?} | Advanced: {} | Propagate EM: {}",
                self.operational_settings.error_handling_strategy,
                self.operational_settings.compatibility_mode,
                self.operational_settings.is_advanced_mode(),
                self.propagate_error_manager,
            ));
        }

        self.initialize_builtin_registries();

        if !self.skip_validation && !self.analyze_phase1_version() {
            return self.finalize_result();
        }
        if !self.analyze_phase2_imports_semantic() && self.should_terminate() {
            return self.finalize_result();
        }
        if !self.analyze_phase3_imports_resolution() && self.should_terminate() {
            return self.finalize_result();
        }
        if !self.analyze_phase4_foundation() && self.should_terminate() {
            return self.finalize_result();
        }

        self.register_enums_with_builtin_system();

        if !self.analyze_phase5_functions() && self.should_terminate() {
            return self.finalize_result();
        }

        self.analyze_phase6_independent();

        if !self.analyze_phase7_data_driven() && self.should_terminate() {
            return self.finalize_result();
        }

        self.analyze_phase8_generated();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Semantic analysis complete: {} — errors: {}, warnings: {}",
                if self.analysis_result.is_success { "SUCCESS" } else { "FAILURE" },
                self.analysis_result.errors.len(),
                self.analysis_result.warnings.len()
            ));
        }

        self.finalize_result()
    }

    fn analyze_phase1_version(&mut self) -> bool {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("Phase 1: version compatibility check");
        }

        let validation = VersionConstraints::new().validate_script(self.ast);

        if !validation.is_valid {
            self.error_manager.log_error(&format!(
                "Version validation failed with {} errors",
                validation.errors.len()
            ));
            for error in &validation.errors {
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
                self.analysis_result.is_success = false;
                return false;
            }
        } else if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Version validation passed (Script v{})",
                validation.detected_version.as_deref().unwrap_or("1.0.0")
            ));
        }

        true
    }

    fn analyze_phase2_imports_semantic(&mut self) -> bool {
        let imports = match &self.ast.imports {
            Some(s) if !s.imports.is_empty() => s,
            _ => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No imports — skipping imports semantic analysis");
                }
                return true;
            }
        };

        if self.debug_config.is_enabled {
            self.error_manager.log_info("Phase 2: IMPORTS semantic analysis");
        }

        if !self.has_imports_enabled {
            self.error_manager.log_error("IMPORTS section found but imports feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "IMPORTS section requires 'imports' feature or advanced mode".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion:   "Add 'imports' to features in CONFIG or enable advanced mode".to_string(),
                position:     None,
            });
            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
            return true;
        }

        let current_file_path = self
            .operational_settings
            .source_file_path
            .as_deref()
            .unwrap_or("");

        let had_errors_before = self.error_manager.has_errors();

        let mut imports_analyzer = ImportsSectionAnalyzer::new_with_error_manager(
            &self.symbol_table,
            self.operational_settings,
            current_file_path,
            self.make_error_manager(),
        );
        imports_analyzer.analyze(Some(imports));
        drop(imports_analyzer);

        let phase_ok   = !self.error_manager.has_errors();
        let mut result = SectionAnalysisResult::new("IMPORTS_SEMANTIC");
        result.is_success = phase_ok;

        if !phase_ok && !had_errors_before {
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_IMPORTS_SEM".to_string(),
                error_type:   "ImportsSemantic".to_string(),
                message:      "IMPORTS section semantic validation failed".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion:   String::new(),
                position:     None,
            });
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!("Phase 2 complete: success={}", phase_ok));
        }

        self.add_section_result("IMPORTS_SEMANTIC", result);

        if !phase_ok && self.should_terminate() {
            self.analysis_result.is_success = false;
            return false;
        }

        true
    }

    fn analyze_phase3_imports_resolution(&mut self) -> bool {
        if self.operational_settings.skip_imports_resolution {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug("Skipping imports resolution (imported file context)");
            }
            return true;
        }

        let imports = match &self.ast.imports {
            Some(s) if !s.imports.is_empty() => s,
            _ => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No imports — skipping resolution");
                }
                return true;
            }
        };

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Phase 3: resolving {} import(s) from disk",
                imports.imports.len()
            ));
        }

        if !self.has_imports_enabled {
            self.error_manager.log_error("IMPORTS section found but imports feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "IMPORTS section requires 'imports' feature or advanced mode".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion:   "Add 'imports' to features in CONFIG or enable advanced mode".to_string(),
                position:     None,
            });
            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
            return true;
        }

        let base_dir = self
            .operational_settings
            .source_file_path
            .as_deref()
            .and_then(|p| std::path::Path::new(p).parent())
            .and_then(|p| p.to_str())
            .unwrap_or(".");

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 3: base_dir='{}' for import resolution", base_dir
            ));
        }

        let mut imports_resolver = ImportsResolver::new_with_error_manager(
            &mut self.symbol_table,
            self.operational_settings,
            self.make_error_manager(),
        );

        let resolve_success = imports_resolver.resolve_from_imports_section(imports, base_dir);

        if !resolve_success {
            self.error_manager.log_error("Import resolution failed");
            for error in self.error_manager.get_imports_resolution_errors() {
                self.analysis_result.errors.push(SemanticErrorInfo {
                    error_id:     error.error_id.clone(),
                    error_type:   format!("{:?}", error.error_type),
                    message:      error.message.clone(),
                    section_name: "IMPORTS".to_string(),
                    suggestion:   error.suggestion.clone().unwrap_or_default(),
                    position:     Some(Position::new(error.line as usize, error.column as usize)),
                });
            }
            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
        } else if self.debug_config.is_enabled {
            let stats = imports_resolver.get_statistics();
            self.error_manager.log_debug(&format!("Imports resolved: {}", stats));
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info("Phase 3 complete");
        }
        true
    }

    fn analyze_phase4_foundation(&mut self) -> bool {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("Phase 4: foundation sections (ENUMS)");
        }

        let enums = match &self.ast.enums {
            Some(s) => s,
            None => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No ENUMS section");
                }
                return true;
            }
        };

        if !self.has_enums_enabled {
            self.error_manager.log_error("ENUMS section found but enums feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "ENUMS section requires 'enums' feature or advanced mode".to_string(),
                section_name: "ENUMS".to_string(),
                suggestion:   "Add 'enums' to features in CONFIG or enable advanced mode".to_string(),
                position:     None,
            });
            if self.should_terminate() { return false; }
            return true;
        }

        let mut analyzer = EnumsSectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            self.make_error_manager(),
        );
        let result = analyzer.analyze(enums, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 4 complete: success={} errors={} warnings={}",
                result.is_success, result.errors.len(), result.warnings.len()
            ));
            if self.debug_config.is_verbose {
                for e in &result.errors {
                    self.error_manager.log_error(&format!(
                        "  ENUMS [{}] {}: {}", e.error_id, e.error_type, e.message
                    ));
                }
            }
        }

        let phase_ok = result.is_success;
        self.add_section_result("ENUMS", result);

        if !phase_ok && self.should_terminate() { return false; }
        true
    }

    fn analyze_phase5_functions(&mut self) -> bool {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("Phase 5: function definitions (QUICKFUNCS)");
        }

        let quickfuncs = match &self.ast.quick_functions {
            Some(s) => s,
            None => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No QUICKFUNCS section");
                }
                return true;
            }
        };

        if !self.has_quickfuncs_enabled {
            self.error_manager.log_error("QUICKFUNCS section found but quickfuncs feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "QUICKFUNCS section requires 'quickfuncs' feature or advanced mode".to_string(),
                section_name: "QUICKFUNCS".to_string(),
                suggestion:   "Add 'quickfuncs' to features in CONFIG or enable advanced mode".to_string(),
                position:     None,
            });
            if self.should_terminate() { return false; }
            return true;
        }

        let mut analyzer = QuickFuncsSectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            self.make_error_manager(),
        );
        let result = analyzer.analyze(quickfuncs, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 5 complete: success={} errors={} warnings={} qi_resolutions={}",
                result.is_success,
                result.errors.len(),
                result.warnings.len(),
                result.qualified_id_resolutions.len()
            ));
            if self.debug_config.is_verbose {
                for e in &result.errors {
                    self.error_manager.log_error(&format!(
                        "  QUICKFUNCS [{}] {}: {}", e.error_id, e.error_type, e.message
                    ));
                    if !e.suggestion.is_empty() {
                        self.error_manager.log_error(&format!("    -> {}", e.suggestion));
                    }
                }
            }
        }

        let phase_ok = result.is_success;
        self.add_section_result("QUICKFUNCS", result);

        if !phase_ok && self.should_terminate() { return false; }
        true
    }

    fn analyze_phase6_independent(&mut self) {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("Phase 6: independent sections (DLM)");
        }

        let dlm = match &self.ast.dlm {
            Some(s) => s,
            None => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No DLM section");
                }
                return;
            }
        };

        let mut analyzer = DlmSectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            self.make_error_manager(),
        );
        let result = analyzer.analyze(dlm, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 6 complete: success={} errors={} warnings={}",
                result.is_success, result.errors.len(), result.warnings.len()
            ));
        }

        self.add_section_result("DLM", result);
    }

    fn analyze_phase7_data_driven(&mut self) -> bool {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("Phase 7: DATA section");
        }

        let data = match &self.ast.data {
            Some(s) => s,
            None => {
                self.error_manager.log_warning("No DATA section present");
                return true;
            }
        };

        let mut analyzer = DataSectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            self.make_error_manager(),
        );
        let result = analyzer.analyze(data, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 7 complete: success={} errors={} warnings={}",
                result.is_success, result.errors.len(), result.warnings.len()
            ));
            if self.debug_config.is_verbose {
                for e in &result.errors {
                    self.error_manager.log_error(&format!(
                        "  DATA [{}] {}: {}", e.error_id, e.error_type, e.message
                    ));
                    if !e.suggestion.is_empty() {
                        self.error_manager.log_error(&format!("    -> {}", e.suggestion));
                    }
                }
                for w in &result.warnings {
                    self.error_manager.log_warning(&format!(
                        "  DATA WARN [{}]: {}", w.warning_id, w.message
                    ));
                }
            }
        }

        let phase_ok = result.is_success;
        let (short_name_idx, type_idx) = analyzer.get_indexes();

        if !short_name_idx.is_empty() {
            self.analysis_result.short_name_index = Some(
                short_name_idx.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            );
        }
        if !type_idx.is_empty() {
            self.analysis_result.type_index = Some(
                type_idx.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            );
        }

        self.add_section_result("DATA", result);

        if !phase_ok && self.should_terminate() {
            self.analysis_result.is_success = false;
            return false;
        }
        true
    }

    fn analyze_phase8_generated(&mut self) {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("Phase 8: generated sections (SECURITY)");
        }

        let requires_security = self
            .ast
            .dlm
            .as_ref()
            .map(|dlm| dlm.modules.iter().any(|m| matches!(m.module_type, DLMModuleType::DEncryptor)))
            .unwrap_or(false);

        let security = match &self.ast.security {
            Some(s) => s,
            None => {
                if requires_security {
                    self.error_manager.log_error(
                        "SECURITY section required when using DEncryptor but not present",
                    );
                    self.analysis_result.errors.push(SemanticErrorInfo {
                        error_id:     "SEM0002".to_string(),
                        error_type:   "MissingSection".to_string(),
                        message:      "SECURITY section required when DEncryptor is used in @DLM".to_string(),
                        section_name: "SECURITY".to_string(),
                        suggestion:   "Add @SECURITY section with encryption configuration".to_string(),
                        position:     None,
                    });
                } else if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No SECURITY section (not required without DEncryptor)");
                }
                return;
            }
        };

        let mut analyzer = SecuritySectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            self.make_error_manager(),
        );
        let result = analyzer.analyze(security, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 8 complete: success={} errors={} warnings={}",
                result.is_success, result.errors.len(), result.warnings.len()
            ));
        }

        self.add_section_result("SECURITY", result);
    }

    fn register_enums_with_builtin_system(&mut self) {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("Registering enums with builtin system");
        }

        enum_object::clear_enums();

        let enum_count   = self.symbol_table.enums.len();
        let mut registered = 0usize;

        for (name, fields) in &self.symbol_table.enums {
            enum_object::register_enum(name.clone(), fields.clone());
            registered += 1;
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "  Registered enum: {} ({} fields)", name, fields.len()
                ));
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Enum registration: {}/{} registered", registered, enum_count
            ));
        }
    }

    fn initialize_builtin_registries(&mut self) {
        for name in &["Math", "Dix", "DateTime", "Array", "Random", "Enum", "Guid", "Ip"] {
            self.symbol_table.add_builtin_static_object(name.to_string());
        }
        self.symbol_table.add_dix_function(
            "logEvent".to_string(), "void".to_string(), vec!["string".to_string()],
        );
        self.symbol_table.add_dix_function(
            "getSystemInfo".to_string(), "object".to_string(), vec![],
        );
        self.symbol_table.add_dix_function(
            "validateConfig".to_string(), "bool".to_string(), vec!["string".to_string()],
        );

        if self.debug_config.is_enabled {
            self.error_manager.log_debug("Built-in registries initialized");
        }
    }

    fn add_section_result(&mut self, section_name: &str, result: SectionAnalysisResult) {
        if !result.is_success && self.debug_config.is_enabled {
            self.error_manager.log_warning(&format!(
                "Section {} analysis completed with errors", section_name
            ));
        }
        self.analysis_result.errors.extend(result.errors.clone());
        self.analysis_result.warnings.extend(result.warnings.clone());
        self.analysis_result.section_results.insert(section_name.to_string(), result);
    }

    #[inline]
    fn should_terminate(&self) -> bool {
        !self.analysis_result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    fn finalize_result(mut self) -> SemanticAnalysisResult {
        self.analysis_result.is_success       = self.analysis_result.errors.is_empty();
        self.analysis_result.analysis_duration = self.stopwatch.elapsed();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Analysis duration: {:.2}ms",
                self.analysis_result.analysis_duration.as_secs_f64() * 1000.0
            ));
        }

        self.analysis_result.symbol_table = Some(self.symbol_table);
        self.analysis_result
    }
    }
