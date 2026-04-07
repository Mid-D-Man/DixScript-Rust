
use std::fmt;

/// Exception thrown during binary serialization/deserialization
#[derive(Debug, Clone)]
pub struct BinarySerializationException {
    message: String,
    file_path: Option<String>,
    byte_position: Option<u64>,
    inner: Option<Box<BinarySerializationException>>,
}

impl BinarySerializationException {
    /// Create a new binary serialization exception with a message
    pub fn new(message: impl Into<String>) -> Self {
        BinarySerializationException {
            message: message.into(),
            file_path: None,
            byte_position: None,
            inner: None,
        }
    }

    /// Create a new binary serialization exception with file context
    pub fn with_file_context(
        message: impl Into<String>,
        file_path: impl Into<String>,
        byte_position: Option<u64>,
    ) -> Self {
        BinarySerializationException {
            message: message.into(),
            file_path: Some(file_path.into()),
            byte_position,
            inner: None,
        }
    }

    /// Create a new binary serialization exception with inner exception
    pub fn with_inner(message: impl Into<String>, inner: BinarySerializationException) -> Self {
        BinarySerializationException {
            message: message.into(),
            file_path: None,
            byte_position: None,
            inner: Some(Box::new(inner)),
        }
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the file path if available
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    /// Get the byte position if available
    pub fn byte_position(&self) -> Option<u64> {
        self.byte_position
    }

    /// Get the inner exception if any
    pub fn inner(&self) -> Option<&BinarySerializationException> {
        self.inner.as_ref().map(|b| b.as_ref())
    }
}

impl fmt::Display for BinarySerializationException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Binary serialization error: {}", self.message)?;

        if let Some(ref path) = self.file_path {
            write!(f, " (file: {})", path)?;
        }

        if let Some(pos) = self.byte_position {
            write!(f, " (position: 0x{:X})", pos)?;
        }

        if let Some(ref inner) = self.inner {
            write!(f, " | Caused by: {}", inner)?;
        }

        Ok(())
    }
}

impl std::error::Error for BinarySerializationException {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.as_ref().map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<String> for BinarySerializationException {
    fn from(message: String) -> Self {
        BinarySerializationException::new(message)
    }
}

impl From<&str> for BinarySerializationException {
    fn from(message: &str) -> Self {
        BinarySerializationException::new(message)
    }
}