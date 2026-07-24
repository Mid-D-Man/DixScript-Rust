//! Central semantic analysis orchestrator — runs all section analyzers in dependency order.

use std::collections::HashMap;
use web_time::Instant;
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
    analysis_result:      SemanticAnalysisResult,
    stopwatch:            Instant,
    skip_validation:      bool,
    has_imports_enabled:    bool,
    has_enums_enabled:      bool,
    has_quickfuncs_enabled: bool,
    has_dlm_enabled:        bool,
    propagate_error_manager: bool,
    /// True only when this analyzer was constructed via
    /// `new_with_seed_namespaces` — i.e. it's analyzing an *imported* file
    /// on behalf of `ImportsResolver`, nested inside some outer file's own
    /// `analyze()` call. Used to gate `register_enums_with_builtin_system()`
    /// so the process-global enum registry (`enum_object::DIXSCRIPT_ENUMS`)
    /// is only cleared/repopulated once, by the outermost compile, instead
    /// of once per file in the import tree. See that function for details.
    is_nested_import_analysis: bool,
}

impl<'a> GeneralSemanticAnalyzer<'a> {

    pub fn new(
        ast:                  &'a DixScript,
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let mut s = Self::build(ast, operational_settings, ErrorManager::get_shared_instance());
        s.propagate_error_manager = false;
        s
    }

    pub fn new_with_error_manager(
        ast:                  &'a DixScript,
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
    ) -> Self {
        let mut s = Self::build(ast, operational_settings, error_manager);
        s.propagate_error_manager = true;
        s
    }

