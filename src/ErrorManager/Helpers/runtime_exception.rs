// src/ErrorManager/helpers/runtime_exception.rs

use std::fmt;

/// Exception thrown during runtime execution
#[derive(Debug, Clone)]
pub struct RuntimeException {
    message: String,
    function_name: Option<String>,
    stack_trace: Vec<String>,
    inner: Option<Box<RuntimeException>>,
}

impl RuntimeException {
    /// Create a new runtime exception with a message
    pub fn new(message: impl Into<String>) -> Self {
        RuntimeException {
            message: message.into(),
            function_name: None,
            stack_trace: Vec::new(),
            inner: None,
        }
    }

    /// Create a new runtime exception with execution context
    pub fn with_context(
        message: impl Into<String>,
        function_name: impl Into<String>,
        stack_trace: Vec<String>,
    ) -> Self {
        RuntimeException {
            message: message.into(),
            function_name: Some(function_name.into()),
            stack_trace,
            inner: None,
        }
    }

    /// Create a new runtime exception with inner exception
    pub fn with_inner(message: impl Into<String>, inner: RuntimeException) -> Self {
        RuntimeException {
            message: message.into(),
            function_name: None,
            stack_trace: Vec::new(),
            inner: Some(Box::new(inner)),
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the function name if available
    pub fn function_name(&self) -> Option<&str> {
        self.function_name.as_deref()
    }

    /// Get the stack trace
    pub fn stack_trace(&self) -> &[String] {
        &self.stack_trace
    }

    /// Get the inner exception if any
    pub fn inner(&self) -> Option<&RuntimeException> {
        self.inner.as_ref().map(|b| b.as_ref())
    }

    /// Add a stack frame to the trace
    pub fn add_stack_frame(&mut self, frame: impl Into<String>) {
        self.stack_trace.push(frame.into());
    }

    /// Set the function name
    pub fn set_function_name(&mut self, name: impl Into<String>) {
        self.function_name = Some(name.into());
    }
}

impl fmt::Display for RuntimeException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Runtime error: {}", self.message)?;

        if let Some(ref func) = self.function_name {
            write!(f, " (in function: {})", func)?;
        }

        if !self.stack_trace.is_empty() {
            write!(f, "\nStack trace:")?;
            for (i, frame) in self.stack_trace.iter().enumerate() {
                write!(f, "\n  {} at {}", i, frame)?;
            }
        }

        if let Some(ref inner) = self.inner {
            write!(f, "\nCaused by: {}", inner)?;
        }

        Ok(())
    }
}

impl std::error::Error for RuntimeException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<String> for RuntimeException {
    fn from(message: String) -> Self {
        RuntimeException::new(message)
    }
}

impl From<&str> for RuntimeException {
    fn from(message: &str) -> Self {
        RuntimeException::new(message)
    }
}