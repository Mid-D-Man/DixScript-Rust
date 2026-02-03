//! Error type for binary serialization operations

use std::fmt;
use crate::Compiler::AST::Position;
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;

/// Binary serialization error with position tracking
#[derive(Debug, Clone)]
pub struct BinarySerializationError {
    pub error_type: BinarySerializationErrorType,
    pub message: String,
    pub location: String,
    pub position: Option<Position>,
}

impl BinarySerializationError {
    /// Create new error
    pub fn new(
        error_type: BinarySerializationErrorType,
        message: impl Into<String>,
        location: impl Into<String>,
    ) -> Self {
        BinarySerializationError {
            error_type,
            message: message.into(),
            location: location.into(),
            position: None,
        }
    }

    /// Create error with position
    pub fn with_position(
        error_type: BinarySerializationErrorType,
        message: impl Into<String>,
        location: impl Into<String>,
        position: Position,
    ) -> Self {
        BinarySerializationError {
            error_type,
            message: message.into(),
            location: location.into(),
            position: Some(position),
        }
    }

    /// Create error from validation failure
    pub fn invalid_state(message: impl Into<String>, location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::InvalidFormat,
            message,
            location,
        )
    }

    /// Create error for corrupted data
    pub fn corrupted_data(message: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::CorruptedData,
            message,
            "BinaryData",
        )
    }

    /// Create error for checksum mismatch
    pub fn checksum_mismatch() -> Self {
        Self::new(
            BinarySerializationErrorType::ChecksumMismatch,
            "Data integrity check failed - checksum mismatch",
            "BinaryData",
        )
    }

    /// Create error for nesting too deep
    pub fn nesting_too_deep(depth: usize, max_depth: usize, location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::InvalidFormat,
            format!("Nesting depth {} exceeds maximum {}", depth, max_depth),
            location,
        )
    }

    /// Create error for string too long
    pub fn string_too_long(length: usize, max_length: usize, location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::InvalidFormat,
            format!("String length {} exceeds maximum {}", length, max_length),
            location,
        )
    }

    /// Create error for array too large
    pub fn array_too_large(count: usize, max_count: usize, location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::InvalidFormat,
            format!("Array count {} exceeds maximum {}", count, max_count),
            location,
        )
    }

    /// Create error for object too large
    pub fn object_too_large(count: usize, max_count: usize, location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::InvalidFormat,
            format!("Object property count {} exceeds maximum {}", count, max_count),
            location,
        )
    }

    /// Create error for invalid type tag
    pub fn invalid_type_tag(tag: u8, location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::InvalidFormat,
            format!("Unknown type tag: 0x{:02X}", tag),
            location,
        )
    }

    /// Create error for invalid section ID
    pub fn invalid_section_id(id: u32, location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::InvalidFormat,
            format!("Invalid section ID: 0x{:08X}", id),
            location,
        )
    }

    /// Create error for unexpected end of file
    pub fn unexpected_eof(location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::CorruptedData,
            "Unexpected end of file",
            location,
        )
    }

    /// Create error for read failure
    pub fn read_error(message: impl Into<String>, location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::DeserializationFailed,
            message,
            location,
        )
    }

    /// Create error for write failure
    pub fn write_error(message: impl Into<String>, location: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::SerializationFailed,
            message,
            location,
        )
    }

    /// Create error for corrupted header
    pub fn corrupted_header(message: impl Into<String>) -> Self {
        Self::new(
            BinarySerializationErrorType::InvalidHeader,
            message,
            "Header",
        )
    }
}

impl fmt::Display for BinarySerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] at {}: {}", self.error_type, self.location, self.message)?;
        if let Some(pos) = self.position {
            write!(f, " ({})", pos)?;
        }
        Ok(())
    }
}

impl std::error::Error for BinarySerializationError {}

impl From<std::io::Error> for BinarySerializationError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::UnexpectedEof => Self::unexpected_eof("IO"),
            _ => Self::read_error(err.to_string(), "IO"),
        }
    }
      }
