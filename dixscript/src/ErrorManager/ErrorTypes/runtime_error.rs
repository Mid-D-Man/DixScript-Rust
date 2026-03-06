use std::fmt;

/// Runtime error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeErrorType {
    NullReference,
    IndexOutOfBounds,
    DivisionByZero,
    StackOverflow,
    InvalidCast,
    InvalidOperation,
    ResourceNotFound,
    PermissionDenied,
    TimeoutExpired,
    InvalidState,
    NotImplemented,
    UnsupportedOperation,
    ExternalCallFailed,
    MemoryAllocationFailed,
    InvalidArgument,
    AssertionFailed,
}

/// Runtime error with execution context
#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub error_id: String,
    pub error_type: RuntimeErrorType,
    pub message: String,
    pub function_name: Option<String>,
    pub line: i32,
    pub column: i32,
    pub stack_trace: Vec<String>,
    pub suggestion: Option<String>,
    pub severity: super::ErrorSeverity,
    pub quick_fixes: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl RuntimeError {
    pub fn new(
        error_type: RuntimeErrorType,
        message: String,
        function_name: Option<String>,
        line: i32,
        column: i32,
        stack_trace: Vec<String>,
        suggestion: Option<String>,
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = if line > 0 || column > 0 {
            format!("DXRT{:03}L{}C{}", error_type as u32, line, column)
        } else {
            format!("DXRT{:03}", error_type as u32)
        };

        let quick_fixes = Self::generate_quick_fixes(error_type);

        RuntimeError {
            error_id,
            error_type,
            message,
            function_name,
            line,
            column,
            stack_trace,
            suggestion,
            severity,
            quick_fixes,
            metadata: std::collections::HashMap::new(),
        }
    }

    fn generate_quick_fixes(error_type: RuntimeErrorType) -> Vec<String> {
        match error_type {
            RuntimeErrorType::NullReference => vec![
                "Check value is initialized before use".to_string(),
                "Add null check".to_string(),
            ],
            RuntimeErrorType::IndexOutOfBounds => vec![
                "Check array/list bounds before access".to_string(),
                "Verify index value is valid".to_string(),
            ],
            RuntimeErrorType::DivisionByZero => vec![
                "Add zero check before division".to_string(),
                "Ensure divisor is non-zero".to_string(),
            ],
            RuntimeErrorType::StackOverflow => vec![
                "Check for infinite recursion".to_string(),
                "Add base case to recursive function".to_string(),
                "Reduce recursion depth".to_string(),
            ],
            RuntimeErrorType::InvalidCast => vec![
                "Check type compatibility".to_string(),
                "Use proper type conversion".to_string(),
            ],
            RuntimeErrorType::InvalidOperation => vec![
                "Verify operation is supported".to_string(),
                "Check operand types".to_string(),
            ],
            RuntimeErrorType::ResourceNotFound => vec![
                "Check resource path".to_string(),
                "Verify resource exists".to_string(),
            ],
            RuntimeErrorType::PermissionDenied => vec![
                "Check file/resource permissions".to_string(),
                "Run with appropriate privileges".to_string(),
            ],
            RuntimeErrorType::TimeoutExpired => vec![
                "Increase timeout value".to_string(),
                "Check for blocking operations".to_string(),
            ],
            RuntimeErrorType::AssertionFailed => vec![
                "Check assertion condition".to_string(),
                "Verify program state".to_string(),
            ],
            _ => Vec::new(),
        }
    }

    /// Generate suggestion for runtime error
    pub fn generate_suggestion(
        error_type: RuntimeErrorType,
        context: &str,
    ) -> String {
        match error_type {
            RuntimeErrorType::NullReference => {
                format!("Null reference encountered for '{}'. Ensure value is initialized.", context)
            }
            RuntimeErrorType::IndexOutOfBounds => {
                format!("Index out of bounds for '{}'. Check array/list size.", context)
            }
            RuntimeErrorType::DivisionByZero => {
                "Division by zero is not allowed. Check divisor value.".to_string()
            }
            RuntimeErrorType::StackOverflow => {
                format!("Stack overflow in '{}'. Check for infinite recursion.", context)
            }
            RuntimeErrorType::InvalidCast => {
                format!("Invalid type cast for '{}'. Check type compatibility.", context)
            }
            RuntimeErrorType::InvalidOperation => {
                format!("Invalid operation on '{}'.", context)
            }
            RuntimeErrorType::ResourceNotFound => {
                format!("Resource '{}' not found.", context)
            }
            RuntimeErrorType::PermissionDenied => {
                format!("Permission denied accessing '{}'.", context)
            }
            RuntimeErrorType::TimeoutExpired => {
                format!("Operation '{}' timed out.", context)
            }
            RuntimeErrorType::InvalidState => {
                format!("Invalid state for operation '{}'.", context)
            }
            RuntimeErrorType::NotImplemented => {
                format!("Feature '{}' is not implemented.", context)
            }
            RuntimeErrorType::UnsupportedOperation => {
                format!("Operation '{}' is not supported.", context)
            }
            RuntimeErrorType::AssertionFailed => {
                format!("Assertion failed: {}", context)
            }
            _ => format!("Runtime error: {}", context),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[{}] {:?}: {:?}", self.error_id, self.severity, self.error_type)?;

        if let Some(ref func) = self.function_name {
            writeln!(f, "Function: {}", func)?;
        }

        if self.line > 0 || self.column > 0 {
            writeln!(f, "Location: Line {}, Column {}", self.line, self.column)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if !self.stack_trace.is_empty() {
            writeln!(f, "Stack Trace:")?;
            for frame in &self.stack_trace {
                writeln!(f, "  at {}", frame)?;
            }
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