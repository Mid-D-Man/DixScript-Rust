/// Error handling strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorHandlingStrategy {
    /// Halt execution on first error
    Halt,
    /// Continue execution, collect all errors
    Continue,
    /// Attempt to recover from errors
    Recover,
}

impl Default for ErrorHandlingStrategy {
    fn default() -> Self {
        ErrorHandlingStrategy::Halt
    }
}

/// Compatibility mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityMode {
    /// Strict mode - enforce all rules
    Strict,
    /// Best effort - try to work with issues
    BestEffort,
    /// Permissive - allow most variations
    Permissive,
}

impl Default for CompatibilityMode {
    fn default() -> Self {
        CompatibilityMode::Strict
    }
}

/// Debug mode level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMode {
    /// No debug output
    Off,
    /// Regular debug output
    Regular,
    /// Verbose debug output
    Verbose,
}

impl Default for DebugMode {
    fn default() -> Self {
        DebugMode::Off
    }
}

/// Operational settings extracted from configuration
/// PLACEHOLDER: This will be fully implemented when we port ConfigSchema
#[derive(Debug, Clone)]
pub struct OperationalSettings {
    pub error_handling_strategy: ErrorHandlingStrategy,
    pub compatibility_mode: CompatibilityMode,
    pub debug_mode: DebugMode,

    /// If true, skip imports resolution in semantic analysis (already being resolved by parent)
    pub skip_imports_resolution: bool,

    /// Source file path for resolving relative imports
    pub source_file_path: Option<String>,

    /// List of enabled features (e.g., "advanced", "basic", "quickfuncs", "enums", etc.)
    pub enabled_features: Vec<String>,

    /// DixScript version
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
    /// Create new operational settings with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if advanced mode is enabled
    /// Advanced mode is enabled if:
    /// 1. Features contains "advanced", OR
    /// 2. Features contains any specific advanced section (quickfuncs, enums, imports, dlm)
    pub fn is_advanced_mode(&self) -> bool {
        self.enabled_features.iter().any(|f| {
            f.eq_ignore_ascii_case("advanced")
                || f.eq_ignore_ascii_case("quickfuncs")
                || f.eq_ignore_ascii_case("enums")
                || f.eq_ignore_ascii_case("imports")
                || f.eq_ignore_ascii_case("dlm")
        })
    }

    /// Check if a specific feature is enabled
    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        // If advanced mode, all features are enabled (except "basic" which is exclusive)
        if self.is_advanced_mode() && !feature.eq_ignore_ascii_case("basic") {
            return true;
        }

        // Check if feature is explicitly listed
        self.enabled_features.iter().any(|f| {
            f.eq_ignore_ascii_case(feature) || f.eq_ignore_ascii_case("basic")
        })
    }
}