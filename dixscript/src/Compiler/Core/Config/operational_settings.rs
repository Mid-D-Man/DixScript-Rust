//! Operational settings extracted from configuration

pub use crate::Compiler::AST::data_types::{
    ErrorHandlingStrategy,
    CompatibilityMode,
    DebugMode,
};

/// Operational settings extracted from configuration
#[derive(Debug, Clone)]
pub struct OperationalSettings {
    pub error_handling_strategy: ErrorHandlingStrategy,
    pub compatibility_mode: CompatibilityMode,
    pub debug_mode: DebugMode,
    pub skip_imports_resolution: bool,
    pub source_file_path: Option<String>,
    pub enabled_features: Vec<String>,
    pub version: String,
}

impl Default for OperationalSettings {
    fn default() -> Self {
        OperationalSettings {
            error_handling_strategy: ErrorHandlingStrategy::Halt,
            compatibility_mode:      CompatibilityMode::Strict,
            debug_mode:              DebugMode::Off,
            skip_imports_resolution: false,
            source_file_path:        None,
            // No @CONFIG → default to "advanced" (everything enabled).
            // This matches the static default in ConfigSchema ("features" -> "advanced").
            enabled_features: vec!["advanced".to_string()],
            version: "1.0.0".to_string(),
        }
    }
}

impl OperationalSettings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` only when the explicit keyword `"advanced"` is in the
    /// features list.  Listing individual section names (`"quickfuncs"`,
    /// `"enums"`, etc.) does NOT count as advanced mode — those grant access
    /// to their specific section only.
    ///
    /// # Previous bug
    /// The old implementation returned `true` for *any* advanced-section name
    /// (quickfuncs / enums / imports / dlm), which made `is_feature_enabled`
    /// silently unlock ALL sections the moment even one was listed.
    pub fn is_advanced_mode(&self) -> bool {
        self.enabled_features
            .iter()
            .any(|f| f.eq_ignore_ascii_case("advanced"))
    }

    /// Returns `true` if `feature` is enabled under the current feature set.
    ///
    /// Rules (evaluated in priority order):
    /// 1. `"advanced"` in features → everything is enabled.
    /// 2. `"basic"` in features (and not advanced) → nothing beyond DATA /
    ///    SECURITY is enabled; always return `false` here.
    /// 3. Otherwise → the specific feature name must appear in the list.
    ///
    /// The parser already hard-codes DATA and SECURITY as unconditionally
    /// allowed (`is_section_allowed` returns `true` for them regardless of
    /// this method), so the `"basic"` short-circuit is a safety net, not the
    /// primary gate for those two sections.
    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        // Rule 1: "advanced" unlocks everything.
        if self.is_advanced_mode() {
            return true;
        }

        // Rule 2: explicit "basic" blocks all gated features.
        if self.enabled_features.iter().any(|f| f.eq_ignore_ascii_case("basic")) {
            return false;
        }

        // Rule 3: specific feature must be present.
        self.enabled_features
            .iter()
            .any(|f| f.eq_ignore_ascii_case(feature))
    }
    }
