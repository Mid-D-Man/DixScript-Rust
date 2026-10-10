// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/VersionControl/version_constraints.rs and version_manager.rs"
// ============================================================================
//! Version Manager - Manages DixScript version features and compatibility
//!
//! SINGLETON PATTERN using LazyLock (thread-safe, zero-cost after first access)

use std::collections::HashSet;
use crate::Compiler::Core::Tokenizer::TokenType;
use std::sync::{LazyLock, RwLock};

/// Language versions a `.mdix` file can declare with `@CONFIG` `version`.
pub const VERSION_1_0: &str = "1.0.0";
/// Same language as 1.0.0 except the DLM codecs: bzip2 and lzma were removed.
pub const VERSION_2_0: &str = "2.0.0";
/// Fallback for a missing or unrecognized version. It matches the defaults in
/// `ConfigSchema` and `OperationalSettings`, which are still 1.0.0.
pub const DEFAULT_VERSION: &str = VERSION_1_0;

/// VersionManager singleton - manages version-specific features
pub struct VersionManager {
    current_version: String,
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

        VersionManager {
            current_version: validated_version,
            feature_map,
        }
    }

    /// Get singleton instance (read-only access)
    pub fn instance() -> &'static RwLock<VersionManager> {
        &VERSION_MANAGER
    }

    /// Initialize with specific version (idempotent — safe to call repeatedly).
    ///
    /// Uses a double-checked read-first pattern:
    /// 1. Acquire a cheap read-lock and return immediately if the version has
    ///    not changed (the common case in the LSP where every document open
    ///    calls this with "1.0.0").
    /// 2. Only acquire the global write-lock when a version transition actually
    ///    needs to happen.
    ///
    /// This eliminates the write-lock contention that previously caused the
    /// LSP server to stall (and sometimes timeout) when multiple documents
    /// opened simultaneously.
    pub fn initialize(version: &str) {
        let validated = Self::validate_version_static(version);

        // Fast path — read-lock only (≈20 ns, never blocks writers).
        // Since all .mdix files use "1.0.0" and the singleton starts at
        // "1.0.0", this branch is taken on every call after the first.
        if let Ok(manager) = VERSION_MANAGER.read() {
            if manager.current_version == validated {
                return;
            }
        }

        // Slow path — write-lock needed (version actually changed).
        if let Ok(mut manager) = VERSION_MANAGER.write() {
            // Double-check: another thread may have already updated while we
            // were waiting for the write-lock.
            if manager.current_version != validated {
                manager.current_version = validated.clone();
                manager.feature_map = Self::initialize_features_for_version(&validated);
            }
        }
    }

    /// Validate version string
    fn validate_version_static(version: &str) -> String {
        Self::normalize_version(version).unwrap_or(DEFAULT_VERSION).to_string()
    }

    /// Canonical form of a declared version, or `None` if it is not a language
    /// version this build knows. `x_N.*` is the extension form of major N.
    fn normalize_version(version: &str) -> Option<&'static str> {
        match version {
            "1.0.0" | "1.0" => Some(VERSION_1_0),
            "2.0.0" | "2.0" => Some(VERSION_2_0),
            v if v.starts_with("x_1.") => Some(VERSION_1_0),
            v if v.starts_with("x_2.") => Some(VERSION_2_0),
            _ => None,
        }
    }

    /// Order of the known language versions, oldest first.
    fn version_rank(version: &str) -> Option<u8> {
        match version {
            VERSION_1_0 => Some(1),
            VERSION_2_0 => Some(2),
            _ => None,
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

    /// True if this manager can read files declared at `target_version`: any
    /// known version up to and including the current one.
    pub fn is_compatible_with(&self, target_version: &str) -> bool {
        let current = Self::version_rank(&self.current_version);
        let target = Self::normalize_version(target_version).and_then(|v| Self::version_rank(v));
        match (current, target) {
            (Some(current), Some(target)) => target <= current,
            _ => false,
        }
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
            TokenType::SectionRaw => self.supports_feature("raw_section"),
            TokenType::SectionSchema => self.supports_feature("schema_section"),
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

        if !script.raw.is_empty() && !self.supports_feature("raw_section") {
            unsupported.push("RAW section".to_string());
        }

        if script.schema.is_some() && !self.supports_feature("schema_section") {
            unsupported.push("SCHEMA section".to_string());
        }

        unsupported
    }

    /// Get version information
    pub fn get_version_info(&self) -> std::collections::HashMap<String, String> {
        let mut info = std::collections::HashMap::new();
        info.insert("CurrentVersion".to_string(), self.current_version.clone());
        let is_v2 = self.current_version == VERSION_2_0;
        info.insert("SupportedVersions".to_string(), format!("{}, {}", VERSION_1_0, VERSION_2_0));
        info.insert("FeatureCount".to_string(), self.feature_map.len().to_string());
        info.insert(
            "CompatibilityMode".to_string(),
            if is_v2 { "v2.0.0" } else { "v1.0.0 Foundation" }.to_string(),
        );
        info.insert(
            "BackwardCompatibility".to_string(),
            if is_v2 {
                "Reads v1.0.0 files; DCompressor.bzip2 and DCompressor.lzma were removed"
            } else {
                "None (foundation version)"
            }
            .to_string(),
        );
        info.insert("ForwardCompatibility".to_string(), "Limited (unknown features handled gracefully)".to_string());
        info.insert("SupportsImports".to_string(), self.supports_feature("imports_section").to_string());
        info
    }

    /// Feature set for a language version. 1.0.0 and 2.0.0 share everything
    /// except the DLM codec keys at the end.
    /// Called ONCE during singleton construction (or on version change)
    fn initialize_features_for_version(version: &str) -> HashSet<String> {
        if version != VERSION_1_0 && version != VERSION_2_0 {
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
        features.insert("raw_section".to_string());
        features.insert("schema_section".to_string());

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

        // DLM codecs: gzip is in every version, bzip2 and lzma only in 1.0.0.
        features.insert("dlm_codec_gzip".to_string());
        if version == VERSION_1_0 {
            features.insert("dlm_codec_bzip2".to_string());
            features.insert("dlm_codec_lzma".to_string());
        }

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

    // The tests below build their own `VersionManager` and call the pure helpers,
    // never `VersionManager::initialize`, so they cannot race with other tests
    // over the global singleton.
    #[test]
    fn test_normalize_version() {
        assert_eq!(VersionManager::normalize_version("1.0.0"), Some(VERSION_1_0));
        assert_eq!(VersionManager::normalize_version("1.0"), Some(VERSION_1_0));
        assert_eq!(VersionManager::normalize_version("2.0.0"), Some(VERSION_2_0));
        assert_eq!(VersionManager::normalize_version("2.0"), Some(VERSION_2_0));
        assert_eq!(VersionManager::normalize_version("x_1.5"), Some(VERSION_1_0));
        assert_eq!(VersionManager::normalize_version("x_2.1"), Some(VERSION_2_0));
        assert_eq!(VersionManager::normalize_version("3.0.0"), None);
        assert_eq!(VersionManager::normalize_version(""), None);
        assert_eq!(VersionManager::validate_version_static("3.0.0"), DEFAULT_VERSION);
        assert_eq!(VersionManager::validate_version_static("2.0.0"), VERSION_2_0);
    }

    #[test]
    fn test_codec_features_by_version() {
        let v1 = VersionManager::initialize_features_for_version(VERSION_1_0);
        let v2 = VersionManager::initialize_features_for_version(VERSION_2_0);
        assert!(v1.contains("dlm_codec_gzip") && v2.contains("dlm_codec_gzip"));
        assert!(v1.contains("dlm_codec_bzip2") && v1.contains("dlm_codec_lzma"));
        assert!(!v2.contains("dlm_codec_bzip2") && !v2.contains("dlm_codec_lzma"));

        // Apart from the codec keys the two versions are the same language.
        let without_codecs = |set: &HashSet<String>| -> HashSet<String> {
            set.iter().filter(|f| !f.starts_with("dlm_codec_")).cloned().collect()
        };
        assert_eq!(without_codecs(&v1), without_codecs(&v2));
        assert!(VersionManager::initialize_features_for_version("9.9.9").is_empty());
    }

    #[test]
    fn test_compatibility_is_up_to_current_version() {
        let v2 = VersionManager::new(VERSION_2_0);
        assert_eq!(v2.current_version(), VERSION_2_0);
        assert!(v2.is_compatible_with("1.0.0"));
        assert!(v2.is_compatible_with("2.0.0"));
        assert!(v2.is_compatible_with("2.0"));
        assert!(!v2.is_compatible_with("3.0.0"));

        let v1 = VersionManager::new(VERSION_1_0);
        assert!(v1.is_compatible_with("1.0.0"));
        assert!(!v1.is_compatible_with("2.0.0"));
    }

    #[test]
    fn test_version_info_for_2_0() {
        let info = VersionManager::new(VERSION_2_0).get_version_info();
        assert_eq!(info["CurrentVersion"], VERSION_2_0);
        assert!(info["SupportedVersions"].contains("1.0.0") && info["SupportedVersions"].contains("2.0.0"));
        assert!(info["BackwardCompatibility"].contains("bzip2"));
    }

    #[test]
    fn test_initialize_idempotent() {
        // Calling initialize multiple times with the same version must be
        // safe and not panic (tests the double-checked locking path).
        VersionManager::initialize("1.0.0");
        VersionManager::initialize("1.0.0");
        VersionManager::initialize("1.0.0");
        let manager = VERSION_MANAGER.read().unwrap();
        assert_eq!(manager.get_current_version(), VERSION_1_0);
    }
}