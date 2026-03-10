// src/Runtime/dix_data.rs

use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use crate::Compiler::AST::DixScript;
use super::dix_value::DixValue;

/// Runtime data container with optimized flattened access.
///
/// Flattened HashMap storage with O(1) access by dotted path and O(1)
/// prefix-child lookup via a secondary index built once at load time.
#[derive(Debug, Clone)]
pub struct DixData {
    flattened_data: HashMap<String, DixValue>,
    prefix_index:   HashMap<String, HashSet<String>>,

    pub config:          Option<HashMap<String, String>>,
    pub enums:           Option<HashMap<String, HashMap<String, i32>>>,
    pub security:        Option<HashMap<String, DixValue>>,
    pub dlm:             Option<Vec<String>>,
    pub version:         String,
    pub compile_time:    DateTime<Utc>,
    pub is_encrypted:    bool,
    pub is_compressed:   bool,
    pub applied_modules: Vec<String>,
}

impl DixData {
    /// Create DixData from a resolved AST.
    ///
    /// Enums are extracted first so their integer values can be resolved
    /// during the data-flattening pass that follows.
    pub fn from_ast(
        ast: DixScript,
        version: String,
        compile_time: DateTime<Utc>,
        is_encrypted: bool,
        is_compressed: bool,
        applied_modules: Vec<String>,
    ) -> Self {
        // Extract enums before flattening so we can resolve enum field values.
        let enums = Self::extract_enums_section(ast.enums.as_ref());

        let mut flattened_data = HashMap::new();
        if let Some(ref data) = ast.data {
            Self::flatten_data_section(data, &mut flattened_data, enums.as_ref());
        }

        let prefix_index = Self::build_prefix_index(&flattened_data);
        let config   = Self::extract_config_section(ast.config.as_ref());
        let security = Self::extract_security_section(ast.security.as_ref());
        let dlm      = Self::extract_dlm_section(ast.dlm.as_ref());

        DixData {
            flattened_data,
            prefix_index,
            config,
            enums,
            security,
            dlm,
            version,
            compile_time,
            is_encrypted,
            is_compressed,
            applied_modules,
        }
    }

    /// Get a value by dotted path with type conversion.
    pub fn get<T>(&self, path: &str) -> Result<T, String>
    where
        T: TryFrom<DixValue>,
        <T as TryFrom<DixValue>>::Error: std::fmt::Display,
    {
        let value = self.flattened_data
            .get(path)
            .ok_or_else(|| format!("Path not found: {}", path))?;

        T::try_from(value.clone())
            .map_err(|e| format!("Type conversion failed for '{}': {}", path, e))
    }

    /// Get a value by path, returning a default if missing or unconvertible.
    pub fn get_or_default<T>(&self, path: &str, default: T) -> T
    where
        T: TryFrom<DixValue>,
    {
        self.flattened_data
            .get(path)
            .and_then(|v| T::try_from(v.clone()).ok())
            .unwrap_or(default)
    }

    /// Get the raw DixValue at a path without any conversion.
    pub fn get_value(&self, path: &str) -> Option<&DixValue> {
        self.flattened_data.get(path)
    }

    /// Check whether a dotted path exists.
    pub fn exists(&self, path: &str) -> bool {
        self.flattened_data.contains_key(path)
    }

    /// Get the direct child segment names under a path prefix.
    ///
    /// Pass an empty string for top-level keys.
    pub fn get_keys(&self, path: &str) -> Vec<String> {
        match self.prefix_index.get(path) {
            Some(children) => children.iter().cloned().collect(),
            None           => Vec::new(),
        }
    }

    /// Select all values whose path matches a dot-separated wildcard pattern.
    ///
    /// Use `*` to match any single path segment.
    /// Example: `"enemies.*.name"` matches `"enemies.0.name"`, `"enemies.1.name"`, etc.
    ///
    /// No regex or allocation — uses a segment-by-segment comparison.
    pub fn select_many<T>(&self, pattern: &str) -> Vec<T>
    where
        T: TryFrom<DixValue>,
    {
        let pattern_segments: Vec<&str> = pattern.split('.').collect();

        self.flattened_data
            .iter()
            .filter(|(key, _)| Self::path_matches_pattern(key, &pattern_segments))
            .filter_map(|(_, v)| T::try_from(v.clone()).ok())
            .collect()
    }

    /// Check whether a key matches a wildcard pattern.
    ///
    /// Both key and pattern are split on `.`. A `*` segment matches any
    /// single key segment. Segment counts must match exactly.
    fn path_matches_pattern(key: &str, pattern_segments: &[&str]) -> bool {
        let key_segments: Vec<&str> = key.split('.').collect();
        if key_segments.len() != pattern_segments.len() {
            return false;
        }
        key_segments
            .iter()
            .zip(pattern_segments.iter())
            .all(|(k, p)| *p == "*" || *k == *p)
    }

