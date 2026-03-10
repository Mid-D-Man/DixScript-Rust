// src/Runtime/converter.rs

use std::collections::HashMap;
use crate::Compiler::AST::{
    DixScript, ConfigSection, ConfigEntry, ConfigValue,
    DataSection, DataEntry, Value, PropertyAssignment,
    TablePath, ObjectProperty, EnumDeclaration, EnumField,
    EnumsSection, Position,
};
use super::dix_value::DixValue;
use super::format_options::DixFormatOptions;

/// Core format conversion utilities.
///
/// Converts between `HashMap<String, DixValue>` and a DixScript AST,
/// and serializes an AST to `.mdix` text format.
///
/// JSON/TOML/YAML/XML parsing is handled by language wrappers — this
/// provides only the core HashMap ↔ AST conversion.
pub struct DixConverter {
    default_options: DixFormatOptions,
}

impl DixConverter {
    pub fn new() -> Self {
        DixConverter { default_options: DixFormatOptions::new() }
    }

    pub fn with_options(options: DixFormatOptions) -> Self {
        DixConverter { default_options: options }
    }

    /// Convert a flat HashMap to a DixScript AST.
    ///
    /// Produces a minimal AST with a CONFIG section (version + timestamp)
    /// and a DATA section derived from the provided map.
    pub fn from_hashmap(&self, data: HashMap<String, DixValue>) -> Result<DixScript, String> {
        let mut flat_properties   = HashMap::new();
        let mut nested_structures: HashMap<String, DixValue> = HashMap::new();

        for (key, value) in data {
            match &value {
                DixValue::Object(_) | DixValue::Array(_) => {
                    nested_structures.insert(key, value);
                }
                _ => {
                    flat_properties.insert(key, value);
                }
            }
        }

        let mut data_entries = Vec::new();

        for (key, value) in flat_properties {
            let ast_value = self.convert_dix_value_to_ast_value(&value)?;
            data_entries.push(DataEntry::SimpleProperty {
                name: key,
                data_type: None,
                value: ast_value,
                position: Position::UNKNOWN,
            });
        }

        for (key, value) in nested_structures {
            self.process_nested_structure(&key, &value, &mut data_entries, "")?;
        }

        let data_section = if !data_entries.is_empty() {
            Some(DataSection { entries: data_entries, position: Position::UNKNOWN })
        } else {
            None
        };

        let config_section = Some(ConfigSection {
            entries: vec![
                ConfigEntry {
                    key: "version".to_string(),
                    value: ConfigValue::String("1.0.0".to_string()),
                    position: Position::UNKNOWN,
                },
                ConfigEntry {
                    key: "created".to_string(),
                    value: ConfigValue::Timestamp(
                        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
                    ),
                    position: Position::UNKNOWN,
                },
            ],
            position: Position::UNKNOWN,
        });

        Ok(DixScript {
            config: config_section,
            imports: None,
            dlm: None,
            enums: None,
            quick_functions: None,
            data: data_section,
            security: None,
        })
    }

    /// Convert a DixScript AST to a flat HashMap with dotted-path keys.
    ///
    /// The enum table from the AST is extracted first so that `EnumValue`
    /// nodes resolve to their correct integer values rather than 0.
    pub fn to_hashmap(&self, ast: &DixScript) -> HashMap<String, DixValue> {
        let mut result = HashMap::new();

        // Extract enums so field values can be resolved during flattening.
        let enums = self.extract_enums(ast);

        if let Some(ref data) = ast.data {
            for entry in &data.entries {
                self.flatten_entry(entry, "", &mut result, enums.as_ref());
            }
        }

        result
    }

