//! GZIP compression implementation
//! Fast compression with good ratio (recommended for most use cases)

use super::compressor_trait::{ICompressor, CompressorResult};
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::ErrorManager::{DlmErrorType, ErrorSeverity};
use flate2::Compression;
use flate2::write::{GzEncoder, GzDecoder};
use std::io::Write;
use std::collections::HashMap;

/// Compression level for Gzip
#[derive(Debug, Clone, Copy)]
pub enum CompressionLevel {
    Fastest,
    Optimal,
    NoCompression,
}

impl CompressionLevel {
    fn to_flate2_compression(&self) -> Compression {
        match self {
            CompressionLevel::Fastest => Compression::fast(),
            CompressionLevel::Optimal => Compression::best(),
            CompressionLevel::NoCompression => Compression::none(),
        }
    }

    fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "fastest" => CompressionLevel::Fastest,
            "optimal" => CompressionLevel::Optimal,
            "nocompression" => CompressionLevel::NoCompression,
            _ => CompressionLevel::Optimal,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            CompressionLevel::Fastest => "fastest",
            CompressionLevel::Optimal => "optimal",
            CompressionLevel::NoCompression => "nocompression",
        }
    }
}

/// GZIP compression implementation
pub struct GzipCompressor {
    base: DLMModuleBase,
    compression_level: CompressionLevel,
}

impl GzipCompressor {
    /// Create new Gzip compressor
    pub fn new() -> Self {
        let base = DLMModuleBase::new("DCompressor.gzip", 2);

        GzipCompressor {
            base,
            compression_level: CompressionLevel::Optimal,
        }
    }
}

impl Default for GzipCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl ICompressor for GzipCompressor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn algorithm(&self) -> &str {
        "gzip"
    }

    fn initialize(&mut self, config: HashMap<String, String>) {
        if let Some(level) = config.get("compression_level") {
            self.compression_level = CompressionLevel::from_string(level);
        }

        if self.base.is_debug_enabled() {
            self.base.log_debug(&format!(
                "Initialized with compression level: {:?}",
                self.compression_level
            ));
        }
    }

    fn compress(&self, data: &[u8]) -> CompressorResult<Vec<u8>> {
        if data.is_empty() {
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Cannot compress null or empty data".to_string(),
                Some(self.module_name().to_string()),
                None,
                Some("Provide non-empty data to compress".to_string()),
                ErrorSeverity::Error,
            );
            return Err("Cannot compress null or empty data".to_string());
        }

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!("Compressing {} bytes with GZIP...", data.len()));
        }

        let mut encoder = GzEncoder::new(Vec::new(), self.compression_level.to_flate2_compression());

        encoder.write_all(data).map_err(|e| {
            let error_msg = format!("GZIP compression failed: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                error_msg.clone(),
                Some(self.module_name().to_string()),
                None,
                Some("Check input data validity and available memory".to_string()),
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        let compressed = encoder.finish().map_err(|e| {
            let error_msg = format!("GZIP compression finish failed: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                error_msg.clone(),
                Some(self.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        let ratio = 1.0 - (compressed.len() as f64 / data.len() as f64);

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!(
                "✅ GZIP compression complete: {} → {} bytes ({:.1}% reduction)",
                data.len(),
                compressed.len(),
                ratio * 100.0
            ));
        }

        Ok(compressed)
    }

    fn decompress(&self, compressed_data: &[u8]) -> CompressorResult<Vec<u8>> {
        if compressed_data.is_empty() {
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvalidFunctionSignature,
                "Cannot decompress null or empty data".to_string(),
                Some(self.module_name().to_string()),
                None,
                Some("Provide non-empty data to decompress".to_string()),
                ErrorSeverity::Error,
            );
            return Err("Cannot decompress null or empty data".to_string());
        }

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!("Decompressing {} bytes with GZIP...", compressed_data.len()));
        }

        let mut decoder = GzDecoder::new(Vec::new());

        decoder.write_all(compressed_data).map_err(|e| {
            let error_msg = format!("GZIP decompression failed: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                error_msg.clone(),
                Some(self.module_name().to_string()),
                None,
                Some("Verify data integrity and format".to_string()),
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        let decompressed = decoder.finish().map_err(|e| {
            let error_msg = format!("GZIP decompression finish failed: {}", e);
            self.base.error_manager().add_dlm_error(
                DlmErrorType::InvocationFailed,
                error_msg.clone(),
                Some(self.module_name().to_string()),
                None,
                None,
                ErrorSeverity::Error,
            );
            error_msg
        })?;

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!(
                "✅ GZIP decompression complete: {} → {} bytes",
                compressed_data.len(),
                decompressed.len()
            ));
        }

        Ok(decompressed)
    }

    fn validate(&self) -> Result<(), String> {
        // GZIP always available in Rust via flate2
        Ok(())
    }

    fn get_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("algorithm".to_string(), "gzip".to_string());
        metadata.insert("compression_level".to_string(), self.compression_level.as_str().to_string());
        metadata.insert("module_name".to_string(), self.module_name().to_string());
        metadata.insert("priority".to_string(), self.priority().to_string());
        metadata
    }

    fn priority(&self) -> i32 {
        self.base.priority()
    }
}