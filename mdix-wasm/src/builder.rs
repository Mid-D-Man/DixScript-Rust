

use crate::database::MdixDatabase;
use crate::error::{freed_err, invalid_path_err, runtime_err};
use dixscript::Runtime::{DixLoadOptions, DixLoader};
use wasm_bindgen::prelude::*;

// ── Internal data containers ──────────────────────────────────────────────────

struct EnumDef {
    name: String,
    fields: Vec<(String, Option<i32>)>,
}

struct TableEntry {
    path: String,
    properties: Vec<(String, String)>,
}

struct GroupEntry {
    path: String,
    items: Vec<String>,
}

// ── Main builder ──────────────────────────────────────────────────────────────

/// Programmatic .mdix builder for JavaScript callers.
/// Mirrors the C# MdixBuilder three-section structure with
/// full two-tier DATA ordering enforcement.
///
/// ```js
/// const db = await new MdixBuilder()
///   .setConfigVersion("1.0.0")
///   .addEnum("LogLevel", JSON.stringify([["DEBUG",0],["INFO",1],["WARN",2]]))
///   .withString("app_name", "MyGame")
///   .withInt("port", 8080)
///   .withBool("ssl", true)
///   .withTableProperties("server", JSON.stringify({host:"localhost",port:8080}))
///   .withGroupArray("tags", JSON.stringify(["alpha","beta"]))
///   .toDatabase();
/// ```
#[wasm_bindgen]
pub struct MdixBuilder {
    valid: bool,

    // @CONFIG section
    config: Vec<(String, String)>,

    // @ENUMS section
    enums: Vec<EnumDef>,

    // @DATA — tier 1: flat scalar properties (must precede tier 2)
    flat: Vec<(String, String)>,

    // @DATA — tier 2: table properties  (path: key=val, key=val)
    tables: Vec<TableEntry>,

    // @DATA — tier 2: group arrays  (path:: item, item)
    arrays: Vec<GroupEntry>,

    // Two-tier guard — true once any tier-2 entry has been added
    has_grouped: bool,
}

#[wasm_bindgen]
impl MdixBuilder {

    // ── Construction ──────────────────────────────────────────────────────

