// src/ErrorManager/helpers/imports_resolution_exception.rs

use std::fmt;

/// Exception thrown during imports resolution
#[derive(Debug, Clone)]
pub struct ImportsResolutionException {
    message: String,
    import_alias: Option<String>,
    import_path: Option<String>,
    inner: Option<Box<ImportsResolutionException>>,
}

impl ImportsResolutionException {
    /// Create a new imports resolution exception with a message
    pub fn new(message: impl Into<String>) -> Self {
        ImportsResolutionException {
            message: message.into(),
            import_alias: None,
            import_path: None,
            inner: None,
        }
    }

    /// Create a new imports resolution exception with import context
    pub fn with_import_context(
        message: impl Into<String>,
        import_alias: impl Into<String>,
        import_path: impl Into<String>,
    ) -> Self {
        ImportsResolutionException {
            message: message.into(),
            import_alias: Some(import_alias.into()),
            import_path: Some(import_path.into()),
            inner: None,
        }
    }

    /// Create a new imports resolution exception with inner exception
    pub fn with_inner(message: impl Into<String>, inner: ImportsResolutionException) -> Self {
        ImportsResolutionException {
            message: message.into(),
            import_alias: None,
            import_path: None,
            inner: Some(Box::new(inner)),
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the import alias if available
    pub fn import_alias(&self) -> Option<&str> {
        self.import_alias.as_deref()
    }

    /// Get the import path if available
    pub fn import_path(&self) -> Option<&str> {
        self.import_path.as_deref()
    }

    /// Get the inner exception if any
    pub fn inner(&self) -> Option<&ImportsResolutionException> {
        self.inner.as_ref().map(|b| b.as_ref())
    }
}

impl fmt::Display for ImportsResolutionException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Imports resolution error: {}", self.message)?;

        if let Some(ref alias) = self.import_alias {
            write!(f, " (alias: {})", alias)?;
        }

        if let Some(ref path) = self.import_path {
            write!(f, " (path: {})", path)?;
        }

        if let Some(ref inner) = self.inner {
            write!(f, " | Caused by: {}", inner)?;
        }

        Ok(())
    }
}

impl std::error::Error for ImportsResolutionException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<String> for ImportsResolutionException {
    fn from(message: String) -> Self {
        ImportsResolutionException::new(message)
    }
}

impl From<&str> for ImportsResolutionException {
    fn from(message: &str) -> Self {
        ImportsResolutionException::new(message)
    }
}