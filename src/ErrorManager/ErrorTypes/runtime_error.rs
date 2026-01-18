//! Runtime execution errors

use super::ErrorSeverity;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    pub error_id: String,
    pub error_type: RuntimeErrorType,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub stack_trace: Option<String>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
}

impl RuntimeError {
    pub fn new(
        error_type: RuntimeErrorType,
        message: String,
        line: usize,
        column: usize,
        stack_trace: Option<String>,
        suggestion: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXRT{:?}L{}C{}", error_type, line, column);

        Self {
            error_id,
            error_type,
            message,
            line,
            column,
            stack_trace,
            suggestion,
            severity,
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} at Line {}, Column {}: {}",
            self.severity, self.error_id, self.line, self.column, self.message
        )?;

        if let Some(ref trace) = self.stack_trace {
            write!(f, "\n📍 Stack Trace:\n{}", trace)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n💡 Suggestion: {}", suggestion)?;
        }

        Ok(())
    }
}