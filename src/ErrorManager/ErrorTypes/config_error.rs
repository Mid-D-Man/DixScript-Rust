//! Configuration error types and handling

use super::error_enums::ErrorSeverity;
use crate::DixCore::List;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigErrorType {
    MissingRequiredField,
    InvalidFieldValue,
    InvalidVersion,
    InvalidEncoding,
    InvalidFeatures,
    InvalidDebugMode,
    InvalidErrorHandling,
    InvalidCompatibilityMode,
    InvalidDateFormat,
    InvalidTimestampFormat,
    MalformedSection,
    MissingArrowOperator,
    EmptyKey,
    EmptyValue,
    DuplicateKey,
    UnsupportedField,
    ParsingFailed,
}

#[derive(Debug, Clone)]
pub struct ConfigError {
    pub error_id: String,
    pub error_type: ConfigErrorType,
    pub message: String,
    pub field_name: Option<String>,
    pub field_value: Option<String>,
    pub line: i32,
    pub column: i32,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
    pub quick_fixes: List<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl ConfigError {
    pub fn new(
        error_type: ConfigErrorType,
        message: String,
        field_name: Option<String>,
        field_value: Option<String>,
        line: i32,
        column: i32,
        suggestion: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = if line > 0 || column > 0 {
            format!("DXCFG{:03}L{}C{}", error_type as u32, line, column)
        } else {
            format!("DXCFG{:03}", error_type as u32)
        };

        let mut error = Self {
            error_id,
            error_type: error_type.clone(),
            message,
            field_name,
            field_value,
            line,
            column,
            suggestion,
            severity,
            quick_fixes: List::New(),
            metadata: std::collections::HashMap::new(),
        };

        error.generate_quick_fixes(&error_type);
        error
    }

    pub fn generate_suggestion(error_type: &ConfigErrorType, context: &str) -> String {
        match error_type {
            ConfigErrorType::MissingRequiredField => {
                format!("Required field '{}' is missing from @CONFIG section.", context)
            }
            ConfigErrorType::InvalidFieldValue => {
                format!("Invalid value for field '{}'.", context)
            }
            ConfigErrorType::InvalidVersion => {
                format!("Version '{}' is invalid. Use format: 1.0.0 or x_1.0", context)
            }
            ConfigErrorType::InvalidEncoding => {
                format!("Encoding '{}' is not supported. Use utf-8, utf-16, ascii, or iso-8859-1.", context)
            }
            ConfigErrorType::InvalidFeatures => {
                format!("Features '{}' is invalid. Use 'basic', 'advanced', or section list.", context)
            }
            ConfigErrorType::InvalidDebugMode => {
                format!("Debug mode '{}' is invalid. Use: off, regular, or verbose.", context)
            }
            ConfigErrorType::InvalidErrorHandling => {
                format!("Error handling '{}' is invalid. Use: halt, continue, or recover.", context)
            }
            ConfigErrorType::InvalidCompatibilityMode => {
                format!("Compatibility mode '{}' is invalid. Use: strict, best_effort, or permissive.", context)
            }
            ConfigErrorType::InvalidDateFormat => {
                format!("Date '{}' is invalid. Use ISO format: YYYY-MM-DD", context)
            }
            ConfigErrorType::InvalidTimestampFormat => {
                format!("Timestamp '{}' is invalid. Use ISO format: YYYY-MM-DDTHH:mm:ss[Z]", context)
            }
            ConfigErrorType::MalformedSection => {
                "CONFIG section is malformed. Check @CONFIG(...) syntax.".to_string()
            }
            ConfigErrorType::MissingArrowOperator => {
                format!("Entry '{}' is missing arrow operator '->'. Use: field -> value", context)
            }
            ConfigErrorType::EmptyKey => {
                "Empty field name before '->'. Provide field name.".to_string()
            }
            ConfigErrorType::EmptyValue => {
                format!("Empty value for field '{}'. Provide value after '->'.", context)
            }
            ConfigErrorType::DuplicateKey => {
                format!("Field '{}' is defined multiple times. Keep only one definition.", context)
            }
            ConfigErrorType::UnsupportedField => {
                format!("Field '{}' is not recognized in @CONFIG section.", context)
            }
            ConfigErrorType::ParsingFailed => {
                format!("Failed to parse CONFIG section: {}", context)
            }
        }
    }

    fn generate_quick_fixes(&mut self, error_type: &ConfigErrorType) {
        match error_type {
            ConfigErrorType::MissingRequiredField => {
                self.quick_fixes.Add("Add required field to @CONFIG section".to_string());
                self.quick_fixes.Add("Check @CONFIG syntax: field -> value".to_string());
            }
            ConfigErrorType::InvalidFieldValue => {
                self.quick_fixes.Add("Check valid values for this field".to_string());
                self.quick_fixes.Add("Verify field value syntax".to_string());
            }
            ConfigErrorType::InvalidVersion => {
                self.quick_fixes.Add("Use format: x_1.0 or 1.0.0".to_string());
                self.quick_fixes.Add("Check supported version numbers".to_string());
            }
            ConfigErrorType::InvalidEncoding => {
                self.quick_fixes.Add("Use: utf-8, utf-16, ascii, or iso-8859-1".to_string());
                self.quick_fixes.Add("Check encoding name spelling".to_string());
            }
            ConfigErrorType::InvalidFeatures => {
                self.quick_fixes.Add("Use 'basic' or 'advanced'".to_string());
                self.quick_fixes.Add("Or specify section list: quickfuncs,enums,dlm".to_string());
            }
            ConfigErrorType::InvalidDebugMode => {
                self.quick_fixes.Add("Use: off, regular, or verbose".to_string());
                self.quick_fixes.Add("Check debug_mode value".to_string());
            }
            ConfigErrorType::InvalidErrorHandling => {
                self.quick_fixes.Add("Use: halt, continue, or recover".to_string());
                self.quick_fixes.Add("Check error_handling value".to_string());
            }
            ConfigErrorType::InvalidCompatibilityMode => {
                self.quick_fixes.Add("Use: strict, best_effort, or permissive".to_string());
                self.quick_fixes.Add("Check compatibility_mode value".to_string());
            }
            ConfigErrorType::InvalidDateFormat => {
                self.quick_fixes.Add("Use ISO format: YYYY-MM-DD".to_string());
                self.quick_fixes.Add("Example: 2025-11-30".to_string());
            }
            ConfigErrorType::InvalidTimestampFormat => {
                self.quick_fixes.Add("Use ISO format: YYYY-MM-DDTHH:mm:ss[Z|±HH:mm]".to_string());
                self.quick_fixes.Add("Example: 2025-11-30T14:30:00Z".to_string());
            }
            ConfigErrorType::MalformedSection => {
                self.quick_fixes.Add("Check @CONFIG(...) syntax".to_string());
                self.quick_fixes.Add("Verify opening and closing parentheses".to_string());
            }
            ConfigErrorType::MissingArrowOperator => {
                self.quick_fixes.Add("Use arrow syntax: field -> value".to_string());
                self.quick_fixes.Add("Separate fields with commas".to_string());
            }
            ConfigErrorType::EmptyKey => {
                self.quick_fixes.Add("Provide field name before '->'".to_string());
                self.quick_fixes.Add("Remove empty entry".to_string());
            }
            ConfigErrorType::EmptyValue => {
                self.quick_fixes.Add("Provide value after '->'".to_string());
                self.quick_fixes.Add("Use quotes for empty string: \"\"".to_string());
            }
            ConfigErrorType::DuplicateKey => {
                self.quick_fixes.Add("Remove duplicate field definition".to_string());
                self.quick_fixes.Add("Keep only one definition per field".to_string());
            }
            _ => {}
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "[{}] {}: {:?}",
            self.error_id, self.severity, self.error_type
        )?;

        if let Some(ref field) = self.field_name {
            writeln!(f, "Field: {}", field)?;
        }

        if let Some(ref value) = self.field_value {
            writeln!(f, "Value: {}", value)?;
        }

        if self.line > 0 || self.column > 0 {
            writeln!(f, "Location: Line {}, Column {}", self.line, self.column)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.IsEmpty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in self.quick_fixes.Iter() {
                writeln!(f, "  - {}", fix)?;
            }
        }

        Ok(())
    }
}