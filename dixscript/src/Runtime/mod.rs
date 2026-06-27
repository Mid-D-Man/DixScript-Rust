//! Runtime — public API for loading and using `.mdix` files.
//!
//! ## Loading
//!
//! ```rust,ignore
//! use dixscript::Runtime::{DixLoader, DixLoadOptions};
//!
//! let loader = DixLoader::new();
//! let data   = loader.load_text("config.mdix", &DixLoadOptions::new())?;
//! ```
//!
//! ## Reading
//!
//! ```rust,ignore
//! let port: i32    = data.get("server.port")?;
//! let host: String = data.get("server.host")?;
//! ```
//!
//! ## Struct deserialization
//!
//! ```rust,ignore
//! use dixscript::Runtime::{DixDeserialize, dix_get};
//!
//! impl DixDeserialize for ServerConfig {
//!     fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
//!         Ok(ServerConfig {
//!             host: dix_get(data, prefix, "host")?,
//!             port: dix_get(data, prefix, "port")?,
//!         })
//!     }
//! }
//!
//! let config: ServerConfig = data.deserialize_at("server")?;
//! ```
//!
//! ## Struct serialization
//!
//! ```rust,ignore
//! use dixscript::Runtime::{DixSerialize, DataBuilder, dix_set_str, dix_set_int};
//!
//! impl DixSerialize for ServerConfig {
//!     fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
//!         dix_set_str(d, prefix, "host", &self.host);
//!         dix_set_int(d, prefix, "port", self.port);
//!         Ok(())
//!     }
//! }
//!
//! let data = DixDataBuilder::new()
//!     .serialize_at("server", &config)
//!     .build()?;
//! ```
//!
//! ## Schema validation
//!
//! ```rust,ignore
//! use dixscript::Runtime::SchemaBuilder;
//!
//! let report = data.validate_schema(
//!     SchemaBuilder::new()
//!         .require_string("server.host")
//!         .require_int("server.port"),
//! );
//!
//! assert!(report.is_valid());
//! ```
//!
//! ## Merging
//!
//! ```rust,ignore
//! use dixscript::Runtime::merge::{MdixMerger, MdixMergeInput, MdixMergeStrategy};
//!
//! // AST-level merge with weight-based conflict resolution.
//! let result = MdixMerger::new()
//!     .with_strategy(MdixMergeStrategy::WeightedPriority)
//!     .merge_all(vec![
//!         MdixMergeInput::new(ast_base).with_weight(1.0).with_label("base"),
//!         MdixMergeInput::new(ast_patch).with_weight(0.8).with_label("patch"),
//!         MdixMergeInput::new(ast_local).with_weight(0.5).with_label("local"),
//!     ]);
//!
//! // File-path convenience — loads, compiles, merges, returns DixData.
//! let data = MdixMerger::new().merge_files(&["base.mdix", "overrides.mdix"])?;
//!
//! // Explicit per-file weights.
//! let data = MdixMerger::new().merge_files_weighted(&[
//!     ("base.mdix",      1.0),
//!     ("overrides.mdix", 0.8),
//!     ("local.mdix",     0.5),
//! ])?;
//! ```
//!
//! ## Hot reload
//!
//! ```rust,ignore
//! use dixscript::Runtime::HotReloadWatcher;
//!
//! let mut watcher = HotReloadWatcher::new("config.mdix");
//!
//! // in your game loop / tick / update:
//! match watcher.check_and_reload() {
//!     Ok(Some(data)) => apply_new_config(data),  // file changed, reloaded
//!     Ok(None)       => {}                       // unchanged, nothing to do
//!     Err(e)         => eprintln!("hot reload failed: {e}"),
//! }
//! ```

pub mod array_homogenizer;
pub mod compactor;
pub mod converter;
pub mod data_builder;
pub mod dix_data;
pub mod dix_deserialize;
pub mod dix_serialize;
pub mod dix_value;
pub mod format_options;
pub mod hot_reload;
pub mod key_resolver;
pub mod load_options;
pub mod loader;
pub mod merge;
pub mod schema;

// ── Core types ────────────────────────────────────────────────────────────────

pub use array_homogenizer::homogenize_data_section;
pub use compactor::DixCompactor;
pub use converter::DixConverter;
pub use dix_data::DixData;
pub use dix_value::DixValue;
pub use format_options::DixFormatOptions;
pub use load_options::DixLoadOptions;
pub use loader::DixLoader;

// ── Builder ───────────────────────────────────────────────────────────────────

pub use data_builder::{
    ConfigBuilder,
    DataBuilder,
    DixDataBuilder,
    EnumsBuilder,
    GroupArrayBuilder,
    TablePropertiesBuilder,
};

// ── Encryption / key management ───────────────────────────────────────────────

pub use key_resolver::{
    KeyFileResolution,
    KeyFileResolver,
    KeyFileSource,
    KeyResolver,
    KeySource,
    ResolvedKey,
};

// ── Deserialization ───────────────────────────────────────────────────────────

pub use dix_deserialize::{
    DixDeserialize,
    dix_path,
    dix_get,
    dix_get_or,
    dix_nested,
    dix_array_of,
};

// Re-export `dix_value` function under a non-colliding name.
pub use dix_deserialize::dix_value as dix_raw_value;

// ── Serialization ─────────────────────────────────────────────────────────────

pub use dix_serialize::{
    DixSerialize,
    dix_set_bool,
    dix_set_double,
    dix_set_float,
    dix_set_int,
    dix_set_long,
    dix_set_nested,
    dix_set_str,
};

// ── Schema validation ─────────────────────────────────────────────────────────

pub use schema::{
    ExpectedValueType,
    SchemaBuilder,
    ValidationError,
    ValidationErrorKind,
    ValidationReport,
};

// ── Merging ───────────────────────────────────────────────────────────────────

pub use merge::{
    ArrayMergeStrategy,
    MdixMergeInput,
    MdixMergeResult,
    MdixMergeStrategy,
    MdixMerger,
    MergeConflict,
};

// ── Hot reload ────────────────────────────────────────────────────────────────
// Poll-based watcher for Rust consumers only.  Each language binding implements
// its own native FS-event mechanism (inotify, FSEvents, ReadDirectoryChangesW).
// See `hot_reload` module docs for the intended game-loop usage pattern.

pub use hot_reload::HotReloadWatcher;
