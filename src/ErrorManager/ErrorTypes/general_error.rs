//! General error types and handling

use super::error_enums::ErrorSeverity;
use std::fmt;

#[derive(Debug, Clone)]
pub struct GeneralError {
    pub error_id: String,
    pub message: String,
    pub source: Option<String>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
    pub inner_exception: Option<String>,
}

impl GeneralError {
    pub fn new(
        message: String,
        source: Option<String>,
        suggestion: Option<String>,
        inner_exception: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        // Generate unique error ID using timestamp-based approach
        let error_id = format!(
            "DXGEN{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .to_string()
                .chars()
                .rev()
                .take(8)
                .collect::<String>()
        );

        Self {
            error_id,
            message,
            source,
            suggestion,
            inner_exception,
            severity,
        }
    }
}

impl fmt::Display for GeneralError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "[{}] {}: General Error",
            self.error_id, self.severity
        )?;

        if let Some(ref source) = self.source {
            writeln!(f, "Source: {}", source)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref inner) = self.inner_exception {
            writeln!(f, "Inner Exception: {}", inner)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        Ok(())
    }
}