    pub fn new_for_lsp(
        ast:                  &'a DixScript,
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
    ) -> Self {
        Self::new_with_error_manager(ast, operational_settings, error_manager)
    }

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
            is_nested_import_analysis: true,
        }
    }

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
            propagate_error_manager: true,
            is_nested_import_analysis: false,
        }
    }

    /// Returns a cloned EM (LSP) or the shared singleton (CLI).
    /// MUST be stored into a local before any mutable borrow of self.
    fn make_error_manager(&self) -> ErrorManager {
        if self.propagate_error_manager {
            self.error_manager.clone()
        } else {
            ErrorManager::get_shared_instance()
        }
    }

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

        // Only the outermost compile touches the process-global enum
        // registry. `ImportsResolver` recursively runs a full `analyze()`
        // on every imported file (see `new_with_seed_namespaces`); if each
        // of those nested calls also cleared+repopulated the registry, the
        // last file processed in the import tree would wipe out every enum
        // registered by files processed earlier in the SAME compile — which
        // is exactly what was making `Enum.*` builtin calls fail to find
        // imported enums. The outermost call (this branch) runs last in
        // program order relative to its own imports (Phase 3 already
        // finished above) and has full transitive knowledge of both its own
        // local enums *and* every imported namespace's enums, so it alone
        // is responsible for the registry's contents.
        if !self.is_nested_import_analysis {
            self.register_enums_with_builtin_system();
        }

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
        }
        true
    }

    fn analyze_phase2_imports_semantic(&mut self) -> bool {
        let imports = match &self.ast.imports {
            Some(s) if !s.imports.is_empty() => s,
            _ => return true,
        };

        if !self.has_imports_enabled {
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "IMPORTS section requires 'imports' feature or advanced mode".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion:   "Add 'imports' to features in CONFIG or enable advanced mode".to_string(),
                position:     None,
            });
            if self.should_terminate() { self.analysis_result.is_success = false; return false; }
            return true;
        }

        let current_file_path = self.operational_settings.source_file_path.as_deref().unwrap_or("");
        let had_errors_before = self.error_manager.has_errors();

        // Hoist EM before any borrow of self fields.
        let em = self.make_error_manager();
        let mut imports_analyzer = ImportsSectionAnalyzer::new_with_error_manager(
            &self.symbol_table,
            self.operational_settings,
            current_file_path,
            em,
        );
        imports_analyzer.analyze(Some(imports));
        drop(imports_analyzer);

        let phase_ok = !self.error_manager.has_errors();
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

        self.add_section_result("IMPORTS_SEMANTIC", result);
        if !phase_ok && self.should_terminate() { self.analysis_result.is_success = false; return false; }
        true
    }

    fn analyze_phase3_imports_resolution(&mut self) -> bool {
        if self.operational_settings.skip_imports_resolution { return true; }

        let imports = match &self.ast.imports {
            Some(s) if !s.imports.is_empty() => s,
            _ => return true,
        };

        if !self.has_imports_enabled {
            self.analysis_result.errors.push(SemanticErrorInfo {
                error_id:     "SEM_FEATURE".to_string(),
                error_type:   "FeatureNotEnabled".to_string(),
                message:      "IMPORTS section requires 'imports' feature or advanced mode".to_string(),
                section_name: "IMPORTS".to_string(),
                suggestion:   "Add 'imports' to features in CONFIG or enable advanced mode".to_string(),
                position:     None,
            });
            if self.should_terminate() { self.analysis_result.is_success = false; return false; }
            return true;
        }

        let base_dir = self.operational_settings.source_file_path.as_deref()
            .and_then(|p| std::path::Path::new(p).parent())
            .and_then(|p| p.to_str())
            .unwrap_or(".");

        // Hoist EM BEFORE mutable borrow of symbol_table.
        let em = self.make_error_manager();
        let mut imports_resolver = ImportsResolver::new_with_error_manager(
            &mut self.symbol_table,
            self.operational_settings,
            em,
        );

        let resolve_success = imports_resolver.resolve_from_imports_section(imports, base_dir);

        if !resolve_success {
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
            if self.should_terminate() { self.analysis_result.is_success = false; return false; }
        }
        true
    }

    fn analyze_phase4_foundation(&mut self) -> bool {
        let enums = match &self.ast.enums {
            Some(s) => s,
            None    => return true,
        };

        if !self.has_enums_enabled {
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

        let em = self.make_error_manager();
        let mut analyzer = EnumsSectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            em,
        );
        let result = analyzer.analyze(enums, &mut self.symbol_table);
        let phase_ok = result.is_success;
        self.add_section_result("ENUMS", result);
        if !phase_ok && self.should_terminate() { return false; }
        true
    }

    fn analyze_phase5_functions(&mut self) -> bool {
        let quickfuncs = match &self.ast.quick_functions {
            Some(s) => s,
            None    => return true,
        };

        if !self.has_quickfuncs_enabled {
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

        let em = self.make_error_manager();
        let mut analyzer = QuickFuncsSectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            em,
        );
        let result = analyzer.analyze(quickfuncs, &mut self.symbol_table);

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Phase 5 complete: success={} errors={} warnings={} qi_resolutions={}",
                result.is_success, result.errors.len(), result.warnings.len(),
                result.qualified_id_resolutions.len()
            ));
        }

        let phase_ok = result.is_success;
        self.add_section_result("QUICKFUNCS", result);
        if !phase_ok && self.should_terminate() { return false; }
        true
    }

    fn analyze_phase6_independent(&mut self) {
        let dlm = match &self.ast.dlm {
            Some(s) => s,
            None    => return,
        };

        let em = self.make_error_manager();
        let mut analyzer = DlmSectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            em,
        );
        let result = analyzer.analyze(dlm, &mut self.symbol_table);
        self.add_section_result("DLM", result);
    }

    fn analyze_phase7_data_driven(&mut self) -> bool {
        let data = match &self.ast.data {
            Some(s) => s,
            None    => return true,
        };

        let em = self.make_error_manager();
        let mut analyzer = DataSectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            em,
        );
        let result = analyzer.analyze(data, &mut self.symbol_table);
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
        if !phase_ok && self.should_terminate() { self.analysis_result.is_success = false; return false; }
        true
    }

    fn analyze_phase8_generated(&mut self) {
        let requires_security = self.ast.dlm.as_ref()
            .map(|dlm| dlm.modules.iter().any(|m| matches!(m.module_type, DLMModuleType::DEncryptor)))
            .unwrap_or(false);

        let security = match &self.ast.security {
            Some(s) => s,
            None => {
                if requires_security {
                    self.analysis_result.errors.push(SemanticErrorInfo {
                        error_id:     "SEM0002".to_string(),
                        error_type:   "MissingSection".to_string(),
                        message:      "SECURITY section required when DEncryptor is used in @DLM".to_string(),
                        section_name: "SECURITY".to_string(),
                        suggestion:   "Add @SECURITY section with encryption configuration".to_string(),
                        position:     None,
                    });
                }
                return;
            }
        };

        let em = self.make_error_manager();
        let mut analyzer = SecuritySectionAnalyzer::new_with_error_manager(
            self.operational_settings,
            em,
        );
        let result = analyzer.analyze(security, &mut self.symbol_table);
        self.add_section_result("SECURITY", result);
    }

    /// Populates the process-global `enum_object::DIXSCRIPT_ENUMS` registry
    /// that backs the `Enum.*` builtin static object (`Enum.getValues`,
    /// `Enum.getValue`, `Enum.getName`, etc.) for this compile.
    ///
    /// MUST only run from the outermost (non-nested) `analyze()` call — see
    /// the `is_nested_import_analysis` gate at the call site. `clear_enums()`
    /// still runs unconditionally first, exactly like before: that's what
    /// keeps a stale registry from a *previous, unrelated* compile from
    /// leaking into this one (the fix that made repeated/fuzz compiles in
    /// the same process safe). What changed is WHEN this runs (once per
    /// compile, not once per file in the import tree) and WHAT it registers.
    ///
    /// Two kinds of enums go in, under two different naming schemes:
    /// - This file's own local `@ENUMS` entries, under their bare name
    ///   (`"Status"`), matching how they're written in this file's own DATA
    ///   section and QUICKFUNCS.
    /// - Every imported namespace's enums, under their qualified
    ///   `"Alias.EnumName"` form (`"EnumMan.Suka"`), matching the exact
    ///   qualification the parser gives `Value::EnumValue.enum_name` for
    ///   `Namespace.Enum.FIELD` data literals and that `ValueResolver`'s
    ///   import merge uses. Keeping both sides of the compiler agreeing on
    ///   one naming scheme is the whole point — an imported enum is *not*
    ///   the same symbol as a local one with the same short name, so it
    ///   can't share a bare-name slot in the registry.
    fn register_enums_with_builtin_system(&mut self) {
        enum_object::clear_enums();

        for (name, fields) in &self.symbol_table.enums {
            enum_object::register_enum(name.clone(), fields.clone());
        }

        let mut imported_count = 0usize;
        for ns in self.symbol_table.namespaces.values() {
            for (enum_name, fields) in &ns.enums {
                let qualified = format!("{}.{}", ns.alias, enum_name);
                enum_object::register_enum(qualified, fields.clone());
                imported_count += 1;
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Registered {} local + {} imported enum(s) with builtin system",
                self.symbol_table.enums.len(),
                imported_count
            ));
        }
    }

    fn initialize_builtin_registries(&mut self) {
        for name in &["Math", "Dix", "DateTime", "Array", "Random", "Enum", "Guid", "Ip"] {
            self.symbol_table.add_builtin_static_object(name.to_string());
        }
        self.symbol_table.add_dix_function("logEvent".to_string(), "void".to_string(), vec!["string".to_string()]);
        self.symbol_table.add_dix_function("getSystemInfo".to_string(), "object".to_string(), vec![]);
        self.symbol_table.add_dix_function("validateConfig".to_string(), "bool".to_string(), vec!["string".to_string()]);
    }

    fn add_section_result(&mut self, section_name: &str, result: SectionAnalysisResult) {
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
        self.analysis_result.symbol_table      = Some(self.symbol_table);
        self.analysis_result
    }
                          }
