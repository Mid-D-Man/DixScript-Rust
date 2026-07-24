use std::collections::HashMap;
use crate::Compiler::AST::{
    DixScript, ConfigSection, ConfigEntry, ConfigValue,
    DataSection, DataEntry, Value, PropertyAssignment,
    TablePath, ObjectProperty, EnumDeclaration, EnumField,
    EnumsSection, Position,
};
use super::dix_data::DixData;
use super::dix_value::DixValue;
use super::format_options::DixFormatOptions;

// ─────────────────────────────────────────────────────────────────────────────
// Structural hashmap helpers
// ─────────────────────────────────────────────────────────────────────────────
// `DixData::to_hashmap()` is fully flattened — it has both aggregate keys
// ("server" -> Object) and synthetic child paths ("server.host" -> String).
// DixScript identifiers can't contain "." or "[", so feeding that straight
// into `from_hashmap` would emit invalid source like `tags[0] = "web"`.
// `filter_structural_keys` drops any key that's a derived child path of
// another key already present, leaving only the roots needed to rebuild the
// structure. Filtering an already-structural map (e.g.
// `DixData::to_structural_hashmap()`) is a no-op.

#[inline]
fn is_child_path(key: &str, parent: &str) -> bool {
    key.len() > parent.len()
        && key.starts_with(parent)
        && matches!(key.as_bytes()[parent.len()], b'.' | b'[')
}

