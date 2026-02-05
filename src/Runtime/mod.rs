// src/Runtime/mod.rs

//! Runtime - Public API for loading and using .mdix files
//! 
//! This module provides the core runtime components for working with DixScript files.
//! 
//! ## Core Types
//! - `DixData` - Loaded data container with flattened access
//! - `DixValue` - Runtime value representation
//! - `DixLoadOptions` - Configuration for loading files
//! - `DixFormatOptions` - Configuration for formatting output
//! 
//! ## Utilities
//! - `DixCompactor` - Minification and compaction
//! - `DixConverter` - HashMap ↔ AST conversion
//! 
//! ## Future Components (not yet ported)
//! - `DixLoader` - File loading engine (depends on full compiler)
//! - `KeyFileResolver` - Key file resolution
//! - `DixSerializer` - Language-specific (will be in wrappers)
//! - `DixDataBuilder` - Fluent builder (language-specific)

// Module declarations
pub mod format_options;
pub mod load_options;
pub mod compactor;
pub mod dix_value;
pub mod converter;
pub mod dix_data;

// Re-exports for convenience
pub use format_options::DixFormatOptions;
pub use load_options::DixLoadOptions;
pub use compactor::DixCompactor;
pub use dix_value::DixValue;
pub use converter::DixConverter;
pub use dix_data::DixData;

// TODO: Port these when compiler is fully ready
// pub mod loader;
// pub mod key_resolver;
// pub use loader::DixLoader;
// pub use key_resolver::KeyFileResolver;
