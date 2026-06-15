
use std::collections::HashMap;
use crate::Compiler::AST::{
    DixScript, ConfigSection, ConfigEntry, ConfigValue,
    DataSection, DataEntry, Value, PropertyAssignment,
    TablePath, ObjectProperty, EnumDeclaration, EnumField,
    EnumsSection, Position,
};
use super::dix_value::DixValue;
use super::format_options::DixFormatOptions;

// ─────────────────────────────────────────────────────────────────────────────
// Structural hashmap helpers (Group D fix)
// ─────────────────────────────────────────────────────────────────────────────
//
// `DixData::to_hashmap()` returns a fully-flattened map for O(1) dotted-path
// access: it contains BOTH aggregate keys (`"server"` -> Object, `"tags"` ->
// Array) AND synthetic child paths (`"server.host"` -> String, `"tags[0]"` ->
// String). DixScript identifiers cannot contain `.` or `[`, so if those
// synthetic child paths are bucketed as top-level `SimpleProperty` entries by
// `from_hashmap`, `to_mdix` emits invalid source like `tags[0] = "web"` or
// `server.host = "..."`.
//
// `filter_structural_keys` removes any key that is a derived child path of
// another key already present in the map, leaving only the aggregate/root
// entries needed to reconstruct the original structure. This makes
// `from_hashmap` safe to call with either `DixData::to_hashmap()` (fully
// flattened) or `DixData::to_structural_hashmap()` (already filtered) —
// filtering an already-structural map is a no-op.

/// Returns `true` if `key` is a synthetic flattened child path of `parent` —
/// i.e. `key == parent + "." + ...` or `key == parent + "[" + ...`.
#[inline]
fn is_child_path(key: &str, parent: &str) -> bool {
    key.len() > parent.len()
        && key.starts_with(parent)
        && matches!(key.as_bytes()[parent.len()], b'.' | b'[')
}

