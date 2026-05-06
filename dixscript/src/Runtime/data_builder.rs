
use chrono::Utc;
use crate::Compiler::AST::*;
use super::dix_data::DixData;
use super::converter::DixConverter;
use super::format_options::DixFormatOptions;

/// Fluent builder for creating DixData programmatically.
///
/// Enforces DixScript's two-tier structure: flat properties must be added
/// before table properties or group arrays. Violations return `Err` from
/// `build()` — they do NOT panic. This is intentional: panics across FFI
/// boundaries are undefined behavior. Callers that discard `Result` are
/// responsible for their own mistakes.
pub struct DixDataBuilder {
    config_builder: ConfigBuilder,
    enums_builder:  EnumsBuilder,
   pub(crate) data_builder:   DataBuilder,
    version:        String,
    compile_time:   chrono::DateTime<Utc>,
}

impl DixDataBuilder {
    pub fn new() -> Self {
        DixDataBuilder {
            config_builder: ConfigBuilder::new(),
            enums_builder:  EnumsBuilder::new(),
            data_builder:   DataBuilder::new(),
            version:        "1.0.0".to_string(),
            compile_time:   Utc::now(),
        }
    }

    pub fn config<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut ConfigBuilder),
    {
        configure(&mut self.config_builder);
        self
    }

    pub fn enums<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut EnumsBuilder),
    {
        configure(&mut self.enums_builder);
        self
    }

    /// Configure the DATA section.
    ///
    /// Flat properties must be added before any table properties or group
    /// arrays. Violations are recorded and surfaced as `Err` from `build()`.
    pub fn data<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut DataBuilder),
    {
        configure(&mut self.data_builder);
        self
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_compile_time(mut self, time: chrono::DateTime<Utc>) -> Self {
        self.compile_time = time;
        self
    }

    /// Build DixData in memory.
    ///
    /// Returns `Err` if any two-tier ordering violations were recorded inside
    /// the `data()` closure, or if other validation failed (e.g. bad hex color).
    /// All violations are collected so the caller sees them all at once.
    pub fn build(self) -> Result<DixData, String> {
        let config_section = self.config_builder.build();
        let enums_section  = self.enums_builder.build();
        let data_section   = self.data_builder.build()?;

        let ast = DixScript {
            config:          config_section,
            imports:         None,
            dlm:             None,
            enums:           enums_section,
            quick_functions: None,
            data:            data_section,
            security:        None,
        };

        Ok(DixData::from_ast(
            ast,
            self.version,
            self.compile_time,
            false,
            false,
            vec![],
        ))
    }

    /// Build and write to a `.dixscript` file.
    pub fn build_and_save(
        self,
        output_path: impl AsRef<std::path::Path>,
        options: Option<&DixFormatOptions>,
    ) -> Result<String, String> {
        let output_path = output_path.as_ref();
        let output_path = if output_path.extension().and_then(|s| s.to_str()) != Some("dixscript") {
            output_path.with_extension("dixscript")
        } else {
            output_path.to_path_buf()
        };

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        let dix_data  = self.build()?;
        let converter = DixConverter::new();

        let ast = DixScript {
            config: dix_data.config.as_ref().map(|cfg| {
                let entries = cfg.iter().map(|(k, v)| ConfigEntry {
                    key:      k.clone(),
                    value:    ConfigValue::String(v.clone()),
                    position: Position::UNKNOWN,
                }).collect();
                ConfigSection { entries, position: Position::UNKNOWN }
            }),
            data: Some(DataSection {
                entries:  vec![],
                position: Position::UNKNOWN,
            }),
            imports:         None,
            dlm:             None,
            enums:           None,
            quick_functions: None,
            security:        None,
        };

        let mdix_content = converter.to_mdix(&ast, options)?;

        std::fs::write(&output_path, mdix_content)
            .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(output_path.to_string_lossy().to_string())
    }
}

impl Default for DixDataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── ConfigBuilder ─────────────────────────────────────────────────────────────