    /// Total number of entries in the flattened data store.
    pub fn entry_count(&self) -> usize {
        self.flattened_data.len()
    }

    /// Clone the internal flat store as a HashMap.
    pub fn to_hashmap(&self) -> HashMap<String, DixValue> {
        self.flattened_data.clone()
    }

    // ── Prefix index ──────────────────────────────────────────────────────────

    fn build_prefix_index(
        flattened: &HashMap<String, DixValue>,
    ) -> HashMap<String, HashSet<String>> {
        let mut index: HashMap<String, HashSet<String>> =
            HashMap::with_capacity(flattened.len());
        for key in flattened.keys() {
            Self::index_key(&mut index, key);
        }
        index
    }

    /// Walk a key's dot-segments upward, registering each segment as a child
    /// of its parent prefix.
    ///
    /// `"database.primary.host"` produces:
    ///   index["database.primary"] ← "host"
    ///   index["database"]         ← "primary"
    ///   index[""]                 ← "database"
    fn index_key(index: &mut HashMap<String, HashSet<String>>, key: &str) {
        let mut remaining = key;
        loop {
            match remaining.rfind('.') {
                None => {
                    index.entry(String::new()).or_default().insert(remaining.to_string());
                    break;
                }
                Some(dot_pos) => {
                    let parent = &remaining[..dot_pos];
                    let child  = &remaining[dot_pos + 1..];
                    index.entry(parent.to_string()).or_default().insert(child.to_string());
                    remaining = parent;
                }
            }
        }
    }

    // ── Section extraction ────────────────────────────────────────────────────

    fn extract_config_section(
        config: Option<&crate::Compiler::AST::ConfigSection>,
    ) -> Option<HashMap<String, String>> {
        config.map(|cfg| {
            cfg.entries
                .iter()
                .map(|entry| (entry.key.clone(), Self::config_value_to_string(&entry.value)))
                .collect()
        })
    }

