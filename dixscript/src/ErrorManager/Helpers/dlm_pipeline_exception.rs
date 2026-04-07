
use crate::ErrorManager::DlmErrorType;
use std::fmt;

/// DLM Pipeline Exception for error propagation in DLM modules
#[derive(Debug, Clone)]
pub struct DLMPipelineException {
    pub module_name: String,
    pub error_type: DlmErrorType,
    message: String,
    inner: Option<String>,
}

impl DLMPipelineException {
    /// Create a new DLM pipeline exception
    pub fn new(
        module_name: impl Into<String>,
        error_type: DlmErrorType,
        message: impl Into<String>,
    ) -> Self {
        DLMPipelineException {
            module_name: module_name.into(),
            error_type,
            message: message.into(),
            inner: None,
        }
    }

    /// Create a new DLM pipeline exception with inner error
    pub fn with_inner(
        module_name: impl Into<String>,
        error_type: DlmErrorType,
        message: impl Into<String>,
        inner_error: impl Into<String>,
    ) -> Self {
        DLMPipelineException {
            module_name: module_name.into(),
            error_type,
            message: message.into(),
            inner: Some(inner_error.into()),
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the inner error if any
    pub fn inner(&self) -> Option<&str> {
        self.inner.as_deref()
    }
}

impl fmt::Display for DLMPipelineException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {:?}: {}", self.module_name, self.error_type, self.message)?;
        if let Some(ref inner) = self.inner {
            write!(f, " | Inner: {}", inner)?;
        }
        Ok(())
    }
}

impl std::error::Error for DLMPipelineException {}