//! MdixBuilder — programmatic .mdix builder with two-tier ordering enforcement.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use dixscript::Runtime::{DixLoadOptions, DixLoader};
use crate::database::MdixDatabase;
use crate::error::{to_py_err, two_tier_err};
use crate::result::MdixResult;

// ── Internal storage types ─────────────────────────────────────────────────

struct EnumDef {
    name:   String,
    fields: Vec<(String, Option<i32>)>,
}

struct TableEntry {
    path:       String,
    properties: Vec<(String, String)>,
}

struct GroupEntry {
    path:  String,
    items: Vec<String>,
}

// ── Builder ────────────────────────────────────────────────────────────────

/// Programmatic builder for `.mdix` databases.
///
/// Enforces two-tier DATA ordering: all flat (tier-1) properties must be added
/// before any table properties or group arrays (tier-2). Violating this raises
/// `MdixError` immediately.
///
/// ```python
/// from midmanstudio.mdix import MdixBuilder
///
/// db = (MdixBuilder()
///       .set_config("version", "1.0.0")
///       .add_enum("LogLevel", ["DEBUG", "INFO", "WARN", "ERROR"])
///       # --- tier 1: flat properties ---
///       .set_string("app_name", "MyGame")
///       .set_int("port", 8080)
///       .set_bool("ssl", True)
///       # --- tier 2: grouped ---
///       .with_table_properties("server", {"host": "localhost", "port": 8080})
///       .with_group_array("enemies", [{"name": "Goblin", "hp": 50}])
///       .to_database())
/// ```
#[pyclass(module = "midmanstudio.mdix")]
pub struct MdixBuilder {
    config:      Vec<(String, String)>,
    enums:       Vec<EnumDef>,
    flat:        Vec<(String, String)>,
    tables:      Vec<TableEntry>,
    arrays:      Vec<GroupEntry>,
    has_grouped: bool,
}

impl MdixBuilder {
    fn check_flat_allowed(&self, property_name: &str) -> PyResult<()> {
        if self.has_grouped {
            Err(two_tier_err(property_name))
        } else {
            Ok(())
        }
    }

