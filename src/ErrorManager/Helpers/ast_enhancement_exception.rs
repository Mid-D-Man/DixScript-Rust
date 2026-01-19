// src/ErrorManager/helpers/ast_enhancement_exception.rs

use std::fmt;

/// Exception thrown during AST enhancement
#[derive(Debug, Clone)]
pub struct AstEnhancementException {
    message: String,
    section_name: Option<String>,
    parameter_name: Option<String>,
    inner: Option<Box<AstEnhancementException>>,
}

impl AstEnhancementException {
    /// Create a new AST enhancement exception with a message
    pub fn new(message: impl Into<String>) -> Self {
        AstEnhancementException {
            message: message.into(),
            section_name: None,
            parameter_name: None,
            inner: None,
        }
    }

    /// Create a new AST enhancement exception with context
    pub fn with_context(
        message: impl Into<String>,
        section_name: impl Into<String>,
        parameter_name: Option<String>,
    ) -> Self {
        AstEnhancementException {
            message: message.into(),
            section_name: Some(section_name.into()),
            parameter_name,
            inner: None,
        }
    }

    /// Create a new AST enhancement exception with inner exception
    pub fn with_inner(message: impl Into<String>, inner: AstEnhancementException) -> Self {
        AstEnhancementException {
            message: message.into(),
            section_name: None,
            parameter_name: None,
            inner: Some(Box::new(inner)),
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the section name if available
    pub fn section_name(&self) -> Option<&str> {
        self.section_name.as_deref()
    }

    /// Get the parameter name if available
    pub fn parameter_name(&self) -> Option<&str> {
        self.parameter_name.as_deref()
    }

    /// Get the inner exception if any
    pub fn inner(&self) -> Option<&AstEnhancementException> {
        self.inner.as_ref().map(|b| b.as_ref())
    }
}

impl fmt::Display for AstEnhancementException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AST enhancement error: {}", self.message)?;

        if let Some(ref section) = self.section_name {
            write!(f, " (section: {})", section)?;
        }

        if let Some(ref param) = self.parameter_name {
            write!(f, " (parameter: {})", param)?;
        }

        if let Some(ref inner) = self.inner {
            write!(f, " | Caused by: {}", inner)?;
        }

        Ok(())
    }
}

impl std::error::Error for AstEnhancementException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<String> for AstEnhancementException {
    fn from(message: String) -> Self {
        AstEnhancementException::new(message)
    }
}

impl From<&str> for AstEnhancementException {
    fn from(message: &str) -> Self {
        AstEnhancementException::new(message)
    }
}