pub struct ConfigBuilder {
    entries: Vec<(String, String)>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        ConfigBuilder { entries: Vec::new() }
    }

    pub fn add_entry(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.push((key.into(), value.into()));
    }

    pub fn with_version(&mut self, version: impl Into<String>) {
        self.add_entry("version", version);
    }

    pub fn with_encoding(&mut self, encoding: impl Into<String>) {
        self.add_entry("encoding", encoding);
    }

    pub fn with_author(&mut self, author: impl Into<String>) {
        self.add_entry("author", author);
    }

    pub fn with_created(&mut self, created: chrono::DateTime<Utc>) {
        self.add_entry("created", created.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    }

    pub fn with_features(&mut self, features: impl Into<String>) {
        self.add_entry("features", features);
    }

    pub fn with_custom(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.add_entry(key, value);
    }

    fn build(self) -> Option<ConfigSection> {
        if self.entries.is_empty() {
            return None;
        }
        let config_entries = self.entries.into_iter().map(|(key, value)| ConfigEntry {
            key,
            value:    ConfigValue::String(value),
            position: Position::UNKNOWN,
        }).collect();
        Some(ConfigSection { entries: config_entries, position: Position::UNKNOWN })
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── EnumsBuilder ──────────────────────────────────────────────────────────────

pub struct EnumsBuilder {
    enums: Vec<(String, Vec<(String, Option<i32>)>)>,
}

impl EnumsBuilder {
    pub fn new() -> Self {
        EnumsBuilder { enums: Vec::new() }
    }

    pub fn with_enum(&mut self, enum_name: impl Into<String>, field_names: &[&str]) {
        let fields = field_names.iter().map(|name| (name.to_string(), None)).collect();
        self.enums.push((enum_name.into(), fields));
    }

    pub fn with_enum_values(
        &mut self,
        enum_name: impl Into<String>,
        fields: &[(impl AsRef<str>, i32)],
    ) {
        let fields_vec = fields.iter()
            .map(|(name, value)| (name.as_ref().to_string(), Some(*value)))
            .collect();
        self.enums.push((enum_name.into(), fields_vec));
    }

    fn build(self) -> Option<EnumsSection> {
        if self.enums.is_empty() {
            return None;
        }
        let enum_declarations = self.enums.into_iter().map(|(name, fields)| {
            let enum_fields = fields.into_iter().map(|(field_name, value)| EnumField {
                name: field_name,
                value,
                position: Position::UNKNOWN,
            }).collect();
            EnumDeclaration { name, fields: enum_fields, position: Position::UNKNOWN }
        }).collect();
        Some(EnumsSection { enums: enum_declarations, position: Position::UNKNOWN })
    }
}

impl Default for EnumsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── DataBuilder ───────────────────────────────────────────────────────────────

/// Builds the DATA section.
///
/// Flat properties must be added before any table properties or group arrays.
/// Violations are **collected** rather than panicking so that the caller sees
/// every problem at once when `build()` is called. This also keeps the type
/// safe to use from FFI wrappers, where a panic would cross a C boundary and
/// cause undefined behavior.
pub struct DataBuilder {
    flat_properties:       Vec<(String, Value)>,
    table_properties:      Vec<(String, Vec<(String, Value)>)>,
    group_arrays:          Vec<(String, Vec<Value>)>,
    has_seen_grouped_data: bool,
    deferred_errors:       Vec<String>,
}

impl DataBuilder {
    pub fn new() -> Self {
        DataBuilder {
            flat_properties:       Vec::new(),
            table_properties:      Vec::new(),
            group_arrays:          Vec::new(),
            has_seen_grouped_data: false,
            deferred_errors:       Vec::new(),
        }
    }
/// Called by DixDataBuilder::serialize / serialize_at to propagate errors
/// from DixSerialize implementations without panicking.
pub fn push_deferred_error(&mut self, error: String) {
    self.deferred_errors.push(error);
}
    // ── Flat properties ───────────────────────────────────────────────────────

    pub fn with_int(&mut self, name: impl Into<String>, value: i32) {
        let name = name.into();
        if self.check_flat_allowed(&name) {
            self.flat_properties.push((
                name,
                Value::Integer { value, position: Position::UNKNOWN },
            ));
        }
    }

    pub fn with_float(&mut self, name: impl Into<String>, value: f32) {
        let name = name.into();
        if self.check_flat_allowed(&name) {
            self.flat_properties.push((
                name,
                Value::Float { value, position: Position::UNKNOWN },
            ));
        }
    }

    pub fn with_double(&mut self, name: impl Into<String>, value: f64) {
        let name = name.into();
        if self.check_flat_allowed(&name) {
            self.flat_properties.push((
                name,
                Value::Double { value, position: Position::UNKNOWN },
            ));
        }
    }

    pub fn with_string(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let name = name.into();
        if self.check_flat_allowed(&name) {
            self.flat_properties.push((
                name,
                Value::String { value: value.into(), position: Position::UNKNOWN },
            ));
        }
    }

    pub fn with_bool(&mut self, name: impl Into<String>, value: bool) {
        let name = name.into();
        if self.check_flat_allowed(&name) {
            self.flat_properties.push((
                name,
                Value::Boolean { value, position: Position::UNKNOWN },
            ));
        }
    }

    pub fn with_date(&mut self, name: impl Into<String>, value: chrono::NaiveDate) {
        let name = name.into();
        if self.check_flat_allowed(&name) {
            self.flat_properties.push((
                name,
                Value::Date {
                    value:    value.format("%Y-%m-%d").to_string(),
                    position: Position::UNKNOWN,
                },
            ));
        }
    }

    pub fn with_hex_color(&mut self, name: impl Into<String>, hex_value: impl Into<String>) {
        let hex = hex_value.into();
        if !hex.starts_with('#') {
            self.deferred_errors.push(format!(
                "Hex color value must start with '#', got: {}", hex
            ));
            return;
        }
        let name = name.into();
        if self.check_flat_allowed(&name) {
            self.flat_properties.push((
                name,
                Value::HexColor { value: hex, position: Position::UNKNOWN },
            ));
        }
    }

    pub fn with_array(&mut self, name: impl Into<String>, items: Vec<Value>) {
        let name = name.into();
        if self.check_flat_allowed(&name) {
            self.flat_properties.push((
                name,
                Value::Array { values: items, position: Position::UNKNOWN },
            ));
        }
    }

    // ── Grouped data ──────────────────────────────────────────────────────────

    pub fn with_table_properties<F>(&mut self, path: impl Into<String>, configure: F)
    where
        F: FnOnce(&mut TablePropertiesBuilder),
    {
        self.has_seen_grouped_data = true;
        let mut builder = TablePropertiesBuilder::new();
        configure(&mut builder);
        self.table_properties.push((path.into(), builder.build()));
    }

    pub fn with_group_array(&mut self, path: impl Into<String>, items: Vec<Value>) {
        self.has_seen_grouped_data = true;
        self.group_arrays.push((path.into(), items));
    }

    pub fn with_group_array_builder<F>(&mut self, path: impl Into<String>, configure: F)
    where
        F: FnOnce(&mut GroupArrayBuilder),
    {
        self.has_seen_grouped_data = true;
        let mut builder = GroupArrayBuilder::new();
        configure(&mut builder);
        self.group_arrays.push((path.into(), builder.build()));
    }

    // ── Validation ────────────────────────────────────────────────────────────

    /// Returns `true` if a flat property may be added, `false` if a two-tier
    /// violation was detected. The error is deferred to `build()` so all
    /// violations are reported together rather than stopping at the first one.
    fn check_flat_allowed(&mut self, name: &str) -> bool {
        if self.has_seen_grouped_data {
            self.deferred_errors.push(format!(
                "Cannot add flat property '{}' after table properties or group arrays. \
                 Flat properties must come first (two-tier structure).",
                name
            ));
            false
        } else {
            true
        }
    }

    fn build(self) -> Result<Option<DataSection>, String> {
        if !self.deferred_errors.is_empty() {
            return Err(self.deferred_errors.join("\n"));
        }

        let mut entries = Vec::new();

        for (name, value) in self.flat_properties {
            entries.push(DataEntry::SimpleProperty {
                name,
                data_type: None,
                value,
                position: Position::UNKNOWN,
            });
        }

        for (path, properties) in self.table_properties {
            let table_path = TablePath {
                segments: path.split('.').map(String::from).collect(),
            };
            let property_assignments = properties.into_iter().map(|(name, value)| {
                PropertyAssignment { name, data_type: None, value, position: Position::UNKNOWN }
            }).collect();
            entries.push(DataEntry::TableProperty {
                path:       table_path,
                properties: property_assignments,
                position:   Position::UNKNOWN,
            });
        }

        for (path, items) in self.group_arrays {
            let array_path = TablePath {
                segments: path.split('.').map(String::from).collect(),
            };
            entries.push(DataEntry::GroupArray {
                path:     array_path,
                items,
                position: Position::UNKNOWN,
            });
        }

        if entries.is_empty() {
            return Ok(None);
        }

        Ok(Some(DataSection { entries, position: Position::UNKNOWN }))
    }
}

impl Default for DataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── TablePropertiesBuilder ────────────────────────────────────────────────────

pub struct TablePropertiesBuilder {
    properties: Vec<(String, Value)>,
}

impl TablePropertiesBuilder {
    pub fn new() -> Self {
        TablePropertiesBuilder { properties: Vec::new() }
    }

    pub fn with_int(&mut self, name: impl Into<String>, value: i32) {
        self.properties.push((name.into(), Value::Integer { value, position: Position::UNKNOWN }));
    }

    pub fn with_float(&mut self, name: impl Into<String>, value: f32) {
        self.properties.push((name.into(), Value::Float { value, position: Position::UNKNOWN }));
    }

    pub fn with_double(&mut self, name: impl Into<String>, value: f64) {
        self.properties.push((name.into(), Value::Double { value, position: Position::UNKNOWN }));
    }

    pub fn with_string(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.properties.push((
            name.into(),
            Value::String { value: value.into(), position: Position::UNKNOWN },
        ));
    }

    pub fn with_bool(&mut self, name: impl Into<String>, value: bool) {
        self.properties.push((name.into(), Value::Boolean { value, position: Position::UNKNOWN }));
    }

    fn build(self) -> Vec<(String, Value)> {
        self.properties
    }
}

impl Default for TablePropertiesBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── GroupArrayBuilder ─────────────────────────────────────────────────────────

pub struct GroupArrayBuilder {
    items: Vec<Value>,
}

impl GroupArrayBuilder {
    pub fn new() -> Self {
        GroupArrayBuilder { items: Vec::new() }
    }

    pub fn add_int(&mut self, value: i32) {
        self.items.push(Value::Integer { value, position: Position::UNKNOWN });
    }

    pub fn add_string(&mut self, value: impl Into<String>) {
        self.items.push(Value::String { value: value.into(), position: Position::UNKNOWN });
    }

    pub fn add_value(&mut self, value: Value) {
        self.items.push(value);
    }

    fn build(self) -> Vec<Value> {
        self.items
    }
}

impl Default for GroupArrayBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_builder() {
        let data = DixDataBuilder::new()
            .config(|c| {
                c.with_version("1.0.0");
                c.with_author("Test");
            })
            .data(|d| {
                d.with_int("x", 42);
                d.with_string("name", "test");
            })
            .build()
            .unwrap();

        assert_eq!(data.version, "1.0.0");
        let x: i32 = data.get("x").unwrap();
        assert_eq!(x, 42);
    }

    #[test]
    fn test_table_properties() {
        let data = DixDataBuilder::new()
            .data(|d| {
                d.with_int("x", 1);
                d.with_table_properties("user", |t| {
                    t.with_string("name", "Bob");
                    t.with_int("age", 30);
                });
            })
            .build()
            .unwrap();

        let name: String = data.get("user.name").unwrap();
        assert_eq!(name, "Bob");
    }

    #[test]
    fn test_two_tier_violation_returns_err_not_panic() {
        let result = DixDataBuilder::new()
            .data(|d| {
                d.with_table_properties("user", |t| {
                    t.with_string("name", "Bob");
                });
                d.with_int("x", 42);
            })
            .build();

        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("two-tier"), "expected two-tier error, got: {}", msg);
    }

    #[test]
    fn test_multiple_violations_all_reported() {
        let result = DixDataBuilder::new()
            .data(|d| {
                d.with_group_array("tags", vec![]);
                d.with_int("x", 1);
                d.with_string("y", "hello");
            })
            .build();

        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains('x'), "expected 'x' in error, got: {}", msg);
        assert!(msg.contains('y'), "expected 'y' in error, got: {}", msg);
    }

    #[test]
    fn test_hex_color_without_hash_returns_err() {
        let result = DixDataBuilder::new()
            .data(|d| {
                d.with_hex_color("color", "FF5733");
            })
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains('#'));
    }

    #[test]
    fn test_empty_data_returns_none_section() {
        let data = DixDataBuilder::new().build().unwrap();
        assert_eq!(data.entry_count(), 0);
    }

    #[test]
    fn test_group_array_builder() {
        let data = DixDataBuilder::new()
            .data(|d| {
                d.with_string("version", "1.0.0");
                d.with_group_array_builder("tags", |arr| {
                    arr.add_string("alpha");
                    arr.add_string("beta");
                });
            })
            .build()
            .unwrap();

        assert!(data.exists("tags"));
        assert!(data.exists("tags[0]"));
        let first: String = data.get("tags[0]").unwrap();
        assert_eq!(first, "alpha");
    }
}
