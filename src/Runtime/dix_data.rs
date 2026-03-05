// src/Runtime/dix_data.rs

use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use crate::Compiler::AST::DixScript;
use super::dix_value::DixValue;

/// Runtime data container with optimized flattened access
///
/// Core features:
/// - Flattened HashMap storage (dotted paths like "user.address.city")
/// - O(1) access by path
/// - O(1) prefix-child lookup via secondary index
/// - Section extraction (Config, Enums, Security, DLM)
/// - Metadata tracking (version, compile time, encryption status)
#[derive(Debug, Clone)]
pub struct DixData {
    /// Flattened data storage — all DATA section values keyed by dotted paths.
    flattened_data: HashMap<String, DixValue>,

    /// Secondary index: maps each path prefix to its set of direct child segments.
    /// Built once in from_ast. Turns get_keys() from O(n) scan to O(1) lookup.
    prefix_index: HashMap<String, HashSet<String>>,

    /// CONFIG section as key-value pairs
    pub config: Option<HashMap<String, String>>,

    /// ENUMS section as nested maps: enum_name -> { field_name -> value }
    pub enums: Option<HashMap<String, HashMap<String, i32>>>,

    /// SECURITY section as key-value pairs
    pub security: Option<HashMap<String, DixValue>>,

    /// DLM modules list (module names)
    pub dlm: Option<Vec<String>>,

    /// DixScript version
    pub version: String,

    /// Compilation timestamp
    pub compile_time: DateTime<Utc>,

    /// Whether file was encrypted
    pub is_encrypted: bool,

    /// Whether file was compressed
    pub is_compressed: bool,

    /// Applied DLM modules during load
    pub applied_modules: Vec<String>,
}

