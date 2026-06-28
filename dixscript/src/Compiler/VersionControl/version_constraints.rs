//! Version Constraints - Validates DixScript features against version requirements
//! Uses native Rust collections (no DixCore wrappers)

use std::collections::{HashMap, HashSet};
use crate::Compiler::AST::*;
use crate::Compiler::VersionControl::{VersionManager, ForwardCompatibilityManager};
use crate::Compiler::Extensions::TypeSystemManager;
use crate::Builtins::Resolver::static_object_registry;
use crate::Builtins::Resolver::instance_method_registry;
use crate::Builtins::Core::DixType;

/// Defines constraints and limitations based on DixScript version.
/// Singleton instance coordinated with VersionManager.
pub struct VersionConstraints {
    version_manager:       &'static std::sync::RwLock<VersionManager>,
    forward_compat_manager: Option<ForwardCompatibilityManager>,
}

impl VersionConstraints {
    /// Create new VersionConstraints instance
    pub fn new() -> Self {
        VersionConstraints {
            version_manager:        VersionManager::instance(),
            forward_compat_manager: None,
        }
    }

    /// Create with forward compatibility manager
    pub fn with_forward_compat(forward_compat: ForwardCompatibilityManager) -> Self {
        VersionConstraints {
            version_manager:        VersionManager::instance(),
            forward_compat_manager: Some(forward_compat),
        }
    }

    // ==================== TYPE VALIDATION ====================

    /// Checks if a value type is valid for the current version
    pub fn is_valid_value_type(&self, value_type: &str) -> bool {
        if value_type.is_empty() {
            return false;
        }
        let supported_types = TypeSystemManager::get_supported_types();
        supported_types.contains(&value_type.to_lowercase().as_str())
    }

    /// Checks if a default value is compatible with a parameter type
    pub fn is_valid_default_value(&self, param_type: DataType, value_type: DataType) -> bool {
        TypeSystemManager::can_convert(value_type, param_type)
    }

    /// Validates type annotation compatibility
    pub fn is_valid_type_annotation(&self, data_type: Option<DataType>, _value: &Value) -> bool {
        if data_type.is_none() {
            return true;
        }
        // TypeSystemManager handles detailed validation
        true
    }

    // ==================== DLM MODULE VALIDATION ====================

