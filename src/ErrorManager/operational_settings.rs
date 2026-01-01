//! Operational settings from @CONFIG section

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorHandlingStrategy {
    Halt,
    Continue,
    Recover,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DebugMode {
    Off = 0,
    Regular = 1,
    Verbose = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityMode {
    Strict,
    BestEffort,
    Permissive,
}

#[derive(Debug, Clone)]
pub struct OperationalSettings {
    pub version: String,
    pub error_handling_strategy: ErrorHandlingStrategy,
    pub debug_mode: DebugMode,
    pub compatibility_mode: CompatibilityMode,
}

impl Default for OperationalSettings {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_string(),
            error_handling_strategy: ErrorHandlingStrategy::Halt,
            debug_mode: DebugMode::Regular,
            compatibility_mode: CompatibilityMode::Strict,
        }
    }
}

impl OperationalSettings {
    pub fn new() -> Self {
        Self::default()
    }
}