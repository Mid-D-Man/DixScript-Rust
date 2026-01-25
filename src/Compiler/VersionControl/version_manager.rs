// src/Compiler/VersionControl/version_manager.rs
//! Version Manager - Manages DixScript version features and compatibility
//!
//! SINGLETON PATTERN using LazyLock (thread-safe, zero-cost after first access)

use std::collections::HashSet;
use crate::Utilities::TokenType;
use std::sync::{LazyLock, RwLock};

/// Version constants
pub const VERSION_1_0: &str = "1.0.0";
pub const DEFAULT_VERSION: &str = VERSION_1_0;

/// VersionManager singleton - manages version-specific features
pub struct VersionManager {
    current_version: String,
    version_hierarchy: HashSet<String>,
    feature_map: HashSet<String>,
}

/// Global singleton instance (thread-safe, lazy-initialized)
static VERSION_MANAGER: LazyLock<RwLock<VersionManager>> = LazyLock::new(|| {
    RwLock::new(VersionManager::new(DEFAULT_VERSION))
});

impl VersionManager {
    /// Create new VersionManager with specified version
    fn new(version: &str) -> Self {
        let validated_version = Self::validate_version_static(version);
        let feature_map = Self::initialize_features_for_version(&validated_version);

        let mut version_hierarchy = HashSet::new();
        version_hierarchy.insert(VERSION_1_0.to_string());

        VersionManager {
            current_version: validated_version,
            version_hierarchy,
            feature_map,
        }
    }

