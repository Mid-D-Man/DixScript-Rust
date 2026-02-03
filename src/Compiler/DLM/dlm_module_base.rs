//! Base functionality for all DLM modules

use crate::ErrorManager::{ErrorManager, DlmErrorType};
use crate::Compiler::Core::Config::DebugMode;
use std::collections::HashMap;

/// Debug configuration cached at module construction
#[derive(Debug, Clone, Copy)]
pub struct DebugConfig {
    pub is_enabled: bool,
    pub is_verbose: bool,
}

impl DebugConfig {
    pub fn from_debug_mode(mode: DebugMode) -> Self {
        match mode {
            DebugMode::Off => DebugConfig {
                is_enabled: false,
                is_verbose: false,
            },
            DebugMode::Regular => DebugConfig {
                is_enabled: true,
                is_verbose: false,
            },
            DebugMode::Verbose => DebugConfig {
                is_enabled: true,
                is_verbose: true,
            },
        }
    }
}

/// Base struct for all DLM modules
/// Provides common functionality and lifecycle management
pub struct DLMModuleBase {
    error_manager: ErrorManager,
    module_name: String,
    priority: i32,
    debug_config: DebugConfig,
}

impl DLMModuleBase {
    /// Create new DLM module base
    pub fn new(module_name: impl Into<String>, priority: i32) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(error_manager.get_debug_mode());

        DLMModuleBase {
            error_manager,
            module_name: module_name.into(),
            priority,
            debug_config,
        }
    }

    /// Get module name
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// Get priority (lower = earlier execution)
    pub fn priority(&self) -> i32 {
        self.priority
    }

    /// Get debug config
    pub fn debug_config(&self) -> DebugConfig {
        self.debug_config
    }

    /// Log info message
    #[inline]
    pub fn log_info(&self, message: &str) {
        self.error_manager.log_info(&format!("[{}] {}", self.module_name, message));
    }

    /// Log debug message (only if debug enabled)
    #[inline]
    pub fn log_debug(&self, message: &str) {
        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!("[{}] {}", self.module_name, message));
        }
    }

    /// Log verbose message (only if verbose debug enabled)
    #[inline]
    pub fn log_verbose(&self, message: &str) {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!("[{}] {}", self.module_name, message));
        }
    }

    /// Log warning message
    #[inline]
    pub fn log_warning(&self, message: &str) {
        self.error_manager.log_warning(&format!("[{}] {}", self.module_name, message));
    }

    /// Log error
    #[inline]
    pub fn log_error(&self, message: &str) {
        self.error_manager.add_dlm_error(
            DlmErrorType::ModuleExecutionFailed,
            message.to_string(),
            Some(self.module_name.clone()),
            None,
            None,
            crate::ErrorManager::ErrorSeverity::Error,
        );
    }

    /// Get error manager reference
    pub fn error_manager(&self) -> &ErrorManager {
        &self.error_manager
    }
                           }
