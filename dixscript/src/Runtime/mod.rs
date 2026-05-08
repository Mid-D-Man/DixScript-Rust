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

pub mod compactor;
pub mod converter;
pub mod data_builder;
pub mod dix_data;
pub mod dix_deserialize;
pub mod dix_serialize;
pub mod dix_value;
pub mod format_options;
pub mod key_resolver;
pub mod load_options;
pub mod loader;
pub mod schema;

// ── Core types ────────────────────────────────────────────────────────────────

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
    // Build a dotted path from a prefix and a field segment.
    dix_path,
    // Read a typed field at `prefix.field`, returning `Err` if absent.
    dix_get,
    // Read a typed field, returning `default` if absent.
    dix_get_or,
    // Deserialize a nested struct at `prefix.field`.
    dix_nested,
    // Deserialize an array of structs at `prefix.field`.
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