    /// Get singleton instance (read-only access)
    pub fn instance() -> &'static RwLock<VersionManager> {
        &VERSION_MANAGER
    }

    /// Initialize with specific version (call once at startup)
    /// Note: This updates the existing singleton, does not return Result
    pub fn initialize(version: &str) {
        let mut manager = VERSION_MANAGER.write().unwrap();
        manager.current_version = Self::validate_version_static(version);
        manager.feature_map = Self::initialize_features_for_version(&manager.current_version);
    }

    /// Validate version string
    fn validate_version_static(version: &str) -> String {
        if version.is_empty() || version != VERSION_1_0 {
            DEFAULT_VERSION.to_string()
        } else {
            version.to_string()
        }
    }

    /// Get current version
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Get current version (deprecated, use current_version)
    pub fn get_current_version(&self) -> &str {
        &self.current_version
    }

    /// Check if feature is supported in current version
    /// PERFORMANCE: O(1) - HashSet lookup (~20ns)
    #[inline]
    pub fn supports_feature(&self, feature_key: &str) -> bool {
        self.feature_map.contains(&feature_key.to_string())
    }

    /// Check if current version is compatible with target version
    pub fn is_compatible_with(&self, target_version: &str) -> bool {
        self.version_hierarchy.contains(&target_version.to_string())
    }

    /// Check if token type is valid for current version
    pub fn is_token_valid_for_version(&self, token_type: &TokenType) -> bool {
        match token_type {
            TokenType::SectionConfig => self.supports_feature("config_section"),
            TokenType::SectionImports => self.supports_feature("imports_section"),
            TokenType::SectionQuickFuncs => self.supports_feature("quickfuncs_section"),
            TokenType::SectionEnums => self.supports_feature("enums_section"),
            TokenType::SectionDLM => self.supports_feature("dlm_section"),
            TokenType::SectionData => self.supports_feature("data_section"),
            TokenType::SectionSecurity => self.supports_feature("security_section"),
            _ => true,
        }
    }

    /// Check if section type is supported
    pub fn supports_section_type(&self, section_type: &str) -> bool {
        let section_feature = format!("{}_section", section_type.to_lowercase());
        self.supports_feature(&section_feature)
    }

    /// Check if feature control is supported
    #[inline]
    pub fn supports_feature_control(&self) -> bool {
        self.supports_feature("feature_control")
    }

    /// Check if builtin registry is supported
    #[inline]
    pub fn supports_builtin_registry(&self) -> bool {
        self.supports_feature("static_object_registry")
            && self.supports_feature("instance_method_registry")
    }

    /// Check if dual parser system is supported
    #[inline]
    pub fn supports_dual_parsers(&self) -> bool {
        self.supports_feature("dual_parser_system")
    }

    /// Get recommended parser for section type
    pub fn get_recommended_parser(&self, section_type: &str) -> &'static str {
        match section_type.to_uppercase().as_str() {
            "CONFIG" | "DLM" | "DATA" | "ENUMS" | "SECURITY" | "IMPORTS" => "LL",
            "QUICKFUNCS" => "LALR",
            _ => "LL",
        }
    }

    /// Validate script features (returns list of unsupported features)
    pub fn validate_script_features(&self, script: &crate::Compiler::AST::DixScript) -> Vec<String> {
        let mut unsupported = Vec::new();

        if script.config.is_some() && !self.supports_feature("config_section") {
            unsupported.push("CONFIG section".to_string());
        }

        if script.imports.is_some() && !self.supports_feature("imports_section") {
            unsupported.push("IMPORTS section".to_string());
        }

        if script.dlm.is_some() && !self.supports_feature("dlm_section") {
            unsupported.push("DLM section".to_string());
        }

        if script.enums.is_some() && !self.supports_feature("enums_section") {
            unsupported.push("ENUMS section".to_string());
        }

        if script.quick_functions.is_some() && !self.supports_feature("quickfuncs_section") {
            unsupported.push("QUICKFUNCS section".to_string());
        }

        if script.data.is_some() && !self.supports_feature("data_section") {
            unsupported.push("DATA section".to_string());
        }

        if script.security.is_some() && !self.supports_feature("security_section") {
            unsupported.push("SECURITY section".to_string());
        }

        unsupported
    }

    /// Get version information
    pub fn get_version_info(&self) -> std::collections::HashMap<String, String> {
        let mut info = std::collections::HashMap::new();
        info.insert("CurrentVersion".to_string(), self.current_version.clone());
        info.insert("SupportedVersions".to_string(), VERSION_1_0.to_string());
        info.insert("FeatureCount".to_string(), self.feature_map.len().to_string());
        info.insert("CompatibilityMode".to_string(), "v1.0.0 Foundation".to_string());
        info.insert("BackwardCompatibility".to_string(), "None (foundation version)".to_string());
        info.insert("ForwardCompatibility".to_string(), "Limited (unknown features handled gracefully)".to_string());
        info.insert("SupportsImports".to_string(), self.supports_feature("imports_section").to_string());
        info
    }

    /// Initialize feature set for v1.0.0
    /// Called ONCE during singleton construction
    fn initialize_features_for_version(version: &str) -> HashSet<String> {
        if version != VERSION_1_0 {
            return HashSet::new();
        }

        let mut features = HashSet::new();

        // Core language
        features.insert("basic_types".to_string());
        features.insert("enhanced_types".to_string());
        features.insert("data_types_with_annotations".to_string());

        // Sections
        features.insert("config_section".to_string());
        features.insert("imports_section".to_string());
        features.insert("dlm_section".to_string());
        features.insert("enums_section".to_string());
        features.insert("quickfuncs_section".to_string());
        features.insert("data_section".to_string());
        features.insert("security_section".to_string());

        // CONFIG features
        features.insert("feature_control".to_string());
        features.insert("debug_modes".to_string());
        features.insert("config_constants".to_string());

        // IMPORTS features
        features.insert("imports_local".to_string());
        features.insert("imports_cloud".to_string());
        features.insert("imports_verification".to_string());
        features.insert("imports_namespaces".to_string());
        features.insert("imports_nested".to_string());
        features.insert("imports_cycle_detection".to_string());

        // DLM modules
        features.insert("dlm_dcompressor".to_string());
        features.insert("dlm_dauditor".to_string());
        features.insert("dlm_dencryptor".to_string());

        // DATA section
        features.insert("table_group_syntax".to_string());
        features.insert("group_arrays".to_string());
        features.insert("simple_properties".to_string());
        features.insert("object_properties".to_string());
        features.insert("property_type_annotations".to_string());

        // QUICKFUNCS
        features.insert("quickfunctions".to_string());
        features.insert("function_scoping".to_string());
        features.insert("function_type_annotations".to_string());
        features.insert("function_parameters".to_string());
        features.insert("quickfunc_calls_in_data".to_string());
        features.insert("parameter_defaults".to_string());
        features.insert("imported_function_calls".to_string());

        // Expressions
        features.insert("expressions_full".to_string());
        features.insert("conditional_expressions".to_string());
        features.insert("property_access".to_string());
        features.insert("index_access".to_string());
        features.insert("method_chaining".to_string());

        // Built-ins
        features.insert("static_object_registry".to_string());
        features.insert("instance_method_registry".to_string());
        features.insert("dix_function_calls".to_string());

        // Control flow
        features.insert("if_elif_else".to_string());
        features.insert("switch_statements".to_string());
        features.insert("return_statements".to_string());

        // String features
        features.insert("interpolated_strings".to_string());
        features.insert("single_quoted_strings".to_string());
        features.insert("double_quoted_strings".to_string());

        // Literals
        features.insert("array_literals".to_string());
        features.insert("object_literals".to_string());
        features.insert("prefixed_constructors".to_string());
        features.insert("hex_colors".to_string());
        features.insert("hex_literals".to_string());
        features.insert("scientific_notation".to_string());
        features.insert("date_literals".to_string());
        features.insert("timestamp_literals".to_string());

        // Access patterns
        features.insert("config_access".to_string());
        features.insert("enum_access".to_string());
        features.insert("imported_enum_access".to_string());
        features.insert("qualified_identifiers".to_string());

        // Architecture
        features.insert("dual_parser_system".to_string());
        features.insert("section_routing".to_string());
        features.insert("context_aware_tokenization".to_string());

        // Compatibility
        features.insert("forward_compatibility".to_string());
        features.insert("version_constraints".to_string());
        features.insert("compatibility_modes".to_string());

        // Format
        features.insert("mdix_extension".to_string());
        features.insert("single_file_format".to_string());

        // Validation
        features.insert("semantic_analysis".to_string());
        features.insert("type_checking".to_string());
        features.insert("scope_validation".to_string());
        features.insert("built_in_validation".to_string());

        features
    }
}

