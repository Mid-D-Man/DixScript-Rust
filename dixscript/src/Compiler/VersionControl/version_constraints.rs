// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/VersionControl/version_constraints.rs and version_manager.rs"
// ============================================================================
//! Version Constraints - Validates DixScript features against version requirements
//! Uses native Rust collections (no DixCore wrappers)

use std::collections::{HashMap, HashSet};
use crate::Compiler::AST::arena::{Arena, DocData, DocDataEntry, DocQuickFuncs, Document, ValueNode};
use crate::Compiler::AST::*;
use crate::Compiler::VersionControl::VersionManager;
use crate::Compiler::Extensions::TypeSystemManager;

/// Defines constraints and limitations based on DixScript version.
/// Singleton instance coordinated with VersionManager.
pub struct VersionConstraints {
    version_manager: &'static std::sync::RwLock<VersionManager>,
}

impl VersionConstraints {
    /// Create new VersionConstraints instance
    pub fn new() -> Self {
        VersionConstraints { version_manager: VersionManager::instance() }
    }

    // ==================== DLM MODULE VALIDATION ====================

    /// Checks if a DLM module type and subtype combination is valid.
    /// `DCompressor.bzip2` and `.lzma` are valid only for language version 1.0.0.
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
            | (DLMModuleType::DCompressor, None) => true,

