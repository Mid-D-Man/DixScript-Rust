// src/ErrorManager/Helpers/parse_exception.rs

use std::fmt;

/// Exception thrown during parsing
#[derive(Debug, Clone)]
pub struct ParseException {
    message: String,
    inner: Option<Box<ParseException>>,
}

impl ParseException {
    /// Create a new parse exception with a message
    pub fn new(message: impl Into<String>) -> Self {
        ParseException {
            message: message.into(),
            inner: None,
        }
    }

    /// Create a new parse exception with a message and inner exception
    pub fn with_inner(message: impl Into<String>, inner: ParseException) -> Self {
        ParseException {
            message: message.into(),
            inner: Some(Box::new(inner)),
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the inner exception if any
    pub fn inner(&self) -> Option<&ParseException> {
        self.inner.as_ref().map(|b| b.as_ref())
    }
}

impl fmt::Display for ParseException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Parse error: {}", self.message)?;
        if let Some(ref inner) = self.inner {
            write!(f, " | Caused by: {}", inner)?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<String> for ParseException {
    fn from(message: String) -> Self {
        ParseException::new(message)
    }
}

impl From<&str> for ParseException {
    fn from(message: &str) -> Self {
        ParseException::new(message)
    }
}