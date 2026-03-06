// src/ErrorManager/helpers/value_resolution_exception.rs

use std::fmt;

/// Exception thrown during value resolution
#[derive(Debug, Clone)]
pub struct ValueResolutionException {
    message: String,
    variable_name: Option<String>,
    function_name: Option<String>,
    inner: Option<Box<ValueResolutionException>>,
}

impl ValueResolutionException {
    /// Create a new value resolution exception with a message
    pub fn new(message: impl Into<String>) -> Self {
        ValueResolutionException {
            message: message.into(),
            variable_name: None,
            function_name: None,
            inner: None,
        }
    }

    /// Create a new value resolution exception with context
    pub fn with_context(
        message: impl Into<String>,
        variable_name: Option<String>,
        function_name: Option<String>,
    ) -> Self {
        ValueResolutionException {
            message: message.into(),
            variable_name,
            function_name,
            inner: None,
        }
    }

    /// Create a new value resolution exception with inner exception
    pub fn with_inner(message: impl Into<String>, inner: ValueResolutionException) -> Self {
        ValueResolutionException {
            message: message.into(),
            variable_name: None,
            function_name: None,
            inner: Some(Box::new(inner)),
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the variable name if available
    pub fn variable_name(&self) -> Option<&str> {
        self.variable_name.as_deref()
    }

    /// Get the function name if available
    pub fn function_name(&self) -> Option<&str> {
        self.function_name.as_deref()
    }

    /// Get the inner exception if any
    pub fn inner(&self) -> Option<&ValueResolutionException> {
        self.inner.as_ref().map(|b| b.as_ref())
    }
}

impl fmt::Display for ValueResolutionException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Value resolution error: {}", self.message)?;

        if let Some(ref var) = self.variable_name {
            write!(f, " (variable: {})", var)?;
        }

        if let Some(ref func) = self.function_name {
            write!(f, " (function: {})", func)?;
        }

        if let Some(ref inner) = self.inner {
            write!(f, " | Caused by: {}", inner)?;
        }

        Ok(())
    }
}

impl std::error::Error for ValueResolutionException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<String> for ValueResolutionException {
    fn from(message: String) -> Self {
        ValueResolutionException::new(message)
    }
}

impl From<&str> for ValueResolutionException {
    fn from(message: &str) -> Self {
        ValueResolutionException::new(message)
    }
}