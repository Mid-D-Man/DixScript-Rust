//! Compressor - Data compression modules

mod compressor_trait;
mod gzip_compressor;
mod bzip2_compressor;
mod lzma_compressor;

pub use compressor_trait::{ICompressor, CompressorResult};
pub use gzip_compressor::{GzipCompressor, CompressionLevel};
pub use bzip2_compressor::Bzip2Compressor;
pub use lzma_compressor::LzmaCompressor;
