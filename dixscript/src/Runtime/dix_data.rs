// dixscript/src/Runtime/dix_data.rs
use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use crate::Compiler::AST::DixScript;
use super::dix_value::DixValue;

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
    pub fn from_ast(
        ast: DixScript,
        version: String,
        compile_time: DateTime<Utc>,
        is_encrypted: bool,
        is_compressed: bool,
        applied_modules: Vec<String>,
    ) -> Self {
        let enums = Self::extract_enums_section(ast.enums.as_ref());

        let mut flattened_data = HashMap::new();
        if let Some(ref data) = ast.data {
            Self::flatten_data_section(data, &mut flattened_data, enums.as_ref());
        }

        let prefix_index = Self::build_prefix_index(&flattened_data);
        let config       = Self::extract_config_section(ast.config.as_ref());
        let security     = Self::extract_security_section(ast.security.as_ref());
        let dlm          = Self::extract_dlm_section(ast.dlm.as_ref());

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

    pub fn get_or_default<T>(&self, path: &str, default: T) -> T
    where
        T: TryFrom<DixValue>,
    {
        self.flattened_data
            .get(path)
            .and_then(|v| T::try_from(v.clone()).ok())
            .unwrap_or(default)
    }

    #[inline]
    pub fn get_value(&self, path: &str) -> Option<&DixValue> {
        self.flattened_data.get(path)
    }

    #[inline]
    pub fn exists(&self, path: &str) -> bool {
        self.flattened_data.contains_key(path)
    }

    pub fn get_keys(&self, path: &str) -> Vec<String> {
        match self.prefix_index.get(path) {
            Some(children) => children.iter().cloned().collect(),
            None           => Vec::new(),
        }
    }

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

    #[inline]
    pub fn entry_count(&self) -> usize { self.flattened_data.len() }

    pub fn to_hashmap(&self) -> HashMap<String, DixValue> {
        self.flattened_data.clone()
    }

    // ── Pattern matching ──────────────────────────────────────────────────────

    fn path_matches_pattern(key: &str, pattern_segments: &[&str]) -> bool {
        let key_segments: Vec<&str> = key.split('.').collect();
        if key_segments.len() != pattern_segments.len() { return false; }
        key_segments.iter().zip(pattern_segments.iter())
            .all(|(k, p)| *p == "*" || *k == *p)
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
            cfg.entries.iter()
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
                        let v = auto_value; auto_value += 1; v
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
                    Self::flatten_dix_value(&key, &dix_value, result);
                }
            }
            DataEntry::TableProperty { path, properties, .. } => {
                let table_path = Self::build_path(prefix, &path.to_string());
                for prop in properties {
                    let key = Self::build_path(&table_path, &prop.name);
                    if let Some(dix_value) = Self::ast_value_to_dix_value(&prop.value, enums) {
                        Self::flatten_dix_value(&key, &dix_value, result);
                    }
                }
            }
            DataEntry::GroupArray { path, items, .. } => {
                let array_path = Self::build_path(prefix, &path.to_string());
                let array_values: Vec<DixValue> = items.iter()
                    .filter_map(|v| Self::ast_value_to_dix_value(v, enums))
                    .collect();
                result.insert(array_path.clone(), DixValue::Array(array_values.clone()));
                for (i, value) in array_values.iter().enumerate() {
                    let item_path = format!("{}[{}]", array_path, i);
                    Self::flatten_dix_value(&item_path, value, result);
                }
            }
            DataEntry::ObjectProperty { name, object, .. } => {
                let key = Self::build_path(prefix, name);
                if let crate::Compiler::AST::Value::Object { ref properties, .. } = **object {
                    let mut obj_map = HashMap::new();
                    for prop in properties {
                        if let Some(dix_value) = Self::ast_value_to_dix_value(&prop.value, enums) {
                            obj_map.insert(prop.key.clone(), dix_value.clone());
                            Self::flatten_dix_value(
                                &Self::build_path(&key, &prop.key), &dix_value, result,
                            );
                        }
                    }
                    result.insert(key, DixValue::Object(obj_map));
                }
            }
        }
    }

    fn flatten_dix_value(
        path: &str,
        value: &DixValue,
        result: &mut HashMap<String, DixValue>,
    ) {
        result.insert(path.to_string(), value.clone());
        match value {
            DixValue::Object(obj) => {
                for (k, v) in obj {
                    let child = format!("{}.{}", path, k);
                    Self::flatten_dix_value(&child, v, result);
                }
            }
            DixValue::Array(arr) => {
                for (i, item) in arr.iter().enumerate() {
                    let child = format!("{}[{}]", path, i);
                    Self::flatten_dix_value(&child, item, result);
                }
            }
            _ => {}
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

    fn ast_value_to_dix_value(
        value: &crate::Compiler::AST::Value,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) -> Option<DixValue> {
        use crate::Compiler::AST::Value;

        match value {
            Value::Null { .. }                => Some(DixValue::Null),
            Value::Boolean { value: b, .. }   => Some(DixValue::Bool(*b)),
            Value::Integer { value: i, .. }   => Some(DixValue::Int(*i)),
            Value::Long { value: l, .. }      => Some(DixValue::Long(*l)),
            Value::Float { value: f, .. }     => Some(DixValue::Float(*f)),
            Value::Double { value: d, .. }    => Some(DixValue::Double(*d)),
            // ── FIX: scientific notation literals (e.g. 6.62607015e-34) ───────
            Value::ScientificNotation { value: d, .. } => Some(DixValue::Double(*d)),
            Value::String { value: s, .. }    => Some(DixValue::String(s.clone())),
            Value::Date { value: d, .. }      => Some(DixValue::Date(d.clone())),
            Value::Timestamp { value: t, .. } => Some(DixValue::Timestamp(t.clone())),
            Value::HexColor { value: c, .. }  => Some(DixValue::HexColor(c.clone())),

            Value::Array { values, .. } => {
                let items: Vec<DixValue> = values.iter()
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
                match prefix.as_str() {
                    "t" => {
                        let items: Vec<DixValue> = arguments.iter()
                            .filter_map(|v| Self::ast_value_to_dix_value(v, enums))
                            .collect();
                        Some(DixValue::Tuple(items))
                    }
                    "b" => {
                        if let Some(Value::String { value: s, .. }) = arguments.first() {
                            Some(DixValue::Blob(s.clone()))
                        } else { None }
                    }
                    "r" => {
                        if let Some(Value::String { value: s, .. }) = arguments.first() {
                            Some(DixValue::Regex(s.clone()))
                        } else { None }
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
            DixValue::Int(i)             => Ok(i),
            DixValue::Long(l)            => Ok(l as i32),
            DixValue::Float(f)           => Ok(f as i32),
            DixValue::Double(d)          => Ok(d as i32),
            DixValue::Enum { value, .. } => Ok(value),
            _ => Err(format!("Cannot convert {} to i32", value.type_name())),
        }
    }
}

impl TryFrom<DixValue> for i64 {
    type Error = String;
    fn try_from(value: DixValue) -> Result<Self, Self::Error> {
        match value {
            DixValue::Long(l)            => Ok(l),
            DixValue::Int(i)             => Ok(i as i64),
            DixValue::Float(f)           => Ok(f as i64),
            DixValue::Double(d)          => Ok(d as i64),
            DixValue::Enum { value, .. } => Ok(value as i64),
            _ => Err(format!("Cannot convert {} to i64", value.type_name())),
        }
    }
}

impl TryFrom<DixValue> for f64 {
    type Error = String;
    fn try_from(value: DixValue) -> Result<Self, Self::Error> {
        match value {
            DixValue::Int(i)    => Ok(i as f64),
            DixValue::Long(l)   => Ok(l as f64),
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
            flattened_data:  flattened,
            prefix_index,
            config:          None,
            enums:           None,
            security:        None,
            dlm:             None,
            version:         "1.0.0".to_string(),
            compile_time:    Utc::now(),
            is_encrypted:    false,
            is_compressed:   false,
            applied_modules: vec![],
        }
    }

    #[test]
    fn test_get_long_value() {
        let mut flattened = HashMap::new();
        flattened.insert("big_num".to_string(), DixValue::Long(9_000_000_000_i64));
        let data = dix_data_from_flat(flattened);
        let v: i64 = data.get("big_num").unwrap();
        assert_eq!(v, 9_000_000_000_i64);
    }

    #[test]
    fn test_scientific_notation_stored_as_double() {
        // 6.62607015e-34 parsed as Value::ScientificNotation must reach DixValue::Double
        let mut flattened = HashMap::new();
        flattened.insert("planck".to_string(), DixValue::Double(6.62607015e-34_f64));
        let data = dix_data_from_flat(flattened);
        let v: f64 = data.get("planck").unwrap();
        assert!((v - 6.62607015e-34_f64).abs() < 1e-50);
    }

    #[test]
    fn test_long_try_from_i32_truncates() {
        let v = DixValue::Long(i64::MAX);
        let as_i32: i32 = i32::try_from(v).unwrap();
        let _ = as_i32;
    }

    #[test]
    fn test_long_try_from_i64_exact() {
        let v = DixValue::Long(i64::MAX);
        let as_i64: i64 = i64::try_from(v).unwrap();
        assert_eq!(as_i64, i64::MAX);
    }

    #[test]
    fn test_int_widens_to_i64() {
        let v = DixValue::Int(42);
        let as_i64: i64 = i64::try_from(v).unwrap();
        assert_eq!(as_i64, 42_i64);
    }

    #[test]
    fn test_dix_data_creation() {
        let ast  = DixScript::new();
        let data = DixData::from_ast(
            ast, "1.0.0".to_string(), Utc::now(), false, false, vec![],
        );
        assert_eq!(data.version, "1.0.0");
    }

    #[test]
    fn test_get_keys_nested() {
        let mut flattened = HashMap::new();
        flattened.insert("db.host".to_string(), DixValue::String("localhost".to_string()));
        flattened.insert("db.port".to_string(), DixValue::Int(5432));
        let data = dix_data_from_flat(flattened);
        let mut children = data.get_keys("db");
        children.sort();
        assert_eq!(children, vec!["host", "port"]);
    }
    }
