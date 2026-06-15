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
//!
//! # Arrays
//!
//! Scalar arrays (`Vec<i32>`, `Vec<String>`, `Vec<bool>`, ...) implement
//! [`DixSerialize`] directly via a blanket impl over any `T: Into<DixValue> + Clone`,
//! writing a `GroupArray` at `prefix`:
//!
//! ```rust,ignore
//! let data = DixDataBuilder::new()
//!     .serialize_at("scores", &vec![10, 20, 30])
//!     .build()
//!     .unwrap();
//! ```
//!
//! Struct arrays use [`dix_set_array_of`] (the write-side mirror of
//! [`dix_array_of`](super::dix_array_of)):
//!
//! ```rust,ignore
//! let data = DixDataBuilder::new()
//!     .data(|d| {
//!         d.with_string("title", "Cluster");
//!         dix_set_array_of(d, "", "servers", &servers).unwrap();
//!     })
//!     .build()
//!     .unwrap();
//! ```

use crate::Compiler::AST::{ObjectProperty, Position, Value};
use super::data_builder::{DataBuilder, DixDataBuilder};
use super::dix_deserialize::dix_path;
use super::dix_value::DixValue;

// ── Core trait ────────────────────────────────────────────────────────────────

/// Implemented by types that can be written into a [`DataBuilder`].
///
/// `prefix` is the full dotted path at which `Self` should be written.
///
/// - Struct impls treat `prefix` as the **parent** path and write each field
///   via the `dix_set_*` helpers (which append `.field` to `prefix`).
/// - Leaf/scalar impls (the primitives below, `Option<T>`, and `Vec<T>`) write
///   a single value directly **at** `prefix` itself.
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

// ── Vec<T> for scalar element types ──────────────────────────────────────────

