// src/Runtime/mod.rs

//! Runtime - Public API for loading and using .dixscript files
//!
//! ## Core Types
//! - `DixData` - Loaded data container with flattened access
//! - `DixValue` - Runtime value representation
//! - `DixLoadOptions` - Configuration for loading files
//! - `DixFormatOptions` - Configuration for formatting output
//! - `DixLoader` - File loading engine
//! - `DixDataBuilder` - Fluent builder for creating DixData programmatically
//!
//! ## Utilities
//! - `DixCompactor` - Minification and compaction
//! - `DixConverter` - HashMap ↔ AST conversion
//! - `KeyFileResolver` - Locates .dxkey files for encrypted loading
//! - `KeyResolver` - Derives/extracts actual encryption key bytes

// Module declarations
pub mod format_options;
pub mod load_options;
pub mod compactor;
pub mod dix_value;
pub mod converter;
pub mod dix_data;
pub mod loader;
pub mod key_resolver;
pub mod data_builder;

// Re-exports for convenience
pub use format_options::DixFormatOptions;
pub use load_options::DixLoadOptions;
pub use compactor::DixCompactor;
pub use dix_value::DixValue;
pub use converter::DixConverter;
pub use dix_data::DixData;
pub use loader::DixLoader;
pub use key_resolver::{
    KeyFileResolver,
    KeyFileResolution,
    KeyFileSource,
    KeyResolver,
    ResolvedKey,
    KeySource,
};
pub use data_builder::{
    DixDataBuilder,
    ConfigBuilder,
    EnumsBuilder,
    DataBuilder,
    TablePropertiesBuilder,
    GroupArrayBuilder,
};