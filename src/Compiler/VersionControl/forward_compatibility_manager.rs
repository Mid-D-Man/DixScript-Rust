// src/Compiler/VersionControl/forward_compatibility_manager.rs
//! Forward Compatibility Manager - Handles unknown features from future versions
//!
//! Simplified framework for DixScript v1.0.0

use super::compatibility_result::CompatibilityResult;
use super::version_manager::VersionManager;
use crate::ErrorManager::ErrorManager;
use std::collections::HashMap;

/// Compatibility mode for handling unknown elements
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityMode {
    Strict,      // Reject all unknown elements
    Tolerant,    // Warn but continue
    BestEffort,  // Try to handle gracefully
}

/// Forward Compatibility Manager
pub struct ForwardCompatibilityManager {
    mode: CompatibilityMode,
}

impl ForwardCompatibilityManager {
    /// Create new manager with specified mode
    pub fn new(mode: CompatibilityMode) -> Self {
        ForwardCompatibilityManager { mode }
    }

    /// Handle unknown element based on compatibility mode
    pub fn handle_unknown_element(
        &self,
        element_type: &str,
        element_name: &str,
        element_data: Option<&str>,
        context: Option<&HashMap<String, String>>,
    ) -> Result<bool, String> {
        let error_manager = ErrorManager::get_shared_instance();

        if error_manager.is_info_enabled() {
            error_manager.log_info(&format!(
                "Processing unknown element: {}::{}",
                element_type, element_name
            ));
        }

        match self.mode {
            CompatibilityMode::Strict => self.handle_strict(element_type, element_name),
            CompatibilityMode::Tolerant => self.handle_tolerant(element_type, element_name),
            CompatibilityMode::BestEffort => self.handle_best_effort(element_type, element_name),
        }
    }

    /// Validate AST for forward compatibility
    pub fn validate_compatibility(
        &self,
        script: &crate::Compiler::AST::DixScript,
    ) -> CompatibilityValidationResult {
        let script_version = super::version_manager::extract_version_from_ast(script);
        let manager = VersionManager::instance().read().unwrap();
        let compiler_version = manager.get_current_version().to_string();

        let mut result = CompatibilityValidationResult {
            is_compatible: false,
            is_newer_version: false,
            script_version: script_version.clone(),
            compiler_version: compiler_version.clone(),
            errors: Vec::new(),
            warnings: Vec::new(),
            handled_elements: Vec::new(),
        };

        let error_manager = ErrorManager::get_shared_instance();
        if error_manager.is_info_enabled() {
            error_manager.log_info(&format!(
                "Validating compatibility: Script v{} with Compiler v{}",
                result.script_version, result.compiler_version
            ));
        }

        if !manager.is_compatible_with(&script_version) {
            if error_manager.is_warning_enabled() {
                error_manager.log_warning(&format!(
                    "Script version {} is newer than compiler",
                    script_version
                ));
            }

            result.is_newer_version = true;

            if script_version != "1.0.0" {
                result.errors.push(format!(
                    "Script version {} not supported by v1.0.0 compiler",
                    script_version
                ));
            }
        } else {
            result.is_compatible = true;
        }

        result.is_compatible = result.errors.is_empty();
        result
    }

    fn handle_strict(&self, element_type: &str, element_name: &str) -> Result<bool, String> {
        let manager = VersionManager::instance().read().unwrap();
        let message = format!(
            "Unknown {} '{}' not supported in v{}",
            element_type,
            element_name,
            manager.get_current_version()
        );

        let error_manager = ErrorManager::get_shared_instance();
        if error_manager.is_error_enabled() {
            error_manager.log_error(&message);
        }

        Err(message)
    }

    fn handle_tolerant(&self, element_type: &str, element_name: &str) -> Result<bool, String> {
        let message = format!("Ignoring unknown {}: {}", element_type, element_name);

        let error_manager = ErrorManager::get_shared_instance();
        if error_manager.is_warning_enabled() {
            error_manager.log_warning(&message);
        }

        Ok(true)
    }

    fn handle_best_effort(&self, element_type: &str, element_name: &str) -> Result<bool, String> {
        let error_manager = ErrorManager::get_shared_instance();
        if error_manager.is_warning_enabled() {
            error_manager.log_warning(&format!(
                "Unknown {} '{}' - using tolerant handling",
                element_type, element_name
            ));
        }

        self.handle_tolerant(element_type, element_name)
    }

    /// Get compatibility info
    pub fn get_compatibility_info(&self) -> HashMap<String, String> {
        let manager = VersionManager::instance().read().unwrap();
        let mut info = HashMap::new();
        info.insert("Mode".to_string(), format!("{:?}", self.mode));
        info.insert(
            "CompilerVersion".to_string(),
            manager.get_current_version().to_string(),
        );
        info.insert(
            "Note".to_string(),
            "v1.0.0 minimal forward compatibility".to_string(),
        );
        info
    }
}

/// Validation result
#[derive(Debug, Clone)]
pub struct CompatibilityValidationResult {
    pub is_compatible: bool,
    pub is_newer_version: bool,
    pub script_version: String,
    pub compiler_version: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub handled_elements: Vec<String>,
}

impl CompatibilityValidationResult {
    pub fn has_issues(&self) -> bool {
        !self.errors.is_empty() || !self.warnings.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

impl std::fmt::Display for CompatibilityValidationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "Compatibility: {}",
            if self.is_compatible {
                "COMPATIBLE"
            } else {
                "INCOMPATIBLE"
            }
        )?;
        writeln!(
            f,
            "Script: v{}, Compiler: v{}",
            self.script_version, self.compiler_version
        )?;

        if self.is_newer_version {
            writeln!(f, "(Script is newer)")?;
        }

        if self.has_errors() {
            writeln!(f, "Errors: {}", self.errors.join("; "))?;
        }

        if self.has_warnings() {
            writeln!(f, "Warnings: {}", self.warnings.join("; "))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compatibility_modes() {
        let strict = ForwardCompatibilityManager::new(CompatibilityMode::Strict);
        let tolerant = ForwardCompatibilityManager::new(CompatibilityMode::Tolerant);

        assert!(strict
            .handle_unknown_element("section", "FUTURE", None, None)
            .is_err());
        assert!(tolerant
            .handle_unknown_element("section", "FUTURE", None, None)
            .is_ok());
    }
}