    #[wasm_bindgen(constructor)]
    pub fn new() -> MdixBuilder {
        MdixBuilder {
            valid: true,
            config: Vec::new(),
            enums: Vec::new(),
            flat: Vec::new(),
            tables: Vec::new(),
            arrays: Vec::new(),
            has_grouped: false,
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────

    #[wasm_bindgen]
    pub fn free(&mut self) {
        self.valid = false;
    }

    #[wasm_bindgen(getter, js_name = isValid)]
    pub fn is_valid(&self) -> bool {
        self.valid
    }

    // ── @CONFIG section ───────────────────────────────────────────────────

    /// Sets the version field in @CONFIG.
    #[wasm_bindgen(js_name = setConfigVersion)]
    pub fn set_config_version(mut self, version: &str) -> Result<MdixBuilder, JsValue> {
        self.check_valid()?;
        self.config.push(("version".into(), format_config_string(version)));
        Ok(self)
    }

    /// Sets the author field in @CONFIG.
    #[wasm_bindgen(js_name = setConfigAuthor)]
    pub fn set_config_author(mut self, author: &str) -> Result<MdixBuilder, JsValue> {
        self.check_valid()?;
        self.config.push(("author".into(), format_config_string(author)));
        Ok(self)
    }

    /// Sets the encoding field in @CONFIG.
    #[wasm_bindgen(js_name = setConfigEncoding)]
    pub fn set_config_encoding(mut self, encoding: &str) -> Result<MdixBuilder, JsValue> {
        self.check_valid()?;
        self.config.push(("encoding".into(), format_config_string(encoding)));
        Ok(self)
    }

    /// Sets the debug_mode field in @CONFIG.
    /// Valid values: "off", "regular", "verbose"
    #[wasm_bindgen(js_name = setConfigDebugMode)]
    pub fn set_config_debug_mode(mut self, mode: &str) -> Result<MdixBuilder, JsValue> {
        self.check_valid()?;
        match mode {
            "off" | "regular" | "verbose" => {}
            other => return Err(runtime_err("setConfigDebugMode",
                format!("invalid debug mode '{}': expected off, regular, or verbose", other))),
        }
        self.config.push(("debug_mode".into(), format_config_string(mode)));
        Ok(self)
    }

    /// Sets any custom key in @CONFIG.
    #[wasm_bindgen(js_name = setConfig)]
    pub fn set_config(mut self, key: &str, value: &str) -> Result<MdixBuilder, JsValue> {
        self.check_valid()?;
        if key.is_empty() {
            return Err(invalid_path_err(key));
        }
        self.config.push((key.to_string(), format_config_string(value)));
        Ok(self)
    }

    // ── @ENUMS section ────────────────────────────────────────────────────

    /// Adds an enum definition to @ENUMS.
    ///
    /// `fields_json` must be either:
    ///   - A JSON array of strings for auto-increment: `["DEBUG","INFO","WARN"]`
    ///   - A JSON array of [name, value] pairs:  `[["DEBUG",0],["INFO",1]]`
    #[wasm_bindgen(js_name = addEnum)]
    pub fn add_enum(mut self, name: &str, fields_json: &str) -> Result<MdixBuilder, JsValue> {
        self.check_valid()?;
        if name.is_empty() {
            return Err(invalid_path_err("enum name"));
        }

        let parsed: serde_json::Value = serde_json::from_str(fields_json)
            .map_err(|e| runtime_err("addEnum parse", e))?;

        let arr = parsed.as_array()
            .ok_or_else(|| runtime_err("addEnum", "fields_json must be a JSON array"))?;

        if arr.is_empty() {
            return Err(runtime_err("addEnum", "enum must have at least one field"));
        }

        let mut fields: Vec<(String, Option<i32>)> = Vec::new();

        for item in arr {
            match item {
                // Plain string — auto-increment
                serde_json::Value::String(s) => {
                    fields.push((s.clone(), None));
                }
                // [name, value] pair
                serde_json::Value::Array(pair) if pair.len() == 2 => {
                    let field_name = pair[0].as_str()
                        .ok_or_else(|| runtime_err("addEnum", "field name must be a string"))?
                        .to_string();
                    let field_value = pair[1].as_i64()
                        .ok_or_else(|| runtime_err("addEnum", "field value must be an integer"))?
                        as i32;
                    fields.push((field_name, Some(field_value)));
                }
                other => {
                    return Err(runtime_err("addEnum",
                        format!("unexpected field format: {}", other)));
                }
            }
        }

        self.enums.push(EnumDef { name: name.to_string(), fields });
        Ok(self)
    }

    // ── @DATA — tier 1: flat scalar properties ────────────────────────────

    #[wasm_bindgen(js_name = withString)]
    pub fn with_string(mut self, path: &str, value: &str) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, format!("\"{}\"", escape_mdix(value)))
    }