/// Filter a hashmap down to its "structural" root entries — entries that are
/// not derived child paths of another entry already present in the map.
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

    // ── from_hashmap ──────────────────────────────────────────────────────────

    /// Convert a `HashMap<String, DixValue>` into a `DixScript` AST.
    ///
    /// **Group D fix**: the input map is first passed through
    /// [`filter_structural_keys`] so that synthetic flattened child paths
    /// (e.g. `"server.host"` when `"server"` is present as an `Object`, or
    /// `"tags[0]"` when `"tags"` is present as an `Array`) are removed before
    /// bucketing into flat properties vs. nested structures. Without this,
    /// `DixData::to_hashmap()`'s fully-flattened output would produce invalid
    /// `.mdix` identifiers like `tags[0] = ...` or `server.host = ...`.
    ///
    /// Callers passing an already-structural map (e.g.
    /// `DixData::to_structural_hashmap()`, or a hand-built map with no
    /// synthetic child keys) are unaffected — filtering is a no-op in that case.
    pub fn from_hashmap(&self, data: HashMap<String, DixValue>) -> Result<DixScript, String> {
        let data = filter_structural_keys(&data);

        let mut flat_properties:   HashMap<String, DixValue> = HashMap::new();
        let mut nested_structures: HashMap<String, DixValue> = HashMap::new();

        for (key, value) in data {
            if matches!(value, DixValue::Object(_) | DixValue::Array(_)) {
                nested_structures.insert(key, value);
            } else {
                flat_properties.insert(key, value);
            }
        }

        let mut data_entries = Vec::new();

        for (key, value) in flat_properties {
            let ast_value = self.convert_dix_value_to_ast_value(&value)?;
            data_entries.push(DataEntry::SimpleProperty {
                name: key, data_type: None, value: ast_value, position: Position::UNKNOWN,
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
            config:          config_section,
            imports:         None,
            dlm:             None,
            enums:           None,
            quick_functions: None,
            data:            data_section,
            security:        None,
        })
    }

    // ── to_hashmap ────────────────────────────────────────────────────────────

    pub fn to_hashmap(&self, ast: &DixScript) -> HashMap<String, DixValue> {
        let mut result = HashMap::new();
        let enums      = self.extract_enums(ast);

        if let Some(ref data) = ast.data {
            for entry in &data.entries {
                self.flatten_entry(entry, "", &mut result, enums.as_ref());
            }
        }
        result
    }

    // ── to_mdix ───────────────────────────────────────────────────────────────

    pub fn to_mdix(&self, ast: &DixScript, options: Option<&DixFormatOptions>) -> Result<String, String> {
        let opts   = options.unwrap_or(&self.default_options);
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

    // ── JSON export ───────────────────────────────────────────────────────────

    pub fn to_json(&self, ast: &DixScript, pretty: bool) -> Result<String, String> {
        let json_value = self.ast_to_json_value(ast)?;
        if pretty {
            serde_json::to_string_pretty(&json_value)
                .map_err(|e| format!("JSON serialization failed: {}", e))
        } else {
            serde_json::to_string(&json_value)
                .map_err(|e| format!("JSON serialization failed: {}", e))
        }
    }

    fn ast_to_json_value(&self, ast: &DixScript) -> Result<serde_json::Value, String> {
        let enums = self.extract_enums(ast);
        let mut root = serde_json::Map::new();

        if let Some(ref data) = ast.data {
            for entry in &data.entries {
                self.insert_entry_into_json(&mut root, entry, enums.as_ref())?;
            }
        }

        Ok(serde_json::Value::Object(root))
    }

    fn insert_entry_into_json(
        &self,
        root: &mut serde_json::Map<String, serde_json::Value>,
        entry: &DataEntry,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) -> Result<(), String> {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let dv = self.convert_ast_value_to_dix_value(value, enums)
                    .unwrap_or(DixValue::Null);
                root.insert(name.clone(), self.dix_value_to_json_value(&dv));
            }

            DataEntry::TableProperty { path, properties, .. } => {
                let mut props = serde_json::Map::new();
                for prop in properties {
                    let dv = self.convert_ast_value_to_dix_value(&prop.value, enums)
                        .unwrap_or(DixValue::Null);
                    props.insert(prop.name.clone(), self.dix_value_to_json_value(&dv));
                }
                Self::insert_nested_json(
                    root,
                    &path.segments,
                    serde_json::Value::Object(props),
                );
            }

            DataEntry::GroupArray { path, items, .. } => {
                let arr: Vec<serde_json::Value> = items.iter()
                    .map(|v| {
                        let dv = self.convert_ast_value_to_dix_value(v, enums)
                            .unwrap_or(DixValue::Null);
                        self.dix_value_to_json_value(&dv)
                    })
                    .collect();
                Self::insert_nested_json(
                    root,
                    &path.segments,
                    serde_json::Value::Array(arr),
                );
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let dv = self.convert_ast_value_to_dix_value(object, enums)
                    .unwrap_or(DixValue::Null);
                root.insert(name.clone(), self.dix_value_to_json_value(&dv));
            }
        }
        Ok(())
    }

    fn insert_nested_json(
        root: &mut serde_json::Map<String, serde_json::Value>,
        segments: &[String],
        value: serde_json::Value,
    ) {
        if segments.is_empty() { return; }

        if segments.len() == 1 {
            let key = &segments[0];
            match (root.get_mut(key), &value) {
                (Some(serde_json::Value::Object(existing)), serde_json::Value::Object(_)) => {
                    if let serde_json::Value::Object(new_map) = value {
                        for (k, v) in new_map {
                            existing.insert(k, v);
                        }
                    }
                }
                _ => { root.insert(key.clone(), value); }
            }
            return;
        }

        let key = segments[0].clone();
        let child = root
            .entry(key)
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

        if let serde_json::Value::Object(ref mut map) = child {
            Self::insert_nested_json(map, &segments[1..], value);
        }
    }

    fn dix_value_to_json_value(&self, value: &DixValue) -> serde_json::Value {
        match value {
            DixValue::Null => serde_json::Value::Null,

            DixValue::Bool(b) => serde_json::Value::Bool(*b),

            DixValue::Int(i) => serde_json::Value::Number((*i).into()),

            DixValue::Long(l) => serde_json::Value::Number((*l).into()),

            DixValue::Float(f) => serde_json::Number::from_f64(*f as f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),

            // Double — includes values produced by Value::ScientificNotation.
            // serde_json::Number::from_f64 returns None ONLY for NaN and Inf;
            // valid finite doubles (6.62607015e-34, 6.02214076e23, etc.) always
            // produce a JSON number, never null.
            DixValue::Double(d) => serde_json::Number::from_f64(*d)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),

            DixValue::String(s)
            | DixValue::Date(s)
            | DixValue::Timestamp(s)
            | DixValue::HexColor(s)
            | DixValue::Blob(s)
            | DixValue::Regex(s) => serde_json::Value::String(s.clone()),

            DixValue::Array(arr) => serde_json::Value::Array(
                arr.iter().map(|v| self.dix_value_to_json_value(v)).collect(),
            ),

            DixValue::Object(obj) => {
                let map: serde_json::Map<String, serde_json::Value> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), self.dix_value_to_json_value(v)))
                    .collect();
                serde_json::Value::Object(map)
            }

            DixValue::Tuple(items) => serde_json::Value::Array(
                items.iter().map(|v| self.dix_value_to_json_value(v)).collect(),
            ),

            DixValue::Enum { value, .. } => serde_json::Value::Number((*value).into()),
        }
    }

    // ── JSON import ───────────────────────────────────────────────────────────

    pub fn from_json(&self, json_str: &str) -> Result<DixScript, String> {
        let json_value: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| format!("JSON parse failed: {}", e))?;
        let map = self.json_value_to_hashmap(json_value)?;
        self.from_hashmap(map)
    }

    fn json_value_to_hashmap(
        &self, value: serde_json::Value,
    ) -> Result<HashMap<String, DixValue>, String> {
        match value {
            serde_json::Value::Object(map) => {
                let mut result = HashMap::with_capacity(map.len());
                for (k, v) in map { result.insert(k, self.json_value_to_dix_value(v)?); }
                Ok(result)
            }
            other => Err(format!("Expected a JSON object at the top level, got: {}", other)),
        }
    }

    fn json_value_to_dix_value(&self, value: serde_json::Value) -> Result<DixValue, String> {
        Ok(match value {
            serde_json::Value::Null      => DixValue::Null,
            serde_json::Value::Bool(b)   => DixValue::Bool(b),
            serde_json::Value::String(s) => DixValue::String(s),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                        DixValue::Int(i as i32)
                    } else {
                        DixValue::Long(i)
                    }
                } else if let Some(f) = n.as_f64() {
                    DixValue::Double(f)
                } else {
                    return Err(format!("Cannot convert JSON number {} to DixValue", n));
                }
            }
            serde_json::Value::Array(arr) => {
                let items: Result<Vec<DixValue>, String> = arr.into_iter()
                    .map(|v| self.json_value_to_dix_value(v))
                    .collect();
                DixValue::Array(items?)
            }
            serde_json::Value::Object(map) => {
                let mut obj = HashMap::with_capacity(map.len());
                for (k, v) in map { obj.insert(k, self.json_value_to_dix_value(v)?); }
                DixValue::Object(obj)
            }
        })
    }

    // ── TOML export ───────────────────────────────────────────────────────────

    pub fn to_toml(&self, ast: &DixScript) -> Result<String, String> {
        let toml_value = self.ast_to_toml_value(ast)?;
        toml::to_string_pretty(&toml_value)
            .map_err(|e| format!("TOML serialization failed: {}", e))
    }

    fn ast_to_toml_value(&self, ast: &DixScript) -> Result<toml::Value, String> {
        let enums = self.extract_enums(ast);
        let mut root = toml::map::Map::new();

        if let Some(ref data) = ast.data {
            for entry in &data.entries {
                self.insert_entry_into_toml(&mut root, entry, enums.as_ref())?;
            }
        }

        Ok(toml::Value::Table(root))
    }

    fn insert_entry_into_toml(
        &self,
        root: &mut toml::map::Map<String, toml::Value>,
        entry: &DataEntry,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) -> Result<(), String> {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let dv = self.convert_ast_value_to_dix_value(value, enums)
                    .unwrap_or(DixValue::Null);
                if let Some(tv) = self.dix_value_to_toml_value(&dv) {
                    root.insert(name.clone(), tv);
                }
            }

            DataEntry::TableProperty { path, properties, .. } => {
                let mut props = toml::map::Map::new();
                for prop in properties {
                    let dv = self.convert_ast_value_to_dix_value(&prop.value, enums)
                        .unwrap_or(DixValue::Null);
                    if let Some(tv) = self.dix_value_to_toml_value(&dv) {
                        props.insert(prop.name.clone(), tv);
                    }
                }
                Self::insert_nested_toml(
                    root,
                    &path.segments,
                    toml::Value::Table(props),
                );
            }

            DataEntry::GroupArray { path, items, .. } => {
                let arr: Vec<toml::Value> = items
                    .iter()
                    .filter_map(|v| {
                        let dv = self.convert_ast_value_to_dix_value(v, enums)
                            .unwrap_or(DixValue::Null);
                        self.dix_value_to_toml_value(&dv)
                    })
                    .collect();
                Self::insert_nested_toml(
                    root,
                    &path.segments,
                    toml::Value::Array(arr),
                );
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let dv = self.convert_ast_value_to_dix_value(object, enums)
                    .unwrap_or(DixValue::Null);
                if let Some(tv) = self.dix_value_to_toml_value(&dv) {
                    root.insert(name.clone(), tv);
                }
            }
        }
        Ok(())
    }

    fn insert_nested_toml(
        root: &mut toml::map::Map<String, toml::Value>,
        segments: &[String],
        value: toml::Value,
    ) {
        if segments.is_empty() { return; }

        if segments.len() == 1 {
            let key = &segments[0];
            match (root.get_mut(key), &value) {
                (Some(toml::Value::Table(existing)), toml::Value::Table(_)) => {
                    if let toml::Value::Table(new_map) = value {
                        for (k, v) in new_map {
                            existing.insert(k, v);
                        }
                    }
                }
                _ => { root.insert(key.clone(), value); }
            }
            return;
        }

        let key = segments[0].clone();
        let child = root
            .entry(key)
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));

        if let toml::Value::Table(ref mut map) = child {
            Self::insert_nested_toml(map, &segments[1..], value);
        }
    }

    fn dix_value_to_toml_value(&self, value: &DixValue) -> Option<toml::Value> {
        match value {
            DixValue::Null         => None,
            DixValue::Bool(b)      => Some(toml::Value::Boolean(*b)),
            DixValue::Int(i)       => Some(toml::Value::Integer(*i as i64)),
            DixValue::Long(l)      => Some(toml::Value::Integer(*l)),
            // Double — finite small/large doubles (from ScientificNotation) are valid TOML floats.
            DixValue::Float(f)     => Some(toml::Value::Float(*f as f64)),
            DixValue::Double(d)    => Some(toml::Value::Float(*d)),
            DixValue::String(s)    => Some(toml::Value::String(s.clone())),
            DixValue::Date(d)      => Some(toml::Value::String(d.clone())),
            DixValue::Timestamp(t) => Some(toml::Value::String(t.clone())),
            DixValue::HexColor(c)  => Some(toml::Value::String(c.clone())),
            DixValue::Blob(b)      => Some(toml::Value::String(b.clone())),
            DixValue::Regex(r)     => Some(toml::Value::String(r.clone())),
            DixValue::Enum { value, .. } => Some(toml::Value::Integer(*value as i64)),
            DixValue::Array(arr) => {
                let items: Vec<toml::Value> = arr.iter()
                    .filter_map(|v| self.dix_value_to_toml_value(v))
                    .collect();
                Some(toml::Value::Array(items))
            }
            DixValue::Object(obj) => {
                let mut table = toml::map::Map::new();
                for (k, v) in obj {
                    if let Some(tv) = self.dix_value_to_toml_value(v) {
                        table.insert(k.clone(), tv);
                    }
                }
                Some(toml::Value::Table(table))
            }
            DixValue::Tuple(items) => {
                let arr: Vec<toml::Value> = items.iter()
                    .filter_map(|v| self.dix_value_to_toml_value(v))
                    .collect();
                Some(toml::Value::Array(arr))
            }
        }
    }

    // ── TOML import ───────────────────────────────────────────────────────────

    pub fn from_toml(&self, toml_str: &str) -> Result<DixScript, String> {
        let toml_value: toml::Value = toml::from_str(toml_str)
            .map_err(|e| format!("TOML parse failed: {}", e))?;
        let map = self.toml_value_to_hashmap(toml_value)?;
        self.from_hashmap(map)
    }

    fn toml_value_to_hashmap(
        &self, value: toml::Value,
    ) -> Result<HashMap<String, DixValue>, String> {
        match value {
            toml::Value::Table(table) => {
                let mut result = HashMap::with_capacity(table.len());
                for (k, v) in table { result.insert(k, self.toml_value_to_dix_value(v)?); }
                Ok(result)
            }
            other => Err(format!(
                "Expected a TOML table at the top level, got type: {}", other.type_str()
            )),
        }
    }

    fn toml_value_to_dix_value(&self, value: toml::Value) -> Result<DixValue, String> {
        Ok(match value {
            toml::Value::String(s)   => DixValue::String(s),
            toml::Value::Integer(i)  => {
                if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                    DixValue::Int(i as i32)
                } else {
                    DixValue::Long(i)
                }
            }
            toml::Value::Float(f)    => DixValue::Double(f),
            toml::Value::Boolean(b)  => DixValue::Bool(b),
            toml::Value::Datetime(d) => DixValue::Timestamp(d.to_string()),
            toml::Value::Array(arr)  => {
                let items: Result<Vec<DixValue>, String> = arr.into_iter()
                    .map(|v| self.toml_value_to_dix_value(v))
                    .collect();
                DixValue::Array(items?)
            }
            toml::Value::Table(table) => {
                let mut obj = HashMap::with_capacity(table.len());
                for (k, v) in table { obj.insert(k, self.toml_value_to_dix_value(v)?); }
                DixValue::Object(obj)
            }
        })
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn extract_enums(
        &self, ast: &DixScript,
    ) -> Option<HashMap<String, HashMap<String, i32>>> {
        ast.enums.as_ref().map(|section| {
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

    fn process_nested_structure(
        &self, key: &str, value: &DixValue,
        entries: &mut Vec<DataEntry>, parent_path: &str,
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
                let mut properties: Vec<PropertyAssignment> = Vec::new();
                let mut nested: Vec<(String, DixValue)>     = Vec::new();

                for (k, v) in obj {
                    if matches!(v, DixValue::Object(_) | DixValue::Array(_)) {
                        nested.push((k.clone(), v.clone()));
                    } else {
                        let ast_value = self.convert_dix_value_to_ast_value(v)?;
                        properties.push(PropertyAssignment {
                            name: k.clone(), data_type: None, value: ast_value,
                            position: Position::UNKNOWN,
                        });
                    }
                }

                if !properties.is_empty() {
                    entries.push(DataEntry::TableProperty {
                        path, properties, position: Position::UNKNOWN,
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
                let items: Result<Vec<Value>, String> = arr.iter()
                    .map(|v| self.convert_dix_value_to_ast_value(v))
                    .collect();
                entries.push(DataEntry::GroupArray {
                    path, items: items?, position: Position::UNKNOWN,
                });
            }
            other => {
                return Err(format!(
                    "Expected object or array for nested structure, got: {}",
                    other.type_name()
                ));
            }
        }

        Ok(())
    }

    /// Convert an AST Value node to a Runtime DixValue.
    ///
    /// ## Handled variants (comprehensive)
    /// Primitive:  Integer, Long, Float, Double, ScientificNotation(*), String,
    ///             Boolean, HexColor, Date, Timestamp, Null
    /// Collection: Array, NestedArray(*), Object, PrefixedConstructor (t/b/r),
    ///             Tuple (via PrefixedConstructor)
    /// Special:    EnumValue, InterpolatedString(*)
    ///
    /// (*) previously missing — now fixed.
    fn convert_ast_value_to_dix_value(
        &self, value: &Value,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) -> Option<DixValue> {
        match value {
            // ── Primitives ────────────────────────────────────────────────────
            Value::Null { .. }                              => Some(DixValue::Null),
            Value::Boolean { value: b, .. }                => Some(DixValue::Bool(*b)),
            Value::Integer { value: i, .. }                => Some(DixValue::Int(*i)),
            Value::Long { value: l, .. }                   => Some(DixValue::Long(*l)),
            Value::Float { value: f, .. }                  => Some(DixValue::Float(*f)),
            Value::Double { value: d, .. }                 => Some(DixValue::Double(*d)),

            // FIX 1: scientific notation (e.g. 6.62607015e-34) — was silently None
            Value::ScientificNotation { value: d, .. }     => Some(DixValue::Double(*d)),

            Value::String { value: s, .. }                 => Some(DixValue::String(s.clone())),
            Value::Date { value: d, .. }                   => Some(DixValue::Date(d.clone())),
            Value::Timestamp { value: t, .. }              => Some(DixValue::Timestamp(t.clone())),
            Value::HexColor { value: c, .. }               => Some(DixValue::HexColor(c.clone())),

            // FIX 2: interpolated strings — use the template text as an approximation.
            // After full value resolution the expressions should have been inlined;
            // if any remain the template is the best we can do without an evaluator.
            Value::InterpolatedString { template, .. }     => Some(DixValue::String(template.clone())),

            // ── Collections ───────────────────────────────────────────────────
            Value::Array { values, .. } => {
                let items: Vec<DixValue> = values.iter()
                    .filter_map(|v| self.convert_ast_value_to_dix_value(v, enums))
                    .collect();
                Some(DixValue::Array(items))
            }

            // FIX 3: nested arrays ([[1,2],[3,4]]) — was silently None
            Value::NestedArray { values, .. } => {
                let items: Vec<DixValue> = values.iter()
                    .filter_map(|v| self.convert_ast_value_to_dix_value(v, enums))
                    .collect();
                Some(DixValue::Array(items))
            }

            Value::Object { properties, .. } => {
                let mut obj = std::collections::HashMap::new();
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
                match prefix.as_str() {
                    "t" => {
                        let items: Vec<DixValue> = arguments.iter()
                            .filter_map(|v| self.convert_ast_value_to_dix_value(v, enums))
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

            // Runtime-only / unresolved nodes — not representable as static data.
            _ => None,
        }
    }

    fn categorize_data_entries<'a>(
        &self, entries: &'a [DataEntry],
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
        if prefix.is_empty() { segment.to_string() } else { format!("{}.{}", prefix, segment) }
    }

    fn flatten_entry(
        &self, entry: &DataEntry, prefix: &str,
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
                let array_values: Vec<DixValue> = items.iter()
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
                    let mut obj_map = std::collections::HashMap::new();
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

    fn convert_dix_value_to_ast_value(&self, value: &DixValue) -> Result<Value, String> {
        Ok(match value {
            DixValue::Null         => Value::Null      { position: Position::UNKNOWN },
            DixValue::Bool(b)      => Value::Boolean   { value: *b,  position: Position::UNKNOWN },
            DixValue::Int(i)       => Value::Integer   { value: *i,  position: Position::UNKNOWN },
            DixValue::Long(l)      => Value::Long      { value: *l,  position: Position::UNKNOWN },
            DixValue::Float(f)     => Value::Float     { value: *f,  position: Position::UNKNOWN },
            DixValue::Double(d)    => Value::Double    { value: *d,  position: Position::UNKNOWN },
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

            DixValue::Array(arr) => {
                let items: Result<Vec<Value>, String> = arr.iter()
                    .map(|v| self.convert_dix_value_to_ast_value(v))
                    .collect();
                Value::Array { values: items?, position: Position::UNKNOWN }
            }

            DixValue::Object(obj) => {
                let mut properties = Vec::with_capacity(obj.len());
                for (k, v) in obj {
                    let ast_value = self.convert_dix_value_to_ast_value(v)?;
                    properties.push(ObjectProperty {
                        key: k.clone(), value: ast_value, position: Position::UNKNOWN,
                    });
                }
                Value::Object { properties, position: Position::UNKNOWN }
            }

            DixValue::Tuple(items) => {
                let args: Result<Vec<Value>, String> = items.iter()
                    .map(|v| self.convert_dix_value_to_ast_value(v))
                    .collect();
                Value::PrefixedConstructor {
                    prefix:    "t".to_string(),
                    arguments: args?,
                    position:  Position::UNKNOWN,
                }
            }

            DixValue::Enum { enum_name, field_name, .. } => Value::EnumValue {
                enum_name: enum_name.clone(),
                value:     field_name.clone(),
                position:  Position::UNKNOWN,
            },
        })
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
            Value::Null { .. }                        => "null".to_string(),
            Value::Boolean { value: b, .. }           => b.to_string(),
            Value::Integer { value: i, .. }           => i.to_string(),
            Value::Long { value: l, .. }              => format!("{}L", l),
            Value::Float { value: f, .. }             => format!("{}f", f),
            // FIX: see values.rs — a whole-number f64 must keep an explicit
            // ".0" or it re-lexes as Integer on the next compile.
            Value::Double { value: d, .. } => {
                if d.is_finite() && d.fract() == 0.0 {
                    format!("{:.1}", d)
                } else {
                    d.to_string()
                }
            }
            Value::ScientificNotation { value: d, .. } => format!("{:e}", d),
            Value::String { value: s, .. }            => format!("\"{}\"", s),
            Value::InterpolatedString { template, .. } => format!("$\"{}\"", template),
            Value::Date { value: d, .. }              => d.clone(),
            Value::Timestamp { value: t, .. }         => t.clone(),
            Value::HexColor { value: c, .. }          => c.clone(),

            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                let items: Vec<String> = values.iter()
                    .map(|v| self.format_value_for_mdix(v, opts))
                    .collect();
                format!("[{}]", items.join(&format!(",{}", sp)))
            }

            Value::Object { properties, .. } => {
                let pairs: Vec<String> = properties.iter()
                    .map(|p| format!(
                        "{}{}={}{}",
                        p.key, sp, sp,
                        self.format_value_for_mdix(&p.value, opts)
                    ))
                    .collect();
                format!("{{{}}}", pairs.join(&format!(",{}", sp)))
            }

            Value::PrefixedConstructor { prefix, arguments, .. } => {
                let args: Vec<String> = arguments.iter()
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
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler::AST::*;

    fn make_ast(entries: Vec<DataEntry>) -> DixScript {
        DixScript {
            data: Some(DataSection { entries, position: Position::UNKNOWN }),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        }
    }

    fn int_val(n: i32) -> Value {
        Value::Integer { value: n, position: Position::UNKNOWN }
    }
    fn sci_val(d: f64) -> Value {
        Value::ScientificNotation { value: d, position: Position::UNKNOWN }
    }
    fn nested_arr(items: Vec<Value>) -> Value {
        Value::NestedArray { values: items, level: 2, position: Position::UNKNOWN }
    }
    fn prop(name: &str, value: Value) -> PropertyAssignment {
        PropertyAssignment { name: name.into(), data_type: None, value, position: Position::UNKNOWN }
    }
    fn path(segs: &[&str]) -> TablePath {
        TablePath { segments: segs.iter().map(|s| s.to_string()).collect() }
    }

    // ── ScientificNotation / NestedArray fixes ────────────────────────────────

    #[test]
    fn test_scientific_notation_to_json_not_null() {
        let converter = DixConverter::new();
        let ast = make_ast(vec![DataEntry::SimpleProperty {
            name: "planck".to_string(), data_type: None,
            value: sci_val(6.62607015e-34_f64), position: Position::UNKNOWN,
        }]);
        let json = converter.to_json(&ast, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["planck"].is_number(), "planck should be number: {}", json);
    }

    #[test]
    fn test_nested_array_to_json() {
        let converter = DixConverter::new();
        let ast = make_ast(vec![DataEntry::SimpleProperty {
            name: "matrix".to_string(), data_type: None,
            value: nested_arr(vec![
                Value::Array { values: vec![int_val(1), int_val(2)], position: Position::UNKNOWN },
                Value::Array { values: vec![int_val(3), int_val(4)], position: Position::UNKNOWN },
            ]),
            position: Position::UNKNOWN,
        }]);
        let json = converter.to_json(&ast, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["matrix"].is_array(), "expected array: {}", json);
    }

    // ── Long round-trips ──────────────────────────────────────────────────────

    #[test]
    fn test_long_round_trips_json() {
        let converter = DixConverter::new();
        let mut data  = HashMap::new();
        data.insert("big".to_string(), DixValue::Long(9_000_000_000_i64));
        let ast  = converter.from_hashmap(data).unwrap();
        let json = converter.to_json(&ast, false).unwrap();
        assert!(json.contains("9000000000"));
        let ast2 = converter.from_json(&json).unwrap();
        let map2 = converter.to_hashmap(&ast2);
        assert_eq!(map2.get("big"), Some(&DixValue::Long(9_000_000_000_i64)));
    }

    #[test]
    fn test_long_format_mdix() {
        let converter = DixConverter::new();
        let mut data  = HashMap::new();
        data.insert("count".to_string(), DixValue::Long(1_000_000_000_000_i64));
        let ast  = converter.from_hashmap(data).unwrap();
        let mdix = converter.to_mdix(&ast, None).unwrap();
        assert!(mdix.contains("1000000000000L"));
    }

    // ── Table / group array round-trips ───────────────────────────────────────

    #[test]
    fn test_table_property_nested_json() {
        let ast = make_ast(vec![DataEntry::TableProperty {
            path:       path(&["my", "me", "mo"]),
            properties: vec![prop("something", int_val(12))],
            position:   Position::UNKNOWN,
        }]);
        let converter = DixConverter::new();
        let json = converter.to_json(&ast, true).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["my"]["me"]["mo"]["something"], 12);
    }

    #[test]
    fn test_group_array_scalar_nested_json() {
        let ast = make_ast(vec![DataEntry::GroupArray {
            path:     path(&["my", "mo"]),
            items:    vec![int_val(1), int_val(2), int_val(3)],
            position: Position::UNKNOWN,
        }]);
        let converter = DixConverter::new();
        let v: serde_json::Value =
            serde_json::from_str(&converter.to_json(&ast, false).unwrap()).unwrap();
        assert_eq!(v["my"]["mo"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_tuple_converts_to_json_array() {
        let converter = DixConverter::new();
        let dv = DixValue::Tuple(vec![
            DixValue::Int(1),
            DixValue::String("hello".into()),
            DixValue::Bool(true),
        ]);
        let jv = converter.dix_value_to_json_value(&dv);
        assert_eq!(jv, serde_json::json!([1, "hello", true]));
    }

    #[test]
    fn test_table_property_nested_toml_no_quoted_keys() {
        let ast = make_ast(vec![DataEntry::TableProperty {
            path:       path(&["my", "me", "mo"]),
            properties: vec![prop("sss", int_val(4))],
            position:   Position::UNKNOWN,
        }]);
        let converter = DixConverter::new();
        let toml_str  = converter.to_toml(&ast).unwrap();
        assert!(!toml_str.contains('"'), "quoted key in TOML output:\n{}", toml_str);
        let v: toml::Value = toml::from_str(&toml_str).unwrap();
        assert_eq!(v["my"]["me"]["mo"]["sss"].as_integer(), Some(4));
    }

    // ── Group D: structural from_hashmap fixes ────────────────────────────────

    #[test]
    fn test_from_hashmap_filters_synthetic_table_children() {
        // Simulates DixData::to_hashmap() output: aggregate "server" -> Object
        // PLUS synthetic children "server.host" / "server.port".
        let mut data = HashMap::new();
        let mut server_obj = HashMap::new();
        server_obj.insert("host".to_string(), DixValue::String("localhost".into()));
        server_obj.insert("port".to_string(), DixValue::Int(8080));
        data.insert("server".to_string(), DixValue::Object(server_obj));
        data.insert("server.host".to_string(), DixValue::String("localhost".into()));
        data.insert("server.port".to_string(), DixValue::Int(8080));

        let converter = DixConverter::new();
        let ast = converter.from_hashmap(data).unwrap();
        let entries = &ast.data.unwrap().entries;

        // Exactly one TableProperty for "server" — no stray SimpleProperty
        // entries named "server.host" / "server.port".
        assert_eq!(entries.len(), 1, "expected one entry, got: {:?}", entries);
        match &entries[0] {
            DataEntry::TableProperty { path, properties, .. } => {
                assert_eq!(path.to_string(), "server");
                assert_eq!(properties.len(), 2);
            }
            other => panic!("expected TableProperty, got: {:?}", other),
        }

        // The emitted .mdix must not contain invalid dotted identifiers.
        let mdix = converter.to_mdix(&ast, None).unwrap();
        assert!(!mdix.contains("server.host ="), "invalid identifier leaked: {}", mdix);
        assert!(!mdix.contains("server.port ="), "invalid identifier leaked: {}", mdix);
    }

    #[test]
    fn test_from_hashmap_filters_synthetic_array_indices() {
        // Simulates DixData::to_hashmap() output: aggregate "tags" -> Array
        // PLUS synthetic indices "tags[0]" / "tags[1]".
        let mut data = HashMap::new();
        data.insert("tags".to_string(), DixValue::Array(vec![
            DixValue::String("alpha".into()),
            DixValue::String("beta".into()),
        ]));
        data.insert("tags[0]".to_string(), DixValue::String("alpha".into()));
        data.insert("tags[1]".to_string(), DixValue::String("beta".into()));

        let converter = DixConverter::new();
        let ast = converter.from_hashmap(data).unwrap();
        let entries = &ast.data.unwrap().entries;

        assert_eq!(entries.len(), 1, "expected one entry, got: {:?}", entries);
        match &entries[0] {
            DataEntry::GroupArray { path, items, .. } => {
                assert_eq!(path.to_string(), "tags");
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected GroupArray, got: {:?}", other),
        }

        let mdix = converter.to_mdix(&ast, None).unwrap();
        assert!(!mdix.contains("tags[0]"), "invalid identifier leaked: {}", mdix);
        assert!(!mdix.contains("tags[1]"), "invalid identifier leaked: {}", mdix);
        assert!(mdix.contains("tags::"), "expected group array syntax: {}", mdix);
    }

    #[test]
    fn test_from_hashmap_preserves_unrelated_prefix_keys() {
        // "matrix2" must survive even though "matrix" / "matrix[0]" /
        // "matrix[1]" are present and "matrix2" shares a string prefix.
        let mut data = HashMap::new();
        data.insert("matrix".to_string(), DixValue::Array(vec![DixValue::Int(1), DixValue::Int(2)]));
        data.insert("matrix[0]".to_string(), DixValue::Int(1));
        data.insert("matrix[1]".to_string(), DixValue::Int(2));
        data.insert("matrix2".to_string(), DixValue::Int(99));

        let converter = DixConverter::new();
        let ast = converter.from_hashmap(data).unwrap();
        let entries = &ast.data.unwrap().entries;

        let names: Vec<String> = entries.iter().map(|e| match e {
            DataEntry::SimpleProperty { name, .. } => name.clone(),
            DataEntry::GroupArray { path, .. } => path.to_string(),
            DataEntry::TableProperty { path, .. } => path.to_string(),
            DataEntry::ObjectProperty { name, .. } => name.clone(),
        }).collect();

        assert!(names.contains(&"matrix".to_string()), "matrix missing: {:?}", names);
        assert!(names.contains(&"matrix2".to_string()), "matrix2 missing: {:?}", names);
        assert_eq!(entries.len(), 2, "expected exactly 2 entries, got: {:?}", names);
    }

    #[test]
    fn test_from_hashmap_already_structural_is_noop() {
        // A map with no synthetic child keys must pass through unchanged.
        let mut data = HashMap::new();
        data.insert("name".to_string(), DixValue::String("MyApp".into()));
        data.insert("port".to_string(), DixValue::Int(8080));

        let converter = DixConverter::new();
        let ast = converter.from_hashmap(data).unwrap();
        let entries = &ast.data.unwrap().entries;
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_full_round_trip_table_and_array_via_to_hashmap() {
        // End-to-end: build an AST with a TableProperty + GroupArray,
        // flatten via to_hashmap (synthetic children included), then
        // reconstruct via from_hashmap and confirm valid .mdix output.
        let ast = make_ast(vec![
            DataEntry::TableProperty {
                path:       path(&["server"]),
                properties: vec![prop("host", Value::String { value: "localhost".into(), position: Position::UNKNOWN })],
                position:   Position::UNKNOWN,
            },
            DataEntry::GroupArray {
                path:     path(&["tags"]),
                items:    vec![Value::String { value: "alpha".into(), position: Position::UNKNOWN }],
                position: Position::UNKNOWN,
            },
        ]);

        let converter = DixConverter::new();
        let flat = converter.to_hashmap(&ast);

        // Sanity: fully-flattened map contains synthetic children.
        assert!(flat.contains_key("server.host"));
        assert!(flat.contains_key("tags[0]"));

        let ast2 = converter.from_hashmap(flat).unwrap();
        let mdix = converter.to_mdix(&ast2, None).unwrap();

        assert!(!mdix.contains("server.host"), "invalid identifier leaked: {}", mdix);
        assert!(!mdix.contains("tags[0]"), "invalid identifier leaked: {}", mdix);
        assert!(mdix.contains("server:"), "expected table property syntax: {}", mdix);
        assert!(mdix.contains("tags::"), "expected group array syntax: {}", mdix);
    }
                }