/// Extract version from AST (helper function)
pub fn extract_version_from_ast(script: &crate::Compiler::AST::DixScript) -> String {
    if let Some(ref config) = script.config {
        for entry in &config.entries {
            if entry.key.eq_ignore_ascii_case("version") {
                if let crate::Compiler::AST::ConfigValue::String(ref version) = entry.value {
                    return version.clone();
                }
            }
        }
    }
    DEFAULT_VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_manager_singleton() {
        let manager = VERSION_MANAGER.read().unwrap();
        assert_eq!(manager.get_current_version(), VERSION_1_0);
    }

    #[test]
    fn test_feature_support() {
        let manager = VERSION_MANAGER.read().unwrap();
        assert!(manager.supports_feature("config_section"));
        assert!(manager.supports_feature("imports_section"));
        assert!(manager.supports_feature("quickfuncs_section"));
        assert!(!manager.supports_feature("nonexistent_feature"));
    }

    #[test]
    fn test_version_compatibility() {
        let manager = VERSION_MANAGER.read().unwrap();
        assert!(manager.is_compatible_with(VERSION_1_0));
    }

    #[test]
    fn test_section_support() {
        let manager = VERSION_MANAGER.read().unwrap();
        assert!(manager.supports_section_type("CONFIG"));
        assert!(manager.supports_section_type("IMPORTS"));
        assert!(manager.supports_section_type("QUICKFUNCS"));
    }
}