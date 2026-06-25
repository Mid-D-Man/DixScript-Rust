//! Trait and helpers for reading Rust structs from a loaded DixScript database.
//!
//! # Quick start
//!
//! Implement [`DixDeserialize`] for your config struct, then call
//! [`DixData::deserialize_at`] to load it.
//!
//! ```rust,ignore
//! use dixscript::Runtime::{DixData, DixDeserialize, dix_get, dix_get_or};
//!
//! #[derive(Debug)]
//! pub struct ServerConfig {
//!     pub host: String,
//!     pub port: i32,
//!     pub ssl:  bool,
//! }
//!
//! impl DixDeserialize for ServerConfig {
//!     fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
//!         Ok(ServerConfig {
//!             host: dix_get(data, prefix, "host")?,
//!             port: dix_get(data, prefix, "port")?,
//!             ssl:  dix_get_or(data, prefix, "ssl", false),
//!         })
//!     }
//! }
//!
//! // .mdix source:
//! // @DATA(
//! //   server: host = "api.example.com", port = 443, ssl = true
//! // )
//!
//! let loader = DixLoader::new();
//! let data   = loader.load_text("config.mdix", &DixLoadOptions::new())?;
//! let server: ServerConfig = data.deserialize_at("server")?;
//! println!("{}", server.host); // api.example.com
//! ```

use super::dix_data::DixData;
use super::dix_value::DixValue;

// ── Core trait ────────────────────────────────────────────────────────────────

/// Implemented by types that can be read from a [`DixData`] store.
///
/// All field paths inside the implementation are resolved relative to `prefix`.
/// Pass `""` to read from the top level.
pub trait DixDeserialize: Sized {
    /// Deserialize `Self` from `data` with all paths relative to `prefix`.
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String>;
}

// ── Primitive implementations ─────────────────────────────────────────────────

impl DixDeserialize for String {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        data.get(prefix)
    }
}

impl DixDeserialize for i32 {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        data.get(prefix)
    }
}

impl DixDeserialize for i64 {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
     data.get(prefix)
    }
}

impl DixDeserialize for f32 {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        let v: f64 = data.get(prefix)?;
        Ok(v as f32)
    }
}

impl DixDeserialize for f64 {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        data.get(prefix)
    }
}

impl DixDeserialize for bool {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        data.get(prefix)
    }
}

impl DixDeserialize for DixValue {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        data.get_value(prefix)
            .cloned()
            .ok_or_else(|| format!("Path not found: {}", prefix))
    }
}

// ── Option<T> ─────────────────────────────────────────────────────────────────

/// `Option<T>` deserializes as `None` when the path is absent,
/// and as `Some(T)` when it is present (and succeeds).
///
/// "Present" means either:
/// - `prefix` itself is a key in the flattened data (true for scalar fields
///   and for `ObjectProperty`-declared nested objects, which are stored both
///   as a whole `DixValue::Object` AND as individual `prefix.field` keys), OR
/// - `prefix` has at least one child key `prefix.*` (true for
///   `TableProperty`-declared nested structs, which only ever appear as
///   `prefix.field` keys — `prefix` itself is never inserted).
///
/// FIX: previously only `data.exists(prefix)` was checked, which meant
/// `Option<T>` for any table-property-declared nested struct (the common
/// case — `server: host = ..., port = ...`) always evaluated to `None`,
/// even when `server.host` / `server.port` were fully present.
impl<T: DixDeserialize> DixDeserialize for Option<T> {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        let present = data.exists(prefix) || !data.get_keys(prefix).is_empty();
        if !present {
            return Ok(None);
        }
        T::from_dix(data, prefix).map(Some)
    }
}

// ── Vec<T> for scalar element types ──────────────────────────────────────────

/// Deserializes a DixScript group array into `Vec<T>` where `T` is a scalar
/// type convertible from [`DixValue`].
///
/// For arrays of complex structs use [`dix_array_of`] instead.
impl<T> DixDeserialize for Vec<T>
where
    T: TryFrom<DixValue>,
    <T as TryFrom<DixValue>>::Error: std::fmt::Display,
{
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        let arr: Vec<DixValue> = data.get(prefix)?;
        arr.into_iter()
            .enumerate()
            .map(|(i, v)| {
                T::try_from(v)
                    .map_err(|e| format!("{}[{}]: {}", prefix, i, e))
            })
            .collect()
    }
}

