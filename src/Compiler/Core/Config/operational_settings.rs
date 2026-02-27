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
            compatibility_mode: CompatibilityMode::Strict,
            debug_mode: DebugMode::Off,
            skip_imports_resolution: false,
            source_file_path: None,
            enabled_features: vec!["advanced".to_string()],
            version: "1.0.0".to_string(),
        }
    }
}

impl OperationalSettings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_advanced_mode(&self) -> bool {
        self.enabled_features.iter().any(|f| {
            f.eq_ignore_ascii_case("advanced")
                || f.eq_ignore_ascii_case("quickfuncs")
                || f.eq_ignore_ascii_case("enums")
                || f.eq_ignore_ascii_case("imports")
                || f.eq_ignore_ascii_case("dlm")
        })
    }

    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        if self.is_advanced_mode() && !feature.eq_ignore_ascii_case("basic") {
            return true;
        }
        self.enabled_features.iter().any(|f| {
            f.eq_ignore_ascii_case(feature) || f.eq_ignore_ascii_case("basic")
        })
    }
}