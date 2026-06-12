// dixscript/tests/serde_schema_integration_test.rs
//! End-to-end integration tests for DixSerialize / DixDeserialize /
//! SchemaBuilder, covering:
//!
//! - Multi-level nested struct round trips (3 levels deep)
//! - Option<T> for both present and absent nested structs
//! - Arrays of structs via `dix_array_of`
//! - Every `ExpectedValueType` variant, including widening rules
//! - Two-tier ordering interaction with `serialize_at`
//! - Known gaps / asymmetries (documented, not silently ignored)

use dixscript::Compiler::AST::{Value, ObjectProperty, Position};
use dixscript::Runtime::{
    DixData, DixDataBuilder, DixDeserialize, DixSerialize, DataBuilder,
    ExpectedValueType, SchemaBuilder, ValidationErrorKind,
    dix_array_of, dix_get, dix_get_or, dix_nested, dix_path, dix_set_bool,
    dix_set_double, dix_set_int, dix_set_nested, dix_set_str,
};
use dixscript::Runtime::dix_serialize::dix_set_long;

// ─────────────────────────────────────────────────────────────────────────────
// Test types — three levels of nesting
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
struct TlsConfig {
    enabled:     bool,
    min_version: String,
}

impl DixSerialize for TlsConfig {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        dix_set_bool(d, prefix, "enabled", self.enabled);
        dix_set_str(d, prefix, "min_version", &self.min_version);
        Ok(())
    }
}

