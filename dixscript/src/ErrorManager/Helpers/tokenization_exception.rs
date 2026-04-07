
use std::fmt;

/// Exception thrown during tokenization/lexical analysis
#[derive(Debug, Clone)]
pub struct TokenizationException {
    message: String,
    inner: Option<Box<TokenizationException>>,
}

impl TokenizationException {
    /// Create a new tokenization exception with a message
    pub fn new(message: impl Into<String>) -> Self {
        TokenizationException {
            message: message.into(),
            inner: None,
        }
    }

    /// Create a new tokenization exception with a message and inner exception
    pub fn with_inner(message: impl Into<String>, inner: TokenizationException) -> Self {
        TokenizationException {
            message: message.into(),
            inner: Some(Box::new(inner)),
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the inner exception if any
    pub fn inner(&self) -> Option<&TokenizationException> {
        self.inner.as_ref().map(|b| b.as_ref())
    }
}

impl fmt::Display for TokenizationException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tokenization error: {}", self.message)?;
        if let Some(ref inner) = self.inner {
            write!(f, " | Caused by: {}", inner)?;
        }
        Ok(())
    }
}

impl std::error::Error for TokenizationException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<String> for TokenizationException {
    fn from(message: String) -> Self {
        TokenizationException::new(message)
    }
}

impl From<&str> for TokenizationException {
    fn from(message: &str) -> Self {
        TokenizationException::new(message)
    }
}