// ── Public helper functions ───────────────────────────────────────────────────

/// Read a typed field at `prefix.field`.
///
/// Returns `Err` if the path is absent or the type does not match.
///
/// ```rust,ignore
/// let port: i32 = dix_get(&data, "server", "port")?;
/// ```
pub fn dix_get<T>(data: &DixData, prefix: &str, field: &str) -> Result<T, String>
where
    T: TryFrom<DixValue>,
    <T as TryFrom<DixValue>>::Error: std::fmt::Display,
{
    data.get(&dix_path(prefix, field))
}

/// Read a typed field, returning `default` if the path is absent or the type
/// does not match.
///
/// ```rust,ignore
/// let ssl = dix_get_or(&data, "server", "ssl", false);
/// ```
pub fn dix_get_or<T>(data: &DixData, prefix: &str, field: &str, default: T) -> T
where
    T: TryFrom<DixValue>,
    <T as TryFrom<DixValue>>::Error: std::fmt::Display,
{
    dix_get(data, prefix, field).unwrap_or(default)
}

/// Deserialize a nested struct at `prefix.field`.
///
/// Equivalent to `T::from_dix(data, "prefix.field")`.
///
/// ```rust,ignore
/// let db: DatabaseConfig = dix_nested(&data, "config", "database")?;
/// ```
pub fn dix_nested<T: DixDeserialize>(
    data: &DixData,
    prefix: &str,
    field: &str,
) -> Result<T, String> {
    T::from_dix(data, &dix_path(prefix, field))
}

/// Deserialize an array of complex structs.
///
/// Each element at `prefix.field[i]` is passed to `T::from_dix`. Use this
/// when the array items are structs rather than scalars.
///
/// ```rust,ignore
/// let enemies: Vec<Enemy> = dix_array_of(&data, "", "enemies")?;
/// // Reads enemies[0].name, enemies[0].hp, enemies[1].name, ...
/// ```
pub fn dix_array_of<T: DixDeserialize>(
    data: &DixData,
    prefix: &str,
    field: &str,
) -> Result<Vec<T>, String> {
    let path = dix_path(prefix, field);
    let arr: Vec<DixValue> = data.get(&path)?;
    let count = arr.len();

    (0..count)
        .map(|i| {
            let item_path = format!("{}[{}]", path, i);
            T::from_dix(data, &item_path)
                .map_err(|e| format!("{}[{}]: {}", path, i, e))
        })
        .collect()
}

/// Read the raw [`DixValue`] at `prefix.field` without type conversion.
pub fn dix_value<'a>(data: &'a DixData, prefix: &str, field: &str) -> Option<&'a DixValue> {
    data.get_value(&dix_path(prefix, field))
}

/// Build a dotted path from a prefix and a field segment.
///
/// Returns `field` unchanged when `prefix` is empty.
#[inline]
pub fn dix_path(prefix: &str, field: &str) -> String {
    if prefix.is_empty() {
        field.to_string()
    } else {
        format!("{}.{}", prefix, field)
    }
}

// ── DixData extension ─────────────────────────────────────────────────────────

impl DixData {
    /// Deserialize the entire database into `T` starting from the root.
    ///
    /// Equivalent to `T::from_dix(self, "")`.
    ///
    /// ```rust,ignore
    /// let config: AppConfig = data.deserialize()?;
    /// ```
    pub fn deserialize<T: DixDeserialize>(&self) -> Result<T, String> {
        T::from_dix(self, "")
    }