    #[wasm_bindgen(js_name = withInt)]
    pub fn with_int(mut self, path: &str, value: i32) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, value.to_string())
    }

    /// Adds a 64-bit integer value, explicitly typed as Long.
    ///
    /// Takes a JS `bigint`, not `number` — e.g. `withLong("id", 123n)`,
    /// not `withLong("id", 123)`. wasm-bindgen will throw a TypeError if
    /// you pass a plain number here. Values that overflow i32 are
    /// auto-promoted to Long by the parser regardless of suffix, but a
    /// small value (e.g. `5n`) would otherwise re-parse as Int — the `L`
    /// suffix pins the type to Long no matter the magnitude, matching
    /// DixScript's own `123L` literal syntax.
    #[wasm_bindgen(js_name = withLong)]
    pub fn with_long(mut self, path: &str, value: i64) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, format!("{}L", value))
    }

    #[wasm_bindgen(js_name = withFloat)]
    pub fn with_float(mut self, path: &str, value: f32) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, format!("{}f", value))
    }

    #[wasm_bindgen(js_name = withDouble)]
    pub fn with_double(mut self, path: &str, value: f64) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, format!("{}", value))
    }

    #[wasm_bindgen(js_name = withBool)]
    pub fn with_bool(mut self, path: &str, value: bool) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, if value { "true".into() } else { "false".into() })
    }

    /// Adds a hex color value. `hex` must start with `#`.
    #[wasm_bindgen(js_name = withHexColor)]
    pub fn with_hex_color(mut self, path: &str, hex: &str) -> Result<MdixBuilder, JsValue> {
        if !hex.starts_with('#') {
            return Err(runtime_err("withHexColor", "hex color must start with '#'"));
        }
        self.add_flat(path, hex.to_string())
    }

    /// Adds a date value. `date` must be in YYYY-MM-DD format.
    #[wasm_bindgen(js_name = withDate)]
    pub fn with_date(mut self, path: &str, date: &str) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, date.to_string())
    }

    /// Adds a timestamp value. `ts` must be ISO 8601.
    #[wasm_bindgen(js_name = withTimestamp)]
    pub fn with_timestamp(mut self, path: &str, ts: &str) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, ts.to_string())
    }

    /// Adds a blob value. `base64` must be valid base64.
    #[wasm_bindgen(js_name = withBlob)]
    pub fn with_blob(mut self, path: &str, base64: &str) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, format!("b:(\"{}\")", base64))
    }

    /// Adds a regex value.
    #[wasm_bindgen(js_name = withRegex)]
    pub fn with_regex(mut self, path: &str, pattern: &str) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, format!("r:(\"{}\")", escape_mdix(pattern)))
    }

    /// Adds an enum reference as a flat property.
    /// Example: `withEnumValue("log_level", "LogLevel", "INFO")`
    /// Produces: `log_level = LogLevel.INFO`
    #[wasm_bindgen(js_name = withEnumValue)]
    pub fn with_enum_value(
        mut self,
        path: &str,
        enum_name: &str,
        field_name: &str,
    ) -> Result<MdixBuilder, JsValue> {
        self.add_flat(path, format!("{}.{}", enum_name, field_name))
    }

    /// Adds a homogeneous scalar array as a flat property.
    /// `items_json` must be a JSON array of scalars: `[1,2,3]` or `["a","b"]`
    #[wasm_bindgen(js_name = withArray)]
    pub fn with_array(mut self, path: &str, items_json: &str) -> Result<MdixBuilder, JsValue> {
        let formatted = format_json_array(items_json)
            .map_err(|e| runtime_err("withArray", e))?;
        self.add_flat(path, formatted)
    }

    /// Adds an inline object literal as a flat property.
    /// `props_json` must be a flat JSON object: `{"host":"localhost","port":8080}`
    #[wasm_bindgen(js_name = withObject)]
    pub fn with_object(mut self, path: &str, props_json: &str) -> Result<MdixBuilder, JsValue> {
        let formatted = format_json_object(props_json)
            .map_err(|e| runtime_err("withObject", e))?;
        self.add_flat(path, formatted)
    }

    /// Adds a tuple (max 6 elements).
    /// `items_json` must be a JSON array: `[1,"hello",true]`
    #[wasm_bindgen(js_name = withTuple)]
    pub fn with_tuple(mut self, path: &str, items_json: &str) -> Result<MdixBuilder, JsValue> {
        let parsed: serde_json::Value = serde_json::from_str(items_json)
            .map_err(|e| runtime_err("withTuple parse", e))?;
        let arr = parsed.as_array()
            .ok_or_else(|| runtime_err("withTuple", "items_json must be a JSON array"))?;
        if arr.len() > 6 {
            return Err(runtime_err("withTuple", "tuples may have at most 6 elements"));
        }
        let items: Vec<String> = arr.iter()
            .map(format_json_scalar)
            .collect::<Result<_, _>>()
            .map_err(|e| runtime_err("withTuple format", e))?;
        self.add_flat(path, format!("t:({})", items.join(", ")))
    }

    // ── @DATA — tier 2: table properties ─────────────────────────────────

    /// Adds a table property block (single-colon syntax).
    /// Produces: `path: key = val, key = val`
    ///
    /// `props_json` must be a flat JSON object: `{"host":"localhost","port":8080}`
    ///
    /// Once this is called, no further flat properties may be added
    /// (two-tier rule enforced).
    #[wasm_bindgen(js_name = withTableProperties)]
    pub fn with_table_properties(
        mut self,
        path: &str,
        props_json: &str,
    ) -> Result<MdixBuilder, JsValue> {
        self.check_valid()?;
        if path.is_empty() {
            return Err(invalid_path_err(path));
        }

        let parsed: serde_json::Value = serde_json::from_str(props_json)
            .map_err(|e| runtime_err("withTableProperties parse", e))?;
        let obj = parsed.as_object()
            .ok_or_else(|| runtime_err("withTableProperties", "props_json must be a JSON object"))?;

        let mut properties: Vec<(String, String)> = Vec::new();
        for (k, v) in obj {
            let formatted = format_json_scalar(v)
                .map_err(|e| runtime_err("withTableProperties format", e))?;
            properties.push((k.clone(), formatted));
        }

        self.has_grouped = true;
        self.tables.push(TableEntry { path: path.to_string(), properties });
        Ok(self)
    }

    /// Adds a group array (double-colon syntax).
    /// Produces: `path:: item, item, item`
    ///
    /// `items_json` must be a JSON array of scalars or objects.
    ///
    /// Once this is called, no further flat properties may be added
    /// (two-tier rule enforced).
    #[wasm_bindgen(js_name = withGroupArray)]
    pub fn with_group_array(
        mut self,
        path: &str,
        items_json: &str,
    ) -> Result<MdixBuilder, JsValue> {
        self.check_valid()?;
        if path.is_empty() {
            return Err(invalid_path_err(path));
        }

        let parsed: serde_json::Value = serde_json::from_str(items_json)
            .map_err(|e| runtime_err("withGroupArray parse", e))?;
        let arr = parsed.as_array()
            .ok_or_else(|| runtime_err("withGroupArray", "items_json must be a JSON array"))?;

        let mut items: Vec<String> = Vec::new();
        for item in arr {
            let formatted = match item {
                serde_json::Value::Object(obj) => {
                    let pairs: Vec<String> = obj.iter()
                        .map(|(k, v)| {
                            format_json_scalar(v)
                                .map(|fv| format!("{} = {}", k, fv))
                        })
                        .collect::<Result<_, _>>()
                        .map_err(|e| runtime_err("withGroupArray format object", e))?;
                    format!("{{ {} }}", pairs.join(", "))
                }
                scalar => format_json_scalar(scalar)
                    .map_err(|e| runtime_err("withGroupArray format scalar", e))?,
            };
            items.push(formatted);
        }

        self.has_grouped = true;
        self.arrays.push(GroupEntry { path: path.to_string(), items });
        Ok(self)
    }

    // ── Serialization ─────────────────────────────────────────────────────

    /// Serializes all sections to a valid .mdix source string.
    #[wasm_bindgen]
    pub fn serialize(&self) -> Result<String, JsValue> {
        self.check_valid()?;
        Ok(serialize_to_mdix(
            &self.config,
            &self.enums,
            &self.flat,
            &self.tables,
            &self.arrays,
        ))
    }

    /// Serializes and loads the result, returning a MdixDatabase.
    #[wasm_bindgen(js_name = toDatabase)]
    pub fn to_database(&self) -> Result<MdixDatabase, JsValue> {
        let src = self.serialize()?;
        MdixDatabase::load_str(&src)
    }

    // ── Private helpers ───────────────────────────────────────────────────

    fn check_valid(&self) -> Result<(), JsValue> {
        if self.valid {
            Ok(())
        } else {
            Err(freed_err("MdixBuilder"))
        }
    }

    fn add_flat(mut self, path: &str, formatted_value: String) -> Result<MdixBuilder, JsValue> {
        self.check_valid()?;
        if path.is_empty() {
            return Err(invalid_path_err(path));
        }
        if self.has_grouped {
            return Err(runtime_err(
                "two-tier violation",
                format!(
                    "cannot add flat property '{}' after table properties or group arrays — \
                     flat properties must come first",
                    path
                ),
            ));
        }
        self.flat.push((path.to_string(), formatted_value));
        Ok(self)
    }
}

