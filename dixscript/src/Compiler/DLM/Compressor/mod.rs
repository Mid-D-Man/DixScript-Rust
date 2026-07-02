//! Compressor — data compression modules.
//! All three compressors are pure Rust and build on every target:
//! wasm32-unknown-unknown, wasm32-wasip2, Android, iOS, Windows.
//!
//! Gzip:  flate2 rust_backend (miniz_oxide)  — always pure Rust.
//! Bzip2: bzip2 0.6+ via libbz2-rs-sys       — pure Rust since June 2025.
//! LZMA:  lzma-rust2 (ported from tukaani xz-for-java) — pure Rust, real
//!        encoder (not the lzma-rs "dumb" literal-only placeholder encoder).
//!        `optimization` feature intentionally off, see Cargo.toml.

mod compressor_trait;
mod gzip_compressor;
mod bzip2_compressor;
mod lzma_compressor;

pub use compressor_trait::{ICompressor, CompressorResult};
pub use gzip_compressor::{GzipCompressor, CompressionLevel};
pub use bzip2_compressor::Bzip2Compressor;
pub use lzma_compressor::LzmaCompressor;
