// src/Runtime/data_builder.rs

use std::collections::HashMap;
use chrono::Utc;
use crate::Compiler::AST::*;
use super::dix_data::DixData;
use super::converter::DixConverter;
use super::format_options::DixFormatOptions;

/// Fluent builder for creating DixData programmatically
///
/// Enforces DixScript structure rules:
/// - Flat properties must come before grouped data (two-tier structure)
/// - Table properties and group arrays can be mixed after flat properties
///
/// # Examples
///
/// ```rust,no_run
/// use dixscript::Runtime::*;
///
/// let data = DixDataBuilder::new()
///     .config(|c| {
///         c.with_version("1.0.0");
///         c.with_author("Alice");
///     })
///     .data(|d| {
///         d.with_int("x", 42);
///         d.with_string("name", "test");
///         d.with_table_properties("user", |t| {
///             t.with_string("name", "Bob");
///             t.with_int("age", 30);
///         });
///     })
///     .build()
///     .unwrap();
/// ```
pub struct DixDataBuilder {
    config_builder: ConfigBuilder,
    enums_builder: EnumsBuilder,
    data_builder: DataBuilder,
    version: String,
    compile_time: chrono::DateTime<Utc>,
}

impl DixDataBuilder {
    /// Create new builder with default settings
    pub fn new() -> Self {
        DixDataBuilder {
            config_builder: ConfigBuilder::new(),
            enums_builder: EnumsBuilder::new(),
            data_builder: DataBuilder::new(),
            version: "1.0.0".to_string(),
            compile_time: Utc::now(),
        }
    }