impl DixData {
    /// Create DixData from resolved AST.
    ///
    /// This is the main constructor used by DixLoader after compilation.
    pub fn from_ast(
        ast: DixScript,
        version: String,
        compile_time: DateTime<Utc>,
        is_encrypted: bool,
        is_compressed: bool,
        applied_modules: Vec<String>,
    ) -> Self {
        let mut flattened_data = HashMap::new();

        if let Some(ref data) = ast.data {
            Self::flatten_data_section(data, &mut flattened_data);
        }

        let prefix_index = Self::build_prefix_index(&flattened_data);

        let config   = Self::extract_config_section(ast.config.as_ref());
        let enums    = Self::extract_enums_section(ast.enums.as_ref());
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

    /// Get value by path with type conversion.
    ///
    /// # Examples
    ///! ```
    /// let name: String = data.get("user.name")?;
    /// let age: i32 = data.get("user.age")?;
    /// ```
    pub fn get<T>(&self, path: &str) -> Result<T, String>
    where
        T: TryFrom<DixValue>,
        <T as TryFrom<DixValue>>::Error: std::fmt::Display,
    {
        let value = self.flattened_data
            .get(path)
            .ok_or_else(|| format!("Path not found: {}", path))?;

        T::try_from(value.clone())
            .map_err(|e| format!("Type conversion failed for path '{}': {}", path, e))
    }

    /// Get value by path, returning default if not found or conversion fails.
    pub fn get_or_default<T>(&self, path: &str, default: T) -> T
    where
        T: TryFrom<DixValue>,
    {
        self.flattened_data
            .get(path)
            .and_then(|v| T::try_from(v.clone()).ok())
            .unwrap_or(default)
    }

    /// Get raw DixValue by path (no conversion).
    pub fn get_value(&self, path: &str) -> Option<&DixValue> {
        self.flattened_data.get(path)
    }

    /// Check if path exists in data.
    pub fn exists(&self, path: &str) -> bool {
        self.flattened_data.contains_key(path)
    }

    /// Get the direct child segment names under a path prefix.
    ///
    /// O(1) lookup via prefix_index, then O(k) clone where k is the child count.
    ///
    /// # Examples
    ///! ```
    ///! let user_keys = data.get_keys("user");     // ["name", "age", "address"]
    ///! let top_keys  = data.get_keys("");          // top-level keys
    ///! ```
    pub fn get_keys(&self, path: &str) -> Vec<String> {
        match self.prefix_index.get(path) {
            Some(children) => children.iter().cloned().collect(),
            None           => Vec::new(),
        }
    }

    /// Select multiple values matching a wildcard pattern.
    ///
    /// Pattern uses `*` to match any single segment.
    ///
    /// # Examples
    ///! ```
    /// let all_names: Vec<String> = data.select_many("users.*.name")?;
    /// ```
    pub fn select_many<T>(&self, pattern: &str) -> Vec<T>
    where
        T: TryFrom<DixValue>,
    {
        let regex_pattern = format!(
            "^{}$",
            pattern.replace('.', r"\.").replace('*', r"[^.]+")
        );

        let regex = match regex::Regex::new(&regex_pattern) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        self.flattened_data
            .iter()
            .filter(|(k, _)| regex.is_match(k))
            .filter_map(|(_, v)| T::try_from(v.clone()).ok())
            .collect()
    }

    /// Get total number of data entries.
    pub fn entry_count(&self) -> usize {
        self.flattened_data.len()
    }

    /// Get all data as HashMap (clone of internal storage).
    pub fn to_hashmap(&self) -> HashMap<String, DixValue> {
        self.flattened_data.clone()
    }

    // ── Index construction ────────────────────────────────────────────────────

    /// Build the prefix index from a completed flattened_data map.
    /// Called once in from_ast after all keys are inserted.
    fn build_prefix_index(
        flattened_data: &HashMap<String, DixValue>,
    ) -> HashMap<String, HashSet<String>> {
        let mut index: HashMap<String, HashSet<String>> =
            HashMap::with_capacity(flattened_data.len());
        for key in flattened_data.keys() {
            Self::index_key(&mut index, key);
        }
        index
    }

    /// Walk a single key upward through its dot-segments, registering each
    /// segment as a direct child of its parent prefix.
    ///
    /// "database.primary.host" produces:
    ///   index["database.primary"] ← "host"
    ///   index["database"]         ← "primary"
    ///   index[""]                 ← "database"
    fn index_key(index: &mut HashMap<String, HashSet<String>>, key: &str) {
        let mut remaining = key;
        loop {
            match remaining.rfind('.') {
                None => {
                    // Top-level segment — parent is the empty-string prefix.
                    index
                        .entry(String::new())
                        .or_default()
                        .insert(remaining.to_string());
                    break;
                }
                Some(dot_pos) => {
                    let parent = &remaining[..dot_pos];
                    let child  = &remaining[dot_pos + 1..];
                    index
                        .entry(parent.to_string())
                        .or_default()
                        .insert(child.to_string());
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
                .map(|entry| {
                    let value = Self::config_value_to_string(&entry.value);
                    (entry.key.clone(), value)
                })
                .collect()
        })
    }

    fn extract_enums_section(
        enums: Option<&crate::Compiler::AST::EnumsSection>,
    ) -> Option<HashMap<String, HashMap<String, i32>>> {
        enums.map(|enums_section| {
            enums_section
                .enums
                .iter()
                .map(|enum_decl| {
                    let mut auto_value = 0;
                    let fields: HashMap<String, i32> = enum_decl
                        .fields
                        .iter()
                        .map(|field| {
                            let value = field.value.unwrap_or_else(|| {
                                let v = auto_value;
                                auto_value += 1;
                                v
                            });
                            auto_value = value + 1;
                            (field.name.clone(), value)
                        })
                        .collect();
                    (enum_decl.name.clone(), fields)
                })
                .collect()
        })
    }

    fn extract_security_section(
        security: Option<&crate::Compiler::AST::SecuritySection>,
    ) -> Option<HashMap<String, DixValue>> {
        security.map(|sec| {
            sec.entries
                .iter()
                .map(|entry| {
                    let mut block_data = HashMap::new();
                    for field in &entry.fields {
                        if let Some(dix_val) = Self::ast_value_to_dix_value(&field.value) {
                            block_data.insert(field.key.clone(), dix_val);
                        }
                    }
                    (entry.block_key.clone(), DixValue::Object(block_data))
                })
                .collect()
        })
    }

    fn extract_dlm_section(
        dlm: Option<&crate::Compiler::AST::DLMSection>,
    ) -> Option<Vec<String>> {
        dlm.map(|dlm_section| {
            dlm_section
                .modules
                .iter()
                .map(|module| format!("{:?}", module.module_type))
                .collect()
        })
    }

    // ── Data flattening ───────────────────────────────────────────────────────

    fn flatten_data_section(
        data: &crate::Compiler::AST::DataSection,
        result: &mut HashMap<String, DixValue>,
    ) {
        for entry in &data.entries {
            Self::flatten_entry(entry, "", result);
        }
    }

    fn flatten_entry(
        entry: &crate::Compiler::AST::DataEntry,
        prefix: &str,
        result: &mut HashMap<String, DixValue>,
    ) {
        use crate::Compiler::AST::DataEntry;

        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let key = Self::build_path(prefix, name);
                if let Some(dix_value) = Self::ast_value_to_dix_value(value) {
                    result.insert(key, dix_value);
                }
            }

            DataEntry::TableProperty { path, properties, .. } => {
                let table_path = Self::build_path(prefix, &path.to_string());
                for prop in properties {
                    let key = Self::build_path(&table_path, &prop.name);
                    if let Some(dix_value) = Self::ast_value_to_dix_value(&prop.value) {
                        result.insert(key, dix_value);
                    }
                }
            }

            DataEntry::GroupArray { path, items, .. } => {
                let array_path = Self::build_path(prefix, &path.to_string());

                let array_values: Vec<DixValue> = items
                    .iter()
                    .filter_map(Self::ast_value_to_dix_value)
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
                        if let Some(dix_value) = Self::ast_value_to_dix_value(&prop.value) {
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

    fn ast_value_to_dix_value(value: &crate::Compiler::AST::Value) -> Option<DixValue> {
        use crate::Compiler::AST::Value;

        match value {
            Value::Null { .. }              => Some(DixValue::Null),
            Value::Boolean { value: b, .. } => Some(DixValue::Bool(*b)),
            Value::Integer { value: i, .. } => Some(DixValue::Int(*i)),
            Value::Float { value: f, .. }   => Some(DixValue::Float(*f)),
            Value::Double { value: d, .. }  => Some(DixValue::Double(*d)),
            Value::String { value: s, .. }  => Some(DixValue::String(s.clone())),
            Value::Date { value: d, .. }    => Some(DixValue::Date(d.clone())),
            Value::Timestamp { value: t, .. } => Some(DixValue::Timestamp(t.clone())),
            Value::HexColor { value: c, .. } => Some(DixValue::HexColor(c.clone())),

            Value::Array { values, .. } => {
                let items: Vec<DixValue> = values
                    .iter()
                    .filter_map(Self::ast_value_to_dix_value)
                    .collect();
                Some(DixValue::Array(items))
            }

            Value::Object { properties, .. } => {
                let mut obj = HashMap::new();
                for prop in properties {
                    if let Some(dix_value) = Self::ast_value_to_dix_value(&prop.value) {
                        obj.insert(prop.key.clone(), dix_value);
                    }
                }
                Some(DixValue::Object(obj))
            }

            Value::EnumValue { enum_name, value: field_value, .. } => {
                Some(DixValue::Enum {
                    enum_name: enum_name.clone(),
                    field_name: field_value.clone(),
                    value: 0,
                })
            }

            Value::PrefixedConstructor { prefix, arguments, .. } => {
                if prefix.to_string() == "t" {
                    let items: Vec<DixValue> = arguments
                        .iter()
                        .filter_map(Self::ast_value_to_dix_value)
                        .collect();
                    Some(DixValue::Tuple(items))
                } else if prefix.to_string() == "b" {
                    if let Some(Value::String { value: s, .. }) = arguments.first() {
                        Some(DixValue::Blob(s.clone()))
                    } else {
                        None
                    }
                } else if prefix.to_string() == "r" {
                    if let Some(Value::String { value: s, .. }) = arguments.first() {
                        Some(DixValue::Regex(s.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }

            _ => None,
        }
    }
}

// ── TryFrom implementations for common types ──────────────────────────────────

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

    /// Helper that builds a DixData directly from a pre-populated flattened map.
    /// Used only in unit tests that don't go through the full AST pipeline.
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
        let data = DixData::from_ast(
            ast,
            "1.0.0".to_string(),
            Utc::now(),
            false,
            false,
            vec![],
        );

        assert_eq!(data.version, "1.0.0");
        assert!(!data.is_encrypted);
        assert!(!data.is_compressed);
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
    fn test_get_keys_missing_prefix() {
        let data = dix_data_from_flat(HashMap::new());
        let keys = data.get_keys("nonexistent");
        assert!(keys.is_empty());
    }

    #[test]
    fn test_index_key_array_indexed() {
        // Array-indexed keys like "tags[0]" have no dot; they should appear
        // as top-level children under the "" prefix.
        let mut flattened = HashMap::new();
        flattened.insert("tags".to_string(),    DixValue::Array(vec![]));
        flattened.insert("tags[0]".to_string(), DixValue::String("prod".to_string()));

        let data = dix_data_from_flat(flattened);

        let mut top = data.get_keys("");
        top.sort();
        assert_eq!(top, vec!["tags", "tags[0]"]);
    }
        }