    /// Checks if a DLM module type and subtype combination is valid
    pub fn is_valid_dlm_module(
        &self,
        module_type: DLMModuleType,
        subtype: Option<DLMModuleSubtype>,
    ) -> bool {
        let manager = self.version_manager.read().unwrap();
        if !manager.supports_feature("dlm_section") {
            return false;
        }

        match (module_type, subtype) {
            (DLMModuleType::DCompressor, Some(DLMModuleSubtype::Gzip))
            | (DLMModuleType::DCompressor, Some(DLMModuleSubtype::Bzip2))
            | (DLMModuleType::DCompressor, Some(DLMModuleSubtype::Lzma))
            | (DLMModuleType::DCompressor, None) => true,

            (DLMModuleType::DAuditor, Some(DLMModuleSubtype::Diy))
            | (DLMModuleType::DAuditor, Some(DLMModuleSubtype::Enhanced))
            | (DLMModuleType::DAuditor, None) => true,

            (DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Aes128))
            | (DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Chacha20))
            | (DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Xor))
            | (DLMModuleType::DEncryptor, Some(DLMModuleSubtype::Aes256))
            | (DLMModuleType::DEncryptor, None) => true,

            (DLMModuleType::ParseError, Some(DLMModuleSubtype::ParseError))
            | (DLMModuleType::ParseError, None) => true,

            _ => false,
        }
    }

    // ==================== SECTION VALIDATION ====================

    /// Validates if a section type is valid for the current version
    pub fn is_valid_section_type(&self, section_type: &str) -> bool {
        if section_type.is_empty() {
            return false;
        }

        let normalized_section = section_type
            .to_uppercase()
            .trim_start_matches('@')
            .to_string();

        let supported_sections = &[
            "CONFIG", "IMPORTS", "DLM", "ENUMS", "QUICKFUNCS", "DATA", "SECURITY",
        ];

        if supported_sections.contains(&normalized_section.as_str()) {
            let manager = self.version_manager.read().unwrap();
            return manager.supports_section_type(&normalized_section);
        }

        // Try forward compatibility for unknown sections
        if let Some(ref fc_manager) = self.forward_compat_manager {
            let result =
                fc_manager.handle_unknown_element("section", &normalized_section, None, None);
            return result.is_ok();
        }

        false
    }

    /// Checks if a section requires advanced mode (informational — not used for gating)
    pub fn requires_advanced_mode(&self, section_type: &str) -> bool {
        let normalized = section_type
            .to_uppercase()
            .trim_start_matches('@')
            .to_string();
        matches!(normalized.as_str(), "QUICKFUNCS" | "ENUMS" | "IMPORTS")
    }

    // ==================== FEATURE CONTROL VALIDATION ====================

    /// Validates feature control configuration
    pub fn is_valid_feature_control(&self, feature_value: &str) -> bool {
        let manager = self.version_manager.read().unwrap();
        if !manager.supports_feature("feature_control") {
            return false;
        }
        match feature_value.to_lowercase().as_str() {
            "basic" | "advanced" => true,
            _ => self.is_valid_section_list(feature_value),
        }
    }

    /// Validates debug mode configuration
    pub fn is_valid_debug_mode(&self, debug_value: &str) -> bool {
        let manager = self.version_manager.read().unwrap();
        if !manager.supports_feature("debug_modes") {
            return false;
        }
        matches!(
            debug_value.to_lowercase().as_str(),
            "off" | "regular" | "verbose"
        )
    }

    /// Validates section list format (comma-separated specific features)
    fn is_valid_section_list(&self, section_list: &str) -> bool {
        if section_list.is_empty() {
            return false;
        }
        let valid_sections: HashSet<&str> =
            ["quickfuncs", "enums", "imports", "dlm", "data", "security"]
                .iter()
                .copied()
                .collect();

        section_list
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .all(|s| valid_sections.contains(s.as_str()))
    }

    // ==================== FUNCTION AND EXPRESSION VALIDATION ====================

    /// Validates if a function type is valid for the current version
    pub fn is_valid_function_type(&self, function_type: &str) -> bool {
        if function_type.is_empty() {
            return false;
        }
        let manager = self.version_manager.read().unwrap();
        match function_type.to_lowercase().as_str() {
            "quickfunc" => manager.supports_feature("quickfuncs_section"),
            "imported"  => manager.supports_feature("imported_function_calls"),
            _           => self.handle_unknown_feature("function", function_type),
        }
    }

    /// Validates QuickFunction signature — delegates to TypeSystemManager
    pub fn validate_quick_function_signature(&self, function: &QuickFunction) -> Vec<String> {
        match TypeSystemManager::validate_quick_function_signature(function) {
            Ok(_)       => Vec::new(),
            Err(errors) => errors,
        }
    }

    // ==================== BUILT-IN SYSTEM VALIDATION ====================

    /// Validates static method call
    pub fn is_valid_static_method_call(&self, object_name: &str, _method_name: &str) -> bool {
        let manager = self.version_manager.read().unwrap();
        if !manager.supports_feature("static_object_registry") {
            return false;
        }
        static_object_registry::has_static_object(object_name)
    }

    /// Validates instance method call on a type
    pub fn is_valid_instance_method_call(&self, type_name: DixType, method_name: &str) -> bool {
        let manager = self.version_manager.read().unwrap();
        if !manager.supports_feature("instance_method_registry") {
            return false;
        }
        instance_method_registry::has_instance_method(type_name, method_name)
    }

    /// Validates Dix function call
    pub fn is_valid_dix_function_call(&self, function_name: &str) -> bool {
        let manager = self.version_manager.read().unwrap();
        if !manager.supports_feature("dix_function_calls") {
            return false;
        }
        let valid_dix_functions: HashSet<&str> =
            ["logEvent", "getSystemInfo", "validateConfig"]
                .iter()
                .copied()
                .collect();
        valid_dix_functions.contains(function_name)
    }

    // ==================== DATA SECTION VALIDATION ====================

    /// Validates data entry based on current version and whether quickfuncs are enabled.
    ///
    /// `quickfuncs_enabled` — true when "advanced" or "quickfuncs" is in the feature set.
    /// QuickFunc calls inside @DATA are only valid when that feature is available.
    pub fn validate_data_entry(
        &self,
        entry: &DataEntry,
        quickfuncs_enabled: bool,
    ) -> Vec<String> {
        match entry {
            DataEntry::SimpleProperty { value, name, .. } => {
                self.validate_simple_property(value, name, quickfuncs_enabled)
            }
            DataEntry::TableProperty { properties, path, .. } => {
                self.validate_table_property(properties, path, quickfuncs_enabled)
            }
            DataEntry::GroupArray { items, path, .. } => {
                self.validate_group_array(items, path, quickfuncs_enabled)
            }
            DataEntry::ObjectProperty { .. } => {
                Vec::new() // No special validation needed
            }
        }
    }

    fn validate_simple_property(
        &self,
        value: &Value,
        name: &str,
        quickfuncs_enabled: bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        if matches!(value, Value::QuickFuncCall { .. }) && !quickfuncs_enabled {
            errors.push(format!(
                "QuickFunction calls in DATA require 'advanced' or 'quickfuncs' feature \
                 (property: {})",
                name
            ));
        }
        errors
    }

    fn validate_table_property(
        &self,
        properties: &[PropertyAssignment],
        path: &TablePath,
        quickfuncs_enabled: bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        if path.segments.is_empty() {
            errors.push("Table property must have a valid path".to_string());
            return errors;
        }
        for assignment in properties {
            if matches!(assignment.value, Value::QuickFuncCall { .. }) && !quickfuncs_enabled {
                errors.push(format!(
                    "QuickFunction calls in DATA require 'advanced' or 'quickfuncs' feature \
                     (property: {}.{})",
                    path, assignment.name
                ));
            }
        }
        errors
    }

    fn validate_group_array(
        &self,
        items: &[Value],
        path: &TablePath,
        quickfuncs_enabled: bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        if path.segments.is_empty() {
            errors.push("Group array must have a valid path".to_string());
            return errors;
        }
        for item in items {
            if matches!(item, Value::QuickFuncCall { .. }) && !quickfuncs_enabled {
                errors.push(format!(
                    "QuickFunction calls in DATA require 'advanced' or 'quickfuncs' feature \
                     (group array: {})",
                    path
                ));
            }
        }
        errors
    }

    // ==================== CONFIG SECTION VALIDATION ====================

    /// Validates CONFIG section entries
    pub fn validate_config_section(&self, config: &ConfigSection) -> Vec<String> {
        let mut errors = Vec::new();
        for entry in &config.entries {
            errors.extend(self.validate_config_entry(entry));
        }
        errors
    }

    /// Validates individual config entry
    pub fn validate_config_entry(&self, entry: &ConfigEntry) -> Vec<String> {
        let mut errors = Vec::new();
        let manager = self.version_manager.read().unwrap();

        match entry.key.to_lowercase().as_str() {
            "version" => {
                if let ConfigValue::String(ref version) = entry.value {
                    if !manager.is_compatible_with(version) {
                        errors.push(format!("Unsupported version: {}", version));
                    }
                } else {
                    errors.push("Version must be a string value".to_string());
                }
            }

            "features" => {
                match &entry.value {
                    ConfigValue::String(ref value) => {
                        if !self.is_valid_feature_control(value) {
                            errors.push(format!("Invalid feature control value: {}", value));
                        }
                    }
                    ConfigValue::Features(ref features) => {
                        if features.len() == 1 {
                            let f = features[0].to_lowercase();
                            if f != "advanced" && f != "basic" {
                                if !self.is_valid_section_list(&features.join(",")) {
                                    errors.push("Invalid feature list".to_string());
                                }
                            }
                        } else {
                            if !self.is_valid_section_list(&features.join(",")) {
                                errors.push("Invalid feature list".to_string());
                            }
                        }
                    }
                    _ => {
                        errors.push("Features must be a string or feature value".to_string());
                    }
                }
            }

            "debug_mode" => {
                match &entry.value {
                    ConfigValue::String(ref value) => {
                        if !self.is_valid_debug_mode(value) {
                            errors.push(format!("Invalid debug mode: {}", value));
                        }
                    }
                    ConfigValue::Debug(_) => {
                        // Already validated as DebugMode enum — nothing to do
                    }
                    _ => {
                        errors.push("Debug mode must be a string or DebugValue".to_string());
                    }
                }
            }

            _ => {
                // Other config keys are allowed without extra validation
            }
        }

        errors
    }

    // ==================== HELPER METHODS ====================

    /// Handles unknown features through forward compatibility manager
    fn handle_unknown_feature(&self, feature_type: &str, feature_name: &str) -> bool {
        if let Some(ref fc_manager) = self.forward_compat_manager {
            fc_manager
                .handle_unknown_element(feature_type, feature_name, None, None)
                .is_ok()
        } else {
            false
        }
    }

    /// Extract the active feature set from a @CONFIG section as a lowercase HashSet.
    ///
    /// Rules:
    /// * No @CONFIG present → `{"advanced"}` (matches `OperationalSettings::default()`).
    /// * @CONFIG present but no `features` key → `{"advanced"}`.
    /// * `features -> advanced` → `{"advanced"}`.
    /// * `features -> basic` → `{"basic"}`.
    /// * `features -> quickfuncs,enums` → `{"quickfuncs", "enums"}`.
    ///
    /// All returned strings are lowercase so callers can use plain `HashSet::contains`.
    fn extract_features(&self, config: Option<&ConfigSection>) -> HashSet<String> {
        let mut features = HashSet::new();

        let config = match config {
            Some(c) => c,
            None => {
                // No @CONFIG → default is "advanced" (everything enabled)
                features.insert("advanced".to_string());
                return features;
            }
        };

        for entry in &config.entries {
            if !entry.key.eq_ignore_ascii_case("features") {
                continue;
            }
            match &entry.value {
                ConfigValue::String(ref s) => {
                    for f in s.split(',') {
                        let f = f.trim().to_lowercase();
                        if !f.is_empty() {
                            features.insert(f);
                        }
                    }
                }
                ConfigValue::Features(ref list) => {
                    for f in list {
                        // Already normalised to lowercase by ConfigSchema::create_feature_value,
                        // but .to_lowercase() here guards against any bypass paths.
                        let f = f.trim().to_lowercase();
                        if !f.is_empty() {
                            features.insert(f);
                        }
                    }
                }
                _ => {}
            }
            return features; // found the entry — done scanning
        }

        // "features" key not present in @CONFIG → default "advanced"
        features.insert("advanced".to_string());
        features
    }

    /// Gets all constraints for the current version (for diagnostic/tooling use)
    pub fn get_version_constraints(&self) -> HashMap<String, serde_json::Value> {
        use serde_json::json;
        let manager = self.version_manager.read().unwrap();

        let mut constraints = HashMap::new();
        constraints.insert("Version".to_string(), json!(manager.get_current_version()));
        constraints.insert(
            "SupportedSections".to_string(),
            json!(["CONFIG", "IMPORTS", "DLM", "ENUMS", "QUICKFUNCS", "DATA", "SECURITY"]),
        );
        constraints.insert(
            "RequiresAdvancedModeOrExplicitFeature".to_string(),
            json!(["QUICKFUNCS", "ENUMS", "IMPORTS", "DLM"]),
        );
        constraints.insert(
            "AlwaysAllowed".to_string(),
            json!(["DATA", "SECURITY"]),
        );
        constraints.insert(
            "SupportsFeatureControl".to_string(),
            json!(manager.supports_feature("feature_control")),
        );
        constraints.insert(
            "SupportsBuiltinRegistry".to_string(),
            json!(manager.supports_builtin_registry()),
        );
        constraints.insert(
            "SupportsImports".to_string(),
            json!(manager.supports_feature("imports_section")),
        );
        constraints.insert(
            "SupportedDLMModules".to_string(),
            json!({
                "DCompressor": ["gzip", "bzip2", "lzma"],
                "DAuditor":    ["diy", "enhanced"],
                "DEncryptor":  ["xor", "aes128", "aes256", "chacha20"]
            }),
        );
        constraints.insert(
            "ValidDebugModes".to_string(),
            json!(["off", "regular", "verbose"]),
        );
        constraints.insert(
            "ValidFeatureControls".to_string(),
            json!(["basic", "advanced", "quickfuncs", "enums", "imports", "dlm", "data", "security"]),
        );
        constraints
    }

    // ==================== SCRIPT-LEVEL VALIDATION ====================

    /// Performs comprehensive validation of a DixScript AST against version
    /// constraints and feature requirements.
    ///
    /// Feature gating rules (all case-insensitive):
    /// * `features -> advanced`          → all sections permitted.
    /// * `features -> basic`             → only DATA and SECURITY permitted.
    /// * `features -> quickfuncs`        → QUICKFUNCS permitted; others still blocked.
    /// * `features -> quickfuncs,enums`  → QUICKFUNCS + ENUMS permitted.
    /// * `features -> imports`           → IMPORTS permitted.
    /// * DATA and SECURITY are always permitted regardless of feature mode.
    /// * DLM is gated by the version manager's `dlm_section` feature flag (true in v1.0.0).
    pub fn validate_script(&self, script: &DixScript) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Extract active features as a lowercase HashSet for O(1) lookup.
        let features    = self.extract_features(script.config.as_ref());
        let is_advanced = features.contains("advanced");
        result.is_advanced_mode = is_advanced;

        // ── @CONFIG ──────────────────────────────────────────────────────────
        if let Some(ref config) = script.config {
            let config_errors = self.validate_config_section(config);
            result.errors.extend(
                config_errors.into_iter().map(|e| format!("CONFIG: {}", e)),
            );
        }

        // ── @IMPORTS — requires "advanced" OR explicit "imports" feature ─────
        if script.imports.is_some() {
            let imports_ok = is_advanced || features.contains("imports");
            if !imports_ok {
                result.errors.push(
                    "IMPORTS section requires advanced mode or the 'imports' feature".to_string(),
                );
            }
            // Version-level check (always passes in v1.0.0, guards future versions)
            let manager = self.version_manager.read().unwrap();
            if !manager.supports_feature("imports_section") {
                result.errors.push(
                    "IMPORTS section is not supported in the current version".to_string(),
                );
            }
        }

        // ── @DLM — content validated regardless of user feature mode ─────────
        // DLM availability is controlled by the version manager, not the user's
        // `features` setting, so we validate its contents unconditionally here.
        if let Some(ref dlm) = script.dlm {
            let dlm_errors = self.validate_dlm_section(dlm);
            result.errors.extend(
                dlm_errors.into_iter().map(|e| format!("DLM: {}", e)),
            );
        }

        // ── @ENUMS — requires "advanced" OR explicit "enums" feature ─────────
        if script.enums.is_some() {
            let enums_ok = is_advanced || features.contains("enums");
            if !enums_ok {
                result.errors.push(
                    "ENUMS section requires advanced mode or the 'enums' feature".to_string(),
                );
            }
        }

        // ── @QUICKFUNCS — requires "advanced" OR explicit "quickfuncs" feature
        if let Some(ref quick_functions) = script.quick_functions {
            let qf_ok = is_advanced || features.contains("quickfuncs");
            if !qf_ok {
                result.errors.push(
                    "QUICKFUNCS section requires advanced mode or the 'quickfuncs' feature"
                        .to_string(),
                );
            } else {
                // Only validate signatures when the section is actually permitted.
                let func_errors = self.validate_quick_functions_section(quick_functions);
                result.errors.extend(
                    func_errors.into_iter().map(|e| format!("QUICKFUNCS: {}", e)),
                );
            }
        }

        // ── @DATA — QuickFunc calls inside DATA need quickfuncs to be enabled ─
        // DATA itself is always permitted; only the QuickFunc-call sub-check
        // depends on whether quickfuncs are available.
        if let Some(ref data) = script.data {
            let qf_available = is_advanced || features.contains("quickfuncs");
            let data_errors  = self.validate_data_section(data, qf_available);
            result.errors.extend(
                data_errors.into_iter().map(|e| format!("DATA: {}", e)),
            );
        }

        // @SECURITY — always permitted, no extra validation needed here.

        result.is_valid = result.errors.is_empty();
        result
    }

    // ==================== PRIVATE SECTION VALIDATORS ====================

    fn validate_dlm_section(&self, dlm: &DLMSection) -> Vec<String> {
        let mut errors = Vec::new();
        for module in &dlm.modules {
            if !self.is_valid_dlm_module(module.module_type, module.subtype) {
                let subtype_str = module.subtype
                    .map(|s| format!(".{:?}", s))
                    .unwrap_or_default();
                errors.push(format!(
                    "Invalid DLM module: {:?}{}",
                    module.module_type, subtype_str
                ));
            }
        }
        errors
    }

    fn validate_quick_functions_section(
        &self,
        quick_funcs: &QuickFuncsSection,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        for function in &quick_funcs.functions {
            errors.extend(self.validate_quick_function_signature(function));
        }
        errors
    }

    fn validate_data_section(
        &self,
        data: &DataSection,
        quickfuncs_enabled: bool,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        for entry in &data.entries {
            errors.extend(self.validate_data_entry(entry, quickfuncs_enabled));
        }
        errors
    }
}

impl Default for VersionConstraints {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== VALIDATION RESULT ====================

/// Result of script-level version/feature validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid:        bool,
    pub errors:          Vec<String>,
    pub warnings:        Vec<String>,
    pub is_advanced_mode: bool,
    pub detected_version: Option<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        ValidationResult {
            is_valid:         false,
            errors:           Vec::new(),
            warnings:         Vec::new(),
            is_advanced_mode: false,
            detected_version: None,
        }
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ValidationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Validation Result: {}",
            if self.is_valid { "VALID" } else { "INVALID" }
        )?;

        if self.is_advanced_mode {
            write!(f, " (Advanced Mode)")?;
        }

        if let Some(ref version) = self.detected_version {
            write!(f, " - Version: {}", version)?;
        }

        if !self.errors.is_empty() {
            write!(
                f,
                "\nErrors ({}):\n  {}",
                self.errors.len(),
                self.errors.join("\n  ")
            )?;
        }

        if !self.warnings.is_empty() {
            write!(
                f,
                "\nWarnings ({}):\n  {}",
                self.warnings.len(),
                self.warnings.join("\n  ")
            )?;
        }

        Ok(())
    }
    }