    fn serialize_internal(&self) -> String {
        let mut out = String::with_capacity(512);

        // @CONFIG
        if !self.config.is_empty() {
            out.push_str("@CONFIG(\n");
            for (k, v) in &self.config {
                out.push_str(&format!("  {} -> {}\n", k, v));
            }
            out.push_str(")\n\n");
        }

        // @ENUMS
        if !self.enums.is_empty() {
            out.push_str("@ENUMS(\n");
            for def in &self.enums {
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
        let has_flat    = !self.flat.is_empty();
        let has_grouped = !self.tables.is_empty() || !self.arrays.is_empty();

        if has_flat || has_grouped {
            out.push_str("@DATA(\n");

            // Tier 1 — flat properties first
            for (k, v) in &self.flat {
                out.push_str(&format!("  {} = {}\n", k, v));
            }

            if has_flat && has_grouped {
                out.push('\n');
            }

            // Tier 2 — table properties
            for table in &self.tables {
                let props: Vec<String> = table.properties.iter()
                    .map(|(k, v)| format!("{} = {}", k, v))
                    .collect();
                out.push_str(&format!("  {}: {}\n", table.path, props.join(", ")));
            }

            // Tier 2 — group arrays
            for arr in &self.arrays {
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
}

// ── Value formatting helpers ───────────────────────────────────────────────

fn escape_mdix(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('"',  "\\\"")
     .replace('\n', "\\n")
     .replace('\r', "\\r")
     .replace('\t', "\\t")
}

fn format_py_scalar(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(if b { "true".into() } else { "false".into() });
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(i.to_string());
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(f.to_string());
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(format!("\"{}\"", escape_mdix(&s)));
    }
    Err(to_py_err(format!("Cannot format Python value as .mdix scalar: {}", obj.repr()?)))
}

fn format_py_dict_as_object(d: &Bound<'_, PyDict>) -> PyResult<String> {
    let mut pairs = Vec::new();
    for (k, v) in d.iter() {
        let key = k.extract::<String>()?;
        let val = format_py_scalar(&v)?;
        pairs.push(format!("{} = {}", key, val));
    }
    Ok(format!("{{ {} }}", pairs.join(", ")))
}

fn format_py_value_for_array(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(d) = obj.downcast::<PyDict>() {
        return format_py_dict_as_object(d);
    }
    format_py_scalar(obj)
}

#[pymethods]
impl MdixBuilder {
    #[new]
    fn new() -> Self {
        MdixBuilder {
            config:      Vec::new(),
            enums:       Vec::new(),
            flat:        Vec::new(),
            tables:      Vec::new(),
            arrays:      Vec::new(),
            has_grouped: false,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "MdixBuilder(flat={}, tables={}, arrays={})",
            self.flat.len(),
            self.tables.len(),
            self.arrays.len(),
        )
    }

    // ── @CONFIG ────────────────────────────────────────────────────────────

    /// Add a key to the `@CONFIG` section.
    ///
    /// ```python
    /// builder.set_config("version", "1.0.0").set_config("author", "MidManStudio")
    /// ```
    fn set_config(
        mut slf: PyRefMut<'_, Self>,
        key: &str,
        value: &str,
    ) -> PyResult<Py<Self>> {
        if key.is_empty() {
            return Err(to_py_err("[mdix] Config key cannot be empty"));
        }
        slf.config.push((key.to_string(), format!("\"{}\"", escape_mdix(value))));
        Ok(slf.into())
    }

    // ── @ENUMS ─────────────────────────────────────────────────────────────

    /// Add an enum to the `@ENUMS` section.
    ///
    /// `fields` must be one of:
    /// - `["DEBUG", "INFO", "WARN"]` — auto-increment values
    /// - `[("DEBUG", 0), ("INFO", 1)]` — explicit integer values
    ///
    /// ```python
    /// builder.add_enum("LogLevel", ["DEBUG", "INFO", "WARN", "ERROR"])
    /// builder.add_enum("Status", [("ACTIVE", 1), ("INACTIVE", 0)])
    /// ```
    fn add_enum(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        name: &str,
        fields: &Bound<'_, PyList>,
    ) -> PyResult<Py<Self>> {
        if name.is_empty() {
            return Err(to_py_err("[mdix] Enum name cannot be empty"));
        }
        if fields.is_empty() {
            return Err(to_py_err("[mdix] Enum must have at least one field"));
        }

        let mut parsed: Vec<(String, Option<i32>)> = Vec::new();

        for item in fields.iter() {
            if let Ok(s) = item.extract::<String>() {
                parsed.push((s, None));
            } else if let Ok(t) = item.downcast::<PyTuple>() {
                if t.len() == 2 {
                    let field_name: String = t.get_item(0)?.extract()?;
                    let field_val: i32     = t.get_item(1)?.extract()?;
                    parsed.push((field_name, Some(field_val)));
                } else {
                    return Err(to_py_err(
                        "[mdix] Enum field tuple must be (name, value)"
                    ));
                }
            } else {
                return Err(to_py_err(
                    "[mdix] Enum fields must be strings or (name, int) tuples"
                ));
            }
        }

        slf.enums.push(EnumDef { name: name.to_string(), fields: parsed });
        Ok(slf.into())
    }

    // ── @DATA tier 1 — flat properties ────────────────────────────────────

    /// Add a string flat property. Must be called before any `with_*` call.
    fn set_string(
        mut slf: PyRefMut<'_, Self>,
        path: &str,
        value: &str,
    ) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((
            path.to_string(),
            format!("\"{}\"", escape_mdix(value)),
        ));
        Ok(slf.into())
    }

    /// Add an integer flat property.
    fn set_int(mut slf: PyRefMut<'_, Self>, path: &str, value: i32) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((path.to_string(), value.to_string()));
        Ok(slf.into())
    }

    /// Add a float flat property.
    fn set_float(mut slf: PyRefMut<'_, Self>, path: &str, value: f32) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((path.to_string(), format!("{}f", value)));
        Ok(slf.into())
    }

    /// Add a double flat property.
    fn set_double(mut slf: PyRefMut<'_, Self>, path: &str, value: f64) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((path.to_string(), value.to_string()));
        Ok(slf.into())
    }

    /// Add a boolean flat property.
    fn set_bool(mut slf: PyRefMut<'_, Self>, path: &str, value: bool) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((
            path.to_string(),
            if value { "true".into() } else { "false".into() },
        ));
        Ok(slf.into())
    }

    /// Add a date flat property. `value` must be `"YYYY-MM-DD"`.
    fn set_date(mut slf: PyRefMut<'_, Self>, path: &str, value: &str) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((path.to_string(), value.to_string()));
        Ok(slf.into())
    }

    /// Add a timestamp flat property. `value` must be ISO 8601.
    fn set_timestamp(
        mut slf: PyRefMut<'_, Self>,
        path: &str,
        value: &str,
    ) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((path.to_string(), value.to_string()));
        Ok(slf.into())
    }

