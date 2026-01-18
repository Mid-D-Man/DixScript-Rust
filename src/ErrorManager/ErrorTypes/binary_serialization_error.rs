//! Binary serialization errors

use super::ErrorSeverity;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinarySerializationErrorType {
    SerializationFailed,
    DeserializationFailed,
    InvalidFormat,
    UnsupportedType,
    DataCorruption,
    SizeLimitExceeded,
    InvalidHeader,
    InvalidFooter,
    ChecksumMismatch,
    VersionMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinarySerializationError {
    pub error_id: String,
    pub error_type: BinarySerializationErrorType,
    pub message: String,
    pub byte_position: Option<usize>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
}

impl BinarySerializationError {
    pub fn new(
        error_type: BinarySerializationErrorType,
        message: String,
        byte_position: Option<usize>,
        suggestion: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXBIN{:?}", error_type);

        Self {
            error_id,
            error_type,
            message,
            byte_position,
            suggestion,
            severity,
        }
    }
}

impl fmt::Display for BinarySerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.error_id, self.message)?;

        if let Some(pos) = self.byte_position {
            write!(f, "\n📍 Byte Position: {}", pos)?;
        }

        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n💡 Suggestion: {}", suggestion)?;
        }

        Ok(())
    }
}