impl DixDeserialize for TlsConfig {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        Ok(TlsConfig {
            enabled:     dix_get(data, prefix, "enabled")?,
            min_version: dix_get(data, prefix, "min_version")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ServerConfig {
    host:    String,
    port:    i32,
    node_id: i64,
    tls:     TlsConfig,
}

impl DixSerialize for ServerConfig {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        dix_set_str(d, prefix, "host", &self.host);
        dix_set_int(d, prefix, "port", self.port);
        dix_set_long(d, prefix, "node_id", self.node_id);
        dix_set_nested(d, prefix, "tls", &self.tls)?;
        Ok(())
    }
}

impl DixDeserialize for ServerConfig {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        Ok(ServerConfig {
            host:    dix_get(data, prefix, "host")?,
            port:    dix_get(data, prefix, "port")?,
            node_id: dix_get(data, prefix, "node_id")?,
            tls:     dix_nested(data, prefix, "tls")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct AppConfig {
    name:        String,
    version:     String,
    debug:       bool,
    sample_rate: f64,
    server:      ServerConfig,
    admin_email: Option<String>,
}

impl DixSerialize for AppConfig {
    fn to_dix(&self, d: &mut DataBuilder, prefix: &str) -> Result<(), String> {
        dix_set_str(d, prefix, "name", &self.name);
        dix_set_str(d, prefix, "version", &self.version);
        dix_set_bool(d, prefix, "debug", self.debug);
        dix_set_double(d, prefix, "sample_rate", self.sample_rate);
        dix_set_nested(d, prefix, "server", &self.server)?;
        self.admin_email.to_dix(d, &dix_path(prefix, "admin_email"))?;
        Ok(())
    }
}

impl DixDeserialize for AppConfig {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        Ok(AppConfig {
            name:        dix_get(data, prefix, "name")?,
            version:     dix_get(data, prefix, "version")?,
            debug:       dix_get(data, prefix, "debug")?,
            sample_rate: dix_get(data, prefix, "sample_rate")?,
            server:      dix_nested(data, prefix, "server")?,
            admin_email: dix_nested(data, prefix, "admin_email")?,
        })
    }
}

fn sample_app() -> AppConfig {
    AppConfig {
        name:        "MyGame".into(),
        version:     "2.3.0".into(),
        debug:       true,
        sample_rate: 0.25,
        server: ServerConfig {
            host:    "api.example.com".into(),
            port:    8443,
            node_id: 9_000_000_001,
            tls: TlsConfig {
                enabled:     true,
                min_version: "1.3".into(),
            },
        },
        admin_email: Some("admin@example.com".into()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Multi-level round trips
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn three_level_nested_struct_round_trip() {
    let original = sample_app();
    let data = DixDataBuilder::new().serialize(&original).build().unwrap();

    // All three levels should be present as flat dotted keys.
    assert!(data.exists("name"));
    assert!(data.exists("server.host"));
    assert!(data.exists("server.node_id"));
    assert!(data.exists("server.tls.enabled"));
    assert!(data.exists("server.tls.min_version"));
    assert!(data.exists("admin_email"));

    let recovered: AppConfig = data.deserialize().unwrap();
    assert_eq!(recovered, original);
}

#[test]
fn three_level_nested_struct_optional_field_absent() {
    let mut original = sample_app();
    original.admin_email = None;
    original.debug = false;
    original.server.tls.enabled = false;

    let data = DixDataBuilder::new().serialize(&original).build().unwrap();
    assert!(!data.exists("admin_email"));
    assert!(data.get_keys("admin_email").is_empty());

    let recovered: AppConfig = data.deserialize().unwrap();
    assert_eq!(recovered, original);
    assert_eq!(recovered.admin_email, None);
}

#[test]
fn long_field_round_trips_exactly() {
    let original = sample_app();
    let data = DixDataBuilder::new().serialize(&original).build().unwrap();

    // node_id must come back as i64, not be silently truncated to i32.
    let node_id: i64 = dix_get(&data, "server", "node_id").unwrap();
    assert_eq!(node_id, 9_000_000_001);

    let recovered: AppConfig = data.deserialize().unwrap();
    assert_eq!(recovered.server.node_id, 9_000_000_001);
}

#[test]
fn dix_nested_at_root_with_empty_prefix() {
    let original = sample_app();
    let data = DixDataBuilder::new().serialize(&original).build().unwrap();

    // dix_nested with prefix="" and field="server" is equivalent to
    // deserialize_at("server").
    let server: ServerConfig = dix_nested(&data, "", "server").unwrap();
    assert_eq!(server, original.server);
}

// ─────────────────────────────────────────────────────────────────────────────
// dix_array_of — arrays of structs
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
struct Enemy {
    name: String,
    hp:   i32,
    boss: bool,
}

impl DixDeserialize for Enemy {
    fn from_dix(data: &DixData, prefix: &str) -> Result<Self, String> {
        Ok(Enemy {
            name: dix_get(data, prefix, "name")?,
            hp:   dix_get(data, prefix, "hp")?,
            boss: dix_get_or(data, prefix, "boss", false),
        })
    }
}

fn enemy_value(name: &str, hp: i32, boss: bool) -> Value {
    Value::Object {
        properties: vec![
            ObjectProperty::new("name".into(), Value::String { value: name.into(), position: Position::UNKNOWN }, Position::UNKNOWN),
            ObjectProperty::new("hp".into(),   Value::Integer { value: hp, position: Position::UNKNOWN }, Position::UNKNOWN),
            ObjectProperty::new("boss".into(), Value::Boolean { value: boss, position: Position::UNKNOWN }, Position::UNKNOWN),
        ],
        position: Position::UNKNOWN,
    }
}

#[test]
fn dix_array_of_populated_structs() {
    let data = DixDataBuilder::new()
        .data(|d| {
            d.with_string("title", "Wave 1");
            d.with_group_array("enemies", vec![
                enemy_value("Goblin", 50, false),
                enemy_value("Orc",    100, false),
                enemy_value("Dragon", 1000, true),
            ]);
        })
        .build()
        .unwrap();

    let enemies: Vec<Enemy> = dix_array_of(&data, "", "enemies").unwrap();
    assert_eq!(enemies.len(), 3);
    assert_eq!(enemies[0], Enemy { name: "Goblin".into(), hp: 50,  boss: false });
    assert_eq!(enemies[1], Enemy { name: "Orc".into(),    hp: 100, boss: false });
    assert_eq!(enemies[2], Enemy { name: "Dragon".into(), hp: 1000, boss: true });
}

#[test]
fn dix_array_of_empty_group_array_returns_empty_vec() {
    let data = DixDataBuilder::new()
        .data(|d| {
            d.with_string("title", "Wave 2");
            d.with_group_array("enemies", vec![]);
        })
        .build()
        .unwrap();

    let enemies: Vec<Enemy> = dix_array_of(&data, "", "enemies").unwrap();
    assert!(enemies.is_empty());
}

#[test]
fn dix_array_of_propagates_per_element_errors_with_index() {
    // One element is missing the required "hp" field.
    let bad_item = Value::Object {
        properties: vec![
            ObjectProperty::new("name".into(), Value::String { value: "Slime".into(), position: Position::UNKNOWN }, Position::UNKNOWN),
            // "hp" intentionally omitted
        ],
        position: Position::UNKNOWN,
    };

    let data = DixDataBuilder::new()
        .data(|d| {
            d.with_group_array("enemies", vec![
                enemy_value("Goblin", 50, false),
                bad_item,
            ]);
        })
        .build()
        .unwrap();

    let result: Result<Vec<Enemy>, String> = dix_array_of(&data, "", "enemies");
    assert!(result.is_err());
    let msg = result.unwrap_err();
    // Error message should point at the failing index for debuggability.
    assert!(msg.contains("enemies[1]"), "expected index 1 in error, got: {}", msg);
}

// ─────────────────────────────────────────────────────────────────────────────
// Schema validation — comprehensive type coverage
// ─────────────────────────────────────────────────────────────────────────────

fn schema_fixture() -> DixData {
    DixDataBuilder::new()
        .data(|d| {
            d.with_string("name", "Widget");
            d.with_int("count", 42);
            d.with_long("big_id", 9_000_000_000);
            d.with_float("ratio_f", 1.5);
            d.with_double("ratio_d", 2.5);
            d.with_bool("enabled", true);
            d.with_date("created", chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
            d.with_hex_color("color", "#FF5733");
            d.with_group_array_builder("tags", |arr| {
                arr.add_string("alpha");
                arr.add_string("beta");
            });
            d.with_table_properties("nested", |t| {
                t.with_string("inner", "value");
                t.with_int("depth", 1);
            });
        })
        .build()
        .unwrap()
}

#[test]
fn schema_covers_every_basic_type_when_correct() {
    let data = schema_fixture();
    let report = data.validate_schema(
        SchemaBuilder::new()
            .require_string("name")
            .require_int("count")
            .require_long("big_id")
            .require_float("ratio_f")
            .require_double("ratio_d")
            .require_bool("enabled")
            .require("created", ExpectedValueType::Date)
            .require("color", ExpectedValueType::HexColor)
            .require_array("tags"),
    );
    assert!(report.is_valid(), "{}", report);
}

#[test]
fn schema_table_property_field_path_validates() {
    let data = schema_fixture();
    let report = data.validate_schema(
        SchemaBuilder::new()
            .require_string("nested.inner")
            .require_int("nested.depth"),
    );
    assert!(report.is_valid(), "{}", report);
}

#[test]
fn schema_group_array_element_path_validates() {
    let data = schema_fixture();
    let report = data.validate_schema(
        SchemaBuilder::new()
            .require_string("tags[0]")
            .require_string("tags[1]"),
    );
    assert!(report.is_valid(), "{}", report);
}

#[test]
fn schema_group_array_out_of_range_index_is_missing() {
    let data = schema_fixture();
    let report = data.validate_schema(SchemaBuilder::new().require_string("tags[5]"));
    assert!(!report.is_valid());
    assert_eq!(report.errors[0].kind, ValidationErrorKind::Missing);
}

#[test]
fn schema_multiple_missing_fields_all_collected_in_one_pass() {
    let data = schema_fixture();
    let report = data.validate_schema(
        SchemaBuilder::new()
            .require_string("name")     // present
            .require_string("missing1") // absent
            .require_int("missing2")    // absent
            .require_bool("missing3"),  // absent
    );
    assert_eq!(report.error_count(), 3);
    assert_eq!(report.failed_paths(), vec!["missing1", "missing2", "missing3"]);
}

#[test]
fn schema_int_field_rejects_long_no_silent_truncation() {
    let data = schema_fixture();
    // big_id is a Long; requiring Int must fail rather than silently
    // accepting a value that would truncate on i32 conversion.
    let report = data.validate_schema(SchemaBuilder::new().require_int("big_id"));
    assert!(!report.is_valid());
    assert_eq!(report.errors[0].kind, ValidationErrorKind::WrongType);
}

#[test]
fn schema_long_field_widens_to_accept_int() {
    let data = schema_fixture();
    let report = data.validate_schema(SchemaBuilder::new().require_long("count"));
    assert!(report.is_valid(), "{}", report);
}

#[test]
fn schema_double_field_widens_to_accept_int_float_and_long() {
    let data = schema_fixture();
    let report = data.validate_schema(
        SchemaBuilder::new()
            .require_double("count")    // Int -> Double
            .require_double("ratio_f")  // Float -> Double
            .require_double("big_id"),  // Long -> Double
    );
    assert!(report.is_valid(), "{}", report);
}

#[test]
fn schema_custom_validator_with_dix_get_on_nested_path() {
    let data = schema_fixture();
    let report = data.validate_schema(
        SchemaBuilder::new().require_with("nested.depth", ExpectedValueType::Int, |data| {
            let depth: i32 = data.get("nested.depth")?;
            if depth > 0 { Ok(()) } else { Err(format!("depth {} must be positive", depth)) }
        }),
    );
    assert!(report.is_valid(), "{}", report);
}

#[test]
fn schema_enum_field_validates_via_get_keys_and_long_widening() {
    // Build a DixData with an EnumValue directly via the AST, since
    // DataBuilder has no dedicated enum setter.
    use dixscript::Compiler::AST::{
        DixScript, DataSection, DataEntry, EnumsSection, EnumDeclaration, EnumField,
    };
    use chrono::Utc;

    let ast = DixScript {
        config: None, imports: None, dlm: None, quick_functions: None, security: None,
        enums: Some(EnumsSection {
            enums: vec![EnumDeclaration {
                name: "Status".into(),
                fields: vec![
                    EnumField { name: "ACTIVE".into(),   value: Some(0), position: Position::UNKNOWN },
                    EnumField { name: "INACTIVE".into(), value: Some(1), position: Position::UNKNOWN },
                ],
                position: Position::UNKNOWN,
            }],
            position: Position::UNKNOWN,
        }),
        data: Some(DataSection {
            entries: vec![DataEntry::SimpleProperty {
                name: "status".into(),
                data_type: None,
                value: Value::EnumValue {
                    enum_name: "Status".into(),
                    value:     "INACTIVE".into(),
                    position:  Position::UNKNOWN,
                },
                position: Position::UNKNOWN,
            }],
            position: Position::UNKNOWN,
        }),
    };

    let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

    let report = data.validate_schema(SchemaBuilder::new().require_enum("status"));
    assert!(report.is_valid(), "{}", report);

    // Enum resolves to its declared integer value, and Int schema check
    // accepts Enum too.
    let status: i32 = data.get("status").unwrap();
    assert_eq!(status, 1);

    let int_report = data.validate_schema(SchemaBuilder::new().require_int("status"));
    assert!(int_report.is_valid(), "{}", int_report);
}

// ─────────────────────────────────────────────────────────────────────────────
// Builder edge cases / two-tier ordering
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn builder_two_tier_violation_via_serialize_at_after_group_array() {
    // KNOWN BEHAVIOR: DataBuilder's two-tier check operates purely on
    // *when* a flat-property method is called, regardless of whether the
    // property's name contains dots. `dix_set_nested` -> `to_dix` for a
    // struct ultimately calls `with_string`/`with_int`/etc with a dotted
    // name like "server.host" — which IS a flat-property call as far as
    // the builder is concerned. So `serialize_at("server", ...)` AFTER a
    // group array has already been added will be rejected, even though
    // "server.host" *looks* like it should be fine as a nested path.
    //
    // This test documents that behavior so it doesn't get "fixed" by
    // accident in a way that silently changes ordering semantics —
    // if you hit this in practice, call `serialize_at` for nested structs
    // BEFORE any `with_group_array*` / `with_table_properties` calls.
    let result = DixDataBuilder::new()
        .data(|d| {
            d.with_group_array("tags", vec![]);
        })
        .serialize_at("server", &ServerConfig {
            host: "x".into(), port: 1, node_id: 1,
            tls: TlsConfig { enabled: false, min_version: "1.0".into() },
        })
        .build();

    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("two-tier"), "expected two-tier error, got: {}", msg);
}

#[test]
fn builder_serialize_at_before_group_array_is_fine() {
    let result = DixDataBuilder::new()
        .serialize_at("server", &ServerConfig {
            host: "x".into(), port: 1, node_id: 1,
            tls: TlsConfig { enabled: false, min_version: "1.0".into() },
        })
        .data(|d| {
            d.with_group_array("tags", vec![]);
        })
        .build();

    assert!(result.is_ok());
    let data = result.unwrap();
    assert_eq!(dix_get::<String>(&data, "server", "host").unwrap(), "x");
}

#[test]
fn builder_serialize_multiple_independent_structs_at_different_prefixes() {
    let server = ServerConfig {
        host: "a".into(), port: 1, node_id: 100,
        tls: TlsConfig { enabled: true, min_version: "1.3".into() },
    };
    let backup = ServerConfig {
        host: "b".into(), port: 2, node_id: 200,
        tls: TlsConfig { enabled: false, min_version: "1.2".into() },
    };

    let data = DixDataBuilder::new()
        .serialize_at("primary", &server)
        .serialize_at("backup", &backup)
        .build()
        .unwrap();

    let p: ServerConfig = data.deserialize_at("primary").unwrap();
    let b: ServerConfig = data.deserialize_at("backup").unwrap();
    assert_eq!(p, server);
    assert_eq!(b, backup);
}

// ─────────────────────────────────────────────────────────────────────────────
// Converter round trips (JSON / TOML) preserve types from the builder pipeline
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn json_round_trip_preserves_long_and_struct_shape() {
    use dixscript::Runtime::DixConverter;

    let original = sample_app();
    let data = DixDataBuilder::new().serialize(&original).build().unwrap();
    let map  = data.to_hashmap();

    let converter = DixConverter::new();
    let ast  = converter.from_hashmap(map).unwrap();
    let json = converter.to_json(&ast, false).unwrap();

    assert!(json.contains("9000000001"), "node_id should round-trip through JSON: {}", json);

    let ast2 = converter.from_json(&json).unwrap();
    let map2 = converter.to_hashmap(&ast2);
    assert_eq!(
        map2.get("server.node_id"),
        Some(&dixscript::Runtime::DixValue::Long(9_000_000_001))
    );
    }