/// Serializes `Vec<T>` (where `T` converts to [`DixValue`]) as a `GroupArray`
/// at `prefix` — the write-side mirror of the `Vec<T>` deserialize blanket
/// impl in `dix_deserialize.rs`.
///
/// Covers `Vec<i32>`, `Vec<i64>`, `Vec<f32>`, `Vec<f64>`, `Vec<bool>`,
/// `Vec<String>`, and `Vec<DixValue>` (via the identity `Into<DixValue>` impl).
///
/// For arrays of structs, use [`dix_set_array_of`] instead — `Vec<T>` here
/// requires `T: Into<DixValue>`, which user-defined structs don't implement.
impl<T> DixSerialize for Vec<T>
where
    T: Into<DixValue> + Clone,
{
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        let items: Vec<Value> = self
            .iter()
            .map(|item| dix_value_to_value(&item.clone().into()))
            .collect();
        d.with_group_array(prefix.to_string(), items);
        Ok(())
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

/// Write an array of structs at `prefix.field` — the write-side mirror of
/// [`dix_array_of`](super::dix_array_of).
///
/// Each item is serialized through a throwaway [`DixDataBuilder`], converted
/// to its [structural hashmap](super::dix_data::DixData::to_structural_hashmap)
/// (which already reconstructs nested objects from any dotted paths produced
/// by [`dix_set_nested`]), and wrapped as a `Value::Object`. The resulting
/// objects are written as a `GroupArray` at `prefix.field`.
///
/// ```rust,ignore
/// struct Server { host: String, port: i32 }
/// impl DixSerialize for Server {
///     fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
///         dix_set_str(d, prefix, "host", &self.host);
///         dix_set_int(d, prefix, "port", self.port);
///         Ok(())
///     }
/// }
///
/// let servers = vec![
///     Server { host: "node-a".into(), port: 7000 },
///     Server { host: "node-b".into(), port: 7001 },
/// ];
///
/// let data = DixDataBuilder::new()
///     .data(|d| {
///         d.with_string("title", "Cluster");
///         dix_set_array_of(d, "", "servers", &servers).unwrap();
///     })
///     .build()
///     .unwrap();
///
/// // Read back via dix_array_of(&data, "", "servers")
/// ```
pub fn dix_set_array_of<T: DixSerialize>(
    d: &mut DataBuilder,
    prefix: &str,
    field: &str,
    items: &[T],
) -> Result<(), String> {
    let mut values = Vec::with_capacity(items.len());

    for item in items {
        let item_data = DixDataBuilder::new().serialize(item).build()?;
        let map = item_data.to_structural_hashmap();
        values.push(dix_value_to_value(&DixValue::Object(map)));
    }

    d.with_group_array(dix_path(prefix, field), values);
    Ok(())
}

// ── DixValue -> Value conversion ──────────────────────────────────────────────

/// Convert a runtime [`DixValue`] back into an AST [`Value`] node.
///
/// Infallible mirror of `DixConverter::convert_dix_value_to_ast_value` —
/// every `DixValue` variant has a corresponding `Value` representation, so
/// this never needs to return `Result`. Used by the `Vec<T>` impl above and
/// by [`dix_set_array_of`] to turn computed [`DixValue`]s back into AST nodes
/// for [`DataBuilder::with_group_array`].
fn dix_value_to_value(value: &DixValue) -> Value {
    match value {
        DixValue::Null      => Value::Null { position: Position::UNKNOWN },
        DixValue::Bool(b)   => Value::Boolean   { value: *b,  position: Position::UNKNOWN },
        DixValue::Int(i)    => Value::Integer   { value: *i,  position: Position::UNKNOWN },
        DixValue::Long(l)   => Value::Long      { value: *l,  position: Position::UNKNOWN },
        DixValue::Float(f)  => Value::Float     { value: *f,  position: Position::UNKNOWN },
        DixValue::Double(d) => Value::Double    { value: *d,  position: Position::UNKNOWN },
        DixValue::String(s)    => Value::String    { value: s.clone(), position: Position::UNKNOWN },
        DixValue::Date(d)      => Value::Date      { value: d.clone(), position: Position::UNKNOWN },
        DixValue::Timestamp(t) => Value::Timestamp { value: t.clone(), position: Position::UNKNOWN },
        DixValue::HexColor(c)  => Value::HexColor  { value: c.clone(), position: Position::UNKNOWN },

        DixValue::Blob(b) => Value::PrefixedConstructor {
            prefix:    "b".to_string(),
            arguments: vec![Value::String { value: b.clone(), position: Position::UNKNOWN }],
            position:  Position::UNKNOWN,
        },
        DixValue::Regex(r) => Value::PrefixedConstructor {
            prefix:    "r".to_string(),
            arguments: vec![Value::String { value: r.clone(), position: Position::UNKNOWN }],
            position:  Position::UNKNOWN,
        },

        DixValue::Array(arr) => Value::Array {
            values:   arr.iter().map(dix_value_to_value).collect(),
            position: Position::UNKNOWN,
        },

        DixValue::Object(obj) => Value::Object {
            properties: obj.iter()
                .map(|(k, v)| ObjectProperty {
                    key:      k.clone(),
                    value:    dix_value_to_value(v),
                    position: Position::UNKNOWN,
                })
                .collect(),
            position: Position::UNKNOWN,
        },

        DixValue::Tuple(items) => Value::PrefixedConstructor {
            prefix:    "t".to_string(),
            arguments: items.iter().map(dix_value_to_value).collect(),
            position:  Position::UNKNOWN,
        },

        DixValue::Enum { enum_name, field_name, .. } => Value::EnumValue {
            enum_name: enum_name.clone(),
            value:     field_name.clone(),
            position:  Position::UNKNOWN,
        },
    }
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Runtime::dix_deserialize::{dix_array_of, dix_get, dix_get_or};
    use crate::Runtime::DixDeserialize;

    // ── Test structs ──────────────────────────────────────────────────────────

    struct ServerCfg {
        host: String,
        port: i32,
        ssl:  bool,
    }

    impl DixSerialize for ServerCfg {
        fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
            dix_set_str(d, prefix, "host", &self.host);
            dix_set_int(d, prefix, "port", self.port);
            d.with_bool(dix_path(prefix, "ssl"), self.ssl);
            Ok(())
        }
    }

    impl DixDeserialize for ServerCfg {
        fn from_dix(data: &crate::Runtime::DixData, prefix: &str) -> Result<Self, String> {
            Ok(ServerCfg {
                host: dix_get(data, prefix, "host")?,
                port: dix_get(data, prefix, "port")?,
                ssl:  dix_get_or(data, prefix, "ssl", false),
            })
        }
    }

    /// Struct with a nested sub-struct, written via `dix_set_nested`.
    struct AppCfg {
        name:   String,
        server: ServerCfg,
    }

    impl DixSerialize for AppCfg {
        fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
            dix_set_str(d, prefix, "name", &self.name);
            dix_set_nested(d, prefix, "server", &self.server)?;
            Ok(())
        }
    }

    // ── Scalars ───────────────────────────────────────────────────────────────

    #[test]
    fn test_scalar_round_trip() {
        let data = DixDataBuilder::new()
            .data(|d| {
                d.with_string("name", "MyApp");
                d.with_int("port", 8080);
                d.with_bool("debug", true);
                d.with_double("ratio", 1.5);
            })
            .build()
            .unwrap();

        assert_eq!(data.get::<String>("name").unwrap(), "MyApp");
        assert_eq!(data.get::<i32>("port").unwrap(), 8080);
        assert!(data.get::<bool>("debug").unwrap());
        assert!((data.get::<f64>("ratio").unwrap() - 1.5).abs() < 1e-9);
    }

    // ── Option<T> ─────────────────────────────────────────────────────────────

    #[test]
    fn test_option_none_writes_nothing_some_writes_value() {
        let none_val: Option<String> = None;
        let some_val: Option<i32>    = Some(42);

        let data = DixDataBuilder::new()
            .serialize_at("absent", &none_val)
            .serialize_at("present", &some_val)
            .build()
            .unwrap();

        assert!(!data.exists("absent"), "None must write nothing");
        assert_eq!(data.get::<i32>("present").unwrap(), 42);
    }

    // ── Nested struct via dix_set_nested ──────────────────────────────────────

    #[test]
    fn test_nested_struct_round_trip() {
        let app = AppCfg {
            name: "Mid Engine".into(),
            server: ServerCfg { host: "localhost".into(), port: 443, ssl: true },
        };

        let data = DixDataBuilder::new()
            .serialize_at("app", &app)
            .build()
            .unwrap();

        assert_eq!(data.get::<String>("app.name").unwrap(), "Mid Engine");
        let server: ServerCfg = data.deserialize_at("app.server").unwrap();
        assert_eq!(server.host, "localhost");
        assert_eq!(server.port, 443);
        assert!(server.ssl);
    }

    // ── Vec<T> scalar arrays ──────────────────────────────────────────────────

    #[test]
    fn test_vec_scalar_serialize_round_trip() {
        let scores = vec![10, 20, 30];
        let tags   = vec!["alpha".to_string(), "beta".to_string()];

        let data = DixDataBuilder::new()
            .data(|d| d.with_string("title", "Leaderboard"))
            .serialize_at("scores", &scores)
            .serialize_at("tags", &tags)
            .build()
            .unwrap();

        let scores_back: Vec<i32>    = data.deserialize_at("scores").unwrap();
        let tags_back:   Vec<String> = data.deserialize_at("tags").unwrap();

        assert_eq!(scores_back, vec![10, 20, 30]);
        assert_eq!(tags_back, vec!["alpha".to_string(), "beta".to_string()]);
    }

    // ── dix_set_array_of: struct arrays ───────────────────────────────────────

    #[test]
    fn test_dix_set_array_of_struct_array_round_trip() {
        let servers = vec![
            ServerCfg { host: "node-a".into(), port: 7000, ssl: false },
            ServerCfg { host: "node-b".into(), port: 7001, ssl: true },
        ];

        let data = DixDataBuilder::new()
            .data(|d| {
                d.with_string("title", "Cluster");
                dix_set_array_of(d, "", "servers", &servers).unwrap();
            })
            .build()
            .unwrap();

        let back: Vec<ServerCfg> = dix_array_of(&data, "", "servers").unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].host, "node-a");
        assert_eq!(back[0].port, 7000);
        assert!(!back[0].ssl);
        assert_eq!(back[1].host, "node-b");
        assert!(back[1].ssl);
    }

    // ── Deferred error propagation ────────────────────────────────────────────

    struct BadColor;
    impl DixSerialize for BadColor {
        fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
            // Missing leading '#' — DataBuilder::with_hex_color records a
            // deferred error rather than returning Err directly.
            d.with_hex_color(dix_path(prefix, "color"), "FF5733");
            Ok(())
        }
    }

    #[test]
    fn test_serialize_deferred_error_propagates_through_build() {
        let result = DixDataBuilder::new()
            .serialize_at("theme", &BadColor)
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains('#'));
    }
    }