    /// Serialize a DixScript AST to `.mdix` format text.
    pub fn to_mdix(&self, ast: &DixScript, options: Option<&DixFormatOptions>) -> Result<String, String> {
        let opts = options.unwrap_or(&self.default_options);
        let mut output = String::new();
        let nl     = opts.get_newline();
        let sp     = opts.get_space();
        let indent = opts.get_indentation(1);

        if opts.include_config_section {
            if let Some(ref config) = ast.config {
                output.push_str("@CONFIG(");
                output.push_str(nl);
                for (i, entry) in config.entries.iter().enumerate() {
                    if i > 0 { output.push(','); output.push_str(nl); }
                    output.push_str(&indent);
                    output.push_str(&entry.key);
                    output.push_str(sp);
                    output.push_str("->");
                    output.push_str(sp);
                    output.push_str(&self.format_config_value(&entry.value));
                }
                output.push_str(nl);
                output.push(')');
                output.push_str(nl);
                output.push_str(nl);
            }
        }

        if let Some(ref enums) = ast.enums {
            output.push_str("@ENUMS(");
            output.push_str(nl);
            for decl in &enums.enums {
                output.push_str(&indent);
                output.push_str(&decl.name);
                output.push_str(sp);
                output.push('{');
                output.push_str(nl);
                for (i, field) in decl.fields.iter().enumerate() {
                    if i > 0 { output.push(','); output.push_str(nl); }
                    output.push_str(&opts.get_indentation(2));
                    output.push_str(&field.name);
                    if let Some(value) = field.value {
                        output.push_str(sp);
                        output.push('=');
                        output.push_str(sp);
                        output.push_str(&value.to_string());
                    }
                }
                output.push_str(nl);
                output.push_str(&indent);
                output.push('}');
                output.push_str(nl);
            }
            output.push(')');
            output.push_str(nl);
            output.push_str(nl);
        }

        if let Some(ref data) = ast.data {
            output.push_str("@DATA(");
            output.push_str(nl);

            let (flat_props, table_props, group_arrays) =
                self.categorize_data_entries(&data.entries);

            for (i, entry) in flat_props.iter().enumerate() {
                if i > 0 { output.push(','); output.push_str(nl); }
                output.push_str(&indent);
                if let DataEntry::SimpleProperty { name, value, .. } = entry {
                    output.push_str(name);
                    output.push_str(sp);
                    output.push('=');
                    output.push_str(sp);
                    output.push_str(&self.format_value_for_mdix(value, opts));
                }
            }

            if !flat_props.is_empty() && (!table_props.is_empty() || !group_arrays.is_empty()) {
                output.push_str(nl);
                output.push_str(nl);
            }

            for entry in table_props {
                if let DataEntry::TableProperty { path, properties, .. } = entry {
                    output.push_str(&indent);
                    output.push_str(&path.to_string());
                    output.push(':');
                    output.push_str(sp);
                    for (i, prop) in properties.iter().enumerate() {
                        if i > 0 { output.push(','); output.push_str(sp); }
                        output.push_str(&prop.name);
                        output.push_str(sp);
                        output.push('=');
                        output.push_str(sp);
                        output.push_str(&self.format_value_for_mdix(&prop.value, opts));
                    }
                    output.push_str(nl);
                }
            }

            for entry in group_arrays {
                if let DataEntry::GroupArray { path, items, .. } = entry {
                    output.push_str(&indent);
                    output.push_str(&path.to_string());
                    output.push_str("::");
                    output.push_str(sp);
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 { output.push(','); output.push_str(sp); }
                        output.push_str(&self.format_value_for_mdix(item, opts));
                    }
                    output.push_str(nl);
                }
            }

            output.push(')');
        }

        if opts.minify {
            output = super::compactor::DixCompactor::minify(&output);
        } else if !opts.indented {
            output = super::compactor::DixCompactor::compact(&output);
        }