    /// Deserialize a section of the database into `T`.
    ///
    /// All paths inside `T::from_dix` are resolved relative to `prefix`.
    ///
    /// ```rust,ignore
    /// let server: ServerConfig = data.deserialize_at("server")?;
    /// let db: DbConfig         = data.deserialize_at("database")?;
    /// ```
    pub fn deserialize_at<T: DixDeserialize>(&self, prefix: &str) -> Result<T, String> {
        T::from_dix(self, prefix)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Runtime::{DixDataBuilder};
    use crate::Compiler::AST::*;
    use chrono::Utc;

    // ── Test fixture ──────────────────────────────────────────────────────────

    fn flat_data() -> DixData {
        DixDataBuilder::new()
            .data(|d| {
                d.with_string("name",  "MyApp");
                d.with_int("port",     8080);
                d.with_bool("debug",   true);
                d.with_double("ratio", 1.5);
            })
            .build()
            .unwrap()
    }

    fn nested_data() -> DixData {
        DixDataBuilder::new()
            .data(|d| {
                d.with_string("version", "1.0.0");
                d.with_table_properties("server", |t| {
                    t.with_string("host", "localhost");
                    t.with_int("port",    443);
                    t.with_bool("ssl",    true);
                });
                d.with_table_properties("db", |t| {
                    t.with_string("host", "db.internal");
                    t.with_int("port",    5432);
                });
            })
            .build()
            .unwrap()
    }

    fn array_data() -> DixData {
        DixDataBuilder::new()
            .data(|d| {
                d.with_string("name", "host");
                d.with_group_array_builder("tags", |arr| {
                    arr.add_string("alpha");
                    arr.add_string("beta");
                    arr.add_string("gamma");
                });
            })
            .build()
            .unwrap()
    }

    // ── Test struct ───────────────────────────────────────────────────────────

    #[derive(Debug, PartialEq)]
    struct ServerCfg {
        host: String,
        port: i32,
        ssl:  bool,
    }

    impl DixDeserialize for ServerCfg {
        fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
            Ok(ServerCfg {
                host: dix_get(data, prefix, "host")?,
                port: dix_get(data, prefix, "port")?,
                ssl:  dix_get_or(data, prefix, "ssl", false),
            })
        }
    }

    // ── Scalar deserialization ────────────────────────────────────────────────

    #[test]
    fn test_string_from_dix() {
        let data = flat_data();
        let name: String = data.deserialize_at("name").unwrap();
        assert_eq!(name, "MyApp");
    }

    #[test]
    fn test_int_from_dix() {
        let data = flat_data();
        let port: i32 = data.deserialize_at("port").unwrap();
        assert_eq!(port, 8080);
    }

    #[test]
    fn test_bool_from_dix() {
        let data = flat_data();
        let debug: bool = data.deserialize_at("debug").unwrap();
        assert!(debug);
    }

    #[test]
    fn test_f64_from_dix() {
        let data = flat_data();
        let r: f64 = data.deserialize_at("ratio").unwrap();
        assert!((r - 1.5).abs() < 1e-9);
    }

    // ── Struct deserialization ────────────────────────────────────────────────

    #[test]
    fn test_struct_deserialize_at_prefix() {
        let data = nested_data();
        let server: ServerCfg = data.deserialize_at("server").unwrap();
        assert_eq!(server.host, "localhost");
        assert_eq!(server.port, 443);
        assert!(server.ssl);
    }

    #[test]
    fn test_struct_deserialize_second_prefix() {
        let data = nested_data();
        let db: ServerCfg = data.deserialize_at("db").unwrap();
        assert_eq!(db.host, "db.internal");
        assert_eq!(db.port, 5432);
        assert!(!db.ssl); // optional, defaults to false
    }

    // ── dix_get / dix_get_or helpers ──────────────────────────────────────────

    #[test]
    fn test_dix_get_ok() {
        let data = nested_data();
        let host: String = dix_get(&data, "server", "host").unwrap();
        assert_eq!(host, "localhost");
    }

