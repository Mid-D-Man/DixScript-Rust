// dixscript/src/Runtime/dix_serialize.rs
//! Trait and helpers for writing Rust structs into a [`DataBuilder`] for
//! `.mdix` serialization.
//!
//! # Quick start
//!
//! Implement [`DixSerialize`] for your config struct, then call
//! [`DixDataBuilder::serialize`] / [`DixDataBuilder::serialize_at`].
//!
//! ```rust,ignore
//! use dixscript::Runtime::{DixDataBuilder, DixSerialize, DataBuilder, dix_set_str, dix_set_int};
//!
//! struct ServerConfig { host: String, port: i32 }
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
//!     .serialize_at("server", &ServerConfig { host: "localhost".into(), port: 8080 })
//!     .build()
//!     .unwrap();
//! ```

use super::data_builder::{DataBuilder, DixDataBuilder};
use super::dix_deserialize::dix_path;

// ── Core trait ────────────────────────────────────────────────────────────────

/// Implemented by types that can be written into a [`DataBuilder`].
///
/// `prefix` is the full dotted path at which `Self` should be written.
///
/// - Struct impls treat `prefix` as the **parent** path and write each field
///   via the `dix_set_*` helpers (which append `.field` to `prefix`).
/// - Leaf/scalar impls (the primitives below, and `Option<T>`) write a single
///   value directly **at** `prefix` itself.
pub trait DixSerialize {
    /// Write `self` into `d`, rooted at `prefix`.
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String>;
}

// ── Primitive implementations ─────────────────────────────────────────────────
// These write a single flat property directly at `prefix`.

impl DixSerialize for String {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_string(prefix.to_string(), self.clone());
        Ok(())
    }
}

impl DixSerialize for i32 {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_int(prefix.to_string(), *self);
        Ok(())
    }
}

impl DixSerialize for i64 {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_long(prefix.to_string(), *self);
        Ok(())
    }
}

impl DixSerialize for f32 {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_float(prefix.to_string(), *self);
        Ok(())
    }
}

impl DixSerialize for f64 {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_double(prefix.to_string(), *self);
        Ok(())
    }
}

impl DixSerialize for bool {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_bool(prefix.to_string(), *self);
        Ok(())
    }
}

// ── Option<T> ─────────────────────────────────────────────────────────────────

/// `None` writes nothing at all — no key, no aggregate.
/// `Some(v)` writes `v` at `prefix` exactly as `T::to_dix` would.
impl<T: DixSerialize> DixSerialize for Option<T> {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        match self {
            Some(v) => v.to_dix(d, prefix),
            None    => Ok(()),
        }
    }
}

// ── Public helper functions ───────────────────────────────────────────────────
// Mirror dix_get / dix_get_or / dix_nested from dix_deserialize.rs.

/// Write a string field at `prefix.field`.
pub fn dix_set_str(d: &mut DataBuilder, prefix: &str, field: &str, value: &str) {
    d.with_string(dix_path(prefix, field), value.to_string());
}

/// Write an `i32` field at `prefix.field`.
pub fn dix_set_int(d: &mut DataBuilder, prefix: &str, field: &str, value: i32) {
    d.with_int(dix_path(prefix, field), value);
}

/// Write an `i64` field at `prefix.field`.
pub fn dix_set_long(d: &mut DataBuilder, prefix: &str, field: &str, value: i64) {
    d.with_long(dix_path(prefix, field), value);
}

/// Write an `f32` field at `prefix.field`.
pub fn dix_set_float(d: &mut DataBuilder, prefix: &str, field: &str, value: f32) {
    d.with_float(dix_path(prefix, field), value);
}

/// Write an `f64` field at `prefix.field`.
pub fn dix_set_double(d: &mut DataBuilder, prefix: &str, field: &str, value: f64) {
    d.with_double(dix_path(prefix, field), value);
}

/// Write a `bool` field at `prefix.field`.
pub fn dix_set_bool(d: &mut DataBuilder, prefix: &str, field: &str, value: bool) {
    d.with_bool(dix_path(prefix, field), value);
}

/// Write a nested struct at `prefix.field`.
///
/// Equivalent to `value.to_dix(d, "prefix.field")`.
pub fn dix_set_nested<T: DixSerialize>(
    d: &mut DataBuilder,
    prefix: &str,
    field: &str,
    value: &T,
) -> Result<(), String> {
    value.to_dix(d, &dix_path(prefix, field))
}

// ── DixDataBuilder extension ──────────────────────────────────────────────────

impl DixDataBuilder {
    /// Serialize `value` at the root (`prefix = ""`).
    ///
    /// Any error returned by `value.to_dix(...)` is deferred and surfaces
    /// from `build()`, same as `DataBuilder`'s two-tier ordering errors.
    pub fn serialize<T: DixSerialize>(mut self, value: &T) -> Self {
        if let Err(e) = value.to_dix(&mut self.data_builder, "") {
            self.data_builder.push_deferred_error(e);
        }
        self
    }

    /// Serialize `value` at `prefix`.
    pub fn serialize_at<T: DixSerialize>(mut self, prefix: &str, value: &T) -> Self {
        if let Err(e) = value.to_dix(&mut self.data_builder, prefix) {
            self.data_builder.push_deferred_error(e);
        }
        self
    }
              }
