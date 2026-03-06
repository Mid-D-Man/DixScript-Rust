//! Result types for binary serialization/deserialization operations

use std::time::Duration;
use super::binary_serialization_context::{
    BinarySerializationStatistics,
    BinaryDeserializationStatistics,
};
use crate::Compiler::AST::DixScript;

/// Result of binary serialization operation
#[derive(Debug, Clone)]
pub struct BinarySerializationResult {
    pub is_success: bool,
    pub binary_data: Vec<u8>,
    pub original_size: usize,
    pub compressed_size: usize,
    pub compression_ratio: f64,
    pub duration: Duration,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub statistics: BinarySerializationStatistics,
}

impl BinarySerializationResult {
    /// Create new successful result
    pub fn success(
        binary_data: Vec<u8>,
        original_size: usize,
        duration: Duration,
        statistics: BinarySerializationStatistics,
    ) -> Self {
        let compressed_size = binary_data.len();
        let compression_ratio = if original_size > 0 {
            1.0 - (compressed_size as f64 / original_size as f64)
        } else {
            0.0
        };

        BinarySerializationResult {
            is_success: true,
            binary_data,
            original_size,
            compressed_size,
            compression_ratio,
            duration,
            errors: Vec::new(),
            warnings: Vec::new(),
            statistics,
        }
    }

    /// Create new failed result
    pub fn failure(
        errors: Vec<String>,
        warnings: Vec<String>,
        duration: Duration,
    ) -> Self {
        BinarySerializationResult {
            is_success: false,
            binary_data: Vec::new(),
            original_size: 0,
            compressed_size: 0,
            compression_ratio: 0.0,
            duration,
            errors,
            warnings,
            statistics: BinarySerializationStatistics::new(),
        }
    }
}

impl std::fmt::Display for BinarySerializationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.is_success { "SUCCESS" } else { "FAILED" };
        write!(
            f,
            "Binary Serialization {}: Size={} bytes ({:.1}% reduction), Duration={:.2}ms, Errors={}",
            status,
            self.compressed_size,
            self.compression_ratio * 100.0,
            self.duration.as_secs_f64() * 1000.0,
            self.errors.len()
        )
    }
}

/// Result of binary deserialization operation
#[derive(Debug)]
pub struct BinaryDeserializationResult {
    pub is_success: bool,
    pub ast: Option<DixScript>,
    pub binary_size: usize,
    pub duration: Duration,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub statistics: BinaryDeserializationStatistics,
}

impl BinaryDeserializationResult {
    /// Create new successful result
    pub fn success(
        ast: DixScript,
        binary_size: usize,
        duration: Duration,
        statistics: BinaryDeserializationStatistics,
    ) -> Self {
        BinaryDeserializationResult {
            is_success: true,
            ast: Some(ast),
            binary_size,
            duration,
            errors: Vec::new(),
            warnings: Vec::new(),
            statistics,
        }
    }

    /// Create new failed result
    pub fn failure(
        errors: Vec<String>,
        warnings: Vec<String>,
        duration: Duration,
    ) -> Self {
        BinaryDeserializationResult {
            is_success: false,
            ast: None,
            binary_size: 0,
            duration,
            errors,
            warnings,
            statistics: BinaryDeserializationStatistics::new(),
        }
    }
}

impl std::fmt::Display for BinaryDeserializationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.is_success { "SUCCESS" } else { "FAILED" };
        write!(
            f,
            "Binary Deserialization {}: Size={} bytes, Duration={:.2}ms, Errors={}",
            status,
            self.binary_size,
            self.duration.as_secs_f64() * 1000.0,
            self.errors.len()
        )
    }
          }
