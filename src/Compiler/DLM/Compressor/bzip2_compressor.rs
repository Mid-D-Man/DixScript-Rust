//! BZIP2 compression implementation
//! Slower compression with better ratio (for larger files)

use super::compressor_trait::{ICompressor, CompressorResult};
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::ErrorManager::{DlmErrorType, ErrorSeverity};
use bzip2::Compression;
use bzip2::write::{BzEncoder, BzDecoder};
use std::io::Write;
use std::collections::HashMap;

/// BZIP2 compression implementation
pub struct Bzip2Compressor {
    base: DLMModuleBase,
}

impl Bzip2Compressor {
    /// Create new Bzip2 compressor
    pub fn new() -> Self {
        let base = DLMModuleBase::new("DCompressor.bzip2", 2);
        
        Bzip2Compressor { base }
    }
}

impl Default for Bzip2Compressor {
    fn default() -> Self {
        Self::new()
    }
}

impl ICompressor for Bzip2Compressor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn algorithm(&self) -> &str {
        "bzip2"
    }

    fn initialize(&mut self, _config: HashMap<String, String>) {
        if self.base.debug_config().is_enabled {
            self.base.log_debug("Initialized BZIP2 compressor");
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

        if self.base.debug_config().is_enabled {
            self.base.log_info(&format!("Compressing {} bytes with BZIP2...", data.len()));
            self.base.log_warning("BZIP2 compression is slow - this may take a while for large files");
        }

        let mut encoder = BzEncoder::new(Vec::new(), Compression::best());
        
        encoder.write_all(data).map_err(|e| {
            let error_msg = format!("BZIP2 compression failed: {}", e);
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
            let error_msg = format!("BZIP2 compression finish failed: {}", e);
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

        if self.base.debug_config().is_enabled {
            self.base.log_info(&format!(
                "✅ BZIP2 compression complete: {} → {} bytes ({:.1}% reduction)",
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

        if self.base.debug_config().is_enabled {
            self.base.log_info(&format!("Decompressing {} bytes with BZIP2...", compressed_data.len()));
        }

        let mut decoder = BzDecoder::new(Vec::new());
        
        decoder.write_all(compressed_data).map_err(|e| {
            let error_msg = format!("BZIP2 decompression failed: {}", e);
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
            let error_msg = format!("BZIP2 decompression finish failed: {}", e);
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

        if self.base.debug_config().is_enabled {
            self.base.log_info(&format!(
                "✅ BZIP2 decompression complete: {} → {} bytes",
                compressed_data.len(),
                decompressed.len()
            ));
        }

        Ok(decompressed)
    }

    fn validate(&self) -> Result<(), String> {
        // BZIP2 always available via bzip2 crate
        Ok(())
    }

    fn get_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("algorithm".to_string(), "bzip2".to_string());
        metadata.insert("module_name".to_string(), self.module_name().to_string());
        metadata.insert("priority".to_string(), self.priority().to_string());
        metadata
    }

    fn priority(&self) -> i32 {
        self.base.priority()
    }
                  }
