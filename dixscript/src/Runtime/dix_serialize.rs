//! Trait and helpers for writing Rust structs into a [`DixDataBuilder`].
//!
//! # Quick start
//!
//! Implement [`DixSerialize`] for your struct, then use [`DixDataBuilder::serialize`]
//! or [`DixDataBuilder::serialize_at`] to build a loadable database.
//!
//! ```rust,ignore
//! use dixscript::Runtime::{DixDataBuilder, DixSerialize, dix_set};
//! use dixscript::Runtime::data_builder::DataBuilder;
//!
//! #[derive(Debug)]
//! pub struct ServerConfig {
//!     pub host: String,
//!     pub port: i32,
//!     pub ssl:  bool,
//! }
//!
//! impl DixSerialize for ServerConfig {
//!     fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
//!         dix_set_str(d,  prefix, "host", &self.host);
//!         dix_set_int(d,  prefix, "port",  self.port);
//!         dix_set_bool(d, prefix, "ssl",   self.ssl);
//!         Ok(())
//!     }
//! }
//!
//! let config = ServerConfig { host: "localhost".into(), port: 8080, ssl: false };
//!
//! let data = DixDataBuilder::new()
//!     .serialize_at("server", &config)
//!     .build()?;
//!
//! // Round-trip: read it back
//! let back: ServerConfig = data.deserialize_at("server")?;
//! assert_eq!(back.host, "localhost");
//! ```

use super::data_builder::{DataBuilder, DixDataBuilder};
use super::dix_deserialize::dix_path;

// ── Core trait ────────────────────────────────────────────────────────────────

/// Implemented by types that can be written into a [`DixDataBuilder`].
///
/// Call [`dix_set_str`], [`dix_set_int`], etc. inside the implementation —
/// they handle path building automatically. All paths are resolved relative
/// to `prefix`.
pub trait DixSerialize {
    /// Write all fields into `d` with paths relative to `prefix`.
    ///
    /// Return `Err` only on structural errors (e.g. two-tier ordering violations).
    /// Type mismatch errors are usually caught at build time, not here.
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String>;
}

// ── Primitive implementations ─────────────────────────────────────────────────

impl DixSerialize for str {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_string(prefix, self);
        Ok(())
    }
}

impl DixSerialize for String {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_string(prefix, self.as_str());
        Ok(())
    }
}

impl DixSerialize for i32 {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_int(prefix, *self);
        Ok(())
    }
}

impl DixSerialize for i64 {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_long(prefix, *self);
        Ok(())
    }
}

impl DixSerialize for f32 {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_float(prefix, *self);
        Ok(())
    }
}

impl DixSerialize for f64 {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_double(prefix, *self);
        Ok(())
    }
}

impl DixSerialize for bool {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        d.with_bool(prefix, *self);
        Ok(())
    }
}

impl<T: DixSerialize> DixSerialize for Option<T> {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        if let Some(inner) = self {
            inner.to_dix(d, prefix)?;
        }
        Ok(())
    }
}

// ── Helper functions ──────────────────────────────────────────────────────────
//
// These resolve `prefix.field` paths and call the appropriate DataBuilder
// method. Use these inside your DixSerialize implementations.

/// Write a string field at `prefix.field`.
#[inline]
pub fn dix_set_str(d: &mut DataBuilder, prefix: &str, field: &str, value: &str) {
    d.with_string(dix_path(prefix, field), value);
}

/// Write an int field at `prefix.field`.
#[inline]
pub fn dix_set_int(d: &mut DataBuilder, prefix: &str, field: &str, value: i32) {
    d.with_int(dix_path(prefix, field), value);
}
#[inline]
pub fn dix_set_long(d: &mut DataBuilder, prefix: &str, field: &str, value: i64) {
    d.with_long(dix_path(prefix, field), value);
}
/// Write a float field at `prefix.field`.
#[inline]
pub fn dix_set_float(d: &mut DataBuilder, prefix: &str, field: &str, value: f32) {
    d.with_float(dix_path(prefix, field), value);
}

/// Write a double field at `prefix.field`.
#[inline]
pub fn dix_set_double(d: &mut DataBuilder, prefix: &str, field: &str, value: f64) {
    d.with_double(dix_path(prefix, field), value);
}

/// Write a bool field at `prefix.field`.
#[inline]
pub fn dix_set_bool(d: &mut DataBuilder, prefix: &str, field: &str, value: bool) {
    d.with_bool(dix_path(prefix, field), value);
}

/// Write a nested struct implementing [`DixSerialize`] at `prefix.field`.
///
/// The struct's fields will be written at `prefix.field.its_field`.
///
/// ```rust,ignore
/// dix_set_nested(d, "config", "database", &self.database)?;
/// // Writes: config.database.host, config.database.port, ...
/// ```
pub fn dix_set_nested<T: DixSerialize>(
    d: &mut DataBuilder,
    prefix: &str,
    field: &str,
    value: &T,
) -> Result<(), String> {
    value.to_dix(d, &dix_path(prefix, field))
}

// ── DixDataBuilder convenience extension ──────────────────────────────────────

impl DixDataBuilder {
    /// Serialize `value` into the database at the root (no prefix).
    ///
    /// ```rust,ignore
    /// let data = DixDataBuilder::new()
    ///     .serialize(&app_config)
    ///     .build()?;
    /// ```
    pub fn serialize<T: DixSerialize>(mut self, value: &T) -> Self {
        let result = value.to_dix(&mut self.data_builder, "");
        if let Err(e) = result {
            self.data_builder.push_deferred_error(e);
        }
        self
    }

