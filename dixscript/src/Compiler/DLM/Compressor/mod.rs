//! Compressor — data compression modules.
//! Gzip is available on all targets including WebAssembly.
//! Bzip2 and LZMA require native targets only (C library dependency).

mod compressor_trait;
mod gzip_compressor;

#[cfg(not(target_arch = "wasm32"))]
mod bzip2_compressor;

#[cfg(not(target_arch = "wasm32"))]
mod lzma_compressor;

pub use compressor_trait::{ICompressor, CompressorResult};
pub use gzip_compressor::{GzipCompressor, CompressionLevel};

#[cfg(not(target_arch = "wasm32"))]
pub use bzip2_compressor::Bzip2Compressor;

#[cfg(not(target_arch = "wasm32"))]
pub use lzma_compressor::LzmaCompressor;
