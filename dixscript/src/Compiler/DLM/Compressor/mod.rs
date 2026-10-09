// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/dlm.md, section "Compressor/mod.rs"
// ============================================================================

//! Compressor: data compression modules for DLM.
//!
//! Gzip is the only codec. It uses flate2 with the `rust_backend`
//! (miniz_oxide), which is pure Rust and builds on every target. The bzip2
//! and lzma codecs were removed, see the DLM doc for the reasoning.

mod compressor_trait;
#[cfg(feature = "dlm-compressor")]
mod gzip_compressor;

pub use compressor_trait::{ICompressor, CompressorResult};
#[cfg(feature = "dlm-compressor")]
pub use gzip_compressor::{GzipCompressor, CompressionLevel};