            (DLMModuleType::DCompressor, Some(DLMModuleSubtype::Bzip2)) => {
                manager.supports_feature("dlm_codec_bzip2")
            }
            (DLMModuleType::DCompressor, Some(DLMModuleSubtype::Lzma)) => {
                manager.supports_feature("dlm_codec_lzma")
            }

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
            "CONFIG", "IMPORTS", "DLM", "ENUMS", "QUICKFUNCS", "DATA", "SECURITY", "RAW", "SCHEMA",
        ];

        if supported_sections.contains(&normalized_section.as_str()) {
            let manager = self.version_manager.read().unwrap();
            return manager.supports_section_type(&normalized_section);
        }

        false
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
        section_list
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .all(|s| matches!(
                s.as_str(),
                "quickfuncs" | "enums" | "imports" | "dlm" | "data" | "security" | "raw" | "schema"
            ))
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
                            if f != "advanced" && f != "basic"
                                && !self.is_valid_section_list(&features.join(",")) {
                                    errors.push("Invalid feature list".to_string());
                                }
                        } else if !self.is_valid_section_list(&features.join(",")) {
                            errors.push("Invalid feature list".to_string());
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
        constraints.insert("Version".to_string(), json!(manager.current_version()));
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
                "DCompressor": ["gzip"],
                "DAuditor":    ["diy", "enhanced"],
                "DEncryptor":  ["xor", "aes128", "aes256", "chacha20"]
            }),
        );
        constraints.insert(
            "RemovedDLMModules".to_string(),
            json!({ "DCompressor": ["bzip2", "lzma"] }),
        );
        constraints.insert(
            "ValidDebugModes".to_string(),
            json!(["off", "regular", "verbose"]),
        );
        constraints.insert(
            "ValidFeatureControls".to_string(),
            json!(["basic", "advanced", "quickfuncs", "enums", "imports", "dlm", "data", "security", "raw", "schema"]),
        );
        constraints
    }

    // ==================== SCRIPT-LEVEL VALIDATION ====================

    /// Performs comprehensive validation of a DixScript AST against version
    /// constraints and feature requirements.
    ///
    /// Feature gating rules (all case-insensitive):
    /// * `features -> advanced`          -> all sections permitted.
    /// * `features -> basic`             -> only DATA and SECURITY permitted.
    /// * `features -> quickfuncs`        -> QUICKFUNCS permitted; others still blocked.
    /// * `features -> quickfuncs,enums`  -> QUICKFUNCS + ENUMS permitted.
    /// * `features -> imports`           -> IMPORTS permitted.
    /// * DATA and SECURITY are always permitted regardless of feature mode.
    /// * DLM is gated by the version manager's `dlm_section` feature flag (true in v1.0.0).
    ///
    /// [`validate_script_doc`](Self::validate_script_doc) does the same for a file held as a
    /// [`Document`].
    pub fn validate_script(&self, script: &DixScript) -> ValidationResult {
        self.validate_sections(
            script.config.as_ref(),
            script.imports.is_some(),
            script.dlm.as_ref(),
            script.enums.is_some(),
            Deep::Owned {
                quick_functions: script.quick_functions.as_ref(),
                data:            script.data.as_ref(),
            },
        )
    }

    /// The validation of [`validate_script`](Self::validate_script), reading a [`Document`].
    ///
    /// `@CONFIG`, `@IMPORTS`, `@DLM` and `@ENUMS` are the same owned types in a document, so only
    /// `@QUICKFUNCS` and `@DATA` are read through the arena, and of those only what the checks look
    /// at: the parameters of each function and the top-level kind of each value.
    pub fn validate_script_doc(&self, document: &Document) -> ValidationResult {
        self.validate_sections(
            document.config.as_ref(),
            document.imports.is_some(),
            document.dlm.as_ref(),
            document.enums.is_some(),
            Deep::Arena {
                arena:           &document.arena,
                quick_functions: document.quick_functions.as_ref(),
                data:            document.data.as_ref(),
            },
        )
    }

    /// The validation both entry points share. `deep` holds the two sections that are looked into,
    /// in whichever form the caller has them; the other sections are only looked at for presence
    /// or are the same types in both.
    fn validate_sections(
        &self,
        config:      Option<&ConfigSection>,
        has_imports: bool,
        dlm:         Option<&DLMSection>,
        has_enums:   bool,
        deep:        Deep<'_>,
    ) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Extract active features as a lowercase HashSet for O(1) lookup.
        let features    = self.extract_features(config);
        let is_advanced = features.contains("advanced");
        result.is_advanced_mode = is_advanced;

        // @CONFIG
        if let Some(config) = config {
            let config_errors = self.validate_config_section(config);
            result.errors.extend(
                config_errors.into_iter().map(|e| format!("CONFIG: {}", e)),
            );
        }

        // @IMPORTS: requires "advanced" OR explicit "imports" feature
        if has_imports {
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

        // @DLM: content validated regardless of user feature mode. DLM availability is controlled
        // by the version manager, not the user's `features` setting.
        if let Some(dlm) = dlm {
            let dlm_errors = self.validate_dlm_section(dlm);
            result.errors.extend(
                dlm_errors.into_iter().map(|e| format!("DLM: {}", e)),
            );
        }

        // @ENUMS: requires "advanced" OR explicit "enums" feature
        if has_enums {
            let enums_ok = is_advanced || features.contains("enums");
            if !enums_ok {
                result.errors.push(
                    "ENUMS section requires advanced mode or the 'enums' feature".to_string(),
                );
            }
        }

        // Both the section's own permission and the permission of the calls in @DATA turn on the
        // same feature.
        let quickfuncs_available = is_advanced || features.contains("quickfuncs");

        // @QUICKFUNCS: requires "advanced" OR explicit "quickfuncs" feature. Signatures are only
        // validated when the section is actually permitted.
        if deep.has_quick_functions() {
            if !quickfuncs_available {
                result.errors.push(
                    "QUICKFUNCS section requires advanced mode or the 'quickfuncs' feature"
                        .to_string(),
                );
            } else {
                result.errors.extend(
                    deep.quick_function_errors().into_iter().map(|e| format!("QUICKFUNCS: {}", e)),
                );
            }
        }

        // @DATA: always permitted; only the QuickFunc-call sub-check depends on whether
        // quickfuncs are available.
        if deep.has_data() {
            result.errors.extend(
                deep.data_errors(quickfuncs_available).into_iter().map(|e| format!("DATA: {}", e)),
            );
        }

        // @SECURITY: always permitted, no extra validation needed here.

        result.is_valid = result.errors.is_empty();
        result
    }

    // ==================== PRIVATE SECTION VALIDATORS ====================

    fn validate_dlm_section(&self, dlm: &DLMSection) -> Vec<String> {
        let mut errors = Vec::new();
        for module in &dlm.modules {
            if !self.is_valid_dlm_module(module.module_type, module.subtype) {
                // A removed codec gets its own message, since "invalid module"
                // would not tell the author what to change.
                if module.module_type == DLMModuleType::DCompressor
                    && matches!(
                        module.subtype,
                        Some(DLMModuleSubtype::Bzip2) | Some(DLMModuleSubtype::Lzma)
                    )
                {
                    let name = match module.subtype {
                        Some(DLMModuleSubtype::Bzip2) => "bzip2",
                        _ => "lzma",
                    };
                    errors.push(format!(
                        "DCompressor.{} was removed in language version 2.0.0; use DCompressor.gzip",
                        name
                    ));
                    continue;
                }
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

}

impl Default for VersionConstraints {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== THE TWO SECTIONS THAT ARE LOOKED INTO ====================

/// `@QUICKFUNCS` and `@DATA` as the caller holds them: the owned sections, or the sections of a
/// [`Document`] with the arena they point into.
///
/// The checks look at the parameters of each function and at the top-level kind of each value,
/// nothing deeper. That is a scan of a few fields per entry, so it is written once for each form
/// rather than copying a whole section into an arena to scan it once.
enum Deep<'a> {
    Owned {
        quick_functions: Option<&'a QuickFuncsSection>,
        data:            Option<&'a DataSection>,
    },
    Arena {
        arena:           &'a Arena,
        quick_functions: Option<&'a DocQuickFuncs>,
        data:            Option<&'a DocData>,
    },
}

impl Deep<'_> {
    fn has_quick_functions(&self) -> bool {
        match self {
            Deep::Owned { quick_functions, .. } => quick_functions.is_some(),
            Deep::Arena { quick_functions, .. } => quick_functions.is_some(),
        }
    }

    fn has_data(&self) -> bool {
        match self {
            Deep::Owned { data, .. } => data.is_some(),
            Deep::Arena { data, .. } => data.is_some(),
        }
    }

    /// The signature errors of every function: a default value that does not convert to the
    /// type declared for its parameter.
    fn quick_function_errors(&self) -> Vec<String> {
        match self {
            Deep::Owned { quick_functions: Some(section), .. } => section
                .functions
                .iter()
                .flat_map(|f| TypeSystemManager::validate_quick_function_signature(f).err().unwrap_or_default())
                .collect(),
            Deep::Arena { arena, quick_functions: Some(section), .. } => section
                .functions
                .iter()
                .flat_map(|f| {
                    TypeSystemManager::validate_doc_quick_function_signature(f, arena).err().unwrap_or_default()
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The errors of every data entry: a table or group array with no path, and (when
    /// `quickfuncs_available` is false) a QuickFunction call as a value.
    fn data_errors(&self, quickfuncs_available: bool) -> Vec<String> {
        match self {
            Deep::Owned { data: Some(data), .. } => owned_data_errors(data, quickfuncs_available),
            Deep::Arena { arena, data: Some(data), .. } => arena_data_errors(data, arena, quickfuncs_available),
            _ => Vec::new(),
        }
    }
}

const EMPTY_TABLE_PATH: &str = "Table property must have a valid path";
const EMPTY_GROUP_PATH: &str = "Group array must have a valid path";

/// The error for a QuickFunction call in `@DATA`, with where it is.
fn call_in_data(location: std::fmt::Arguments<'_>) -> String {
    format!("QuickFunction calls in DATA require 'advanced' or 'quickfuncs' feature ({})", location)
}

fn owned_data_errors(data: &DataSection, quickfuncs_available: bool) -> Vec<String> {
    let is_call = |value: &Value| !quickfuncs_available && matches!(value, Value::QuickFuncCall { .. });
    let mut errors = Vec::new();
    for entry in &data.entries {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                if is_call(value) {
                    errors.push(call_in_data(format_args!("property: {}", name)));
                }
            }
            DataEntry::TableProperty { path, properties, .. } => {
                if path.segments.is_empty() {
                    errors.push(EMPTY_TABLE_PATH.to_string());
                    continue;
                }
                for assignment in properties.iter().filter(|a| is_call(&a.value)) {
                    errors.push(call_in_data(format_args!("property: {}.{}", path, assignment.name)));
                }
            }
            DataEntry::GroupArray { path, items, .. } => {
                if path.segments.is_empty() {
                    errors.push(EMPTY_GROUP_PATH.to_string());
                    continue;
                }
                for _ in items.iter().filter(|v| is_call(v)) {
                    errors.push(call_in_data(format_args!("group array: {}", path)));
                }
            }
            DataEntry::ObjectProperty { .. } => {}
        }
    }
    errors
}

fn arena_data_errors(data: &DocData, arena: &Arena, quickfuncs_available: bool) -> Vec<String> {
    let is_call = |id| !quickfuncs_available && matches!(arena.value(id), ValueNode::QuickFuncCall { .. });
    let mut errors = Vec::new();
    for entry in &data.entries {
        match entry {
            DocDataEntry::SimpleProperty { name, value, .. } => {
                if is_call(*value) {
                    errors.push(call_in_data(format_args!("property: {}", name)));
                }
            }
            DocDataEntry::TableProperty { path, properties, .. } => {
                if path.segments.is_empty() {
                    errors.push(EMPTY_TABLE_PATH.to_string());
                    continue;
                }
                for assignment in properties.iter().filter(|a| is_call(a.value)) {
                    errors.push(call_in_data(format_args!("property: {}.{}", path, assignment.name)));
                }
            }
            DocDataEntry::GroupArray { path, items, .. } => {
                if path.segments.is_empty() {
                    errors.push(EMPTY_GROUP_PATH.to_string());
                    continue;
                }
                for _ in arena.value_list(*items).iter().filter(|id| is_call(**id)) {
                    errors.push(call_in_data(format_args!("group array: {}", path)));
                }
            }
            DocDataEntry::ObjectProperty { .. } => {}
        }
    }
    errors
}

// ==================== VALIDATION RESULT ====================

/// Result of script-level version/feature validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid:        bool,
    pub errors:          Vec<String>,
    pub is_advanced_mode: bool,
}

impl ValidationResult {
    pub fn new() -> Self {
        ValidationResult {
            is_valid:         false,
            errors:           Vec::new(),
            is_advanced_mode: false,
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

        if !self.errors.is_empty() {
            write!(
                f,
                "\nErrors ({}):\n  {}",
                self.errors.len(),
                self.errors.join("\n  ")
            )?;
        }

        Ok(())
    }
    }
