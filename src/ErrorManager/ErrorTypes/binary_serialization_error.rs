use std::fmt;

/// Binary serialization error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinarySerializationErrorType {
    SerializationFailed,
    DeserializationFailed,
    InvalidFormat,
    VersionMismatch,
    CorruptedData,
    UnsupportedType,
    CompressionFailed,
    DecompressionFailed,
    InvalidHeader,
    ChecksumMismatch,
    EncodingError,
    BufferOverflow,
}

/// Binary serialization error
#[derive(Debug, Clone)]
pub struct BinarySerializationError {
    pub error_id: String,
    pub error_type: BinarySerializationErrorType,
    pub message: String,
    pub file_path: Option<String>,
    pub expected_version: Option<String>,
    pub actual_version: Option<String>,
    pub suggestion: Option<String>,
    pub severity: super::ErrorSeverity,
    pub quick_fixes: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl BinarySerializationError {
    pub fn new(
        error_type: BinarySerializationErrorType,
        message: String,
        file_path: Option<String>,
        expected_version: Option<String>,
        actual_version: Option<String>,
        suggestion: Option<String>,
        severity: super::ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXBIN{:03}", error_type as u32);
        let quick_fixes = Self::generate_quick_fixes(error_type);

        BinarySerializationError {
            error_id,
            error_type,
            message,
            file_path,
            expected_version,
            actual_version,
            suggestion,
            severity,
            quick_fixes,
            metadata: std::collections::HashMap::new(),
        }
    }

    fn generate_quick_fixes(error_type: BinarySerializationErrorType) -> Vec<String> {
        match error_type {
            BinarySerializationErrorType::SerializationFailed => vec![
                "Check data types are serializable".to_string(),
                "Verify data structure is valid".to_string(),
            ],
            BinarySerializationErrorType::DeserializationFailed => vec![
                "Check file is not corrupted".to_string(),
                "Verify file format is correct".to_string(),
                "Try regenerating the file".to_string(),
            ],
            BinarySerializationErrorType::InvalidFormat => vec![
                "Check file has correct extension (.mdix.bin)".to_string(),
                "Verify file is a valid DixScript binary".to_string(),
            ],
            BinarySerializationErrorType::VersionMismatch => vec![
                "Update DixScript to compatible version".to_string(),
                "Regenerate binary from source .mdix file".to_string(),
            ],
            BinarySerializationErrorType::CorruptedData => vec![
                "Restore from backup".to_string(),
                "Regenerate binary from source".to_string(),
                "Check file integrity".to_string(),
            ],
            BinarySerializationErrorType::ChecksumMismatch => vec![
                "File may be corrupted or modified".to_string(),
                "Regenerate from source".to_string(),
            ],
            BinarySerializationErrorType::CompressionFailed => vec![
                "Check available memory".to_string(),
                "Reduce data size".to_string(),
            ],
            BinarySerializationErrorType::DecompressionFailed => vec![
                "Check file is not corrupted".to_string(),
                "Verify compression format".to_string(),
            ],
            _ => Vec::new(),
        }
    }

    /// Generate suggestion for binary serialization error
    pub fn generate_suggestion(
        error_type: BinarySerializationErrorType,
        file_path: Option<&str>,
    ) -> String {
        match error_type {
            BinarySerializationErrorType::SerializationFailed => {
                "Failed to serialize data to binary format. Check data structure is valid.".to_string()
            }
            BinarySerializationErrorType::DeserializationFailed => {
                format!(
                    "Failed to deserialize binary file '{}'. File may be corrupted.",
                    file_path.unwrap_or("unknown")
                )
            }
            BinarySerializationErrorType::InvalidFormat => {
                format!(
                    "File '{}' is not a valid DixScript binary format.",
                    file_path.unwrap_or("unknown")
                )
            }
            BinarySerializationErrorType::VersionMismatch => {
                "Binary file was created with a different DixScript version. Regenerate from source.".to_string()
            }
            BinarySerializationErrorType::CorruptedData => {
                format!(
                    "Binary file '{}' is corrupted. Restore from backup or regenerate.",
                    file_path.unwrap_or("unknown")
                )
            }
            BinarySerializationErrorType::UnsupportedType => {
                "Data type not supported in binary serialization format.".to_string()
            }
            BinarySerializationErrorType::CompressionFailed => {
                "Failed to compress data. Check available memory.".to_string()
            }
            BinarySerializationErrorType::DecompressionFailed => {
                "Failed to decompress data. File may be corrupted.".to_string()
            }
            BinarySerializationErrorType::InvalidHeader => {
                "Binary file header is invalid or corrupted.".to_string()
            }
            BinarySerializationErrorType::ChecksumMismatch => {
                "Checksum verification failed. File may have been modified or corrupted.".to_string()
            }
            BinarySerializationErrorType::EncodingError => {
                "Failed to encode data to binary format.".to_string()
            }
            BinarySerializationErrorType::BufferOverflow => {
                "Buffer overflow during serialization. Data may be too large.".to_string()
            }
        }
    }
}

impl fmt::Display for BinarySerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "[{}] {:?}: {:?}", self.error_id, self.severity, self.error_type)?;

        if let Some(ref path) = self.file_path {
            writeln!(f, "File: {}", path)?;
        }

        if let Some(ref expected) = self.expected_version {
            writeln!(f, "Expected Version: {}", expected)?;
        }

        if let Some(ref actual) = self.actual_version {
            writeln!(f, "Actual Version: {}", actual)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.is_empty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in &self.quick_fixes {
                writeln!(f, "  - {}", fix)?;
            }
        }

        Ok(())
    }
}