    /// Configure CONFIG section (closure style - like C#)
    pub fn config<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut ConfigBuilder),
    {
        configure(&mut self.config_builder);
        self
    }

    /// Configure ENUMS section
    pub fn enums<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut EnumsBuilder),
    {
        configure(&mut self.enums_builder);
        self
    }

    /// Configure DATA section
    pub fn data<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(&mut DataBuilder),
    {
        configure(&mut self.data_builder);
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set compile time
    pub fn with_compile_time(mut self, time: chrono::DateTime<Utc>) -> Self {
        self.compile_time = time;
        self
    }

    /// Build DixData in memory
    pub fn build(self) -> Result<DixData, String> {
        let config_section = self.config_builder.build();
        let enums_section = self.enums_builder.build();
        let data_section = self.data_builder.build()?;

        let ast = DixScript {
            config: config_section,
            imports: None,
            dlm: None,
            enums: enums_section,
            quick_functions: None,
            data: data_section,
            security: None,
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

    /// Build and save to .dixscript file
    pub fn build_and_save(
        self,
        output_path: impl AsRef<std::path::Path>,
        options: Option<&DixFormatOptions>,
    ) -> Result<String, String> {
        let output_path = output_path.as_ref();

        // Ensure .dixscript extension
        let output_path = if output_path.extension().and_then(|s| s.to_str()) != Some("dixscript") {
            output_path.with_extension("dixscript")
        } else {
            output_path.to_path_buf()
        };

        // Create directory if needed
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        // Build DixData
        let dix_data = self.build()?;

        // Convert to MDIX format
        let converter = DixConverter::new();

        // Reconstruct AST from DixData (simplified - just for export)
        let ast = DixScript {
            config: dix_data.config.as_ref().map(|cfg| {
                let entries = cfg
                    .iter()
                    .map(|(k, v)| ConfigEntry {
                        key: k.clone(),
                        value: ConfigValue::String(v.clone()),
                        position: Position::UNKNOWN,
                    })
                    .collect();
                ConfigSection {
                    entries,
                    position: Position::UNKNOWN,
                }
            }),
            data: Some(DataSection {
                entries: vec![], // Would need full reconstruction - simplified for now
                position: Position::UNKNOWN,
            }),
            imports: None,
            dlm: None,
            enums: None,
            quick_functions: None,
            security: None,
        };

        let mdix_content = converter.to_mdix(&ast, options)?;

        // Write to file
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

// ===== SUB-BUILDERS =====

/// CONFIG section builder
pub struct ConfigBuilder {
    entries: Vec<(String, String)>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        ConfigBuilder {
            entries: Vec::new(),
        }
    }

    /// Add config entry (mutable - for closure style)
    pub fn add_entry(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.push((key.into(), value.into()));
    }

    /// Set version
    pub fn with_version(&mut self, version: impl Into<String>) {
        self.add_entry("version", version);
    }

    /// Set encoding
    pub fn with_encoding(&mut self, encoding: impl Into<String>) {
        self.add_entry("encoding", encoding);
    }

    /// Set author
    pub fn with_author(&mut self, author: impl Into<String>) {
        self.add_entry("author", author);
    }

    /// Set created timestamp
    pub fn with_created(&mut self, created: chrono::DateTime<Utc>) {
        self.add_entry("created", created.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    }

    /// Set features
    pub fn with_features(&mut self, features: impl Into<String>) {
        self.add_entry("features", features);
    }

    /// Add custom entry
    pub fn with_custom(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.add_entry(key, value);
    }

    fn build(self) -> Option<ConfigSection> {
        if self.entries.is_empty() {
            return None;
        }

        let config_entries = self
            .entries
            .into_iter()
            .map(|(key, value)| ConfigEntry {
                key,
                value: ConfigValue::String(value),
                position: Position::UNKNOWN,
            })
            .collect();

        Some(ConfigSection {
            entries: config_entries,
            position: Position::UNKNOWN,
        })
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// ENUMS section builder
pub struct EnumsBuilder {
    enums: Vec<(String, Vec<(String, Option<i32>)>)>,
}

impl EnumsBuilder {
    pub fn new() -> Self {
        EnumsBuilder { enums: Vec::new() }
    }

    /// Add enum with auto-values (0, 1, 2, ...)
    pub fn with_enum(&mut self, enum_name: impl Into<String>, field_names: &[&str]) {
        let fields = field_names
            .iter()
            .map(|name| (name.to_string(), None))
            .collect();
        self.enums.push((enum_name.into(), fields));
    }

    /// Add enum with explicit values
    pub fn with_enum_values(
        &mut self,
        enum_name: impl Into<String>,
        fields: &[(impl AsRef<str>, i32)],
    ) {
        let fields_vec = fields
            .iter()
            .map(|(name, value)| (name.as_ref().to_string(), Some(*value)))
            .collect();
        self.enums.push((enum_name.into(), fields_vec));
    }

    fn build(self) -> Option<EnumsSection> {
        if self.enums.is_empty() {
            return None;
        }

        let enum_declarations = self
            .enums
            .into_iter()
            .map(|(name, fields)| {
                let enum_fields = fields
                    .into_iter()
                    .map(|(field_name, value)| EnumField {
                        name: field_name,
                        value,
                        position: Position::UNKNOWN,
                    })
                    .collect();
                EnumDeclaration {
                    name,
                    fields: enum_fields,
                    position: Position::UNKNOWN,
                }
            })
            .collect();

        Some(EnumsSection {
            enums: enum_declarations,
            position: Position::UNKNOWN,
        })
    }
}

impl Default for EnumsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// DATA section builder
///
/// Enforces two-tier structure:
/// - Flat properties must come first
/// - Table properties and group arrays come after
pub struct DataBuilder {
    flat_properties: Vec<(String, Value)>,
    table_properties: Vec<(String, Vec<(String, Value)>)>,
    group_arrays: Vec<(String, Vec<Value>)>,
    has_seen_grouped_data: bool,
}

impl DataBuilder {
    pub fn new() -> Self {
        DataBuilder {
            flat_properties: Vec::new(),
            table_properties: Vec::new(),
            group_arrays: Vec::new(),
            has_seen_grouped_data: false,
        }
    }

    // ===== FLAT PROPERTIES (must come first) =====

    /// Add integer property
    pub fn with_int(&mut self, name: impl Into<String>, value: i32) {
        self.validate_flat_property_allowed();
        self.flat_properties.push((
            name.into(),
            Value::Integer { value, position: Position::UNKNOWN }
        ));
    }

    /// Add float property
    pub fn with_float(&mut self, name: impl Into<String>, value: f32) {
        self.validate_flat_property_allowed();
        self.flat_properties.push((
            name.into(),
            Value::Float { value, position: Position::UNKNOWN }
        ));
    }

    /// Add double property
    pub fn with_double(&mut self, name: impl Into<String>, value: f64) {
        self.validate_flat_property_allowed();
        self.flat_properties.push((
            name.into(),
            Value::Double { value, position: Position::UNKNOWN }
        ));
    }

    /// Add string property
    pub fn with_string(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.validate_flat_property_allowed();
        self.flat_properties.push((
            name.into(),
            Value::String { value: value.into(), position: Position::UNKNOWN }
        ));
    }

    /// Add boolean property
    pub fn with_bool(&mut self, name: impl Into<String>, value: bool) {
        self.validate_flat_property_allowed();
        self.flat_properties.push((
            name.into(),
            Value::Boolean { value, position: Position::UNKNOWN }
        ));
    }

    /// Add date property
    pub fn with_date(&mut self, name: impl Into<String>, value: chrono::NaiveDate) {
        self.validate_flat_property_allowed();
        self.flat_properties.push((
            name.into(),
            Value::Date {
                value: value.format("%Y-%m-%d").to_string(),
                position: Position::UNKNOWN
            },
        ));
    }

    /// Add hex color property
    pub fn with_hex_color(&mut self, name: impl Into<String>, hex_value: impl Into<String>) {
        self.validate_flat_property_allowed();
        let hex = hex_value.into();
        if !hex.starts_with('#') {
            panic!("Hex color must start with #");
        }
        self.flat_properties.push((
            name.into(),
            Value::HexColor { value: hex, position: Position::UNKNOWN }
        ));
    }

    /// Add array property
    pub fn with_array(&mut self, name: impl Into<String>, items: Vec<Value>) {
        self.validate_flat_property_allowed();
        self.flat_properties.push((
            name.into(),
            Value::Array { values: items, position: Position::UNKNOWN }
        ));
    }

    // ===== GROUPED DATA (comes after flat properties) =====

    /// Add table properties (e.g., user: name = "Bob", age = 30)
    pub fn with_table_properties<F>(
        &mut self,
        path: impl Into<String>,
        configure: F,
    )
    where
        F: FnOnce(&mut TablePropertiesBuilder),
    {
        self.has_seen_grouped_data = true;

        let mut builder = TablePropertiesBuilder::new();
        configure(&mut builder);

        self.table_properties.push((path.into(), builder.build()));
    }

    /// Add group array (e.g., items:: 1, 2, 3)
    pub fn with_group_array(&mut self, path: impl Into<String>, items: Vec<Value>) {
        self.has_seen_grouped_data = true;
        self.group_arrays.push((path.into(), items));
    }

    /// Add group array with builder
    pub fn with_group_array_builder<F>(
        &mut self,
        path: impl Into<String>,
        configure: F,
    )
    where
        F: FnOnce(&mut GroupArrayBuilder),
    {
        self.has_seen_grouped_data = true;

        let mut builder = GroupArrayBuilder::new();
        configure(&mut builder);

        self.group_arrays.push((path.into(), builder.build()));
    }

    /// Validate that flat properties can still be added
    fn validate_flat_property_allowed(&self) {
        if self.has_seen_grouped_data {
            panic!(
                "Cannot add flat properties after table properties or group arrays. \
                 Flat properties must come first (two-tier structure)."
            );
        }
    }

    fn build(self) -> Result<Option<DataSection>, String> {
        let mut entries = Vec::new();

        // Add flat properties
        for (name, value) in self.flat_properties {
            entries.push(DataEntry::SimpleProperty {
                name,
                data_type: None,
                value,
                position: Position::UNKNOWN,
            });
        }

        // Add table properties
        for (path, properties) in self.table_properties {
            let table_path = TablePath {
                segments: path.split('.').map(String::from).collect(),
            };

            let property_assignments = properties
                .into_iter()
                .map(|(name, value)| PropertyAssignment {
                    name,
                    data_type: None,
                    value,
                    position: Position::UNKNOWN,
                })
                .collect();

            entries.push(DataEntry::TableProperty {
                path: table_path,
                properties: property_assignments,
                position: Position::UNKNOWN,
            });
        }

        // Add group arrays
        for (path, items) in self.group_arrays {
            let array_path = TablePath {
                segments: path.split('.').map(String::from).collect(),
            };

            entries.push(DataEntry::GroupArray {
                path: array_path,
                items,
                position: Position::UNKNOWN,
            });
        }

        if entries.is_empty() {
            return Ok(None);
        }

        Ok(Some(DataSection {
            entries,
            position: Position::UNKNOWN,
        }))
    }
}

impl Default for DataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Table properties builder (for table: prop1 = val1, prop2 = val2)
pub struct TablePropertiesBuilder {
    properties: Vec<(String, Value)>,
}

impl TablePropertiesBuilder {
    pub fn new() -> Self {
        TablePropertiesBuilder {
            properties: Vec::new(),
        }
    }

    pub fn with_int(&mut self, name: impl Into<String>, value: i32) {
        self.properties.push((
            name.into(),
            Value::Integer { value, position: Position::UNKNOWN }
        ));
    }

    pub fn with_float(&mut self, name: impl Into<String>, value: f32) {
        self.properties.push((
            name.into(),
            Value::Float { value, position: Position::UNKNOWN }
        ));
    }

    pub fn with_double(&mut self, name: impl Into<String>, value: f64) {
        self.properties.push((
            name.into(),
            Value::Double { value, position: Position::UNKNOWN }
        ));
    }

    pub fn with_string(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.properties.push((
            name.into(),
            Value::String { value: value.into(), position: Position::UNKNOWN }
        ));
    }

    pub fn with_bool(&mut self, name: impl Into<String>, value: bool) {
        self.properties.push((
            name.into(),
            Value::Boolean { value, position: Position::UNKNOWN }
        ));
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

/// Group array builder (for array:: item1, item2, item3)
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
    #[should_panic(expected = "Cannot add flat properties after table properties")]
    fn test_two_tier_enforcement() {
        let _ = DixDataBuilder::new()
            .data(|d| {
                d.with_table_properties("user", |t| {
                    t.with_string("name", "Bob");
                });
                // This should panic - flat property after grouped data
                d.with_int("x", 42);
            })
            .build();
    }
}