        Ok(output)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Extract the enum table from an AST for use during value conversion.
    fn extract_enums(
        &self,
        ast: &DixScript,
    ) -> Option<HashMap<String, HashMap<String, i32>>> {
        ast.enums.as_ref().map(|section| {
            section.enums.iter().map(|decl| {
                let mut auto_value = 0i32;
                let fields: HashMap<String, i32> = decl.fields.iter().map(|field| {
                    let value = field.value.unwrap_or_else(|| {
                        let v = auto_value;
                        auto_value += 1;
                        v
                    });
                    auto_value = value + 1;
                    (field.name.clone(), value)
                }).collect();
                (decl.name.clone(), fields)
            }).collect()
        })
    }

    fn process_nested_structure(
        &self,
        key: &str,
        value: &DixValue,
        entries: &mut Vec<DataEntry>,
        parent_path: &str,
    ) -> Result<(), String> {
        let current_path = if parent_path.is_empty() {
            key.to_string()
        } else {
            format!("{}.{}", parent_path, key)
        };

        match value {
            DixValue::Object(obj) => {
                let path = TablePath {
                    segments: current_path.split('.').map(String::from).collect(),
                };

                let mut properties = Vec::new();
                let mut nested     = Vec::new();

                for (k, v) in obj {
                    match v {
                        DixValue::Object(_) | DixValue::Array(_) => {
                            nested.push((k.clone(), v.clone()));
                        }
                        _ => {
                            let ast_value = self.convert_dix_value_to_ast_value(v)?;
                            properties.push(PropertyAssignment {
                                name: k.clone(),
                                data_type: None,
                                value: ast_value,
                                position: Position::UNKNOWN,
                            });
                        }
                    }
                }

                if !properties.is_empty() {
                    entries.push(DataEntry::TableProperty {
                        path,
                        properties,
                        position: Position::UNKNOWN,
                    });
                }

                for (k, v) in nested {
                    self.process_nested_structure(&k, &v, entries, &current_path)?;
                }
            }

            DixValue::Array(arr) => {
                let path = TablePath {
                    segments: current_path.split('.').map(String::from).collect(),
                };
                let items: Result<Vec<Value>, String> = arr
                    .iter()
                    .map(|v| self.convert_dix_value_to_ast_value(v))
                    .collect();
                entries.push(DataEntry::GroupArray {
                    path,
                    items: items?,
                    position: Position::UNKNOWN,
                });
            }

            _ => {
                return Err(format!(
                    "Expected object or array for nested structure, got: {}",
                    value.type_name()
                ));
            }
        }

        Ok(())
    }

    fn convert_dix_value_to_ast_value(&self, value: &DixValue) -> Result<Value, String> {
        Ok(match value {
            DixValue::Null           => Value::Null { position: Position::UNKNOWN },
            DixValue::Bool(b)        => Value::Boolean { value: *b, position: Position::UNKNOWN },
            DixValue::Int(i)         => Value::Integer { value: *i, position: Position::UNKNOWN },
            DixValue::Float(f)       => Value::Float   { value: *f, position: Position::UNKNOWN },
            DixValue::Double(d)      => Value::Double  { value: *d, position: Position::UNKNOWN },
            DixValue::String(s)      => Value::String  { value: s.clone(), position: Position::UNKNOWN },
            DixValue::Date(d)        => Value::Date     { value: d.clone(), position: Position::UNKNOWN },
            DixValue::Timestamp(t)   => Value::Timestamp { value: t.clone(), position: Position::UNKNOWN },
            DixValue::HexColor(c)    => Value::HexColor { value: c.clone(), position: Position::UNKNOWN },

            DixValue::Blob(b) => Value::PrefixedConstructor {
                prefix: "b".to_string(),
                arguments: vec![Value::String { value: b.clone(), position: Position::UNKNOWN }],
                position: Position::UNKNOWN,
            },
            DixValue::Regex(r) => Value::PrefixedConstructor {
                prefix: "r".to_string(),
                arguments: vec![Value::String { value: r.clone(), position: Position::UNKNOWN }],
                position: Position::UNKNOWN,
            },

            DixValue::Array(arr) => {
                let items: Result<Vec<Value>, String> = arr
                    .iter()
                    .map(|v| self.convert_dix_value_to_ast_value(v))
                    .collect();
                Value::Array { values: items?, position: Position::UNKNOWN }
            }

            DixValue::Object(obj) => {
                let mut properties = Vec::new();
                for (k, v) in obj {
                    let ast_value = self.convert_dix_value_to_ast_value(v)?;
                    properties.push(ObjectProperty {
                        key: k.clone(),
                        value: ast_value,
                        position: Position::UNKNOWN,
                    });
                }
                Value::Object { properties, position: Position::UNKNOWN }
            }

            DixValue::Tuple(items) => {
                let args: Result<Vec<Value>, String> = items
                    .iter()
                    .map(|v| self.convert_dix_value_to_ast_value(v))
                    .collect();
                Value::PrefixedConstructor {
                    prefix: "t".to_string(),
                    arguments: args?,
                    position: Position::UNKNOWN,
                }
            }

            // Round-trip enums by name — the integer value is not stored in
            // the AST node itself, only the enum and field names are.
            DixValue::Enum { enum_name, field_name, .. } => Value::EnumValue {
                enum_name: enum_name.clone(),
                value:     field_name.clone(),
                position:  Position::UNKNOWN,
            },
        })
    }

