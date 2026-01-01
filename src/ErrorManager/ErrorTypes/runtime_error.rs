//! Runtime error types and handling

use super::error_enums::ErrorSeverity;
use crate::DixCore::List;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeErrorType {
    NullReference,
    IndexOutOfRange,
    InvalidCast,
    DivisionByZero,
    StackOverflow,
    OutOfMemory,
    TimeoutExceeded,
    InvalidOperation,
    ResourceNotFound,
    AccessViolation,
    UnhandledException,
}

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub error_id: String,
    pub error_type: RuntimeErrorType,
    pub message: String,
    pub function_name: Option<String>,
    pub location: Option<String>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
    pub quick_fixes: List<String>,
    pub metadata: std::collections::HashMap<String, String>,
    pub inner_exception: Option<String>,
}

impl RuntimeError {
    pub fn new(
        error_type: RuntimeErrorType,
        message: String,
        function_name: Option<String>,
        location: Option<String>,
        suggestion: Option<String>,
        inner_exception: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXRT{:03}", error_type as u32);

        let mut error = Self {
            error_id,
            error_type: error_type.clone(),
            message,
            function_name,
            location,
            suggestion,
            inner_exception,
            severity,
            quick_fixes: List::New(),
            metadata: std::collections::HashMap::new(),
        };

        error.generate_quick_fixes(&error_type);
        error
    }

    fn generate_quick_fixes(&mut self, error_type: &RuntimeErrorType) {
        match error_type {
            RuntimeErrorType::NullReference => {
                self.quick_fixes.Add("Check for null before access".to_string());
                self.quick_fixes.Add("Use null-conditional operator".to_string());
            }
            RuntimeErrorType::IndexOutOfRange => {
                self.quick_fixes.Add("Verify array/list bounds".to_string());
                self.quick_fixes.Add("Add bounds checking".to_string());
            }
            RuntimeErrorType::InvalidCast => {
                self.quick_fixes.Add("Check type compatibility".to_string());
                self.quick_fixes.Add("Use type checking before cast".to_string());
            }
            RuntimeErrorType::DivisionByZero => {
                self.quick_fixes.Add("Add zero check before division".to_string());
                self.quick_fixes.Add("Handle edge case".to_string());
            }
            RuntimeErrorType::StackOverflow => {
                self.quick_fixes.Add("Check for infinite recursion".to_string());
                self.quick_fixes.Add("Add recursion limit".to_string());
            }
            RuntimeErrorType::OutOfMemory => {
                self.quick_fixes.Add("Reduce data size".to_string());
                self.quick_fixes.Add("Optimize memory usage".to_string());
            }
            RuntimeErrorType::TimeoutExceeded => {
                self.quick_fixes.Add("Optimize algorithm".to_string());
                self.quick_fixes.Add("Increase timeout limit".to_string());
            }
            _ => {}
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "[{}] {}: {:?}",
            self.error_id, self.severity, self.error_type
        )?;

        if let Some(ref func) = self.function_name {
            writeln!(f, "Function: {}", func)?;
        }

        if let Some(ref loc) = self.location {
            writeln!(f, "Location: {}", loc)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref inner) = self.inner_exception {
            writeln!(f, "Inner Exception: {}", inner)?;
        }

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