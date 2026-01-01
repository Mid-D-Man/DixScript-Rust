//! Binary Serialization error types and handling

use super::error_enums::ErrorSeverity;
use crate::DixCore::List;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub struct BinarySerializationError {
    pub error_id: String,
    pub error_type: BinarySerializationErrorType,
    pub message: String,
    pub section_name: Option<String>,
    pub byte_position: Option<i64>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
    pub quick_fixes: List<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl BinarySerializationError {
    pub fn new(
        error_type: BinarySerializationErrorType,
        message: String,
        section_name: Option<String>,
        byte_position: Option<i64>,
        suggestion: Option<String>,
        severity: ErrorSeverity,
    ) -> Self {
        let error_id = format!("DXBIN{:03}", error_type as u32);

        let mut error = Self {
            error_id,
            error_type: error_type.clone(),
            message,
            section_name,
            byte_position,
            suggestion,
            severity,
            quick_fixes: List::New(),
            metadata: std::collections::HashMap::new(),
        };

        error.generate_quick_fixes(&error_type);
        error
    }

    fn generate_quick_fixes(&mut self, error_type: &BinarySerializationErrorType) {
        match error_type {
            BinarySerializationErrorType::SerializationFailed => {
                self.quick_fixes.Add("Check AST structure is valid".to_string());
                self.quick_fixes.Add("Verify all values are serializable".to_string());
            }
            BinarySerializationErrorType::DeserializationFailed => {
                self.quick_fixes.Add("Verify binary format version".to_string());
                self.quick_fixes.Add("Check data integrity".to_string());
            }
            BinarySerializationErrorType::UnsupportedType => {
                self.quick_fixes.Add("Use supported value types".to_string());
                self.quick_fixes.Add("Check type compatibility".to_string());
            }
            BinarySerializationErrorType::DataCorruption => {
                self.quick_fixes.Add("Verify file integrity".to_string());
                self.quick_fixes.Add("Check for transmission errors".to_string());
            }
            BinarySerializationErrorType::ChecksumMismatch => {
                self.quick_fixes.Add("File may be corrupted".to_string());
                self.quick_fixes.Add("Verify data integrity".to_string());
                self.quick_fixes.Add("Try regenerating from source".to_string());
            }
            _ => {}
        }
    }
}

impl fmt::Display for BinarySerializationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "[{}] {}: {:?}",
            self.error_id, self.severity, self.error_type
        )?;

        if let Some(ref section) = self.section_name {
            writeln!(f, "Section: {}", section)?;
        }

        if let Some(pos) = self.byte_position {
            writeln!(f, "Position: 0x{:X} ({} bytes)", pos, pos)?;
        }

        writeln!(f, "Message: {}", self.message)?;

        if let Some(ref suggestion) = self.suggestion {
            writeln!(f, "Suggestion: {}", suggestion)?;
        }

        if !self.quick_fixes.IsEmpty() {
            writeln!(f, "Quick Fixes:")?;
            for fix in self.quick_fixes.Iter() {
                writeln!(f, "  - {}", fix)?;
            }
        }

        Ok(())
    }
}