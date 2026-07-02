//! LZMA/XZ compression implementation.
//! Pure Rust via lzma-rust2 (ported from tukaani's xz-for-java, not a
//! liblzma C binding) — wasm32 and Android safe. `optimization` feature is
//! deliberately disabled (see Cargo.toml) so this is 100% safe standard Rust
//! on every target, no hand-written-asm fast path anywhere.
//! Slowest compression with the best ratio.
use super::compressor_trait::{ICompressor, CompressorResult};
use crate::Compiler::DLM::dlm_module_base::DLMModuleBase;
use crate::ErrorManager::{DlmErrorType, ErrorSeverity};
use lzma_rust2::{XzOptions, XzReader, XzWriter};
use std::io::{Read, Write};
use std::collections::HashMap;

/// XZ compression level (0-9). 9 = best compression, slowest.
const XZ_PRESET_LEVEL: u32 = 9;

/// LZMA/XZ compression implementation
pub struct LzmaCompressor {
    base: DLMModuleBase,
}

impl LzmaCompressor {
    /// Create new LZMA compressor
    pub fn new() -> Self {
        let base = DLMModuleBase::new("DCompressor.lzma", 2);

        LzmaCompressor { base }
    }
}

impl Default for LzmaCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl ICompressor for LzmaCompressor {
    fn module_name(&self) -> &str {
        self.base.module_name()
    }

    fn algorithm(&self) -> &str {
        "lzma"
    }

    fn initialize(&mut self, _config: HashMap<String, String>) {
        if self.base.is_debug_enabled() {
            self.base.log_debug("Initialized LZMA compressor");
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
            self.base.log_info(&format!("Compressing {} bytes with LZMA/XZ...", data.len()));
            self.base.log_warning("⏱️ LZMA compression is slow - this may take a while for large files");
        }

        let options = XzOptions::with_preset(XZ_PRESET_LEVEL);

        let mut encoder = XzWriter::new(Vec::new(), options).map_err(|e| {
            let error_msg = format!("LZMA encoder init failed: {}", e);
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

        encoder.write_all(data).map_err(|e| {
            let error_msg = format!("LZMA compression failed: {}", e);
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
            let error_msg = format!("LZMA compression finish failed: {}", e);
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
                " LZMA compression complete: {} → {} bytes ({:.1}% reduction)",
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
            self.base.log_info(&format!("Decompressing {} bytes with LZMA/XZ...", compressed_data.len()));
        }

        // `false`: we only ever emit a single XZ stream ourselves, so we
        // don't need to tolerate concatenated multi-stream .xz input here.
        let mut decoder = XzReader::new(compressed_data, false);
        let mut decompressed = Vec::new();

        decoder.read_to_end(&mut decompressed).map_err(|e| {
            let error_msg = format!("LZMA decompression failed: {}", e);
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

        if self.base.is_debug_enabled() {
            self.base.log_info(&format!(
                " LZMA decompression complete: {} → {} bytes",
                compressed_data.len(),
                decompressed.len()
            ));
        }

        Ok(decompressed)
    }

    fn validate(&self) -> Result<(), String> {
        // LZMA/XZ always available via lzma-rust2 crate
        Ok(())
    }

    fn get_metadata(&self) -> HashMap<String, String> {
        let mut metadata = HashMap::new();
        metadata.insert("algorithm".to_string(), "lzma".to_string());
        metadata.insert("module_name".to_string(), self.module_name().to_string());
        metadata.insert("priority".to_string(), self.priority().to_string());
        metadata
    }

    fn priority(&self) -> i32 {
        self.base.priority()
    }
                }