// ── File serializer ───────────────────────────────────────────────────────────

fn serialize_to_mdix(
    config: &[(String, String)],
    enums: &[EnumDef],
    flat: &[(String, String)],
    tables: &[TableEntry],
    arrays: &[GroupEntry],
) -> String {
    let mut out = String::with_capacity(512);

    // @CONFIG
    if !config.is_empty() {
        out.push_str("@CONFIG(\n");
        for (k, v) in config {
            out.push_str(&format!("  {} -> {}\n", k, v));
        }
        out.push_str(")\n\n");
    }

    // @ENUMS
    if !enums.is_empty() {
        out.push_str("@ENUMS(\n");
        for def in enums {
            let fields: Vec<String> = def.fields.iter().map(|(name, val)| {
                match val {
                    Some(v) => format!("{} = {}", name, v),
                    None    => name.clone(),
                }
            }).collect();
            out.push_str(&format!("  {} {{ {} }}\n", def.name, fields.join(", ")));
        }
        out.push_str(")\n\n");
    }

    // @DATA
    let has_flat    = !flat.is_empty();
    let has_grouped = !tables.is_empty() || !arrays.is_empty();
    if has_flat || has_grouped {
        out.push_str("@DATA(\n");

        for (k, v) in flat {
            out.push_str(&format!("  {} = {}\n", k, v));
        }

        if has_flat && has_grouped {
            out.push('\n');
        }

        for table in tables {
            let props: Vec<String> = table.properties.iter()
                .map(|(k, v)| format!("{} = {}", k, v))
                .collect();
            out.push_str(&format!("  {}: {}\n", table.path, props.join(", ")));
        }

        for arr in arrays {
            let is_complex = arr.items.iter().any(|i| i.starts_with('{'));
            if is_complex {
                out.push_str(&format!("  {}::\n", arr.path));
                for (i, item) in arr.items.iter().enumerate() {
                    let comma = if i < arr.items.len() - 1 { "," } else { "" };
                    out.push_str(&format!("    {}{}\n", item, comma));
                }
            } else {
                out.push_str(&format!("  {}:: {}\n", arr.path, arr.items.join(", ")));
            }
        }

        out.push(')');
    }

    out.trim_end().to_string()
}