// ─────────────────────────────────────────────────────────────────────────────
// `.mdix` enum-name sanitization for `to_mdix`
// ─────────────────────────────────────────────────────────────────────────────
// `ValueResolver::resolve_all_enum_values` (Compiler/Core/ValueResolution/
// value_resolver.rs) synthesizes an `EnumDeclaration` for every *imported*
// enum a file actually uses, named with the full qualified form
// ("EnumMan.Suka") so it can never collide with a real local declaration.
// That's the right key for in-memory lookups (`extract_enums`,
// `ast_value_to_dix_value`), but a `.mdix` enum declaration name is a plain
// identifier -- `.mdix` grammar has no syntax for a dot inside one. Writing
// `decl.name` straight into `@ENUMS(...)` (as this file used to) produced
// text like `EnumMan.Suka { ... }`, which fails to re-parse, and the
// matching `@DATA` value (`format_value_for_mdix`'s `Value::EnumValue` arm)
// wrote the equally-unparseable 3-part `EnumMan.Suka.Crack`. Since a
// round-tripped `.mdix` file (via `mdix format`, `mdix decrypt`, or
// `to_mdix` with `DixFormatOptions::minify`) never carries the original
// `@IMPORTS` forward, there's no namespace left to qualify against anyway --
// so the only sane fix is to flatten the qualified name into a genuinely
// local declaration: replace "." with "_" (a valid identifier is what's
// left), dedupe against every other enum name already in play so two
// different imports can never collide into the same flattened name or
// shadow an unrelated local enum that already has that name, and rewrite
// the corresponding `@DATA` reference from the 3-part imported form down to
// the 2-part local form (`EnumMan_Suka.Crack`). This keeps the enum
// identity (name + field) alive in the output file -- just as a real local
// enum instead of a dangling cross-file reference with nothing left to
// resolve it.
fn build_enum_rename_map_for_mdix(enums: &EnumsSection) -> HashMap<String, String> {
    let mut rename_map = HashMap::new();
    let mut taken: std::collections::HashSet<String> =
        enums.enums.iter().map(|d| d.name.clone()).collect();

    for decl in &enums.enums {
        if !decl.name.contains('.') {
            continue;
        }

        let base = decl.name.replace('.', "_");
        let mut candidate = base.clone();
        let mut suffix = 2u32;
        while taken.contains(&candidate) {
            candidate = format!("{}_{}", base, suffix);
            suffix += 1;
        }

        taken.insert(candidate.clone());
        rename_map.insert(decl.name.clone(), candidate);
    }

    rename_map
}

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

    /// Build a `DixScript` AST from a bare `HashMap<String, DixValue>` — the
    /// "I only have a value map, no other context" entry point used by
    /// `from_json`/`from_toml` and by callers who built a map by hand.
    ///
    /// Synthetic flattened child keys are filtered out first (see
    /// `filter_structural_keys`). Entries are rebuilt in sorted-key order so
    /// repeated calls over identical data always produce identical output —
    /// `HashMap` iteration order is not guaranteed stable across instances.
    ///
    /// Any `DixValue::Enum` found anywhere in the map (including nested
    /// inside objects/arrays) is used to reconstruct a matching `@ENUMS`
    /// section — without this, emitted `.mdix` would reference an
    /// undeclared enum and fail to recompile. If you already have a real
    /// `DixData` (not just a bare map), prefer `from_dix_data` instead — it
    /// uses the *actual* declared enum table and `@CONFIG`, not a
    /// reconstruction inferred from usage.
    ///
    /// The `@CONFIG` section emitted here is a synthetic placeholder
    /// (`version = "1.0.0"` only) since a bare map carries no config
    /// metadata — deliberately deterministic, no timestamp.
    pub fn from_hashmap(&self, data: HashMap<String, DixValue>) -> Result<DixScript, String> {
        let data = filter_structural_keys(&data);

        let mut enum_table: HashMap<String, HashMap<String, i32>> = HashMap::new();
        for value in data.values() {
            Self::collect_enum_usages(value, &mut enum_table);
        }
        let enums_section = Self::build_enums_section_from_table(&enum_table);

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

        let mut flat_keys: Vec<String> = flat_properties.keys().cloned().collect();
        flat_keys.sort();
        for key in flat_keys {
            let value = flat_properties[&key].clone();
            let ast_value = self.convert_dix_value_to_ast_value(&value)?;
            data_entries.push(DataEntry::SimpleProperty {
                name: key, data_type: None, value: ast_value, position: Position::UNKNOWN,
            });
        }

        let mut nested_keys: Vec<String> = nested_structures.keys().cloned().collect();
        nested_keys.sort();
        for key in nested_keys {
            let value = nested_structures[&key].clone();
            self.process_nested_structure(&key, &value, &mut data_entries, "")?;
        }

        let config_section = Some(ConfigSection {
            entries: vec![ConfigEntry {
                key: "version".to_string(),
                value: ConfigValue::String("1.0.0".to_string()),
                position: Position::UNKNOWN,
            }],
            position: Position::UNKNOWN,
        });

        Ok(DixScript {
            config: config_section,
            imports: None,
            dlm: None,
            enums: enums_section,
            quick_functions: None,
            security: None,
            data: Some(DataSection { entries: data_entries, position: Position::UNKNOWN }),
        })
    }

    // ── from_dix_data ─────────────────────────────────────────────────────────

    /// Reconstruct a `DixScript` AST from an already-loaded `DixData`.
    ///
    /// This is the correct entry point whenever a real `DixData` is on hand
    /// (e.g. `mdix decrypt`, `mdix format`) — unlike `from_hashmap`, which
    /// only ever sees a bare value map, this pulls the authoritative
    /// `@CONFIG` and `@ENUMS` straight from `DixData::config` /
    /// `DixData::enums`, the same tables `DixLoader` populated when the
    /// source was first compiled. Nothing is guessed or reconstructed from
    /// usage.
    pub fn from_dix_data(&self, data: &DixData) -> Result<DixScript, String> {
        let mut ast = self.from_hashmap(data.to_structural_hashmap())?;

        if let Some(ref enums) = data.enums {
            ast.enums = Self::build_enums_section_from_table(enums);
        }
        if let Some(ref config) = data.config {
            ast.config = Self::build_config_section_from_map(config);
        }

        Ok(ast)
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

        let enum_rename_map: HashMap<String, String> = ast
            .enums
            .as_ref()
            .map(build_enum_rename_map_for_mdix)
            .unwrap_or_default();

        if let Some(ref enums) = ast.enums {
            output.push_str("@ENUMS(");
            output.push_str(nl);
            for decl in &enums.enums {
                output.push_str(&indent);
                let written_name = enum_rename_map.get(&decl.name).unwrap_or(&decl.name);
                output.push_str(written_name);
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
                    output.push_str(&self.format_value_for_mdix(value, opts, &enum_rename_map));
                }
            }

            let grouped_count = table_props.len() + group_arrays.len();

            if !flat_props.is_empty() && grouped_count > 0 {
                output.push(',');
                output.push_str(nl);
                output.push_str(nl);
            }

            let mut grouped_index = 0usize;

            for entry in &table_props {
                if grouped_index > 0 {
                    output.push(',');
                    output.push_str(nl);
                }
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
                        output.push_str(&self.format_value_for_mdix(&prop.value, opts, &enum_rename_map));
                    }
                }
                grouped_index += 1;
            }

            for entry in &group_arrays {
                if grouped_index > 0 {
                    output.push(',');
                    output.push_str(nl);
                }
                if let DataEntry::GroupArray { path, items, .. } = entry {
                    output.push_str(&indent);
                    output.push_str(&path.to_string());
                    output.push_str("::");
                    output.push_str(sp);
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 { output.push(','); output.push_str(sp); }
                        output.push_str(&self.format_value_for_mdix(item, opts, &enum_rename_map));
                    }
                }
                grouped_index += 1;
            }

            if grouped_count > 0 {
                output.push_str(nl);
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
/// Like `to_json`, but serializes the *flat* hashmap representation
    /// (see `to_hashmap`) instead of reconstructing nested JSON objects from
    /// dotted `TablePath` segments.
    ///
    /// Every dotted path becomes its own literal top-level JSON key —
    /// `"crates.midn-ecs"` and `"crates.midn-ecs.src"` are two independent
    /// keys, never nested into each other. This is the format
    /// mdix-scaffold's generate_structure.py (key_to_dir / collect_dir_groups)
    /// expects, and the one `to_json`'s nested form can't safely produce
    /// whenever one dotted path is a prefix of another: a GroupArray at
    /// "crates.midn-ecs" followed by a deeper one at "crates.midn-ecs.src"
    /// collides in the nested form (the already-inserted Array can't be
    /// turned into an Object to hold "src"), and the deeper one is silently
    /// dropped.
    pub fn to_json_flat(&self, ast: &DixScript, pretty: bool) -> Result<String, String> {
        let flat = self.to_hashmap(ast);
        let mut map = serde_json::Map::with_capacity(flat.len());
        for (key, value) in flat {
            map.insert(key, self.dix_value_to_json_value(&value));
        }
        let json_value = serde_json::Value::Object(map);
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
                Self::insert_nested_json(root, &path.segments, serde_json::Value::Object(props));
            }

            DataEntry::GroupArray { path, items, .. } => {
                let arr: Vec<serde_json::Value> = items.iter()
                    .map(|v| {
                        let dv = self.convert_ast_value_to_dix_value(v, enums)
                            .unwrap_or(DixValue::Null);
                        self.dix_value_to_json_value(&dv)
                    })
                    .collect();
                Self::insert_nested_json(root, &path.segments, serde_json::Value::Array(arr));
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
        let child = root.entry(key).or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
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
                .map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null),
            DixValue::Double(d) => serde_json::Number::from_f64(*d)
                .map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null),
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
                let map: serde_json::Map<String, serde_json::Value> = obj.iter()
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

    /// Parse JSON text and build a `DixScript`. JSON has no enum type, so a
    /// round trip through here always loses enum identity (the int survives,
    /// the symbolic name doesn't) — that's an inherent format limitation,
    /// not something this method can recover.
    pub fn from_json(&self, json_str: &str) -> Result<DixScript, String> {
        let json_value: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| format!("JSON parse failed: {}", e))?;
        let map = self.json_value_to_hashmap(json_value)?;
        self.from_hashmap(map)
    }

    fn json_value_to_hashmap(&self, value: serde_json::Value) -> Result<HashMap<String, DixValue>, String> {
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
                    // Fits in i64 — prefer Int (i32) when it fits, Long otherwise.
                    if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                        DixValue::Int(i as i32)
                    } else {
                        DixValue::Long(i)
                    }
                } else if n.as_u64().is_some() {
                    // Value is in (i64::MAX, u64::MAX] — too large for Long (i64).
                    // All such values exceed 2^53, so f64 cannot represent them
                    // exactly — silently downgrading would corrupt data silently.
                    // The caller must store this value as a JSON string instead.
                    return Err(format!(
                        "JSON number {} exceeds DixScript's Long range (i64::MAX = {}). \
                         f64 cannot represent it exactly (2^53 = {} is the largest \
                         exactly-representable integer in f64). Store this value as a \
                         JSON string and parse it in your application code.",
                        n, i64::MAX, 9_007_199_254_740_992_u64
                    ));
                } else if let Some(f) = n.as_f64() {
                    DixValue::Double(f)
                } else {
                    return Err(format!("Cannot convert JSON number {} to any DixValue type", n));
                }
            }

            serde_json::Value::Array(arr) => {
                let items: Result<Vec<DixValue>, String> = arr.into_iter()
                    .map(|v| self.json_value_to_dix_value(v))
                    .collect();
                let items = items?;
                // Heterogeneous items (values from different type-kind buckets)
                // become a DixValue::Tuple, which round-trips as t:(a, b, c) in
                // DixScript — an explicitly mixed-type construct.
                // Homogeneous items stay as DixValue::Array (:: syntax in .mdix).
                if Self::is_heterogeneous(&items) {
                    DixValue::Tuple(items)
                } else {
                    DixValue::Array(items)
                }
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
                let dv = self.convert_ast_value_to_dix_value(value, enums).unwrap_or(DixValue::Null);
                if let Some(tv) = self.dix_value_to_toml_value(&dv) {
                    root.insert(name.clone(), tv);
                }
            }
            DataEntry::TableProperty { path, properties, .. } => {
                let mut props = toml::map::Map::new();
                for prop in properties {
                    let dv = self.convert_ast_value_to_dix_value(&prop.value, enums).unwrap_or(DixValue::Null);
                    if let Some(tv) = self.dix_value_to_toml_value(&dv) {
                        props.insert(prop.name.clone(), tv);
                    }
                }
                Self::insert_nested_toml(root, &path.segments, toml::Value::Table(props));
            }
            DataEntry::GroupArray { path, items, .. } => {
                let arr: Vec<toml::Value> = items.iter()
                    .filter_map(|v| {
                        let dv = self.convert_ast_value_to_dix_value(v, enums).unwrap_or(DixValue::Null);
                        self.dix_value_to_toml_value(&dv)
                    })
                    .collect();
                Self::insert_nested_toml(root, &path.segments, toml::Value::Array(arr));
            }
            DataEntry::ObjectProperty { name, object, .. } => {
                let dv = self.convert_ast_value_to_dix_value(object, enums).unwrap_or(DixValue::Null);
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
                        for (k, v) in new_map { existing.insert(k, v); }
                    }
                }
                _ => { root.insert(key.clone(), value); }
            }
            return;
        }

        let key = segments[0].clone();
        let child = root.entry(key).or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
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

    fn toml_value_to_hashmap(&self, value: toml::Value) -> Result<HashMap<String, DixValue>, String> {
        match value {
            toml::Value::Table(map) => {
                let mut result = HashMap::with_capacity(map.len());
                for (k, v) in map { result.insert(k, self.toml_value_to_dix_value(v)?); }
                Ok(result)
            }
            other => Err(format!("Expected a TOML table at the top level, got: {}", other.type_str())),
        }
    }

    fn toml_value_to_dix_value(&self, value: toml::Value) -> Result<DixValue, String> {
        Ok(match value {
            toml::Value::Boolean(b)  => DixValue::Bool(b),
            toml::Value::Integer(i)  => {
                if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                    DixValue::Int(i as i32)
                } else {
                    DixValue::Long(i)
                }
            }
            toml::Value::Float(f)    => DixValue::Double(f),
            toml::Value::String(s)   => DixValue::String(s),
            toml::Value::Datetime(d) => DixValue::String(d.to_string()),
            toml::Value::Array(arr)  => {
                let items: Result<Vec<DixValue>, String> = arr.into_iter()
                    .map(|v| self.toml_value_to_dix_value(v))
                    .collect();
                let items = items?;
                // TOML 1.0 requires homogeneous arrays, but apply the same guard
                // defensively — future-proofs against lenient parser behaviour.
                if Self::is_heterogeneous(&items) {
                    DixValue::Tuple(items)
                } else {
                    DixValue::Array(items)
                }
            }
            toml::Value::Table(map) => {
                let mut obj = HashMap::with_capacity(map.len());
                for (k, v) in map { obj.insert(k, self.toml_value_to_dix_value(v)?); }
                DixValue::Object(obj)
            }
        })
    }

    // ── Array homogeneity helpers ─────────────────────────────────────────────

    /// Classify a `DixValue` into a type-kind bucket for array homogeneity
    /// checks. Items in the same bucket coexist cleanly in a DixScript `::`
    /// array without type ambiguity. Items from different buckets produce a
    /// `Tuple` (explicitly mixed-type) instead.
    ///
    /// Numeric types (`Int`/`Long`/`Float`/`Double`) share bucket 0 — they
    /// all emit as number literals and the runtime promotes to the widest
    /// type, so mixing them in an array is safe and expected.
    #[inline]
    fn type_kind(v: &DixValue) -> u8 {
        match v {
            DixValue::Int(_) | DixValue::Long(_) |
            DixValue::Float(_) | DixValue::Double(_) => 0,
            DixValue::Bool(_) => 1,
            DixValue::String(_) | DixValue::Date(_) | DixValue::Timestamp(_)
                | DixValue::HexColor(_) | DixValue::Blob(_) | DixValue::Regex(_) => 2,
            DixValue::Null     => 3,
            DixValue::Object(_) => 4,
            DixValue::Array(_) | DixValue::Tuple(_) => 5,
            DixValue::Enum { .. } => 6,
        }
    }

    /// Returns `true` if `items` contains values from more than one type-kind
    /// bucket (i.e. the array cannot be represented as a homogeneous `::`
    /// group array in DixScript).
    fn is_heterogeneous(items: &[DixValue]) -> bool {
        if items.len() <= 1 { return false; }
        let first = Self::type_kind(&items[0]);
        items[1..].iter().any(|v| Self::type_kind(v) != first)
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn extract_enums(&self, ast: &DixScript) -> Option<HashMap<String, HashMap<String, i32>>> {
        ast.enums.as_ref().map(|enums_section| {
            enums_section.enums.iter().map(|decl| {
                let fields: HashMap<String, i32> = decl.fields.iter()
                    .filter_map(|f| f.value.map(|v| (f.name.clone(), v)))
                    .collect();
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
                let mut nested: Vec<(String, DixValue)> = Vec::new();

                let mut obj_keys: Vec<&String> = obj.keys().collect();
                obj_keys.sort();

                for k in obj_keys {
                    let v = &obj[k];
                    if matches!(v, DixValue::Object(_) | DixValue::Array(_)) {
                        nested.push((k.clone(), v.clone()));
                    } else {
                        let ast_value = self.convert_dix_value_to_ast_value(v)?;
                        properties.push(PropertyAssignment {
                            name: k.clone(), data_type: None,
                            value: ast_value, position: Position::UNKNOWN,
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

    fn convert_ast_value_to_dix_value(
        &self, value: &Value,
        enums: Option<&HashMap<String, HashMap<String, i32>>>,
    ) -> Option<DixValue> {
        super::dix_value::ast_value_to_dix_value(value, enums)
    }

    fn categorize_data_entries<'a>(
        &self, entries: &'a [DataEntry],
    ) -> (Vec<&'a DataEntry>, Vec<&'a DataEntry>, Vec<&'a DataEntry>) {
        let mut flat   = Vec::new();
        let mut tables = Vec::new();
        let mut arrays = Vec::new();

        for entry in entries {
            match entry {
                DataEntry::SimpleProperty { .. } | DataEntry::ObjectProperty { .. } => flat.push(entry),
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
                let mut obj_map = HashMap::new();

                for prop in properties {
                    let key = Self::build_path(&table_path, &prop.name);
                    if let Some(dix_value) = self.convert_ast_value_to_dix_value(&prop.value, enums) {
                        obj_map.insert(prop.name.clone(), dix_value.clone());
                        result.insert(key, dix_value);
                    }
                }

                if !obj_map.is_empty() {
                    match result.entry(table_path) {
                        std::collections::hash_map::Entry::Vacant(e) => {
                            e.insert(DixValue::Object(obj_map));
                        }
                        std::collections::hash_map::Entry::Occupied(mut e) => {
                            if let DixValue::Object(ref mut existing) = e.get_mut() {
                                for (k, v) in obj_map { existing.entry(k).or_insert(v); }
                            }
                        }
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
                    prefix: "t".to_string(), arguments: args?, position: Position::UNKNOWN,
                }
            }
            DixValue::Enum { enum_name, field_name, .. } => Value::EnumValue {
                enum_name: enum_name.clone(), value: field_name.clone(), position: Position::UNKNOWN,
            },
        })
    }

    fn format_config_value(&self, value: &ConfigValue) -> String {
        match value {
            ConfigValue::String(s)         => format!("\"{}\"", s),
            ConfigValue::Integer(i)        => i.to_string(),
            ConfigValue::Float(f)          => format!("{}f", f),
            ConfigValue::Boolean(b)        => b.to_string(),
            ConfigValue::Date(d)           => d.clone(),
            ConfigValue::Timestamp(t)      => t.clone(),
            ConfigValue::Features(feats)   => format!("\"{}\"", feats.join(",")),
            ConfigValue::ErrorHandling(eh) => format!("\"{}\"", eh),
            ConfigValue::Compatibility(cm) => format!("\"{}\"", cm),
            ConfigValue::Debug(dm)         => format!("\"{}\"", dm),
        }
    }

    fn format_value_for_mdix(
        &self,
        value: &Value,
        opts: &DixFormatOptions,
        enum_rename_map: &HashMap<String, String>,
    ) -> String {
        let sp = opts.get_space();
        match value {
            Value::Null { .. }              => "null".to_string(),
            Value::Boolean { value: b, .. } => b.to_string(),
            Value::Integer { value: i, .. } => i.to_string(),
            Value::Long { value: l, .. }    => format!("{}L", l),
            Value::Float { value: f, .. }   => format!("{}f", f),
            Value::Double { value: d, .. } => {
                if d.is_finite() && d.fract() == 0.0 { format!("{:.1}", d) } else { d.to_string() }
            }
            Value::ScientificNotation { value: d, .. } => format!("{:e}", d),
            Value::String { value: s, .. }  => format!("\"{}\"", s),
            Value::InterpolatedString { template, .. } => format!("$\"{}\"", template),
            Value::Date { value: d, .. }     => d.clone(),
            Value::Timestamp { value: t, .. } => t.clone(),
            Value::HexColor { value: c, .. } => c.clone(),
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                let items: Vec<String> = values.iter()
                    .map(|v| self.format_value_for_mdix(v, opts, enum_rename_map))
                    .collect();
                format!("[{}]", items.join(&format!(",{}", sp)))
            }
            Value::Object { properties, .. } => {
                let pairs: Vec<String> = properties.iter()
                    .map(|p| format!("{}{}={}{}", p.key, sp, sp, self.format_value_for_mdix(&p.value, opts, enum_rename_map)))
                    .collect();
                format!("{{{}}}", pairs.join(&format!(",{}", sp)))
            }
            Value::PrefixedConstructor { prefix, arguments, .. } => {
                let args: Vec<String> = arguments.iter()
                    .map(|v| self.format_value_for_mdix(v, opts, enum_rename_map))
                    .collect();
                format!("{}:({})", prefix, args.join(&format!(",{}", sp)))
            }
            Value::EnumValue { enum_name, value: field_value, .. } => {
                // For a local enum, `enum_name` is already a valid bare
                // identifier and never appears in the rename map. For an
                // imported enum, `enum_name` is the synthesized qualified
                // form ("EnumMan.Suka") -- write the flattened local name
                // `build_enum_rename_map_for_mdix` assigned it instead, so
                // this stays a valid 2-part local enum reference in the
                // output file instead of an unparseable 3-part one with no
                // `@IMPORTS` left to back it.
                let written_name = enum_rename_map
                    .get(enum_name)
                    .map(String::as_str)
                    .unwrap_or(enum_name.as_str());
                format!("{}.{}", written_name, field_value)
            }
            _ => String::new(),
        }
    }

    // ── @ENUMS reconstruction helpers ─────────────────────────────────────────

    fn collect_enum_usages(value: &DixValue, out: &mut HashMap<String, HashMap<String, i32>>) {
        match value {
            DixValue::Enum { enum_name, field_name, value } => {
                out.entry(enum_name.clone()).or_default()
                   .entry(field_name.clone()).or_insert(*value);
            }
            DixValue::Array(items) | DixValue::Tuple(items) => {
                for item in items { Self::collect_enum_usages(item, out); }
            }
            DixValue::Object(obj) => {
                for v in obj.values() { Self::collect_enum_usages(v, out); }
            }
            _ => {}
        }
    }

    fn build_enums_section_from_table(
        enums: &HashMap<String, HashMap<String, i32>>,
    ) -> Option<EnumsSection> {
        if enums.is_empty() { return None; }

        let mut names: Vec<&String> = enums.keys().collect();
        names.sort();

        let decls = names.into_iter().map(|name| {
            let fields_map = &enums[name];
            let mut field_names: Vec<&String> = fields_map.keys().collect();
            field_names.sort_by_key(|f| fields_map[*f]);
            let fields = field_names.into_iter().map(|f| EnumField {
                name: f.clone(), value: Some(fields_map[f]), position: Position::UNKNOWN,
            }).collect();
            EnumDeclaration { name: name.clone(), fields, position: Position::UNKNOWN }
        }).collect();

        Some(EnumsSection { enums: decls, position: Position::UNKNOWN })
    }

    fn build_config_section_from_map(config: &HashMap<String, String>) -> Option<ConfigSection> {
        if config.is_empty() { return None; }

        let mut keys: Vec<&String> = config.keys().collect();
        keys.sort();
        let entries = keys.into_iter().map(|k| ConfigEntry {
            key: k.clone(),
            value: ConfigValue::String(config[k].clone()),
            position: Position::UNKNOWN,
        }).collect();

        Some(ConfigSection { entries, position: Position::UNKNOWN })
    }
}

impl Default for DixConverter {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Compiler::AST::*;
    use crate::Runtime::DixDataBuilder;

    fn make_ast(entries: Vec<DataEntry>) -> DixScript {
        DixScript {
            data: Some(DataSection { entries, position: Position::UNKNOWN }),
            config: None, imports: None, dlm: None,
            enums: None, quick_functions: None, security: None,
        }
    }

    fn int_val(n: i32) -> Value { Value::Integer { value: n, position: Position::UNKNOWN } }
    fn prop(name: &str, value: Value) -> PropertyAssignment {
        PropertyAssignment { name: name.into(), data_type: None, value, position: Position::UNKNOWN }
    }
    fn path(segs: &[&str]) -> TablePath {
        TablePath { segments: segs.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn test_scientific_notation_to_json_not_null() {
        let converter = DixConverter::new();
        let ast = make_ast(vec![DataEntry::SimpleProperty {
            name: "planck".to_string(), data_type: None,
            value: Value::ScientificNotation { value: 6.62607015e-34_f64, position: Position::UNKNOWN },
            position: Position::UNKNOWN,
        }]);
        let json = converter.to_json(&ast, false).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v["planck"].is_number(), "planck should be number: {}", json);
    }

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
    fn test_table_property_nested_json() {
        let ast = make_ast(vec![DataEntry::TableProperty {
            path: path(&["my", "me", "mo"]),
            properties: vec![prop("something", int_val(12))],
            position: Position::UNKNOWN,
        }]);
        let converter = DixConverter::new();
        let json = converter.to_json(&ast, true).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["my"]["me"]["mo"]["something"], 12);
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
    fn test_from_hashmap_filters_synthetic_table_children() {
        let mut data = HashMap::new();
        let mut server_obj = HashMap::new();
        server_obj.insert("host".to_string(), DixValue::String("localhost".into()));
        server_obj.insert("port".to_string(), DixValue::Int(8080));
        data.insert("server".to_string(), DixValue::Object(server_obj));
        data.insert("server.host".to_string(), DixValue::String("localhost".into()));
        data.insert("server.port".to_string(), DixValue::Int(8080));

        let converter = DixConverter::new();
        let ast = converter.from_hashmap(data).unwrap();
        let entries = &ast.data.as_ref().unwrap().entries;

        assert_eq!(entries.len(), 1, "expected one entry, got: {:?}", entries);
        let mdix = converter.to_mdix(&ast, None).unwrap();
        assert!(!mdix.contains("server.host ="), "invalid identifier leaked: {}", mdix);
    }

    #[test]
    fn test_from_hashmap_filters_synthetic_array_indices() {
        let mut data = HashMap::new();
        data.insert("tags".to_string(), DixValue::Array(vec![
            DixValue::String("alpha".into()), DixValue::String("beta".into()),
        ]));
        data.insert("tags[0]".to_string(), DixValue::String("alpha".into()));
        data.insert("tags[1]".to_string(), DixValue::String("beta".into()));

        let converter = DixConverter::new();
        let ast = converter.from_hashmap(data).unwrap();
        let mdix = converter.to_mdix(&ast, None).unwrap();
        assert!(!mdix.contains("tags[0]"), "invalid identifier leaked: {}", mdix);
        assert!(mdix.contains("tags::"), "expected group array syntax: {}", mdix);
    }

    #[test]
    fn test_from_hashmap_reconstructs_enums_section() {
        let mut data = HashMap::new();
        data.insert("weapon_type".to_string(), DixValue::Enum {
            enum_name: "WeaponClass".into(), field_name: "ASSAULT".into(), value: 0,
        });

        let converter = DixConverter::new();
        let ast = converter.from_hashmap(data).unwrap();

        let enums = ast.clone().enums.expect("expected @ENUMS section to be reconstructed");
        assert_eq!(enums.enums[0].name, "WeaponClass");
        assert_eq!(enums.enums[0].fields[0].name, "ASSAULT");
        assert_eq!(enums.enums[0].fields[0].value, Some(0));

        let mdix = converter.to_mdix(&ast, None).unwrap();
        assert!(mdix.contains("@ENUMS("), "missing @ENUMS section: {}", mdix);
        assert!(mdix.contains("WeaponClass"), "got: {}", mdix);
    }

    #[test]
    fn test_from_hashmap_finds_enum_nested_in_array_of_objects() {
        let mut item = HashMap::new();
        item.insert("status".to_string(), DixValue::Enum {
            enum_name: "Status".into(), field_name: "ACTIVE".into(), value: 0,
        });
        let mut data = HashMap::new();
        data.insert("items".to_string(), DixValue::Array(vec![DixValue::Object(item)]));

        let converter = DixConverter::new();
        let ast = converter.from_hashmap(data).unwrap();
        let enums = ast.enums.expect("enum nested inside array-of-objects must still be found");
        assert_eq!(enums.enums[0].name, "Status");
    }

    #[test]
    fn test_from_dix_data_preserves_real_config_and_enums() {
        let data = DixDataBuilder::new()
            .config(|c| { c.with_version("2.3.1"); c.with_author("MidManStudio"); })
            .enums(|e| { e.with_enum_values("Status", &[("ACTIVE", 0), ("INACTIVE", 1)]); })
            .data(|d| { d.with_enum("state", "Status", "ACTIVE"); })
            .build()
            .unwrap();

        let converter = DixConverter::new();
        let ast = converter.from_dix_data(&data).unwrap();

        let cfg = ast.clone().config.expect("expected real @CONFIG, not placeholder");
        let version = cfg.entries.iter().find(|e| e.key == "version").unwrap();
        assert!(matches!(&version.value, ConfigValue::String(s) if *s == "2.3.1"));
        let author = cfg.entries.iter().find(|e| e.key == "author").unwrap();
        assert!(matches!(&author.value, ConfigValue::String(s) if *s == "MidManStudio"));

        let enums = ast.clone().enums.expect("expected @ENUMS from DixData.enums");
        let status = enums.enums.iter().find(|e| e.name == "Status").unwrap();
        assert_eq!(status.fields.len(), 2);

        let mdix = converter.to_mdix(&ast, None).unwrap();
        assert!(mdix.contains("2.3.1"));
        assert!(mdix.contains("@ENUMS("));
    }

    #[test]
    fn test_format_config_value_handles_all_variants() {
        let converter = DixConverter::new();
        assert_eq!(
            converter.format_config_value(&ConfigValue::ErrorHandling(ErrorHandlingStrategy::Recover)),
            "\"recover\""
        );
        assert_eq!(
            converter.format_config_value(&ConfigValue::Compatibility(CompatibilityMode::BestEffort)),
            "\"best_effort\""
        );
        assert_eq!(
            converter.format_config_value(&ConfigValue::Debug(DebugMode::Verbose)),
            "\"verbose\""
        );
        assert_eq!(
            converter.format_config_value(&ConfigValue::Features(vec!["a".into(), "b".into()])),
            "\"a,b\""
        );
    }

    #[test]
    fn test_minified_output_separates_flat_and_table_tier_with_comma() {
        let ast = make_ast(vec![
            DataEntry::SimpleProperty {
                name: "rydberg_constant".to_string(), data_type: None,
                value: Value::Double { value: 10973731.568160, position: Position::UNKNOWN },
                position: Position::UNKNOWN,
            },
            DataEntry::TableProperty {
                path: path(&["elements", "hydrogen", "identity"]),
                properties: vec![prop("name", Value::String {
                    value: "Hydrogen".into(), position: Position::UNKNOWN,
                })],
                position: Position::UNKNOWN,
            },
        ]);

        let converter = DixConverter::new();
        let minified = converter.to_mdix(&ast, Some(&DixFormatOptions::minified())).unwrap();

        assert!(
            !minified.contains("568160elements"),
            "flat property fused with table path: {}", minified
        );
        assert!(
            minified.contains(" elements.hydrogen.identity:"),
            "expected space separator before table path: {}", minified
        );
        assert!(
            minified.contains("elements.hydrogen.identity:"),
            "table property dropped: {}", minified
        );
    }

    // ── JSON import: number edge cases ────────────────────────────────────────

    #[test]
    fn test_json_number_small_int_becomes_int() {
        let converter = DixConverter::new();
        let ast = converter.from_json(r#"{"n": 42}"#).unwrap();
        let map = converter.to_hashmap(&ast);
        assert_eq!(map.get("n"), Some(&DixValue::Int(42)));
    }

    #[test]
    fn test_json_number_fits_i64_max_stays_long() {
        // i64::MAX = 9223372036854775807 — must produce Long, not demote to Double.
        let converter = DixConverter::new();
        let ast = converter.from_json(r#"{"n": 9223372036854775807}"#).unwrap();
        let map = converter.to_hashmap(&ast);
        assert_eq!(map.get("n"), Some(&DixValue::Long(i64::MAX)));
    }

    #[test]
    fn test_json_number_float_becomes_double() {
        let converter = DixConverter::new();
        let ast = converter.from_json(r#"{"x": 3.14}"#).unwrap();
        let map = converter.to_hashmap(&ast);
        assert!(matches!(map.get("x"), Some(DixValue::Double(_))));
    }

    #[test]
    fn test_json_big_u64_over_i64_max_errors() {
        // 9999999999999999999 > i64::MAX — serde_json stores as u64.
        // Silent precision loss to f64 would corrupt data; must return Err.
        let converter = DixConverter::new();
        let result = converter.from_json(r#"{"n": 9999999999999999999}"#);
        assert!(result.is_err(), "u64 > i64::MAX must return Err, got Ok");
        let err = result.unwrap_err();
        assert!(
            err.contains("Long range") || err.contains("i64::MAX"),
            "error must mention overflow, got: {}", err
        );
    }

    // ── JSON import: array homogeneity ────────────────────────────────────────

    #[test]
    fn test_json_homogeneous_int_array_stays_array() {
        let converter = DixConverter::new();
        let ast = converter.from_json(r#"{"nums": [1, 2, 3]}"#).unwrap();
        let map = converter.to_hashmap(&ast);
        assert!(
            matches!(map.get("nums"), Some(DixValue::Array(_))),
            "homogeneous int array must stay Array, got: {:?}", map.get("nums")
        );
    }

    #[test]
    fn test_json_mixed_numeric_array_stays_array() {
        // Int + Double are same type-kind bucket — no Tuple.
        let converter = DixConverter::new();
        let ast = converter.from_json(r#"{"nums": [1, 2.5, 3]}"#).unwrap();
        let map = converter.to_hashmap(&ast);
        assert!(
            matches!(map.get("nums"), Some(DixValue::Array(_))),
            "mixed-numeric array must stay Array, got: {:?}", map.get("nums")
        );
    }

    #[test]
    fn test_json_heterogeneous_array_becomes_tuple() {
        // int + string + bool → three different type-kind buckets → Tuple.
        let converter = DixConverter::new();
        let ast = converter.from_json(r#"{"mixed": [1, "hello", true]}"#).unwrap();
        let map = converter.to_hashmap(&ast);
        assert!(
            matches!(map.get("mixed"), Some(DixValue::Tuple(_))),
            "[int, string, bool] must become Tuple, got: {:?}", map.get("mixed")
        );
    }

    #[test]
    fn test_json_null_mixed_with_string_becomes_tuple() {
        // null (kind 3) mixed with string (kind 2) → heterogeneous → Tuple.
        let converter = DixConverter::new();
        let ast = converter.from_json(r#"{"maybe": [null, "value"]}"#).unwrap();
        let map = converter.to_hashmap(&ast);
        assert!(
            matches!(map.get("maybe"), Some(DixValue::Tuple(_))),
            "[null, string] must become Tuple, got: {:?}", map.get("maybe")
        );
    }

    #[test]
    fn test_json_homogeneous_string_array_stays_array() {
        let converter = DixConverter::new();
        let ast = converter.from_json(r#"{"tags": ["a", "b", "c"]}"#).unwrap();
        let map = converter.to_hashmap(&ast);
        assert!(
            matches!(map.get("tags"), Some(DixValue::Array(_))),
            "homogeneous string array must stay Array, got: {:?}", map.get("tags")
        );
    }

    #[test]
    fn test_json_empty_array_stays_array() {
        let converter = DixConverter::new();
        let ast = converter.from_json(r#"{"empty": []}"#).unwrap();
        let map = converter.to_hashmap(&ast);
        assert!(
            matches!(map.get("empty"), Some(DixValue::Array(_))),
            "empty array must stay Array"
        );
    }
}
