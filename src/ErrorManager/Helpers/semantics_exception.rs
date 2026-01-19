// src/ErrorManager/Helpers/semantics_exception.rs

use std::fmt;

/// Exception thrown during semantic analysis
#[derive(Debug, Clone)]
pub struct SemanticsException {
    message: String,
    inner: Option<Box<SemanticsException>>,
}

impl SemanticsException {
    /// Create a new semantics exception with a message
    pub fn new(message: impl Into<String>) -> Self {
        SemanticsException {
            message: message.into(),
            inner: None,
        }
    }

    /// Create a new semantics exception with a message and inner exception
    pub fn with_inner(message: impl Into<String>, inner: SemanticsException) -> Self {
        SemanticsException {
            message: message.into(),
            inner: Some(Box::new(inner)),
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the inner exception if any
    pub fn inner(&self) -> Option<&SemanticsException> {
        self.inner.as_ref().map(|b| b.as_ref())
    }
}

impl fmt::Display for SemanticsException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Semantic error: {}", self.message)?;
        if let Some(ref inner) = self.inner {
            write!(f, " | Caused by: {}", inner)?;
        }
        Ok(())
    }
}

impl std::error::Error for SemanticsException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<String> for SemanticsException {
    fn from(message: String) -> Self {
        SemanticsException::new(message)
    }
}

impl From<&str> for SemanticsException {
    fn from(message: &str) -> Self {
        SemanticsException::new(message)
    }
}