// ── Value formatting helpers ──────────────────────────────────────────────────

fn format_config_string(s: &str) -> String {
    format!("\"{}\"", escape_mdix(s))
}

fn format_json_scalar(value: &serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::Null              => Ok("null".into()),
        serde_json::Value::Bool(b)           => Ok(if *b { "true".into() } else { "false".into() }),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.to_string())
            } else if let Some(f) = n.as_f64() {
                Ok(f.to_string())
            } else {
                Err(format!("unsupported number: {}", n))
            }
        }
        serde_json::Value::String(s) => Ok(format!("\"{}\"", escape_mdix(s))),
        other => Err(format!("expected scalar, got: {}", other)),
    }
}

fn format_json_array(json: &str) -> Result<String, String> {
    let parsed: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| e.to_string())?;
    let arr = parsed.as_array()
        .ok_or_else(|| "expected JSON array".to_string())?;
    let items: Vec<String> = arr.iter()
        .map(format_json_scalar)
        .collect::<Result<_, _>>()?;
    Ok(format!("[{}]", items.join(", ")))
}

fn format_json_object(json: &str) -> Result<String, String> {
    let parsed: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| e.to_string())?;
    let obj = parsed.as_object()
        .ok_or_else(|| "expected JSON object".to_string())?;
    let pairs: Vec<String> = obj.iter()
        .map(|(k, v)| {
            format_json_scalar(v).map(|fv| format!("{} = {}", k, fv))
        })
        .collect::<Result<_, _>>()?;
    Ok(format!("{{ {} }}", pairs.join(", ")))
}

fn escape_mdix(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('"',  "\\\"")
     .replace('\n', "\\n")
     .replace('\r', "\\r")
     .replace('\t', "\\t")
    }
