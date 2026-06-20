use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use crate::Compiler::AST::DixScript;
use super::dix_value::DixValue;

// ─────────────────────────────────────────────────────────────────────────────
// Structural hashmap helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` if `key` is a synthetic flattened child path of `parent` —
/// i.e. `key == parent + "." + ...` or `key == parent + "[" + ...`.
#[inline]
fn is_child_path(key: &str, parent: &str) -> bool {
    key.len() > parent.len()
        && key.starts_with(parent)
        && matches!(key.as_bytes()[parent.len()], b'.' | b'[')
}

/// Filter a fully-flattened hashmap down to its "structural" root entries —
/// entries that are not derived child paths of another entry already present
/// in the map (e.g. `"server.host"` when `"server"` is present as an
/// `Object`, or `"tags[0]"` when `"tags"` is present as an `Array`).
///
/// [`DixData::to_hashmap`] (and `DixConverter::to_hashmap`) both produce a
/// fully-flattened map for O(1) dotted-path access. Feeding that map directly
/// into `DixConverter::from_hashmap` would emit invalid `.mdix` identifiers
/// like `tags[0] = ...` or `server.host = ...` (DixScript identifiers cannot
/// contain `[` or `.`). This filter keeps only the aggregate/root values
/// needed to reconstruct the original structure.
fn filter_structural_keys(map: &HashMap<String, DixValue>) -> HashMap<String, DixValue> {
    let keys: Vec<&String> = map.keys().collect();
    map.iter()
        .filter(|(key, _)| {
            !keys.iter().any(|other| {
                other.as_str() != key.as_str() && is_child_path(key.as_str(), other.as_str())
            })
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

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
            None => Vec::new(),
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

    /// Returns the fully-flattened data map, including synthetic child paths
    /// like `"server.host"` and `"tags[0]"` used for O(1) dotted-path access.
    ///
    /// **Not suitable for round-tripping through `DixConverter::from_hashmap`**
    /// — those synthetic paths are not valid DixScript identifiers on their
    /// own. Use [`to_structural_hashmap`](Self::to_structural_hashmap) instead
    /// for that purpose.
    pub fn to_hashmap(&self) -> HashMap<String, DixValue> {
        self.flattened_data.clone()
    }

    /// Like [`to_hashmap`](Self::to_hashmap), but returns only the
    /// "structural" root entries — synthetic flattened child paths
    /// (`"server.host"`, `"tags[0]"`, nested array/object indices, ...) are
    /// removed, leaving only the aggregate/root `Object` / `Array` / scalar
    /// values needed to reconstruct the original `.mdix` structure.
    ///
    /// Use this when feeding data into [`DixConverter::from_hashmap`] /
    /// [`DixConverter::to_mdix`] — e.g. for `format`, `convert`, or any other
    /// round-trip through `.mdix` source. For the common case of "I already
    /// have a real `DixData` and want faithful `.mdix` back", prefer
    /// `DixConverter::from_dix_data(&self)` instead — it also restores the
    /// real `@CONFIG` and `@ENUMS` sections rather than `from_hashmap`'s
    /// best-effort reconstruction.
    ///
    /// ```rust,ignore
    /// let data = loader.load_text("config.mdix", &DixLoadOptions::new())?;
    /// let map  = data.to_structural_hashmap();
    /// let ast  = converter.from_hashmap(map)?;
    /// let src  = converter.to_mdix(&ast, None)?; // valid .mdix source
    /// ```
    pub fn to_structural_hashmap(&self) -> HashMap<String, DixValue> {
        filter_structural_keys(&self.flattened_data)
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
                    if let Some(dix_val) = super::dix_value::ast_value_to_dix_value(&field.value, None) {
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
        // Build aggregate Object values for any flat keys that have a single-level
        // dot prefix but no parent aggregate yet.  This handles:
        //   1. DixDataBuilder.serialize_at() which writes "server.host" as a
        //      SimpleProperty rather than a TableProperty.
        //   2. Any other path that didn't get an aggregate during entry processing.
        Self::build_missing_prefix_aggregates(result);
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
                if let Some(dix_value) = super::dix_value::ast_value_to_dix_value(value, enums) {
                    Self::flatten_dix_value(&key, &dix_value, result);
                }
            }

            // FIX: TableProperty now also inserts an aggregate DixValue::Object
            // so that data.exists("server") and schema require_object("server")
            // work correctly — not just the individual leaf paths.
            DataEntry::TableProperty { path, properties, .. } => {
                let table_path = Self::build_path(prefix, &path.to_string());
                let mut obj_map = HashMap::new();

                for prop in properties {
                    let key = Self::build_path(&table_path, &prop.name);
                    if let Some(dix_value) = super::dix_value::ast_value_to_dix_value(&prop.value, enums) {
                        obj_map.insert(prop.name.clone(), dix_value.clone());
                        Self::flatten_dix_value(&key, &dix_value, result);
                    }
                }

                // Insert or merge the aggregate Object.
                if !obj_map.is_empty() {
                    match result.entry(table_path) {
                        std::collections::hash_map::Entry::Vacant(e) => {
                            e.insert(DixValue::Object(obj_map));
                        }
                        std::collections::hash_map::Entry::Occupied(mut e) => {
                            // Two TableProperty entries share the same path — merge.
                            if let DixValue::Object(ref mut existing) = e.get_mut() {
                                for (k, v) in obj_map {
                                    existing.entry(k).or_insert(v);
                                }
                            }
                        }
                    }
                }
            }

            DataEntry::GroupArray { path, items, .. } => {
                let array_path = Self::build_path(prefix, &path.to_string());
                let array_values: Vec<DixValue> = items.iter()
                    .filter_map(|v| super::dix_value::ast_value_to_dix_value(v, enums))
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
                        if let Some(dix_value) = super::dix_value::ast_value_to_dix_value(&prop.value, enums) {
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

    /// After all entries have been flattened, scan for leaf keys of the form
    /// `"prefix.field"` (single dot, no `[` in either part) that have no
    /// corresponding aggregate at `"prefix"`.  Build and insert a
    /// `DixValue::Object` aggregate for each such prefix.
    ///
    /// This handles the case where `DixDataBuilder::serialize_at("server", …)`
    /// writes flat `SimpleProperty` entries named `"server.host"`,
    /// `"server.port"`, etc. without going through `with_table_properties`.
    /// After this pass, `data.exists("server")` returns `true` and
    /// `SchemaBuilder::require_object("server")` works correctly.
    fn build_missing_prefix_aggregates(result: &mut HashMap<String, DixValue>) {
        // First pass: collect (prefix → {field → value}) for single-level dotted keys
        // that don't yet have an aggregate.
        let mut prefix_children: HashMap<String, HashMap<String, DixValue>> = HashMap::new();

        for (key, value) in result.iter() {
            if let Some(dot_pos) = key.rfind('.') {
                let prefix = &key[..dot_pos];
                let field  = &key[dot_pos + 1..];
                // Only immediate children: no arrays, no nested dots.
                if !prefix.contains('[')
                    && !field.contains('.')
                    && !field.contains('[')
                    && !result.contains_key(prefix)  // skip if aggregate already exists
                {
                    prefix_children
                        .entry(prefix.to_string())
                        .or_default()
                        .insert(field.to_string(), value.clone());
                }
            }
        }

        // Second pass: insert missing aggregates.
        for (prefix, children) in prefix_children {
            // Use or_insert_with so we never overwrite a freshly-added aggregate
            // that appeared between our two passes (shouldn't happen, but safe).
            result.entry(prefix).or_insert_with(|| DixValue::Object(children));
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

    /// Render a `ConfigValue` to its raw string form for `DixData::config`.
    ///
    /// **FIX**: previously this had a catch-all `_ => String::new()` arm that
    /// silently dropped `Features`, `ErrorHandling`, `Compatibility`, and
    /// `Debug` config values to an empty string — e.g.
    /// `@CONFIG(error_handling -> "recover")` would read back as
    /// `data.config["error_handling"] == ""`. Every `ConfigValue` variant is
    /// now handled explicitly (and the match is exhaustive, so a future new
    /// variant fails to compile here instead of silently going blank again).
    /// Note this intentionally returns the *raw* value (`"recover"`, not
    /// `"\"recover\""`) — `DixConverter::format_config_value` is the
    /// quote-wrapping counterpart used when re-emitting `.mdix` source.
    fn config_value_to_string(value: &crate::Compiler::AST::ConfigValue) -> String {
        use crate::Compiler::AST::ConfigValue;
        match value {
            ConfigValue::String(s)         => s.clone(),
            ConfigValue::Integer(i)        => i.to_string(),
            ConfigValue::Float(f)          => f.to_string(),
            ConfigValue::Boolean(b)        => b.to_string(),
            ConfigValue::Date(d)           => d.clone(),
            ConfigValue::Timestamp(t)      => t.clone(),
            ConfigValue::Features(feats)   => feats.join(","),
            ConfigValue::ErrorHandling(eh) => eh.to_string(),
            ConfigValue::Compatibility(cm) => cm.to_string(),
            ConfigValue::Debug(dm)         => dm.to_string(),
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

impl TryFrom<DixValue> for f32 {
    type Error = String;
    fn try_from(value: DixValue) -> Result<Self, Self::Error> {
        match value {
            DixValue::Float(f)  => Ok(f),
            DixValue::Int(i)    => Ok(i as f32),
            DixValue::Long(l)   => Ok(l as f32),
            DixValue::Double(d) => Ok(d as f32),
            _ => Err(format!("Cannot convert {} to f32", value.type_name())),
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
            DixValue::Array(arr)  => Ok(arr),
            DixValue::Tuple(items) => Ok(items),
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler::AST::*;

    // ── helpers ───────────────────────────────────────────────────────────────

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

    fn ast_with_table_prop(path_segs: &[&str], props: Vec<(&str, Value)>) -> DixScript {
        let path = TablePath::new(path_segs.iter().map(|s| s.to_string()).collect());
        let property_assignments = props.into_iter().map(|(name, value)| {
            PropertyAssignment::new(name.to_string(), None, value, Position::UNKNOWN)
        }).collect();
        let entry = DataEntry::TableProperty { path, properties: property_assignments, position: Position::UNKNOWN };
        DixScript {
            data: Some(DataSection::new(vec![entry], Position::UNKNOWN)),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        }
    }

    fn int_val(n: i32) -> Value { Value::Integer { value: n, position: Position::UNKNOWN } }
    fn str_val(s: &str) -> Value { Value::String { value: s.into(), position: Position::UNKNOWN } }
    fn bool_val(b: bool) -> Value { Value::Boolean { value: b, position: Position::UNKNOWN } }
    fn long_val(l: i64) -> Value { Value::Long { value: l, position: Position::UNKNOWN } }

    // ── TableProperty aggregate fix ───────────────────────────────────────────

    #[test]
    fn test_table_property_creates_aggregate_exists() {
        let ast = ast_with_table_prop(
            &["server"],
            vec![("host", str_val("localhost")), ("port", int_val(8080))],
        );
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        // Leaf paths still work
        assert!(data.exists("server.host"), "server.host missing");
        assert!(data.exists("server.port"), "server.port missing");
        // FIX: aggregate must also be accessible
        assert!(data.exists("server"), "aggregate 'server' missing after fix");
    }

    #[test]
    fn test_table_property_aggregate_is_object() {
        let ast = ast_with_table_prop(
            &["server"],
            vec![("host", str_val("localhost")), ("port", int_val(8080))],
        );
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        let obj: HashMap<String, DixValue> = data.get("server").expect("server should be Object");
        assert_eq!(obj.get("host"), Some(&DixValue::String("localhost".into())));
        assert_eq!(obj.get("port"), Some(&DixValue::Int(8080)));
    }

    #[test]
    fn test_table_property_aggregate_partial_read() {
        let ast = ast_with_table_prop(
            &["db"],
            vec![("host", str_val("db.local")), ("port", int_val(5432)), ("ssl", bool_val(true))],
        );
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        let host: String = data.get("db.host").unwrap();
        let port: i32    = data.get("db.port").unwrap();
        let ssl: bool    = data.get("db.ssl").unwrap();
        assert_eq!(host, "db.local");
        assert_eq!(port, 5432);
        assert!(ssl);

        // Aggregate is an Object with all three fields.
        let obj: HashMap<String, DixValue> = data.get("db").unwrap();
        assert_eq!(obj.len(), 3);
    }

    #[test]
    fn test_two_table_properties_same_path_aggregate_merged() {
        // Two TableProperty entries sharing path "db" should merge into one Object.
        let path1 = TablePath::new(vec!["db".into()]);
        let path2 = TablePath::new(vec!["db".into()]);
        let e1 = DataEntry::TableProperty {
            path: path1,
            properties: vec![PropertyAssignment::new("host".into(), None, str_val("localhost"), Position::UNKNOWN)],
            position: Position::UNKNOWN,
        };
        let e2 = DataEntry::TableProperty {
            path: path2,
            properties: vec![PropertyAssignment::new("port".into(), None, int_val(5432), Position::UNKNOWN)],
            position: Position::UNKNOWN,
        };
        let ast = DixScript {
            data: Some(DataSection::new(vec![e1, e2], Position::UNKNOWN)),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        };
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        let obj: HashMap<String, DixValue> = data.get("db").unwrap();
        assert!(obj.contains_key("host"), "merged obj missing host");
        assert!(obj.contains_key("port"), "merged obj missing port");
    }

    #[test]
    fn test_nested_table_path_aggregate() {
        let ast = ast_with_table_prop(
            &["my", "me", "mo"],
            vec![("value", int_val(42))],
        );
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        assert!(data.exists("my.me.mo.value"), "leaf missing");
        assert!(data.exists("my.me.mo"), "aggregate missing");
        let v: i32 = data.get("my.me.mo.value").unwrap();
        assert_eq!(v, 42);
    }

    // ── build_missing_prefix_aggregates (serialize_at path) ──────────────────

    #[test]
    fn test_dotted_simple_property_aggregate_built() {
        // SimpleProperty with dotted name (as written by DixDataBuilder.serialize_at)
        let entry = DataEntry::SimpleProperty {
            name:      "server.host".into(),
            data_type: None,
            value:     str_val("localhost"),
            position:  Position::UNKNOWN,
        };
        let ast = DixScript {
            data: Some(DataSection::new(vec![entry], Position::UNKNOWN)),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        };
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        // Leaf path always works.
        assert!(data.exists("server.host"), "leaf missing");
        // After build_missing_prefix_aggregates the aggregate should exist too.
        assert!(data.exists("server"), "aggregate missing for dotted SimpleProperty");
    }

    #[test]
    fn test_multiple_dotted_simple_properties_build_aggregate() {
        let entries = vec![
            DataEntry::SimpleProperty { name: "app.name".into(), data_type: None, value: str_val("MyApp"), position: Position::UNKNOWN },
            DataEntry::SimpleProperty { name: "app.port".into(), data_type: None, value: int_val(8080),   position: Position::UNKNOWN },
            DataEntry::SimpleProperty { name: "app.debug".into(), data_type: None, value: bool_val(true), position: Position::UNKNOWN },
        ];
        let ast = DixScript {
            data: Some(DataSection::new(entries, Position::UNKNOWN)),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        };
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        assert!(data.exists("app"), "aggregate missing");
        let obj: HashMap<String, DixValue> = data.get("app").unwrap();
        assert_eq!(obj.len(), 3);
        assert_eq!(obj.get("name"), Some(&DixValue::String("MyApp".into())));
    }

    // ── Long ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_get_long_value() {
        let mut flat = HashMap::new();
        flat.insert("big".to_string(), DixValue::Long(9_000_000_000_i64));
        let data = dix_data_from_flat(flat);
        let v: i64 = data.get("big").unwrap();
        assert_eq!(v, 9_000_000_000_i64);
    }

    #[test]
    fn test_long_widens_to_i64_from_int() {
        let v = DixValue::Int(42);
        let as_i64: i64 = i64::try_from(v).unwrap();
        assert_eq!(as_i64, 42_i64);
    }

    #[test]
    fn test_long_truncates_to_i32() {
        let v = DixValue::Long(i64::MAX);
        let _: i32 = i32::try_from(v).unwrap(); // truncation is expected
    }

    #[test]
    fn test_f32_try_from_float() {
        let v = DixValue::Float(3.14_f32);
        let f: f32 = f32::try_from(v).unwrap();
        assert!((f - 3.14_f32).abs() < 1e-6);
    }

    #[test]
    fn test_f32_try_from_int() {
        let v = DixValue::Int(42);
        let f: f32 = f32::try_from(v).unwrap();
        assert_eq!(f, 42.0_f32);
    }

    // ── GroupArray (existing behaviour confirmed) ─────────────────────────────

    #[test]
    fn test_group_array_aggregate_exists() {
        let path = TablePath::new(vec!["scores".into()]);
        let entry = DataEntry::GroupArray {
            path,
            items: vec![int_val(10), int_val(20), int_val(30)],
            position: Position::UNKNOWN,
        };
        let ast = DixScript {
            data: Some(DataSection::new(vec![entry], Position::UNKNOWN)),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        };
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        assert!(data.exists("scores"),    "aggregate missing");
        assert!(data.exists("scores[0]"), "indexed element missing");
        let v: i32 = data.get("scores[0]").unwrap();
        assert_eq!(v, 10);
    }

    // ── prefix index ─────────────────────────────────────────────────────────

    #[test]
    fn test_get_keys_nested() {
        let mut flat = HashMap::new();
        flat.insert("db.host".to_string(), DixValue::String("localhost".into()));
        flat.insert("db.port".to_string(), DixValue::Int(5432));
        let data = dix_data_from_flat(flat);
        let mut children = data.get_keys("db");
        children.sort();
        assert_eq!(children, vec!["host", "port"]);
    }

    #[test]
    fn test_dix_data_creation() {
        let ast  = DixScript::new();
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);
        assert_eq!(data.version, "1.0.0");
    }

    #[test]
    fn test_scientific_notation_stored_as_double() {
        let mut flat = HashMap::new();
        flat.insert("planck".to_string(), DixValue::Double(6.62607015e-34_f64));
        let data = dix_data_from_flat(flat);
        let v: f64 = data.get("planck").unwrap();
        assert!((v - 6.62607015e-34_f64).abs() < 1e-50);
    }

    // ── to_structural_hashmap (Group D fix) ───────────────────────────────────

    #[test]
    fn test_structural_hashmap_filters_table_property_children() {
        let ast = ast_with_table_prop(
            &["server"],
            vec![("host", str_val("localhost")), ("port", int_val(8080))],
        );
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        // Sanity: the fully-flattened map contains the synthetic children.
        let flat = data.to_hashmap();
        assert!(flat.contains_key("server.host"));
        assert!(flat.contains_key("server.port"));
        assert!(flat.contains_key("server"));

        let structural = data.to_structural_hashmap();
        assert!(!structural.contains_key("server.host"), "synthetic child 'server.host' leaked through");
        assert!(!structural.contains_key("server.port"), "synthetic child 'server.port' leaked through");
        assert!(structural.contains_key("server"), "aggregate 'server' missing");

        match structural.get("server") {
            Some(DixValue::Object(obj)) => {
                assert_eq!(obj.get("host"), Some(&DixValue::String("localhost".into())));
                assert_eq!(obj.get("port"), Some(&DixValue::Int(8080)));
            }
            other => panic!("expected Object for 'server', got {:?}", other),
        }
    }

    #[test]
    fn test_structural_hashmap_filters_group_array_indices() {
        let path = TablePath::new(vec!["tags".into()]);
        let entry = DataEntry::GroupArray {
            path,
            items: vec![str_val("alpha"), str_val("beta"), str_val("gamma")],
            position: Position::UNKNOWN,
        };
        let ast = DixScript {
            data: Some(DataSection::new(vec![entry], Position::UNKNOWN)),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        };
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        let flat = data.to_hashmap();
        assert!(flat.contains_key("tags[0]"));
        assert!(flat.contains_key("tags[1]"));
        assert!(flat.contains_key("tags[2]"));

        let structural = data.to_structural_hashmap();
        assert!(!structural.contains_key("tags[0]"), "synthetic index 'tags[0]' leaked through");
        assert!(!structural.contains_key("tags[1]"), "synthetic index 'tags[1]' leaked through");
        assert!(!structural.contains_key("tags[2]"), "synthetic index 'tags[2]' leaked through");
        assert!(structural.contains_key("tags"), "aggregate 'tags' missing");

        match structural.get("tags") {
            Some(DixValue::Array(items)) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], DixValue::String("alpha".into()));
            }
            other => panic!("expected Array for 'tags', got {:?}", other),
        }
    }

    #[test]
    fn test_structural_hashmap_preserves_unrelated_keys() {
        // "matrix" and "matrix2" must not be confused — "matrix2" is not a
        // child path of "matrix" even though it shares a string prefix.
        let mut flat = HashMap::new();
        flat.insert("matrix".to_string(),  DixValue::Array(vec![DixValue::Int(1), DixValue::Int(2)]));
        flat.insert("matrix[0]".to_string(), DixValue::Int(1));
        flat.insert("matrix[1]".to_string(), DixValue::Int(2));
        flat.insert("matrix2".to_string(), DixValue::Int(99));
        let data = dix_data_from_flat(flat);

        let structural = data.to_structural_hashmap();
        assert!(structural.contains_key("matrix"));
        assert!(structural.contains_key("matrix2"), "unrelated key 'matrix2' incorrectly filtered");
        assert!(!structural.contains_key("matrix[0]"));
        assert!(!structural.contains_key("matrix[1]"));
    }

    #[test]
    fn test_structural_hashmap_nested_object_in_array() {
        // servers:: { host = "a", port = 1 }, { host = "b", port = 2 }
        let item = |host: &str, port: i32| Value::Object {
            properties: vec![
                ObjectProperty::new("host".into(), str_val(host), Position::UNKNOWN),
                ObjectProperty::new("port".into(), int_val(port), Position::UNKNOWN),
            ],
            position: Position::UNKNOWN,
        };

        let entry = DataEntry::GroupArray {
            path: TablePath::new(vec!["servers".into()]),
            items: vec![item("a.local", 1), item("b.local", 2)],
            position: Position::UNKNOWN,
        };
        let ast = DixScript {
            data: Some(DataSection::new(vec![entry], Position::UNKNOWN)),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        };
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);

        // Sanity on fully-flattened map.
        assert!(data.exists("servers[0].host"));
        assert!(data.exists("servers[1].port"));

        let structural = data.to_structural_hashmap();
        assert!(structural.contains_key("servers"), "aggregate 'servers' missing");
        assert!(!structural.contains_key("servers[0]"));
        assert!(!structural.contains_key("servers[0].host"));
        assert!(!structural.contains_key("servers[1]"));
        assert!(!structural.contains_key("servers[1].port"));

        match structural.get("servers") {
            Some(DixValue::Array(items)) => {
                assert_eq!(items.len(), 2);
                match &items[0] {
                    DixValue::Object(obj) => {
                        assert_eq!(obj.get("host"), Some(&DixValue::String("a.local".into())));
                        assert_eq!(obj.get("port"), Some(&DixValue::Int(1)));
                    }
                    other => panic!("expected Object element, got {:?}", other),
                }
            }
            other => panic!("expected Array for 'servers', got {:?}", other),
        }
    }

    #[test]
    fn test_structural_hashmap_flat_scalars_unaffected() {
        let mut flat = HashMap::new();
        flat.insert("name".to_string(), DixValue::String("MyApp".into()));
        flat.insert("port".to_string(), DixValue::Int(8080));
        let data = dix_data_from_flat(flat);

        let structural = data.to_structural_hashmap();
        assert_eq!(structural.len(), 2);
        assert_eq!(structural.get("name"), Some(&DixValue::String("MyApp".into())));
        assert_eq!(structural.get("port"), Some(&DixValue::Int(8080)));
    }

    // ── config_value_to_string fix ─────────────────────────────────────────────

    #[test]
    fn test_config_features_value_survives() {
        let ast = DixScript {
            config: Some(ConfigSection {
                entries: vec![ConfigEntry {
                    key: "features".into(),
                    value: ConfigValue::Features(vec!["foo".into(), "bar".into()]),
                    position: Position::UNKNOWN,
                }],
                position: Position::UNKNOWN,
            }),
            imports: None, dlm: None, enums: None, quick_functions: None,
            data: None, security: None,
        };
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);
        let cfg = data.config.unwrap();
        assert_eq!(cfg.get("features").map(String::as_str), Some("foo,bar"));
    }

    #[test]
    fn test_config_error_handling_compatibility_debug_values_survive() {
        let ast = DixScript {
            config: Some(ConfigSection {
                entries: vec![
                    ConfigEntry {
                        key: "error_handling".into(),
                        value: ConfigValue::ErrorHandling(ErrorHandlingStrategy::Recover),
                        position: Position::UNKNOWN,
                    },
                    ConfigEntry {
                        key: "compatibility".into(),
                        value: ConfigValue::Compatibility(CompatibilityMode::BestEffort),
                        position: Position::UNKNOWN,
                    },
                    ConfigEntry {
                        key: "debug".into(),
                        value: ConfigValue::Debug(DebugMode::Verbose),
                        position: Position::UNKNOWN,
                    },
                ],
                position: Position::UNKNOWN,
            }),
            imports: None, dlm: None, enums: None, quick_functions: None,
            data: None, security: None,
        };
        let data = DixData::from_ast(ast, "1.0.0".into(), Utc::now(), false, false, vec![]);
        let cfg = data.config.unwrap();
        // FIX: previously all three of these came back as "" via the `_ =>
        // String::new()` catch-all.
        assert_eq!(cfg.get("error_handling").map(String::as_str), Some("recover"));
        assert_eq!(cfg.get("compatibility").map(String::as_str), Some("best_effort"));
        assert_eq!(cfg.get("debug").map(String::as_str), Some("verbose"));
    }
}
