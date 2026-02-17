//! Base functionality for all DLM modules

use crate::ErrorManager::{ErrorManager, DlmErrorType};
use crate::Compiler::Core::Config::DebugMode;
use std::collections::HashMap;

/// Base struct for all DLM modules
/// Provides common functionality and lifecycle management
pub struct DLMModuleBase {
    error_manager: ErrorManager,
    module_name: String,
    priority: i32,
    is_debug_enabled: bool,
    is_verbose_enabled: bool,
}

impl DLMModuleBase {
    /// Create new DLM module base
    pub fn new(module_name: impl Into<String>, priority: i32) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_mode = error_manager.get_debug_mode();

        let (is_debug_enabled, is_verbose_enabled) = match debug_mode {
            DebugMode::Off => (false, false),
            DebugMode::Regular => (true, false),
            DebugMode::Verbose => (true, true),
        };

        DLMModuleBase {
            error_manager,
            module_name: module_name.into(),
            priority,
            is_debug_enabled,
            is_verbose_enabled,
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

    /// Check if debug is enabled
    pub fn is_debug_enabled(&self) -> bool {
        self.is_debug_enabled
    }

    /// Check if verbose is enabled
    pub fn is_verbose_enabled(&self) -> bool {
        self.is_verbose_enabled
    }

    /// Log info message
    #[inline]
    pub fn log_info(&self, message: &str) {
        self.error_manager.log_info(&format!("[{}] {}", self.module_name, message));
    }

    /// Log debug message (only if debug enabled)
    #[inline]
    pub fn log_debug(&self, message: &str) {
        if self.is_debug_enabled {
            self.error_manager.log_debug(&format!("[{}] {}", self.module_name, message));
        }
    }

    /// Log verbose message (only if verbose debug enabled)
    #[inline]
    pub fn log_verbose(&self, message: &str) {
        if self.is_verbose_enabled {
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