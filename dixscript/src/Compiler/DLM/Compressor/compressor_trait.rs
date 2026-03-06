//! Compressor trait definition

use std::collections::HashMap;

/// Result type for compressor operations
pub type CompressorResult<T> = Result<T, String>;

/// Trait for compression modules
pub trait ICompressor {
    /// Get module name
    fn module_name(&self) -> &str;

    /// Get compression algorithm name
    fn algorithm(&self) -> &str;

    /// Initialize compressor with configuration
    fn initialize(&mut self, config: HashMap<String, String>);

    /// Compress binary data
    fn compress(&self, data: &[u8]) -> CompressorResult<Vec<u8>>;

    /// Decompress binary data
    fn decompress(&self, compressed_data: &[u8]) -> CompressorResult<Vec<u8>>;

    /// Validate compressor can execute
    fn validate(&self) -> Result<(), String>;

    /// Get metadata for .dixscript.key file
    fn get_metadata(&self) -> HashMap<String, String>;

    /// Get priority (lower = earlier execution)
    fn priority(&self) -> i32;
  }
