// dixscript/src/Runtime/converter.rs
use std::collections::HashMap;
use crate::Compiler::AST::{
    DixScript, ConfigSection, ConfigEntry, ConfigValue,
    DataSection, DataEntry, Value, PropertyAssignment,
    TablePath, ObjectProperty, EnumDeclaration, EnumField,
    EnumsSection, Position,
};
use super::dix_value::DixValue;
use super::format_options::DixFormatOptions;

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
    // Builds an AST from a flat DixValue map.  Used internally when importing
    // from JSON / TOML via the hashmap intermediary.

    pub fn from_hashmap(&self, data: HashMap<String, DixValue>) -> Result<DixScript, String> {
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
    // Flattens AST to dotted-path keys for the runtime O(1) access layer.
    // Do NOT use this for JSON/TOML export — structural nesting is lost.

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
    //
    // Builds a proper nested serde_json::Value directly from AST entries.
    //
    //   SimpleProperty  name = 42          →  { "name": 42 }
    //   ObjectProperty  name = { ... }     →  { "name": { ... } }
    //   TableProperty   my.me.mo: sss=4    →  { "my": { "me": { "mo": { "sss": 4 } } } }
    //   GroupArray      my.mo:: 1,2,3      →  { "my": { "mo": [1,2,3] } }
    //
    // JSON arrays are allowed to be heterogeneous by the JSON spec (RFC 8259
    // §5), so DixScript tuples t:(1,"x",true) map cleanly to [1,"x",true].
    // Blob and Regex values are exported as plain strings since JSON has no
    // equivalent type; the b:() / r:() wrapper is preserved in the string
    // content if callers need to round-trip them.

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

            // my.me.mo: sss=4  →  {"my":{"me":{"mo":{"sss":4}}}}
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

            // my.mo:: 1,2,3  →  {"my":{"mo":[1,2,3]}}
            // my.mo:: {a=1},{a=2}  →  {"my":{"mo":[{"a":1},{"a":2}]}}
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

    /// Walks `segments` creating intermediate Object nodes as needed, then
    /// places `value` at the leaf.  When a same-named Object already exists at
    /// the leaf and the new value is also an Object, the two maps are **merged**
    /// so that multiple TableProperty entries at the same path co-exist.
    fn insert_nested_json(
        root: &mut serde_json::Map<String, serde_json::Value>,
        segments: &[String],
        value: serde_json::Value,
    ) {
        if segments.is_empty() { return; }

        if segments.len() == 1 {
            let key = &segments[0];
            match (root.get_mut(key), &value) {
                // Merge two objects at the same leaf rather than overwrite.
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
        // If a scalar was written at this key earlier, the insertion is silently
        // skipped — path collisions are prevented at the DixScript parser level.
    }

    /// Convert a DixValue to a serde_json::Value.
    ///
    /// Type mapping:
    ///   Int / Long  →  Number (integer)
    ///   Float / Double  →  Number (float; NaN/Inf become null)
    ///   String / Date / Timestamp / HexColor  →  String
    ///   Blob  →  String  (b:(...) not re-wrapped; raw content exported)
    ///   Regex  →  String  (r:(...) not re-wrapped; raw pattern exported)
    ///   Array / Tuple  →  Array  (heterogeneous arrays are valid JSON per RFC 8259)
    ///   Object  →  Object
    ///   Enum  →  Number (integer ordinal value)
    ///   Null  →  null
    fn dix_value_to_json_value(&self, value: &DixValue) -> serde_json::Value {
        match value {
            DixValue::Null => serde_json::Value::Null,

            DixValue::Bool(b) => serde_json::Value::Bool(*b),

            DixValue::Int(i) => serde_json::Value::Number((*i).into()),

            DixValue::Long(l) => serde_json::Value::Number((*l).into()),

            DixValue::Float(f) => serde_json::Number::from_f64(*f as f64)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),

            DixValue::Double(d) => serde_json::Number::from_f64(*d)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),

            // String-like variants — all become plain JSON strings.
            // Blob and Regex lose their DixScript type marker; if round-tripping
            // is needed, consumers should use the binary format instead.
            DixValue::String(s)
            | DixValue::Date(s)
            | DixValue::Timestamp(s)
            | DixValue::HexColor(s)
            | DixValue::Blob(s)
            | DixValue::Regex(s) => serde_json::Value::String(s.clone()),

            // Arrays — DixScript arrays are homogeneous, but JSON arrays can
            // hold any mix of types so the conversion is always safe.
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

            // Tuples are positional heterogeneous collections.  JSON has no
            // dedicated tuple type; mapping to Array is the standard approach.
            // Per RFC 8259, JSON arrays may contain values of different types.
            DixValue::Tuple(items) => serde_json::Value::Array(
                items.iter().map(|v| self.dix_value_to_json_value(v)).collect(),
            ),

            // Enum exports as its integer ordinal.
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
    //
    // Builds a proper nested toml::Value directly from AST entries.
    //
    //   SimpleProperty  name = 42          →  name = 42
    //   ObjectProperty  name = { ... }     →  [name]\n...
    //   TableProperty   my.me.mo: sss=4    →  [my.me.mo]\nsss = 4
    //   GroupArray (scalars)  my.mo:: 1,2  →  [my]\nmo = [1, 2]
    //   GroupArray (objects)  my.mo:: {...} →  [[my.mo]]\n...
    //
    // The distinction between [section] and [[section]] is handled automatically
    // by the `toml` crate: a toml::Value::Array containing Table values is
    // serialised as [[array-of-tables]] syntax; an Array of scalars/arrays is
    // serialised as an inline key = [...] value.
    //
    // Type mapping notes:
    //   Null   → skipped (TOML has no null type)
    //   Blob   → String  (raw content, type marker dropped)
    //   Regex  → String  (raw pattern, type marker dropped)
    //   Tuple  → Array   (TOML 1.0 allows heterogeneous inline arrays)
    //   Enum   → String  "EnumName.FIELD"  (ordinal available via to_hashmap)

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
                // DixValue::Null produces None → silently skipped (TOML has no null)
            }

            // my.me.mo: sss=4
            //   → toml::Value::Table{"my": Table{"me": Table{"mo": Table{"sss": 4}}}}
            //   → serialised as:  [my.me.mo]\nsss = 4
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

            // GroupArray with scalar items  →  key = [...]   (inline array)
            // GroupArray with object items  →  [[section]]   (array of tables)
            //
            // The distinction is made automatically by the toml crate when it
            // serialises toml::Value::Array:
            //   Array([Table, Table, ...])   → [[my.mo]] headers
            //   Array([Integer, String, ...]) → my.mo = [1, "x", ...]
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

    /// Walks `segments` creating intermediate Table nodes as needed, then places
    /// `value` at the leaf.  Table nodes at the same leaf path are **merged** so
    /// that multiple TableProperty entries at the same path level all contribute
    /// their keys to one TOML table section.
    fn insert_nested_toml(
        root: &mut toml::map::Map<String, toml::Value>,
        segments: &[String],
        value: toml::Value,
    ) {
        if segments.is_empty() { return; }

        if segments.len() == 1 {
            let key = &segments[0];
            match (root.get_mut(key), &value) {
                // Merge two tables at the same key rather than overwrite.
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
        // If a scalar was written at this key earlier the insertion is silently
        // skipped — path collisions are prevented at the DixScript parser level.
    }

/// Convert a DixValue to a toml::Value.
///
/// Returns None for Null (TOML has no null type — callers should skip it).
///
/// Enum note: exported as its integer ordinal, consistent with the JSON
/// export path.  Both formats export the raw integer so that consuming code
/// can switch on the value directly.  If full round-trip fidelity
/// (enum_name + field_name + ordinal) is required, use the binary
/// `.mdix.enc` format instead.
///
/// Tuple note: DixScript tuples are positional and may be heterogeneous.
/// TOML 1.0 allows heterogeneous inline arrays; older TOML 0.5 parsers
/// may reject them.  We emit a toml::Value::Array regardless and let the
/// consuming TOML parser decide.
fn dix_value_to_toml_value(&self, value: &DixValue) -> Option<toml::Value> {
    match value {
        // TOML has no null — caller should skip None return values.
        DixValue::Null => None,

        DixValue::Bool(b)      => Some(toml::Value::Boolean(*b)),
        DixValue::Int(i)       => Some(toml::Value::Integer(*i as i64)),
        DixValue::Long(l)      => Some(toml::Value::Integer(*l)),
        DixValue::Float(f)     => Some(toml::Value::Float(*f as f64)),
        DixValue::Double(d)    => Some(toml::Value::Float(*d)),
        DixValue::String(s)    => Some(toml::Value::String(s.clone())),
        DixValue::Date(d)      => Some(toml::Value::String(d.clone())),
        DixValue::Timestamp(t) => Some(toml::Value::String(t.clone())),
        DixValue::HexColor(c)  => Some(toml::Value::String(c.clone())),

        // Blob and Regex: no TOML equivalent — export as plain strings.
        // The type annotation is dropped; use the binary format to preserve it.
        DixValue::Blob(b)  => Some(toml::Value::String(b.clone())),
        DixValue::Regex(r) => Some(toml::Value::String(r.clone())),

        // Enum: export as integer ordinal, consistent with the JSON export.
        // Consuming code switches on the integer value, not the name string.
        // Use to_hashmap() or the binary format if you need enum_name/field_name.
        DixValue::Enum { value, .. } => Some(toml::Value::Integer(*value as i64)),

        DixValue::Array(arr) => {
            // Items that are Null (no TOML equivalent) are dropped from the array.
            let items: Vec<toml::Value> = arr.iter()
                .filter_map(|v| self.dix_value_to_toml_value(v))
                .collect();
            Some(toml::Value::Array(items))
            // Note: if items are Table values the toml crate will serialise
            // this as [[array-of-tables]] headers automatically.
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

        // Tuple: positional heterogeneous collection.  Maps to TOML inline
        // array.  TOML 1.0 allows heterogeneous inline arrays; TOML 0.5
        // technically requires homogeneous arrays (though many parsers are
        // lenient).  Drop any Null slots since TOML has no null type.
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
        &self, value: &Value,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) -> Option<DixValue> {
        match value {
            Value::Null { .. }                => Some(DixValue::Null),
            Value::Boolean { value: b, .. }   => Some(DixValue::Bool(*b)),
            Value::Integer { value: i, .. }   => Some(DixValue::Int(*i)),
            Value::Long { value: l, .. }      => Some(DixValue::Long(*l)),
            Value::Float { value: f, .. }     => Some(DixValue::Float(*f)),
            Value::Double { value: d, .. }    => Some(DixValue::Double(*d)),
            Value::String { value: s, .. }    => Some(DixValue::String(s.clone())),
            Value::Date { value: d, .. }      => Some(DixValue::Date(d.clone())),
            Value::Timestamp { value: t, .. } => Some(DixValue::Timestamp(t.clone())),
            Value::HexColor { value: c, .. }  => Some(DixValue::HexColor(c.clone())),

            Value::Array { values, .. } => {
                let items: Vec<DixValue> = values.iter()
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
            Value::Long { value: l, .. }      => format!("{}L", l),
            Value::Float { value: f, .. }     => format!("{}f", f),
            Value::Double { value: d, .. }    => d.to_string(),
            Value::String { value: s, .. }    => format!("\"{}\"", s),
            Value::Date { value: d, .. }      => d.clone(),
            Value::Timestamp { value: t, .. } => t.clone(),
            Value::HexColor { value: c, .. }  => c.clone(),

            Value::Array { values, .. } => {
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

    // ── helper: build a bare DixScript with only a DATA section ──────────────

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
    fn str_val(s: &str) -> Value {
        Value::String { value: s.into(), position: Position::UNKNOWN }
    }
    fn prop(name: &str, value: Value) -> PropertyAssignment {
        PropertyAssignment { name: name.into(), data_type: None, value, position: Position::UNKNOWN }
    }
    fn path(segs: &[&str]) -> TablePath {
        TablePath { segments: segs.iter().map(|s| s.to_string()).collect() }
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
    fn test_long_round_trips_toml() {
        let converter = DixConverter::new();
        let mut data  = HashMap::new();
        data.insert("big".to_string(), DixValue::Long(9_000_000_000_i64));
        let ast  = converter.from_hashmap(data).unwrap();
        let toml = converter.to_toml(&ast).unwrap();
        assert!(toml.contains("9000000000"));
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

    #[test]
    fn test_json_large_int_promotes_to_long() {
        let converter = DixConverter::new();
        let json      = r#"{"big": 5000000000}"#;
        let ast       = converter.from_json(json).unwrap();
        let map       = converter.to_hashmap(&ast);
        assert_eq!(map.get("big"), Some(&DixValue::Long(5_000_000_000_i64)));
    }

    #[test]
    fn test_from_hashmap_simple() {
        let converter = DixConverter::new();
        let mut data  = HashMap::new();
        data.insert("name".to_string(), DixValue::String("Alice".to_string()));
        data.insert("age".to_string(),  DixValue::Int(30));
        let ast = converter.from_hashmap(data).unwrap();
        assert!(ast.data.is_some());
    }

    // ── JSON: table property nesting ──────────────────────────────────────────

    #[test]
    fn test_table_property_nested_json() {
        // my.me.mo: something=12  →  {"my":{"me":{"mo":{"something":12}}}}
        let ast = make_ast(vec![DataEntry::TableProperty {
            path:       path(&["my", "me", "mo"]),
            properties: vec![prop("something", int_val(12))],
            position:   Position::UNKNOWN,
        }]);

        let converter = DixConverter::new();
        let json = converter.to_json(&ast, true).unwrap();

        assert!(!json.contains("\"my.me.mo"), "dotted key in JSON: {}", json);

        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["my"]["me"]["mo"]["something"], 12,
            "nested value wrong:\n{}", json);
    }

    #[test]
    fn test_table_property_single_level_json() {
        // db: host="localhost", port=5432  →  {"db":{"host":"localhost","port":5432}}
        let ast = make_ast(vec![DataEntry::TableProperty {
            path:       path(&["db"]),
            properties: vec![
                prop("host", str_val("localhost")),
                prop("port", int_val(5432)),
            ],
            position: Position::UNKNOWN,
        }]);

        let converter = DixConverter::new();
        let v: serde_json::Value =
            serde_json::from_str(&converter.to_json(&ast, false).unwrap()).unwrap();
        assert_eq!(v["db"]["host"], "localhost");
        assert_eq!(v["db"]["port"], 5432);
    }

    #[test]
    fn test_multiple_table_props_same_path_merge_json() {
        // Two entries at [db] should merge, not overwrite.
        let ast = make_ast(vec![
            DataEntry::TableProperty {
                path:       path(&["db"]),
                properties: vec![prop("host", str_val("localhost"))],
                position:   Position::UNKNOWN,
            },
            DataEntry::TableProperty {
                path:       path(&["db"]),
                properties: vec![prop("port", int_val(5432))],
                position:   Position::UNKNOWN,
            },
        ]);

        let converter = DixConverter::new();
        let v: serde_json::Value =
            serde_json::from_str(&converter.to_json(&ast, false).unwrap()).unwrap();
        assert_eq!(v["db"]["host"], "localhost");
        assert_eq!(v["db"]["port"], 5432);
    }

    // ── JSON: group array nesting ─────────────────────────────────────────────

    #[test]
    fn test_group_array_scalar_nested_json() {
        // my.mo:: 1,2,3  →  {"my":{"mo":[1,2,3]}}
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
    fn test_group_array_top_level_json() {
        // tags:: "a","b"  →  {"tags":["a","b"]}
        let ast = make_ast(vec![DataEntry::GroupArray {
            path:     path(&["tags"]),
            items:    vec![str_val("a"), str_val("b")],
            position: Position::UNKNOWN,
        }]);

        let converter = DixConverter::new();
        let v: serde_json::Value =
            serde_json::from_str(&converter.to_json(&ast, false).unwrap()).unwrap();
        assert_eq!(v["tags"], serde_json::json!(["a", "b"]));
    }

    // ── JSON: tuple handling ──────────────────────────────────────────────────

    #[test]
    fn test_tuple_converts_to_json_array() {
        // t:(1, "hello", true) → [1, "hello", true]
        // JSON arrays may be heterogeneous per RFC 8259.
        let converter = DixConverter::new();
        let dv = DixValue::Tuple(vec![
            DixValue::Int(1),
            DixValue::String("hello".into()),
            DixValue::Bool(true),
        ]);
        let jv = converter.dix_value_to_json_value(&dv);
        assert_eq!(jv, serde_json::json!([1, "hello", true]));
    }

    // ── TOML: table property nesting ──────────────────────────────────────────

    #[test]
    fn test_table_property_nested_toml_no_quoted_keys() {
        // my.me.mo: sss=4  →  [my.me.mo]\nsss = 4  (bare keys, no quoting)
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

    #[test]
    fn test_multiple_table_props_same_path_merge_toml() {
        let ast = make_ast(vec![
            DataEntry::TableProperty {
                path:       path(&["server"]),
                properties: vec![prop("host", str_val("localhost"))],
                position:   Position::UNKNOWN,
            },
            DataEntry::TableProperty {
                path:       path(&["server"]),
                properties: vec![prop("port", int_val(8080))],
                position:   Position::UNKNOWN,
            },
        ]);

        let converter = DixConverter::new();
        let toml_str  = converter.to_toml(&ast).unwrap();
        let v: toml::Value = toml::from_str(&toml_str).unwrap();
        assert_eq!(v["server"]["host"].as_str(), Some("localhost"));
        assert_eq!(v["server"]["port"].as_integer(), Some(8080));
    }

    // ── TOML: group array nesting ─────────────────────────────────────────────

    #[test]
    fn test_group_array_scalars_toml() {
        // my.mo:: 1,4,4  →  [my]\nmo = [1, 4, 4]  (inline array, bare keys)
        let ast = make_ast(vec![DataEntry::GroupArray {
            path:     path(&["my", "mo"]),
            items:    vec![int_val(1), int_val(4), int_val(4)],
            position: Position::UNKNOWN,
        }]);

        let converter = DixConverter::new();
        let toml_str  = converter.to_toml(&ast).unwrap();

        assert!(!toml_str.contains('"'), "quoted key in TOML:\n{}", toml_str);

        let v: toml::Value = toml::from_str(&toml_str).unwrap();
        let arr = v["my"]["mo"].as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_integer(), Some(1));
    }

    #[test]
    fn test_group_array_objects_toml_array_of_tables() {
        // enemies:: {name="Goblin",hp=50}, {name="Orc",hp=100}
        // → [[enemies]]\nname = "Goblin"\nhp = 50\n\n[[enemies]]\n...
        use crate::Compiler::AST::ObjectProperty;
        let make_obj = |name: &str, hp: i32| -> Value {
            Value::Object {
                properties: vec![
                    ObjectProperty { key: "name".into(), value: str_val(name), position: Position::UNKNOWN },
                    ObjectProperty { key: "hp".into(),   value: int_val(hp),   position: Position::UNKNOWN },
                ],
                position: Position::UNKNOWN,
            }
        };

        let ast = make_ast(vec![DataEntry::GroupArray {
            path:     path(&["enemies"]),
            items:    vec![make_obj("Goblin", 50), make_obj("Orc", 100)],
            position: Position::UNKNOWN,
        }]);

        let converter = DixConverter::new();
        let toml_str  = converter.to_toml(&ast).unwrap();

        // The toml crate emits [[enemies]] for an array of tables.
        assert!(toml_str.contains("[[enemies]]"),
            "expected [[enemies]] in:\n{}", toml_str);

        let v: toml::Value = toml::from_str(&toml_str).unwrap();
        let arr = v["enemies"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"].as_str(), Some("Goblin"));
        assert_eq!(arr[1]["hp"].as_integer(), Some(100));
    }

    #[test]
    fn test_group_array_objects_nested_path_toml() {
        // game.enemies:: {name="Goblin"}, {name="Orc"}
        // → [[game.enemies]]\nname = "Goblin"\n\n[[game.enemies]]\n...
        use crate::Compiler::AST::ObjectProperty;
        let make_obj = |name: &str| -> Value {
            Value::Object {
                properties: vec![ObjectProperty {
                    key: "name".into(), value: str_val(name), position: Position::UNKNOWN,
                }],
                position: Position::UNKNOWN,
            }
        };

        let ast = make_ast(vec![DataEntry::GroupArray {
            path:     path(&["game", "enemies"]),
            items:    vec![make_obj("Goblin"), make_obj("Orc")],
            position: Position::UNKNOWN,
        }]);

        let converter = DixConverter::new();
        let toml_str  = converter.to_toml(&ast).unwrap();

        assert!(toml_str.contains("[[game.enemies]]") || toml_str.contains("[[game"),
            "expected [[game.enemies]] in:\n{}", toml_str);

        let v: toml::Value = toml::from_str(&toml_str).unwrap();
        let arr = v["game"]["enemies"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    // ── TOML: tuple handling ──────────────────────────────────────────────────

    #[test]
    fn test_tuple_converts_to_toml_array() {
        // Tuple with homogeneous ints — safe for all TOML versions.
        let converter = DixConverter::new();
        let dv = DixValue::Tuple(vec![DixValue::Int(1), DixValue::Int(2), DixValue::Int(3)]);
        let tv = converter.dix_value_to_toml_value(&dv).unwrap();
        match tv {
            toml::Value::Array(arr) => assert_eq!(arr.len(), 3),
            other => panic!("expected Array, got {:?}", other),
        }
    }

    // ── TOML: null is skipped ─────────────────────────────────────────────────

    #[test]
    fn test_null_skipped_in_toml() {
        let converter = DixConverter::new();
        assert!(converter.dix_value_to_toml_value(&DixValue::Null).is_none());
    }
            }
