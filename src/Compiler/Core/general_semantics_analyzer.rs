// src/Compiler/Core/general_semantics_analyzer.rs
//! Central semantic analysis orchestrator — runs all section analyzers in dependency order.

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
use crate::Compiler::VersionControl::VersionConstraints;
use crate::Compiler::ImportsResolution::ImportsResolver;
use crate::ErrorManager::{DebugConfig, ErrorManager};
use crate::Builtins::Static::enum_object;

pub struct GeneralSemanticAnalyzer<'a> {
    ast: &'a DixScript,
    operational_settings: &'a OperationalSettings,
    symbol_table: SymbolTable,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    enums_analyzer: Option<EnumsSectionAnalyzer<'a>>,
    dlm_analyzer: Option<DlmSectionAnalyzer<'a>>,
    security_analyzer: Option<SecuritySectionAnalyzer<'a>>,
    quickfuncs_analyzer: Option<QuickFuncsSectionAnalyzer<'a>>,
    data_analyzer: Option<DataSectionAnalyzer<'a>>,
    analysis_result: SemanticAnalysisResult,
    stopwatch: Instant,
    skip_validation: bool,
}

impl<'a> GeneralSemanticAnalyzer<'a> {
    pub fn new(
        ast: &'a DixScript,
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(error_manager.get_debug_mode());

        GeneralSemanticAnalyzer {
            ast,
            operational_settings,
            symbol_table: SymbolTable::new(),
            error_manager,
            debug_config,
            enums_analyzer: None,
            dlm_analyzer: None,
            security_analyzer: None,
            quickfuncs_analyzer: None,
            data_analyzer: None,
            analysis_result: SemanticAnalysisResult::new(),
            stopwatch: Instant::now(),
            skip_validation: false,
        }
    }

