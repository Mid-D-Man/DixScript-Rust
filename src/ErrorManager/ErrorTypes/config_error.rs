//! @CONFIG section parsing errors

use super::ErrorSeverity;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub error_id: String,
    pub error_type: ConfigErrorType,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub field_name: Option<String>,
    pub invalid_value: Option<String>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
}

impl ConfigError {
    pub fn new(
        error_type: ConfigErrorType,
        message: String,
        line: usize,
        column: usize,
        field_name: Option<String>,
        invalid_value: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXCFG{:?}L{}C{}", error_type, line, column);
        let suggestion = Self::generate_suggestion(error_type, field_name.as_deref(), invalid_value.as_deref());

        Self {
            error_id,
            error_type,
            message,
            line,
            column,
            field_name,
            invalid_value,
            suggestion: Some(suggestion),
            severity,
        }
    }

    #[inline]
    fn generate_suggestion(
        error_type: ConfigErrorType,
        field_name: Option<&str>,
        invalid_value: Option<&str>,
    ) -> String {
        match error_type {
            ConfigErrorType::InvalidVersion => {
                "Version must be in format: MAJOR.MINOR.PATCH (e.g., 1.0.0)".to_string()
            }
            ConfigErrorType::InvalidEncoding => {
                "Valid encodings: utf8, utf16, ascii".to_string()
            }
            ConfigErrorType::InvalidDebugMode => {
                "Valid debug modes: 0 (off), 1 (regular), 2 (verbose)".to_string()
            }
            ConfigErrorType::InvalidErrorHandling => {
                "Valid values: halt, continue, recover".to_string()
            }
            ConfigErrorType::InvalidCompatibilityMode => {
                "Valid values: strict, best_effort, permissive".to_string()
            }
            ConfigErrorType::MissingArrowOperator => {
                "Each config line must use: key => value".to_string()
            }
            ConfigErrorType::DuplicateKey => {
                if let Some(field) = field_name {
                    format!("Field '{}' is already defined", field)
                } else {
                    "Remove duplicate configuration key".to_string()
                }
            }
            ConfigErrorType::InvalidFieldValue => {
                if let Some(field) = field_name {
                    if let Some(value) = invalid_value {
                        format!("'{}' is not a valid value for '{}'", value, field)
                    } else {
                        format!("Invalid value for field '{}'", field)
                    }
                } else {
                    "Check the field value format".to_string()
                }
            }
            _ => "Check @CONFIG section syntax".to_string(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} at Line {}, Column {}: {}",
            self.severity, self.error_id, self.line, self.column, self.message
        )?;

        if let Some(ref field) = self.field_name {
            write!(f, "\n📍 Field: {}", field)?;
        }

        if let Some(ref value) = self.invalid_value {
            write!(f, "\n📍 Invalid Value: {}", value)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n💡 Suggestion: {}", suggestion)?;
        }

        Ok(())
    }
}