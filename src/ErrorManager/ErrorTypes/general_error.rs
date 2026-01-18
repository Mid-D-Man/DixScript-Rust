//! General uncategorized errors

use super::ErrorSeverity;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralError {
    pub error_id: String,
    pub message: String,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
    pub timestamp: String,
}

impl GeneralError {
    pub fn new(message: String, suggestion: Option<String>, severity: ErrorSeverity) -> Self {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        let error_id = format!("DXGEN{}", timestamp.replace([':', '-', ' ', '.'], ""));

        Self {
            error_id,
            message,
            suggestion,
            severity,
            timestamp,
        }
    }
}

impl fmt::Display for GeneralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} at {}: {}",
            self.severity, self.error_id, self.timestamp, self.message
        )?;

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n💡 Suggestion: {}", suggestion)?;
        }

        Ok(())
    }
}