    #[test]
    fn test_dix_get_missing_returns_err() {
        let data = flat_data();
        let result: Result<String, _> = dix_get(&data, "", "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_dix_get_or_returns_default() {
        let data = flat_data();
        let val = dix_get_or(&data, "", "nonexistent", 9090_i32);
        assert_eq!(val, 9090);
    }

    // ── Option<T> — scalar ───────────────────────────────────────────────────

    #[test]
    fn test_option_present_returns_some() {
        let data = flat_data();
        let name: Option<String> = data.deserialize_at("name").unwrap();
        assert_eq!(name, Some("MyApp".to_string()));
    }

    #[test]
    fn test_option_absent_returns_none() {
        let data = flat_data();
        let val: Option<String> = data.deserialize_at("nonexistent").unwrap();
        assert_eq!(val, None);
    }

    // ── Option<T> — nested struct (the bug that was found) ────────────────────

    #[test]
    fn test_option_struct_present_via_table_property_returns_some() {
        // "server" is a TableProperty: "server" itself is never inserted as
        // a key, only "server.host" / "server.port" / "server.ssl" are.
        // Before the fix, `data.exists("server")` was false, so
        // `Option<ServerCfg>` always returned None here.
        let data = nested_data();
        let server: Option<ServerCfg> = data.deserialize_at("server").unwrap();
        assert!(server.is_some(), "expected Some(ServerCfg) for a table-property prefix");

        let server = server.unwrap();
        assert_eq!(server.host, "localhost");
        assert_eq!(server.port, 443);
        assert!(server.ssl);
    }

    #[test]
    fn test_option_struct_absent_returns_none_with_no_related_keys() {
        let data = nested_data();
        let missing: Option<ServerCfg> = data.deserialize_at("nonexistent").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_option_struct_present_via_object_property_returns_some() {
        // ObjectProperty-declared nested objects DID work before the fix
        // (they get a literal "server" key as DixValue::Object). This test
        // guards against a regression in that path.
        let ast = DixScript {
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
            data: Some(DataSection {
                entries: vec![DataEntry::ObjectProperty {
                    name: "server".into(),
                    data_type: None,
                    object: Box::new(Value::Object {
                        properties: vec![
                            ObjectProperty::new(
                                "host".into(),
                                Value::String { value: "localhost".into(), position: Position::UNKNOWN },
                                Position::UNKNOWN,
                            ),
                            ObjectProperty::new(
                                "port".into(),
                                Value::Integer { value: 443, position: Position::UNKNOWN },
                                Position::UNKNOWN,
                            ),
                        ],
                        position: Position::UNKNOWN,
                    }),
                    position: Position::UNKNOWN,
                }],
                position: Position::UNKNOWN,
            }),
        };

        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        let server: Option<ServerCfg> = data.deserialize_at("server").unwrap();
        assert!(server.is_some());
        let server = server.unwrap();
        assert_eq!(server.host, "localhost");
        assert_eq!(server.port, 443);
        assert!(!server.ssl); // not present, defaults via dix_get_or
    }

    // ── Vec<T> scalar ────────────────────────────────────────────────────────

    #[test]
    fn test_vec_string_deserialization() {
        let data = array_data();
        let tags: Vec<String> = data.deserialize_at("tags").unwrap();
        assert_eq!(tags, vec!["alpha", "beta", "gamma"]);
    }

    // ── dix_nested ───────────────────────────────────────────────────────────

    #[test]
    fn test_dix_nested() {
        let data = nested_data();
        let server: ServerCfg = dix_nested(&data, "", "server").unwrap();
        assert_eq!(server.host, "localhost");
    }

    // ── dix_array_of ─────────────────────────────────────────────────────────

    #[test]
    fn test_dix_array_of_structs_when_absent_errors_not_panics() {
        let data = nested_data();
        // "servers" doesn't exist → should error on the array lookup, not panic
        let result: Result<Vec<ServerCfg>, _> = dix_array_of(&data, "", "servers");
        assert!(result.is_err());
    }

    #[test]
    fn test_dix_array_of_populated_structs() {
        fn item(name: &str, port: i32) -> Value {
            Value::Object {
                properties: vec![
                    ObjectProperty::new("host".into(), Value::String { value: name.into(), position: Position::UNKNOWN }, Position::UNKNOWN),
                    ObjectProperty::new("port".into(), Value::Integer { value: port, position: Position::UNKNOWN }, Position::UNKNOWN),
                ],
                position: Position::UNKNOWN,
            }
        }

        let data = DixDataBuilder::new()
            .data(|d| {
                d.with_string("title", "Cluster");
                d.with_group_array("servers", vec![
                    item("node-a", 7000),
                    item("node-b", 7001),
                ]);
            })
            .build()
            .unwrap();

        let servers: Vec<ServerCfg> = dix_array_of(&data, "", "servers").unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].host, "node-a");
        assert_eq!(servers[0].port, 7000);
        assert_eq!(servers[1].host, "node-b");
        assert_eq!(servers[1].port, 7001);
    }

    // ── dix_path ─────────────────────────────────────────────────────────────

    #[test]
    fn test_dix_path_empty_prefix() {
        assert_eq!(dix_path("", "port"), "port");
    }

    #[test]
    fn test_dix_path_with_prefix() {
        assert_eq!(dix_path("server", "port"), "server.port");
    }

    #[test]
    fn test_dix_path_nested() {
        assert_eq!(dix_path("config.database", "host"), "config.database.host");
    }
}