    fn extract_enums_section(
        enums: Option<&crate::Compiler::AST::EnumsSection>,
    ) -> Option<HashMap<String, HashMap<String, i32>>> {
        enums.map(|section| {
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

    fn extract_security_section(
        security: Option<&crate::Compiler::AST::SecuritySection>,
    ) -> Option<HashMap<String, DixValue>> {
        security.map(|sec| {
            sec.entries.iter().map(|entry| {
                let mut block_data = HashMap::new();
                for field in &entry.fields {
                    // Security sections don't reference game enums; pass None.
                    if let Some(dix_val) = Self::ast_value_to_dix_value(&field.value, None) {
                        block_data.insert(field.key.clone(), dix_val);
                    }
                }
                (entry.block_key.clone(), DixValue::Object(block_data))
            }).collect()
        })
    }

    fn extract_dlm_section(
        dlm: Option<&crate::Compiler::AST::DLMSection>,
    ) -> Option<Vec<String>> {
        dlm.map(|section| {
            section.modules.iter()
                .map(|module| format!("{:?}", module.module_type))
                .collect()
        })
    }

    // ── Data flattening ───────────────────────────────────────────────────────

    fn flatten_data_section(
        data: &crate::Compiler::AST::DataSection,
        result: &mut HashMap<String, DixValue>,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) {
        for entry in &data.entries {
            Self::flatten_entry(entry, "", result, enums);
        }
    }

    fn flatten_entry(
        entry: &crate::Compiler::AST::DataEntry,
        prefix: &str,
        result: &mut HashMap<String, DixValue>,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) {
        use crate::Compiler::AST::DataEntry;

        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let key = Self::build_path(prefix, name);
                if let Some(dix_value) = Self::ast_value_to_dix_value(value, enums) {
                    result.insert(key, dix_value);
                }
            }

            DataEntry::TableProperty { path, properties, .. } => {
                let table_path = Self::build_path(prefix, &path.to_string());
                for prop in properties {
                    let key = Self::build_path(&table_path, &prop.name);
                    if let Some(dix_value) = Self::ast_value_to_dix_value(&prop.value, enums) {
                        result.insert(key, dix_value);
                    }
                }
            }

            DataEntry::GroupArray { path, items, .. } => {
                let array_path = Self::build_path(prefix, &path.to_string());

                let array_values: Vec<DixValue> = items
                    .iter()
                    .filter_map(|v| Self::ast_value_to_dix_value(v, enums))
                    .collect();

                result.insert(array_path.clone(), DixValue::Array(array_values.clone()));

                for (i, value) in array_values.iter().enumerate() {
                    result.insert(format!("{}[{}]", array_path, i), value.clone());
                }
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let key = Self::build_path(prefix, name);

                if let crate::Compiler::AST::Value::Object { ref properties, .. } = **object {
                    let mut obj_map = HashMap::new();
                    for prop in properties {
                        if let Some(dix_value) = Self::ast_value_to_dix_value(&prop.value, enums) {
                            obj_map.insert(prop.key.clone(), dix_value.clone());
                            result.insert(Self::build_path(&key, &prop.key), dix_value);
                        }
                    }
                    result.insert(key, DixValue::Object(obj_map));
                }
            }
        }
    }

    fn build_path(prefix: &str, segment: &str) -> String {
        if prefix.is_empty() {
            segment.to_string()
        } else {
            format!("{}.{}", prefix, segment)
        }
    }

    // ── Conversion helpers ────────────────────────────────────────────────────

    fn config_value_to_string(value: &crate::Compiler::AST::ConfigValue) -> String {
        use crate::Compiler::AST::ConfigValue;
        match value {
            ConfigValue::String(s)    => s.clone(),
            ConfigValue::Integer(i)   => i.to_string(),
            ConfigValue::Float(f)     => f.to_string(),
            ConfigValue::Boolean(b)   => b.to_string(),
            ConfigValue::Date(d)      => d.clone(),
            ConfigValue::Timestamp(t) => t.clone(),
            _                         => String::new(),
        }
    }

    /// Convert an AST Value to a DixValue.
    ///
    /// `enums` is the resolved enum table. When present, `EnumValue` nodes
    /// have their integer field value looked up rather than defaulting to 0.
    fn ast_value_to_dix_value(
        value: &crate::Compiler::AST::Value,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) -> Option<DixValue> {
        use crate::Compiler::AST::Value;

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
                    .filter_map(|v| Self::ast_value_to_dix_value(v, enums))
                    .collect();
                Some(DixValue::Array(items))
            }

            Value::Object { properties, .. } => {
                let mut obj = HashMap::new();
                for prop in properties {
                    if let Some(dix_value) = Self::ast_value_to_dix_value(&prop.value, enums) {
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
                            .filter_map(|v| Self::ast_value_to_dix_value(v, enums))
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
}

// ── TryFrom implementations ───────────────────────────────────────────────────

impl TryFrom<DixValue> for String {
    type Error = String;
    fn try_from(value: DixValue) -> Result<Self, Self::Error> {
        match value {
            DixValue::String(s)    => Ok(s),
            DixValue::Date(d)      => Ok(d),
            DixValue::Timestamp(t) => Ok(t),
            DixValue::HexColor(c)  => Ok(c),
            _ => Err(format!("Cannot convert {} to String", value.type_name())),
        }
    }
}

impl TryFrom<DixValue> for i32 {
    type Error = String;
    fn try_from(value: DixValue) -> Result<Self, Self::Error> {
        match value {
            DixValue::Int(i)    => Ok(i),
            DixValue::Float(f)  => Ok(f as i32),
            DixValue::Double(d) => Ok(d as i32),
            // Enum values are now resolved integers — expose them as i32 directly.
            DixValue::Enum { value, .. } => Ok(value),
            _ => Err(format!("Cannot convert {} to i32", value.type_name())),
        }
    }
}

impl TryFrom<DixValue> for f64 {
    type Error = String;
    fn try_from(value: DixValue) -> Result<Self, Self::Error> {
        match value {
            DixValue::Int(i)    => Ok(i as f64),
            DixValue::Float(f)  => Ok(f as f64),
            DixValue::Double(d) => Ok(d),
            _ => Err(format!("Cannot convert {} to f64", value.type_name())),
        }
    }
}

impl TryFrom<DixValue> for bool {
    type Error = String;
    fn try_from(value: DixValue) -> Result<Self, Self::Error> {
        match value {
            DixValue::Bool(b) => Ok(b),
            _ => Err(format!("Cannot convert {} to bool", value.type_name())),
        }
    }
}

impl TryFrom<DixValue> for Vec<DixValue> {
    type Error = String;
    fn try_from(value: DixValue) -> Result<Self, Self::Error> {
        match value {
            DixValue::Array(arr) => Ok(arr),
            _ => Err(format!("Cannot convert {} to Vec<DixValue>", value.type_name())),
        }
    }
}

impl TryFrom<DixValue> for HashMap<String, DixValue> {
    type Error = String;
    fn try_from(value: DixValue) -> Result<Self, Self::Error> {
        match value {
            DixValue::Object(obj) => Ok(obj),
            _ => Err(format!("Cannot convert {} to HashMap", value.type_name())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler::AST::*;

    fn dix_data_from_flat(flattened: HashMap<String, DixValue>) -> DixData {
        let prefix_index = DixData::build_prefix_index(&flattened);
        DixData {
            flattened_data: flattened,
            prefix_index,
            config: None,
            enums: None,
            security: None,
            dlm: None,
            version: "1.0.0".to_string(),
            compile_time: Utc::now(),
            is_encrypted: false,
            is_compressed: false,
            applied_modules: vec![],
        }
    }

    #[test]
    fn test_dix_data_creation() {
        let ast = DixScript::new();
        let data = DixData::from_ast(ast, "1.0.0".to_string(), Utc::now(), false, false, vec![]);
        assert_eq!(data.version, "1.0.0");
        assert!(!data.is_encrypted);
    }

    #[test]
    fn test_get_value() {
        let mut flattened = HashMap::new();
        flattened.insert("name".to_string(), DixValue::String("Alice".to_string()));
        flattened.insert("age".to_string(), DixValue::Int(30));
        let data = dix_data_from_flat(flattened);

        let name: String = data.get("name").unwrap();
        assert_eq!(name, "Alice");
        let age: i32 = data.get("age").unwrap();
        assert_eq!(age, 30);
    }

    #[test]
    fn test_exists() {
        let mut flattened = HashMap::new();
        flattened.insert("x".to_string(), DixValue::Int(42));
        let data = dix_data_from_flat(flattened);
        assert!(data.exists("x"));
        assert!(!data.exists("y"));
    }

    #[test]
    fn test_get_or_default() {
        let data = dix_data_from_flat(HashMap::new());
        let value: i32 = data.get_or_default("missing", 999);
        assert_eq!(value, 999);
    }

    #[test]
    fn test_get_keys_top_level() {
        let mut flattened = HashMap::new();
        flattened.insert("a".to_string(),   DixValue::Int(1));
        flattened.insert("b.c".to_string(), DixValue::Int(2));
        flattened.insert("b.d".to_string(), DixValue::Int(3));
        let data = dix_data_from_flat(flattened);
        let mut top = data.get_keys("");
        top.sort();
        assert_eq!(top, vec!["a", "b"]);
    }

    #[test]
    fn test_get_keys_nested() {
        let mut flattened = HashMap::new();
        flattened.insert("db.host".to_string(), DixValue::String("localhost".to_string()));
        flattened.insert("db.port".to_string(), DixValue::Int(5432));
        flattened.insert("db.ssl".to_string(),  DixValue::Bool(true));
        let data = dix_data_from_flat(flattened);
        let mut children = data.get_keys("db");
        children.sort();
        assert_eq!(children, vec!["host", "port", "ssl"]);
    }

    #[test]
    fn test_select_many_wildcard() {
        let mut flattened = HashMap::new();
        flattened.insert("enemies.0.name".to_string(), DixValue::String("Goblin".to_string()));
        flattened.insert("enemies.1.name".to_string(), DixValue::String("Orc".to_string()));
        flattened.insert("enemies.0.hp".to_string(),   DixValue::Int(50));
        let data = dix_data_from_flat(flattened);
        let mut names: Vec<String> = data.select_many("enemies.*.name");
        names.sort();
        assert_eq!(names, vec!["Goblin", "Orc"]);
    }

    #[test]
    fn test_select_many_no_match() {
        let data = dix_data_from_flat(HashMap::new());
        let results: Vec<String> = data.select_many("missing.*.field");
        assert!(results.is_empty());
    }

    #[test]
    fn test_enum_value_resolves_correctly() {
        // Simulate what from_ast does: enums extracted, then threaded into flattening.
        let mut enum_table: HashMap<String, HashMap<String, i32>> = HashMap::new();
        let mut ai_type = HashMap::new();
        ai_type.insert("PASSIVE".to_string(), 0);
        ai_type.insert("AGGRESSIVE".to_string(), 1);
        ai_type.insert("BOSS".to_string(), 2);
        enum_table.insert("AIType".to_string(), ai_type);

        let value = DixData::ast_value_to_dix_value(
            &crate::Compiler::AST::Value::EnumValue {
                enum_name: "AIType".to_string(),
                value: "BOSS".to_string(),
                position: crate::Compiler::AST::Position::UNKNOWN,
            },
            Some(&enum_table),
        );

        assert_eq!(
            value,
            Some(DixValue::Enum {
                enum_name:  "AIType".to_string(),
                field_name: "BOSS".to_string(),
                value:      2,
            })
        );
    }

    #[test]
    fn test_enum_value_as_i32() {
        let dix_enum = DixValue::Enum {
            enum_name:  "AIType".to_string(),
            field_name: "BOSS".to_string(),
            value:      2,
        };
        let as_int: i32 = i32::try_from(dix_enum).unwrap();
        assert_eq!(as_int, 2);
    }

    #[test]
    fn test_get_keys_missing_prefix() {
        let data = dix_data_from_flat(HashMap::new());
        assert!(data.get_keys("nonexistent").is_empty());
    }
            }
