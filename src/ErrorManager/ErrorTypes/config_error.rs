use std::fmt;

/// Configuration error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigErrorType {
    MissingRequiredSection,
    InvalidSectionFormat,
    DuplicateSection,
    InvalidConfiguration,
    ValidationFailed,
    InvalidValue,
    MissingRequiredField,
    InvalidFieldType,
    ConstraintViolation,
    SchemaValidationFailed,
    InvalidSectionName,
    CircularReference,
    IncompatibleVersions,
    InvalidVersion,
}

/// Configuration error with validation context
#[derive(Debug, Clone)]
pub struct ConfigError {
    pub error_id: String,
    pub error_type: ConfigErrorType,
    pub message: String,
    pub section_name: Option<String>,
    pub field_name: Option<String>,
    pub expected_value: Option<String>,
    pub actual_value: Option<String>,
    pub line: i32,
    pub column: i32,
    pub suggestion: Option<String>,
    pub severity: super::ErrorSeverity,
    pub quick_fixes: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl ConfigError {
    pub fn new(
        error_type: ConfigErrorType,
        message: String,
        section_name: Option<String>,
        field_name: Option<String>,
        expected_value: Option<String>,
        actual_value: Option<String>,
        line: i32,
        column: i32,
        suggestion: Option<String>,
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = if line > 0 || column > 0 {
            format!("DXCFG{:03}L{}C{}", error_type as u32, line, column)
        } else {
            format!("DXCFG{:03}", error_type as u32)
        };

        let quick_fixes = Self::generate_quick_fixes(error_type);

        ConfigError {
            error_id,
            error_type,
            message,
            section_name,
            field_name,
            expected_value,
            actual_value,
            line,
            column,
            suggestion,
            severity,
            quick_fixes,
            metadata: std::collections::HashMap::new(),
        }
    }

    fn generate_quick_fixes(error_type: ConfigErrorType) -> Vec<String> {
        match error_type {
            ConfigErrorType::MissingRequiredSection => vec![
                "Add missing section to configuration".to_string(),
                "Check section name spelling".to_string(),
            ],
            ConfigErrorType::InvalidSectionFormat => vec![
                "Check section syntax".to_string(),
                "Verify section follows DixScript format".to_string(),
            ],
            ConfigErrorType::DuplicateSection => vec![
                "Remove duplicate section".to_string(),
                "Merge duplicate sections".to_string(),
            ],
            ConfigErrorType::InvalidValue => vec![
                "Check value type matches expected".to_string(),
                "Verify value format".to_string(),
            ],
            ConfigErrorType::MissingRequiredField => vec![
                "Add missing field".to_string(),
                "Check field name spelling".to_string(),
            ],
            ConfigErrorType::InvalidFieldType => vec![
                "Use correct type for field".to_string(),
                "Check type definition".to_string(),
            ],
            ConfigErrorType::ConstraintViolation => vec![
                "Check value meets constraints".to_string(),
                "Review validation rules".to_string(),
            ],
            ConfigErrorType::CircularReference => vec![
                "Break circular dependency".to_string(),
                "Restructure configuration".to_string(),
            ],
            _ => Vec::new(),
        }
    }

    /// Generate suggestion for config error
    pub fn generate_suggestion(
        error_type: ConfigErrorType,
        section_name: Option<&str>,
        field_name: Option<&str>,
    ) -> String {
        match error_type {
            ConfigErrorType::MissingRequiredSection => {
                format!("Required section '{}' is missing from configuration.",
                        section_name.unwrap_or("unknown"))
            }
            ConfigErrorType::InvalidSectionFormat => {
                format!("Section '{}' has invalid format. Check syntax.",
                        section_name.unwrap_or("unknown"))
            }
            ConfigErrorType::DuplicateSection => {
                format!("Section '{}' is defined multiple times. Remove duplicates.",
                        section_name.unwrap_or("unknown"))
            }
            ConfigErrorType::InvalidValue => {
                format!("Invalid value for field '{}' in section '{}'.",
                        field_name.unwrap_or("unknown"),
                        section_name.unwrap_or("unknown"))
            }
            ConfigErrorType::MissingRequiredField => {
                format!("Required field '{}' is missing in section '{}'.",
                        field_name.unwrap_or("unknown"),
                        section_name.unwrap_or("unknown"))
            }
            ConfigErrorType::InvalidFieldType => {
                format!("Field '{}' has incorrect type.",
                        field_name.unwrap_or("unknown"))
            }
            ConfigErrorType::ConstraintViolation => {
                format!("Field '{}' violates validation constraint.",
                        field_name.unwrap_or("unknown"))
            }
            ConfigErrorType::CircularReference => {
                "Circular reference detected in configuration.".to_string()
            }
            ConfigErrorType::IncompatibleVersions => {
                "Configuration version incompatible with current DixScript version.".to_string()
            }
            _ => "Configuration validation failed.".to_string(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[{}] {:?}: {:?}", self.error_id, self.severity, self.error_type)?;

        if let Some(ref section) = self.section_name {
            writeln!(f, "Section: {}", section)?;
        }

        if let Some(ref field) = self.field_name {
            writeln!(f, "Field: {}", field)?;
        }

        if self.line > 0 || self.column > 0 {
            writeln!(f, "Location: Line {}, Column {}", self.line, self.column)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref expected) = self.expected_value {
            writeln!(f, "Expected: {}", expected)?;
        }

        if let Some(ref actual) = self.actual_value {
            writeln!(f, "Actual: {}", actual)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.is_empty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in &self.quick_fixes {
                writeln!(f, "  - {}", fix)?;
            }
        }

        Ok(())
    }
}