    /// Add a hex color flat property. `value` must start with `"#"`.
    fn set_hex_color(
        mut slf: PyRefMut<'_, Self>,
        path: &str,
        value: &str,
    ) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        if !value.starts_with('#') {
            return Err(to_py_err("[mdix] Hex color must start with '#'"));
        }
        slf.flat.push((path.to_string(), value.to_string()));
        Ok(slf.into())
    }

    /// Add a blob flat property. `base64_data` must be valid base64.
    fn set_blob(
        mut slf: PyRefMut<'_, Self>,
        path: &str,
        base64_data: &str,
    ) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((
            path.to_string(),
            format!("b:(\"{}\")", base64_data),
        ));
        Ok(slf.into())
    }

    /// Add a regex flat property.
    fn set_regex(
        mut slf: PyRefMut<'_, Self>,
        path: &str,
        pattern: &str,
    ) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((
            path.to_string(),
            format!("r:(\"{}\")", escape_mdix(pattern)),
        ));
        Ok(slf.into())
    }

    /// Add an enum reference flat property.
    ///
    /// ```python
    /// builder.set_enum("log_level", "LogLevel", "INFO")
    /// # produces: log_level = LogLevel.INFO
    /// ```
    fn set_enum(
        mut slf: PyRefMut<'_, Self>,
        path: &str,
        enum_name: &str,
        field_name: &str,
    ) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        slf.flat.push((
            path.to_string(),
            format!("{}.{}", enum_name, field_name),
        ));
        Ok(slf.into())
    }

    /// Add a flat array property.
    ///
    /// ```python
    /// builder.set_array("ids", [1, 2, 3])
    /// builder.set_array("tags", ["alpha", "beta"])
    /// ```
    fn set_array(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        path: &str,
        items: &Bound<'_, PyList>,
    ) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        let formatted: Vec<String> = items
            .iter()
            .map(|item| format_py_scalar(&item))
            .collect::<PyResult<_>>()?;
        slf.flat.push((path.to_string(), format!("[{}]", formatted.join(", "))));
        Ok(slf.into())
    }

    /// Add a tuple flat property. Maximum 6 elements.
    ///
    /// ```python
    /// builder.set_tuple("coords", [10, 20, 30])
    /// ```
    fn set_tuple(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        path: &str,
        items: &Bound<'_, PyList>,
    ) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        if items.len() > 6 {
            return Err(to_py_err("[mdix] Tuples may have at most 6 elements"));
        }
        let formatted: Vec<String> = items
            .iter()
            .map(|item| format_py_scalar(&item))
            .collect::<PyResult<_>>()?;
        slf.flat.push((path.to_string(), format!("t:({})", formatted.join(", "))));
        Ok(slf.into())
    }

    /// Add an inline object literal flat property.
    ///
    /// ```python
    /// builder.set_object("config", {"host": "localhost", "port": 8080})
    /// # produces: config = { host = "localhost", port = 8080 }
    /// ```
    fn set_object(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        path: &str,
        props: &Bound<'_, PyDict>,
    ) -> PyResult<Py<Self>> {
        slf.check_flat_allowed(path)?;
        let formatted = format_py_dict_as_object(props)?;
        slf.flat.push((path.to_string(), formatted));
        Ok(slf.into())
    }

    // ── @DATA tier 2 — grouped properties ─────────────────────────────────

    /// Add a table property block (single-colon syntax).
    ///
    /// Accepts a `dict` of properties, or keyword arguments, or both.
    /// Once called, no further flat properties may be added.
    ///
    /// ```python
    /// # dict style
    /// builder.with_table_properties("server", {"host": "localhost", "port": 8080})
    /// # kwargs style
    /// builder.with_table_properties("server", host="localhost", port=8080)
    /// # produces: server: host = "localhost", port = 8080
    /// ```
    #[pyo3(signature = (path, props = None, **kwargs))]
    fn with_table_properties(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        path: &str,
        props: Option<&Bound<'_, PyDict>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<Self>> {
        if path.is_empty() {
            return Err(to_py_err("[mdix] Table path cannot be empty"));
        }

        let mut properties: Vec<(String, String)> = Vec::new();

        if let Some(d) = props {
            for (k, v) in d.iter() {
                let key = k.extract::<String>()?;
                let val = format_py_scalar(&v)?;
                properties.push((key, val));
            }
        }

        if let Some(kw) = kwargs {
            for (k, v) in kw.iter() {
                let key = k.extract::<String>()?;
                let val = format_py_scalar(&v)?;
                properties.push((key, val));
            }
        }

        if properties.is_empty() {
            return Err(to_py_err(
                "[mdix] with_table_properties requires at least one property"
            ));
        }

        slf.has_grouped = true;
        slf.tables.push(TableEntry { path: path.to_string(), properties });
        Ok(slf.into())
    }

    /// Add a group array (double-colon syntax).
    ///
    /// `items` is a list of scalars or dicts.
    /// Once called, no further flat properties may be added.
    ///
    /// ```python
    /// # scalar array
    /// builder.with_group_array("tags", ["alpha", "beta", "gamma"])
    /// # object array
    /// builder.with_group_array("enemies", [
    ///     {"name": "Goblin", "hp": 50},
    ///     {"name": "Orc",    "hp": 100},
    /// ])
    /// # produces:
    /// # enemies::
    /// #   { name = "Goblin", hp = 50 },
    /// #   { name = "Orc", hp = 100 }
    /// ```
    fn with_group_array(
        mut slf: PyRefMut<'_, Self>,
        py: Python<'_>,
        path: &str,
        items: &Bound<'_, PyList>,
    ) -> PyResult<Py<Self>> {
        if path.is_empty() {
            return Err(to_py_err("[mdix] Group array path cannot be empty"));
        }

        let formatted: Vec<String> = items
            .iter()
            .map(|item| format_py_value_for_array(&item))
            .collect::<PyResult<_>>()?;

        slf.has_grouped = true;
        slf.arrays.push(GroupEntry { path: path.to_string(), items: formatted });
        Ok(slf.into())
    }

    // ── Finalization ───────────────────────────────────────────────────────

    /// Serialize all sections to a `.mdix` source string.
    fn serialize(&self) -> String {
        self.serialize_internal()
    }

    /// Build and load the database. Raises `MdixError` on failure.
    fn to_database(&self) -> PyResult<MdixDatabase> {
        let src = self.serialize_internal();
        let loader = DixLoader::new();
        loader
            .load_from_str(&src, &DixLoadOptions::new())
            .map(|data| MdixDatabase::from_data_pub(data))
            .map_err(|e| crate::error::runtime_err("to_database", e))
    }

    /// Build and load the database — railway variant. Never raises.
    fn try_to_database(&self, py: Python<'_>) -> MdixResult {
        match self.to_database() {
            Ok(db)  => MdixResult::ok(py, db),
            Err(e)  => MdixResult::from_py_err(e),
        }
    }

    /// Resets all tier-2 grouped data. Flat properties and config are kept.
    /// Useful for re-using a builder with different grouped sections.
    fn reset_grouped(&mut self) {
        self.tables.clear();
        self.arrays.clear();
        self.has_grouped = false;
    }

    /// Resets the entire builder to its initial empty state.
    fn reset(&mut self) {
        self.config.clear();
        self.enums.clear();
        self.flat.clear();
        self.tables.clear();
        self.arrays.clear();
        self.has_grouped = false;
    }
                 }