    /// Flatten a DataEntry into a HashMap, resolving enum values via the table.
    fn flatten_entry(
        &self,
        entry: &DataEntry,
        prefix: &str,
        result: &mut HashMap<String, DixValue>,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let key = Self::build_path(prefix, name);
                if let Some(dix_value) = self.convert_ast_value_to_dix_value(value, enums) {
                    result.insert(key, dix_value);
                }
            }

            DataEntry::TableProperty { path, properties, .. } => {
                let table_path = Self::build_path(prefix, &path.to_string());
                for prop in properties {
                    let key = Self::build_path(&table_path, &prop.name);
                    if let Some(dix_value) = self.convert_ast_value_to_dix_value(&prop.value, enums) {
                        result.insert(key, dix_value);
                    }
                }
            }

            DataEntry::GroupArray { path, items, .. } => {
                let array_path = Self::build_path(prefix, &path.to_string());
                let array_values: Vec<DixValue> = items
                    .iter()
                    .filter_map(|v| self.convert_ast_value_to_dix_value(v, enums))
                    .collect();
                result.insert(array_path.clone(), DixValue::Array(array_values.clone()));
                for (i, value) in array_values.iter().enumerate() {
                    result.insert(format!("{}[{}]", array_path, i), value.clone());
                }
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let key = Self::build_path(prefix, name);
                if let Value::Object { ref properties, .. } = **object {
                    let mut obj_map = HashMap::new();
                    for prop in properties {
                        if let Some(dix_value) = self.convert_ast_value_to_dix_value(&prop.value, enums) {
                            obj_map.insert(prop.key.clone(), dix_value.clone());
                            result.insert(Self::build_path(&key, &prop.key), dix_value);
                        }
                    }
                    result.insert(key, DixValue::Object(obj_map));
                }
            }
        }
    }

    fn convert_ast_value_to_dix_value(
        &self,
        value: &Value,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) -> Option<DixValue> {
        match value {
            Value::Null { .. }                => Some(DixValue::Null),
            Value::Boolean { value: b, .. }   => Some(DixValue::Bool(*b)),
            Value::Integer { value: i, .. }   => Some(DixValue::Int(*i)),
            Value::Float { value: f, .. }     => Some(DixValue::Float(*f)),
            Value::Double { value: d, .. }    => Some(DixValue::Double(*d)),
            Value::String { value: s, .. }    => Some(DixValue::String(s.clone())),
            Value::Date { value: d, .. }      => Some(DixValue::Date(d.clone())),
            Value::Timestamp { value: t, .. } => Some(DixValue::Timestamp(t.clone())),
            Value::HexColor { value: c, .. }  => Some(DixValue::HexColor(c.clone())),

            Value::Array { values, .. } => {
                let items: Vec<DixValue> = values
                    .iter()
                    .filter_map(|v| self.convert_ast_value_to_dix_value(v, enums))
                    .collect();
                Some(DixValue::Array(items))
            }

            Value::Object { properties, .. } => {
                let mut obj = HashMap::new();
                for prop in properties {
                    if let Some(dix_value) = self.convert_ast_value_to_dix_value(&prop.value, enums) {
                        obj.insert(prop.key.clone(), dix_value);
                    }
                }
                Some(DixValue::Object(obj))
            }

            Value::EnumValue { enum_name, value: field_name, .. } => {
                let resolved = enums
                    .and_then(|e| e.get(enum_name.as_str()))
                    .and_then(|fields| fields.get(field_name.as_str()))
                    .copied()
                    .unwrap_or(0);

                Some(DixValue::Enum {
                    enum_name:  enum_name.clone(),
                    field_name: field_name.clone(),
                    value:      resolved,
                })
            }

            Value::PrefixedConstructor { prefix, arguments, .. } => {
                let prefix_str = prefix.to_string();
                match prefix_str.as_str() {
                    "t" => {
                        let items: Vec<DixValue> = arguments
                            .iter()
                            .filter_map(|v| self.convert_ast_value_to_dix_value(v, enums))
                            .collect();
                        Some(DixValue::Tuple(items))
                    }
                    "b" => {
                        if let Some(Value::String { value: s, .. }) = arguments.first() {
                            Some(DixValue::Blob(s.clone()))
                        } else {
                            None
                        }
                    }
                    "r" => {
                        if let Some(Value::String { value: s, .. }) = arguments.first() {
                            Some(DixValue::Regex(s.clone()))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }

            _ => None,
        }
    }

    fn categorize_data_entries<'a>(
        &self,
        entries: &'a [DataEntry],
    ) -> (Vec<&'a DataEntry>, Vec<&'a DataEntry>, Vec<&'a DataEntry>) {
        let mut flat   = Vec::new();
        let mut tables = Vec::new();
        let mut arrays = Vec::new();

        for entry in entries {
            match entry {
                DataEntry::SimpleProperty { .. } | DataEntry::ObjectProperty { .. } => {
                    flat.push(entry);
                }
                DataEntry::TableProperty { .. } => tables.push(entry),
                DataEntry::GroupArray { .. }    => arrays.push(entry),
            }
        }

        (flat, tables, arrays)
    }

    fn build_path(prefix: &str, segment: &str) -> String {
        if prefix.is_empty() {
            segment.to_string()
        } else {
            format!("{}.{}", prefix, segment)
        }
    }

    fn format_config_value(&self, value: &ConfigValue) -> String {
        match value {
            ConfigValue::String(s)    => format!("\"{}\"", s),
            ConfigValue::Integer(i)   => i.to_string(),
            ConfigValue::Float(f)     => format!("{}f", f),
            ConfigValue::Boolean(b)   => b.to_string(),
            ConfigValue::Date(d)      => d.clone(),
            ConfigValue::Timestamp(t) => t.clone(),
            _                         => String::new(),
        }
    }

    fn format_value_for_mdix(&self, value: &Value, opts: &DixFormatOptions) -> String {
        let sp = opts.get_space();
        match value {
            Value::Null { .. }                => "null".to_string(),
            Value::Boolean { value: b, .. }   => b.to_string(),
            Value::Integer { value: i, .. }   => i.to_string(),
            Value::Float { value: f, .. }     => format!("{}f", f),
            Value::Double { value: d, .. }    => d.to_string(),
            Value::String { value: s, .. }    => format!("\"{}\"", s),
            Value::Date { value: d, .. }      => d.clone(),
            Value::Timestamp { value: t, .. } => t.clone(),
            Value::HexColor { value: c, .. }  => c.clone(),

            Value::Array { values, .. } => {
                let items: Vec<String> = values
                    .iter()
                    .map(|v| self.format_value_for_mdix(v, opts))
                    .collect();
                format!("[{}]", items.join(&format!(",{}", sp)))
            }

            Value::Object { properties, .. } => {
                let pairs: Vec<String> = properties
                    .iter()
                    .map(|p| format!(
                        "{}{}={}{}",
                        p.key, sp, sp,
                        self.format_value_for_mdix(&p.value, opts)
                    ))
                    .collect();
                format!("{{{}}}", pairs.join(&format!(",{}", sp)))
            }

            Value::PrefixedConstructor { prefix, arguments, .. } => {
                let args: Vec<String> = arguments
                    .iter()
                    .map(|v| self.format_value_for_mdix(v, opts))
                    .collect();
                format!("{}:({})", prefix, args.join(&format!(",{}", sp)))
            }

            Value::EnumValue { enum_name, value: field_value, .. } => {
                format!("{}.{}", enum_name, field_value)
            }

            _ => String::new(),
        }
    }
}

impl Default for DixConverter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_hashmap_simple() {
        let converter = DixConverter::new();
        let mut data = HashMap::new();
        data.insert("name".to_string(), DixValue::String("Alice".to_string()));
        data.insert("age".to_string(),  DixValue::Int(30));
        let ast = converter.from_hashmap(data).unwrap();
        assert!(ast.config.is_some());
        assert!(ast.data.is_some());
    }

    #[test]
    fn test_to_mdix_basic() {
        let converter = DixConverter::new();
        let ast = DixScript {
            config: Some(ConfigSection {
                entries: vec![ConfigEntry {
                    key: "version".to_string(),
                    value: ConfigValue::String("1.0.0".to_string()),
                    position: Position::UNKNOWN,
                }],
                position: Position::UNKNOWN,
            }),
            data: Some(DataSection {
                entries: vec![DataEntry::SimpleProperty {
                    name: "x".to_string(),
                    data_type: None,
                    value: Value::Integer { value: 42, position: Position::UNKNOWN },
                    position: Position::UNKNOWN,
                }],
                position: Position::UNKNOWN,
            }),
            imports: None,
            dlm: None,
            enums: None,
            quick_functions: None,
            security: None,
        };
        let mdix = converter.to_mdix(&ast, None).unwrap();
        assert!(mdix.contains("@CONFIG"));
        assert!(mdix.contains("@DATA"));
        assert!(mdix.contains("x = 42"));
    }

    #[test]
    fn test_enum_value_resolved_in_to_hashmap() {
        let converter = DixConverter::new();
        let ast = DixScript {
            enums: Some(EnumsSection {
                enums: vec![EnumDeclaration {
                    name: "AIType".to_string(),
                    fields: vec![
                        EnumField { name: "PASSIVE".to_string(),    value: Some(0), position: Position::UNKNOWN },
                        EnumField { name: "AGGRESSIVE".to_string(), value: Some(1), position: Position::UNKNOWN },
                        EnumField { name: "BOSS".to_string(),       value: Some(2), position: Position::UNKNOWN },
                    ],
                    position: Position::UNKNOWN,
                }],
                position: Position::UNKNOWN,
            }),
            data: Some(DataSection {
                entries: vec![DataEntry::SimpleProperty {
                    name: "enemy_type".to_string(),
                    data_type: None,
                    value: Value::EnumValue {
                        enum_name: "AIType".to_string(),
                        value: "BOSS".to_string(),
                        position: Position::UNKNOWN,
                    },
                    position: Position::UNKNOWN,
                }],
                position: Position::UNKNOWN,
            }),
            config: None,
            imports: None,
            dlm: None,
            quick_functions: None,
            security: None,
        };

        let map = converter.to_hashmap(&ast);
        match map.get("enemy_type") {
            Some(DixValue::Enum { value, .. }) => assert_eq!(*value, 2),
            other => panic!("expected resolved enum, got {:?}", other),
        }
    }
}