    /// Serialize `value` into the database with all paths rooted at `prefix`.
    ///
    /// ```rust,ignore
    /// let data = DixDataBuilder::new()
    ///     .serialize_at("server", &server_config)
    ///     .serialize_at("database", &db_config)
    ///     .build()?;
    /// ```
    pub fn serialize_at<T: DixSerialize>(mut self, prefix: &str, value: &T) -> Self {
        let result = value.to_dix(&mut self.data_builder, prefix);
        if let Err(e) = result {
            self.data_builder.push_deferred_error(e);
        }
        self
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Runtime::{DixDataBuilder, DixDeserialize, dix_deserialize::dix_get};

    // ── Test structs ──────────────────────────────────────────────────────────

    #[derive(Debug, PartialEq, Clone)]
    struct Point {
        x: f64,
        y: f64,
    }

    impl DixSerialize for Point {
        fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
            dix_set_double(d, prefix, "x", self.x);
            dix_set_double(d, prefix, "y", self.y);
            Ok(())
        }
    }

    impl DixDeserialize for Point {
        fn from_dix(
            data: &crate::Runtime::DixData,
            prefix: &str,
        ) -> Result<Self, String> {
            Ok(Point {
                x: dix_get(data, prefix, "x")?,
                y: dix_get(data, prefix, "y")?,
            })
        }
    }

    #[derive(Debug, PartialEq, Clone)]
    struct AppConfig {
        name:    String,
        version: String,
        port:    i32,
        debug:   bool,
    }

    impl DixSerialize for AppConfig {
        fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
            dix_set_str(d,  prefix, "name",    &self.name);
            dix_set_str(d,  prefix, "version", &self.version);
            dix_set_int(d,  prefix, "port",     self.port);
            dix_set_bool(d, prefix, "debug",    self.debug);
            Ok(())
        }
    }

    impl DixDeserialize for AppConfig {
        fn from_dix(
            data: &crate::Runtime::DixData,
            prefix: &str,
        ) -> Result<Self, String> {
            Ok(AppConfig {
                name:    dix_get(data, prefix, "name")?,
                version: dix_get(data, prefix, "version")?,
                port:    dix_get(data, prefix, "port")?,
                debug:   dix_get(data, prefix, "debug")
                            .unwrap_or(false),
            })
        }
    }

    // ── Serialization tests ───────────────────────────────────────────────────

    #[test]
    fn test_serialize_flat_struct_at_root() {
        let config = AppConfig {
            name:    "TestApp".to_string(),
            version: "1.0.0".to_string(),
            port:    9090,
            debug:   false,
        };

        let data = DixDataBuilder::new()
            .serialize(&config)
            .build()
            .unwrap();

        assert!(data.exists("name"));
        assert!(data.exists("port"));
        let name: String = data.get("name").unwrap();
        let port: i32    = data.get("port").unwrap();
        assert_eq!(name, "TestApp");
        assert_eq!(port, 9090);
    }

    #[test]
    fn test_serialize_at_prefix() {
        let cfg = AppConfig {
            name:    "API".to_string(),
            version: "2.0.0".to_string(),
            port:    443,
            debug:   true,
        };

        let data = DixDataBuilder::new()
            .serialize_at("app", &cfg)
            .build()
            .unwrap();

        assert!(data.exists("app.name"));
        assert!(data.exists("app.port"));
        let name: String = data.get("app.name").unwrap();
        assert_eq!(name, "API");
    }

    #[test]
    fn test_serialize_multiple_structs() {
        let server = Point { x: 1.0, y: 2.0 };
        let client = Point { x: 3.0, y: 4.0 };

        let data = DixDataBuilder::new()
            .serialize_at("server", &server)
            .serialize_at("client", &client)
            .build()
            .unwrap();

        let s: f64 = data.get("server.x").unwrap();
        let c: f64 = data.get("client.y").unwrap();
        assert!((s - 1.0).abs() < 1e-9);
        assert!((c - 4.0).abs() < 1e-9);
    }

    // ── Round-trip tests ──────────────────────────────────────────────────────

    #[test]
    fn test_round_trip_flat() {
        let original = AppConfig {
            name:    "RoundTrip".to_string(),
            version: "3.0.0".to_string(),
            port:    7777,
            debug:   true,
        };

        let data = DixDataBuilder::new()
            .serialize(&original)
            .build()
            .unwrap();

        let recovered: AppConfig = data.deserialize().unwrap();
        assert_eq!(recovered, original);
    }

    #[test]
    fn test_round_trip_with_prefix() {
        let original = Point { x: 12.5, y: -3.14 };

        let data = DixDataBuilder::new()
            .serialize_at("position", &original)
            .build()
            .unwrap();

        let recovered: Point = data.deserialize_at("position").unwrap();
        assert!((recovered.x - original.x).abs() < 1e-9);
        assert!((recovered.y - original.y).abs() < 1e-9);
    }

    #[test]
    fn test_option_some_is_written() {
        let val: Option<i32> = Some(42);

        let data = DixDataBuilder::new()
            .data(|d| {
                val.to_dix(d, "count").unwrap();
            })
            .build()
            .unwrap();

        assert!(data.exists("count"));
        let v: i32 = data.get("count").unwrap();
        assert_eq!(v, 42);
    }

    #[test]
    fn test_option_none_writes_nothing() {
        let val: Option<i32> = None;

        let data = DixDataBuilder::new()
            .data(|d| {
                val.to_dix(d, "count").unwrap();
                // Add something else so the builder doesn't produce an empty section
                d.with_string("name", "test");
            })
            .build()
            .unwrap();

        assert!(!data.exists("count"));
    }
              }