    pub fn analyze(mut self) -> SemanticAnalysisResult {
        self.error_manager
            .log_info("Starting General Semantic Analysis v1.0.0");

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Error Handling Strategy: {:?}",
                self.operational_settings.error_handling_strategy
            ));
            self.error_manager.log_debug(&format!(
                "Compatibility Mode: {:?}",
                self.operational_settings.compatibility_mode
            ));
            self.error_manager.log_debug(&format!(
                "Advanced Mode: {}",
                self.operational_settings.is_advanced_mode()
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
            self.error_manager.log_debug(&format!(
                "Semantic analysis complete. Success: {}",
                self.analysis_result.is_success
            ));
            self.error_manager.log_debug(&format!(
                "Total errors: {}, Warnings: {}",
                self.analysis_result.errors.len(),
                self.analysis_result.warnings.len()
            ));
        }

        self.finalize_result()
    }

    fn analyze_phase1_version(&mut self) -> bool {
        self.error_manager.log_info("Phase 1: version compatibility check");

        let validation = VersionConstraints::new().validate_script(self.ast);

        if !validation.is_valid {
            self.error_manager.log_error(&format!(
                "Version validation failed with {} errors",
                validation.errors.len()
            ));

            for error in &validation.errors {
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
                    self.error_manager
                        .log_debug("No imports — skipping imports semantic analysis");
                }
                return true;
            }
        };

        self.error_manager
            .log_info("Phase 2: IMPORTS semantic analysis");

        if !self.operational_settings.is_feature_enabled("imports")
            && !self.operational_settings.is_advanced_mode()
        {
            self.error_manager
                .log_error("IMPORTS section found but imports feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id: "SEM_FEATURE".to_string(),
                error_type: "FeatureNotEnabled".to_string(),
                message: "IMPORTS section requires 'imports' feature or advanced mode".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion: "Add 'imports' to features in CONFIG or enable advanced mode"
                    .to_string(),
                position: None,
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

        let mut imports_analyzer = ImportsSectionAnalyzer::new(
            &self.symbol_table,
            self.operational_settings,
            current_file_path,
        );

        imports_analyzer.analyze(Some(imports));
        drop(imports_analyzer);

        let phase_ok = !self.error_manager.has_errors();
        let mut result = SectionAnalysisResult::new("IMPORTS_SEMANTIC");
        result.is_success = phase_ok;

        if !phase_ok && !had_errors_before {
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id: "SEM_IMPORTS_SEM".to_string(),
                error_type: "ImportsSemantic".to_string(),
                message: "IMPORTS section semantic validation failed".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion: String::new(),
                position: None,
            });
        }

        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug(&format!("Phase 2 complete: success={}", phase_ok));
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
                self.error_manager
                    .log_debug("Skipping imports resolution (imported file context)");
            }
            return true;
        }

        let imports = match &self.ast.imports {
            Some(s) if !s.imports.is_empty() => s,
            _ => {
                if self.debug_config.is_enabled {
                    self.error_manager
                        .log_debug("No imports — skipping resolution");
                }
                return true;
            }
        };

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 3: resolving {} imports",
                imports.imports.len()
            ));
        }

        if !self.operational_settings.is_feature_enabled("imports")
            && !self.operational_settings.is_advanced_mode()
        {
            self.error_manager
                .log_error("IMPORTS section found but imports feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id: "SEM_FEATURE".to_string(),
                error_type: "FeatureNotEnabled".to_string(),
                message: "IMPORTS section requires 'imports' feature or advanced mode".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion: "Add 'imports' to features in CONFIG or enable advanced mode"
                    .to_string(),
                position: None,
            });
            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
            return true;
        }

        let parsed_imports = HashMap::new();

        let mut imports_resolver =
            ImportsResolver::new(&mut self.symbol_table, self.operational_settings);

        let resolve_success = imports_resolver.resolve_imports(&parsed_imports);

        if !resolve_success {
            self.error_manager.log_error("Import resolution failed");

            for error in self.error_manager.get_imports_resolution_errors() {
                self.analysis_result.errors.push(SemanticErrorInfo {
                    error_id: error.error_id.clone(),
                    error_type: format!("{:?}", error.error_type),
                    message: error.message.clone(),
                    section_name: "IMPORTS".to_string(),
                    suggestion: error.suggestion.clone().unwrap_or_default(),
                    position: Some(Position::new(
                        error.line as usize,
                        error.column as usize,
                    )),
                });
            }

            if self.should_terminate() {
                self.analysis_result.is_success = false;
                return false;
            }
        } else if self.debug_config.is_enabled {
            let stats = imports_resolver.get_statistics();
            self.error_manager
                .log_debug(&format!("Imports resolved: {}", stats));
        }

        self.error_manager.log_info("Phase 3 complete");
        true
    }

    fn analyze_phase4_foundation(&mut self) -> bool {
        self.error_manager.log_info("Phase 4: foundation sections (ENUMS)");

        let enums = match &self.ast.enums {
            Some(s) => s,
            None => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No ENUMS section");
                }
                return true;
            }
        };

        if !self.operational_settings.is_feature_enabled("enums")
            && !self.operational_settings.is_advanced_mode()
        {
            self.error_manager
                .log_error("ENUMS section found but enums feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id: "SEM_FEATURE".to_string(),
                error_type: "FeatureNotEnabled".to_string(),
                message: "ENUMS section requires 'enums' feature or advanced mode".to_string(),
                section_name: "ENUMS".to_string(),
                suggestion: "Add 'enums' to features in CONFIG or enable advanced mode"
                    .to_string(),
                position: None,
            });
            if self.should_terminate() {
                return false;
            }
            return true;
        }

        if self.enums_analyzer.is_none() {
            self.enums_analyzer = Some(EnumsSectionAnalyzer::new(self.operational_settings));
        }

        let result = self
            .enums_analyzer
            .as_mut()
            .unwrap()
            .analyze(enums, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 4 complete: success={} errors={} warnings={}",
                result.is_success,
                result.errors.len(),
                result.warnings.len()
            ));
            for e in &result.errors {
                self.error_manager.log_error(&format!(
                    "  ENUMS [{}] {}: {}",
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

    fn analyze_phase5_functions(&mut self) -> bool {
        self.error_manager
            .log_info("Phase 5: function definitions (QUICKFUNCS)");

        let quickfuncs = match &self.ast.quick_functions {
            Some(s) => s,
            None => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No QUICKFUNCS section");
                }
                return true;
            }
        };

        if !self.operational_settings.is_feature_enabled("quickfuncs")
            && !self.operational_settings.is_advanced_mode()
        {
            self.error_manager
                .log_error("QUICKFUNCS section found but quickfuncs feature not enabled");
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id: "SEM_FEATURE".to_string(),
                error_type: "FeatureNotEnabled".to_string(),
                message: "QUICKFUNCS section requires 'quickfuncs' feature or advanced mode"
                    .to_string(),
                section_name: "QUICKFUNCS".to_string(),
                suggestion: "Add 'quickfuncs' to features in CONFIG or enable advanced mode"
                    .to_string(),
                position: None,
            });
            if self.should_terminate() {
                return false;
            }
            return true;
        }

        if self.quickfuncs_analyzer.is_none() {
            self.quickfuncs_analyzer =
                Some(QuickFuncsSectionAnalyzer::new(self.operational_settings));
        }

        let result = self
            .quickfuncs_analyzer
            .as_mut()
            .unwrap()
            .analyze(quickfuncs, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 5 complete: success={} errors={} warnings={} qi_resolutions={}",
                result.is_success,
                result.errors.len(),
                result.warnings.len(),
                result.qualified_id_resolutions.len()
            ));
            for e in &result.errors {
                self.error_manager.log_error(&format!(
                    "  QUICKFUNCS [{}] {}: {}",
                    e.error_id, e.error_type, e.message
                ));
                if !e.suggestion.is_empty() {
                    self.error_manager
                        .log_error(&format!("    -> {}", e.suggestion));
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

    fn analyze_phase6_independent(&mut self) {
        self.error_manager
            .log_info("Phase 6: independent sections (DLM)");

        let dlm = match &self.ast.dlm {
            Some(s) => s,
            None => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No DLM section");
                }
                return;
            }
        };

        if self.dlm_analyzer.is_none() {
            self.dlm_analyzer = Some(DlmSectionAnalyzer::new(self.operational_settings));
        }

        let result = self
            .dlm_analyzer
            .as_mut()
            .unwrap()
            .analyze(dlm, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 6 complete: success={} errors={} warnings={}",
                result.is_success,
                result.errors.len(),
                result.warnings.len()
            ));
        }

        self.add_section_result("DLM", result);
    }

    fn analyze_phase7_data_driven(&mut self) -> bool {
        self.error_manager.log_info("Phase 7: DATA section");

        let data = match &self.ast.data {
            Some(s) => s,
            None => {
                self.error_manager.log_warning("No DATA section present");
                return true;
            }
        };

        if self.data_analyzer.is_none() {
            self.data_analyzer = Some(DataSectionAnalyzer::new(self.operational_settings));
        }

        let result = self
            .data_analyzer
            .as_mut()
            .unwrap()
            .analyze(data, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 7 complete: success={} errors={} warnings={}",
                result.is_success,
                result.errors.len(),
                result.warnings.len()
            ));
            for e in &result.errors {
                self.error_manager.log_error(&format!(
                    "  DATA [{}] {}: {}",
                    e.error_id, e.error_type, e.message
                ));
                if !e.suggestion.is_empty() {
                    self.error_manager
                        .log_error(&format!("    -> {}", e.suggestion));
                }
            }
            for w in &result.warnings {
                self.error_manager.log_warning(&format!(
                    "  DATA WARN [{}]: {}",
                    w.warning_id, w.message
                ));
            }
        }

        let phase_ok = result.is_success;
        self.add_section_result("DATA", result);

        if let Some(ref da) = self.data_analyzer {
            let (short_name_idx, type_idx) = da.get_indexes();

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
                    type_idx.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                );
            }
        }

        if !phase_ok && self.should_terminate() {
            self.analysis_result.is_success = false;
            return false;
        }

        true
    }

    fn analyze_phase8_generated(&mut self) {
        self.error_manager
            .log_info("Phase 8: generated sections (SECURITY)");

        let requires_security = self
            .ast
            .dlm
            .as_ref()
            .map(|dlm| {
                dlm.modules
                    .iter()
                    .any(|m| matches!(m.module_type, DLMModuleType::DEncryptor))
            })
            .unwrap_or(false);

        let security = match &self.ast.security {
            Some(s) => s,
            None => {
                if requires_security {
                    self.error_manager.log_error(
                        "SECURITY section required when using DEncryptor but not present",
                    );
                    self.analysis_result.errors.push(SemanticErrorInfo {
                        error_id: "SEM0002".to_string(),
                        error_type: "MissingSection".to_string(),
                        message: "SECURITY section required when DEncryptor is used in @DLM"
                            .to_string(),
                        section_name: "SECURITY".to_string(),
                        suggestion: "Add @SECURITY section with encryption configuration"
                            .to_string(),
                        position: None,
                    });
                } else if self.debug_config.is_enabled {
                    self.error_manager
                        .log_debug("No SECURITY section (not required without DEncryptor)");
                }
                return;
            }
        };

        if self.security_analyzer.is_none() {
            self.security_analyzer =
                Some(SecuritySectionAnalyzer::new(self.operational_settings));
        }

        let result = self
            .security_analyzer
            .as_mut()
            .unwrap()
            .analyze(security, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 8 complete: success={} errors={} warnings={}",
                result.is_success,
                result.errors.len(),
                result.warnings.len()
            ));
        }

        self.add_section_result("SECURITY", result);
    }

    fn register_enums_with_builtin_system(&mut self) {
        self.error_manager
            .log_info("Registering enums with builtin system");
        enum_object::clear_enums();

        let enum_count = self.symbol_table.enums.len();
        let mut registered = 0usize;

        for (name, fields) in &self.symbol_table.enums {
            enum_object::register_enum(name.clone(), fields.clone());
            registered += 1;
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "  Registered enum: {} ({} fields)",
                    name,
                    fields.len()
                ));
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Enum registration: {}/{} registered",
                registered, enum_count
            ));
        }
    }

    fn initialize_builtin_registries(&mut self) {
        for name in &["Math", "Dix", "DateTime", "Array", "Random", "Enum", "Guid", "Ip"] {
            self.symbol_table.add_builtin_static_object(name.to_string());
        }

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

        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug("Built-in registries initialized");
        }
    }

    fn add_section_result(&mut self, section_name: &str, result: SectionAnalysisResult) {
        if !result.is_success && self.debug_config.is_enabled {
            self.error_manager.log_warning(&format!(
                "Section {} analysis completed with errors",
                section_name
            ));
        }
        self.analysis_result.errors.extend(result.errors.clone());
        self.analysis_result.warnings.extend(result.warnings.clone());
        self.analysis_result
            .section_results
            .insert(section_name.to_string(), result);
    }

    #[inline]
    fn should_terminate(&self) -> bool {
        !self.analysis_result.errors.is_empty()
            && self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
    }

    fn finalize_result(mut self) -> SemanticAnalysisResult {
        self.analysis_result.is_success = self.analysis_result.errors.is_empty();
        self.analysis_result.analysis_duration = self.stopwatch.elapsed();

        let ms = self.analysis_result.analysis_duration.as_secs_f64() * 1000.0;
        self.error_manager
            .log_info(&format!("Analysis duration: {:.2}ms", ms));

        self.analysis_result.symbol_table = Some(self.symbol_table);
        self.analysis_result
    }
}