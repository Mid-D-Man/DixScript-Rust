// mdix-lua/src/builder.rs
//
// MdixBuilder — builds .mdix source programmatically and loads it.
//
// Two-tier DATA ordering is enforced: all set_* (flat tier-1) calls must come
// before any with_table() or with_array() (grouped tier-2) calls.
// Violating this raises an error immediately.

use mlua::{
    Error as LuaError, MetaMethod, Result as LuaResult,
    Table as LuaTable, UserData, UserDataMethods, Value as LuaValue,
};
use dixscript::Runtime::{DixLoadOptions, DixLoader};
use crate::database::LuaMdixDatabase;
use crate::error::*;
use crate::value::{escape_mdix, lua_to_mdix, table_is_sequence};

// ── Internal storage types ────────────────────────────────────────────────────

struct EnumEntry {
    name:   String,
    fields: Vec<(String, Option<i32>)>,
}

struct TableEntry {
    path:  String,
    props: Vec<(String, String)>,
}

struct ArrayEntry {
    path:  String,
    items: Vec<String>,
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn lua_key_to_string(k: &LuaValue) -> LuaResult<String> {
    match k {
        LuaValue::String(s)  => Ok(s.to_str()?.to_string()),
        LuaValue::Integer(i) => Ok(i.to_string()),
        other => Err(LuaError::RuntimeError(format!(
            "[mdix] from_table: keys must be strings or integers, got {}",
            other.type_name()
        ))),
    }
}

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct LuaMdixBuilder {
    config:      Vec<(String, String)>,
    enums:       Vec<EnumEntry>,
    flat:        Vec<(String, String)>,
    tables:      Vec<TableEntry>,
    arrays:      Vec<ArrayEntry>,
    has_grouped: bool,
}

impl LuaMdixBuilder {
    pub fn new() -> Self {
        LuaMdixBuilder {
            config:      Vec::new(),
            enums:       Vec::new(),
            flat:        Vec::new(),
            tables:      Vec::new(),
            arrays:      Vec::new(),
            has_grouped: false,
        }
    }

    fn check_flat_ok(&self, prop: &str) -> LuaResult<()> {
        if self.has_grouped {
            Err(two_tier_err(prop))
        } else {
            Ok(())
        }
    }

    /// Build a `LuaMdixBuilder` from a single nested Lua table in one shot
    /// — the dynamic-language counterpart of MidManStudio.Mdix.Core's
    /// reflection-based object serializer (Lua has no static classes or
    /// attributes to reflect over, so the table's own shape stands in for
    /// the schema). The reverse of `Database:to_table()`:
    ///
    ///   local builder = mdix.from_table({
    ///       app_name = "AirStrike",
    ///       port     = 7777,
    ///       tags     = {"alpha", "beta"},
    ///       server   = { host = "localhost", port = 7777 },
    ///   })
    ///   local db = builder:build()
    ///
    /// Top-level scalar entries become flat properties, top-level array
    /// entries become group arrays, and top-level table (hash) entries
    /// become table property blocks. Structures nested *inside* one of
    /// those (e.g. a table inside `server`) are emitted as inline literals
    /// by `lua_to_mdix`, so arbitrary depth still works — only the
    /// top level needs to route into one of DixScript's three section kinds.
    /// Empty nested tables are skipped rather than erroring, so a round
    /// trip through `db:to_table()` never fails on a legitimately empty
    /// nested object.
    pub fn from_table(table: &LuaTable) -> LuaResult<Self> {
        let mut builder = LuaMdixBuilder::new();

        // Pass 1: flat scalars first — required by the two-tier ordering
        // rule (all set_* entries must precede any grouped entry).
        for pair in table.clone().pairs::<LuaValue, LuaValue>() {
            let (k, v) = pair?;
            if matches!(v, LuaValue::Table(_)) {
                continue; // handled in pass 2
            }
            builder.flat.push((lua_key_to_string(&k)?, lua_to_mdix(&v)?));
        }

        // Pass 2: grouped — arrays and nested tables.
        for pair in table.clone().pairs::<LuaValue, LuaValue>() {
            let (k, v) = pair?;
            let path = lua_key_to_string(&k)?;
            if let LuaValue::Table(t) = v {
                if table_is_sequence(&t)? {
                    let len = t.len()? as i64;
                    let mut items = Vec::with_capacity(len as usize);
                    for i in 1..=len {
                        let item: LuaValue = t.get(i)?;
                        items.push(lua_to_mdix(&item)?);
                    }
                    builder.has_grouped = true;
                    builder.arrays.push(ArrayEntry { path, items });
                } else {
                    let mut props = Vec::new();
                    for inner in t.clone().pairs::<LuaValue, LuaValue>() {
                        let (pk, pv) = inner?;
                        props.push((lua_key_to_string(&pk)?, lua_to_mdix(&pv)?));
                    }
                    if !props.is_empty() {
                        builder.has_grouped = true;
                        builder.tables.push(TableEntry { path, props });
                    }
                }
            }
        }

        Ok(builder)
    }

    /// Render all registered sections to .mdix source text.
    pub fn serialize(&self) -> String {
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
        let has_data = !self.flat.is_empty()
            || !self.tables.is_empty()
            || !self.arrays.is_empty();

        if has_data {
            out.push_str("@DATA(\n");

            // Tier 1 — flat properties
            for (k, v) in &self.flat {
                out.push_str(&format!("  {} = {}\n", k, v));
            }

            // Blank line between tiers for readability
            if !self.flat.is_empty() && (!self.tables.is_empty() || !self.arrays.is_empty()) {
                out.push('\n');
            }

            // Tier 2 — table property blocks
            for table in &self.tables {
                let props: Vec<String> = table.props.iter()
                    .map(|(k, v)| format!("{} = {}", k, v))
                    .collect();
                out.push_str(&format!("  {}: {}\n", table.path, props.join(", ")));
            }

            // Tier 2 — group arrays
            for arr in &self.arrays {
                let has_objects = arr.items.iter().any(|i| i.starts_with('{'));
                if has_objects {
                    // Multi-line format for object arrays
                    out.push_str(&format!("  {}::\n", arr.path));
                    for (idx, item) in arr.items.iter().enumerate() {
                        let comma = if idx + 1 < arr.items.len() { "," } else { "" };
                        out.push_str(&format!("    {}{}\n", item, comma));
                    }
                } else {
                    // Single-line format for scalar arrays
                    out.push_str(&format!("  {}:: {}\n", arr.path, arr.items.join(", ")));
                }
            }

            out.push(')');
        }

        out.trim_end().to_string()
    }
}

impl UserData for LuaMdixBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {

        // ── @CONFIG ────────────────────────────────────────────────────────

        /// Add a @CONFIG section entry.
        ///   builder:set_config("version", "1.0.0")
        methods.add_method_mut("set_config", |_, this, (key, value): (String, String)| {
            if key.is_empty() {
                return Err(LuaError::RuntimeError("[mdix] config key cannot be empty".to_string()));
            }
            this.config.push((key, format!("\"{}\"", escape_mdix(&value))));
            Ok(())
        });

        // ── @ENUMS ─────────────────────────────────────────────────────────

        /// Add an enum declaration to the @ENUMS section.
        ///
        /// Auto-increment:
        ///   builder:add_enum("LogLevel", {"DEBUG", "INFO", "WARN", "ERROR"})
        ///
        /// Explicit values (array of {name, value} pairs):
        ///   builder:add_enum("Status", {{"ACTIVE", 1}, {"INACTIVE", 0}})
        methods.add_method_mut("add_enum", |_, this, (name, fields): (String, LuaTable)| {
            if name.is_empty() {
                return Err(LuaError::RuntimeError("[mdix] enum name cannot be empty".to_string()));
            }
            let len = fields.len()? as i64;
            if len == 0 {
                return Err(LuaError::RuntimeError("[mdix] enum must have at least one field".to_string()));
            }
            let mut parsed: Vec<(String, Option<i32>)> = Vec::with_capacity(len as usize);
            for i in 1..=len {
                let item: LuaValue = fields.get(i)?;
                match item {
                    LuaValue::String(s) => {
                        parsed.push((s.to_str()?.to_string(), None));
                    }
                    LuaValue::Table(pair) => {
                        let field_name: String = pair.get(1)?;
                        let field_val:  i32    = pair.get(2)?;
                        parsed.push((field_name, Some(field_val)));
                    }
                    other => {
                        return Err(LuaError::RuntimeError(format!(
                            "[mdix] enum fields must be strings or {{name, value}} pairs, got {}",
                            other.type_name()
                        )));
                    }
                }
            }
            this.enums.push(EnumEntry { name, fields: parsed });
            Ok(())
        });

        // ── @DATA tier 1 — flat properties ─────────────────────────────────
        // All set_* methods must be called before any with_table/with_array.

        /// Set a string flat property.
        ///   builder:set_string("app_name", "AirStrike")
        methods.add_method_mut("set_string", |_, this, (path, value): (String, String)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, format!("\"{}\"", escape_mdix(&value))));
            Ok(())
        });

        /// Set an integer flat property.
        ///   builder:set_int("port", 7777)
        methods.add_method_mut("set_int", |_, this, (path, value): (String, i64)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, value.to_string()));
            Ok(())
        });

        /// Set a 64-bit integer flat property, explicitly typed as Long.
        ///
        /// Values that overflow i32 are auto-promoted to Long by the parser
        /// regardless, but small values (e.g. `5`) would otherwise re-parse
        /// as Int — the `L` suffix pins the type to Long no matter the
        /// magnitude, matching DixScript's own `123L` literal syntax.
        ///
        ///   builder:set_long("created_at_ms", 1750000000000)
        methods.add_method_mut("set_long", |_, this, (path, value): (String, i64)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, format!("{}L", value)));
            Ok(())
        });

        /// Set a double-precision flat property (no type suffix — this is
        /// DixScript's default for any bare decimal literal).
        ///   builder:set_double("gravity", 9.81)
        methods.add_method_mut("set_double", |_, this, (path, value): (String, f64)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, value.to_string()));
            Ok(())
        });

        /// Set a single-precision flat property, explicitly typed as
        /// Float via the `f` suffix — without it, a bare decimal literal
        /// always lexes as Double, matching the `withFloat`/`withDouble`
        /// split already in mdix-wasm.
        ///   builder:set_float("scale", 1.5)
        methods.add_method_mut("set_float", |_, this, (path, value): (String, f32)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, format!("{}f", value)));
            Ok(())
        });

        /// Alias for set_double — kept for backwards compatibility with
        /// code written before set_float/set_double were split out.
        ///   builder:set_number("gravity", 9.81)
        methods.add_method_mut("set_number", |_, this, (path, value): (String, f64)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, value.to_string()));
            Ok(())
        });

        /// Set a boolean flat property.
        ///   builder:set_bool("debug", false)
        methods.add_method_mut("set_bool", |_, this, (path, value): (String, bool)| {
            this.check_flat_ok(&path)?;
            let s = if value { "true" } else { "false" };
            this.flat.push((path, s.to_string()));
            Ok(())
        });

        /// Set a date flat property. value must be "YYYY-MM-DD".
        ///   builder:set_date("release", "2025-12-31")
        methods.add_method_mut("set_date", |_, this, (path, value): (String, String)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, value));
            Ok(())
        });

        /// Set a hex color flat property. value must start with '#'.
        ///   builder:set_hex_color("sky_color", "#87CEEB")
        methods.add_method_mut("set_hex_color", |_, this, (path, value): (String, String)| {
            this.check_flat_ok(&path)?;
            if !value.starts_with('#') {
                return Err(LuaError::RuntimeError(
                    "[mdix] hex color must start with '#', e.g. \"#FF5733\"".to_string()
                ));
            }
            this.flat.push((path, value));
            Ok(())
        });

        /// Set a base64 blob flat property.
        ///   builder:set_blob("icon", "SGVsbG8=")
        methods.add_method_mut("set_blob", |_, this, (path, value): (String, String)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, format!("b:(\"{}\")", value)));
            Ok(())
        });

        /// Set a regex flat property.
        ///   builder:set_regex("email_pattern", "^[a-z@.]+$")
        methods.add_method_mut("set_regex", |_, this, (path, value): (String, String)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, format!("r:(\"{}\")", escape_mdix(&value))));
            Ok(())
        });

        /// Set a flat property that references a named enum field.
        ///   builder:set_enum("log_level", "LogLevel", "INFO")
        ///   -- produces: log_level = LogLevel.INFO
        methods.add_method_mut("set_enum", |_, this, (path, enum_name, field): (String, String, String)| {
            this.check_flat_ok(&path)?;
            this.flat.push((path, format!("{}.{}", enum_name, field)));
            Ok(())
        });

        /// Set a flat property with automatic type detection from Lua value.
        /// Accepts string, integer, number, boolean, nil, or table (array / object).
        ///
        ///   builder:set("port",    7777)
        ///   builder:set("name",    "AirStrike")
        ///   builder:set("flags",   {1, 2, 3})
        ///   builder:set("config",  {host = "localhost", port = 7777})
        methods.add_method_mut("set", |_, this, (path, value): (String, LuaValue)| {
            this.check_flat_ok(&path)?;
            let formatted = lua_to_mdix(&value)?;
            this.flat.push((path, formatted));
            Ok(())
        });

        // ── @DATA tier 2 — grouped ─────────────────────────────────────────
        // Must be called after all set_* calls.

        /// Add a table property block (single-colon syntax).
        /// The table argument must be a Lua table with string keys.
        ///
        ///   builder:with_table("server", {host = "localhost", port = 7777, ssl = false})
        ///   -- produces: server: host = "localhost", port = 7777, ssl = false
        methods.add_method_mut("with_table", |_, this, (path, props): (String, LuaTable)| {
            if path.is_empty() {
                return Err(LuaError::RuntimeError("[mdix] path cannot be empty".to_string()));
            }
            let mut properties: Vec<(String, String)> = Vec::new();
            for pair in props.clone().pairs::<LuaValue, LuaValue>() {
                let (k, v) = pair?;
                let key = match &k {
                    LuaValue::String(s)  => s.to_str()?.to_string(),
                    LuaValue::Integer(i) => i.to_string(),
                    other => {
                        return Err(LuaError::RuntimeError(format!(
                            "[mdix] with_table: property keys must be strings, got {}",
                            other.type_name()
                        )));
                    }
                };
                properties.push((key, lua_to_mdix(&v)?));
            }
            if properties.is_empty() {
                return Err(LuaError::RuntimeError(
                    "[mdix] with_table: table must have at least one property".to_string()
                ));
            }
            this.has_grouped = true;
            this.tables.push(TableEntry { path, props: properties });
            Ok(())
        });

        /// Add a group array (double-colon syntax).
        /// Items may be scalars or tables (which become inline object literals).
        ///
        ///   builder:with_array("tags", {"alpha", "beta", "rc"})
        ///   -- produces: tags:: "alpha", "beta", "rc"
        ///
        ///   builder:with_array("enemies", {
        ///       {name = "Goblin", hp = 50},
        ///       {name = "Orc",    hp = 100},
        ///   })
        ///   -- produces:
        ///   -- enemies::
        ///   --   { name = "Goblin", hp = 50 },
        ///   --   { name = "Orc", hp = 100 }
        methods.add_method_mut("with_array", |_, this, (path, items): (String, LuaTable)| {
            if path.is_empty() {
                return Err(LuaError::RuntimeError("[mdix] path cannot be empty".to_string()));
            }
            let len = items.len()? as i64;
            let mut formatted: Vec<String> = Vec::with_capacity(len as usize);
            for i in 1..=len {
                let v: LuaValue = items.get(i)?;
                formatted.push(lua_to_mdix(&v)?);
            }
            this.has_grouped = true;
            this.arrays.push(ArrayEntry { path, items: formatted });
            Ok(())
        });

        // ── Finalization ────────────────────────────────────────────────────

        /// Returns the .mdix source string without loading it.
        /// Useful for debugging or saving to disk.
        methods.add_method("serialize", |_, this, ()| {
            Ok(this.serialize())
        });

        /// Build and load: serialize to .mdix source, then parse and return a
        /// MdixDatabase. Raises an error on empty builder or parse failure.
        ///
        ///   local db = builder:build()
        ///   local port = db:get_int("port")
        methods.add_method("build", |_, this, ()| {
            let src = this.serialize();
            if src.is_empty() {
                return Err(LuaError::RuntimeError(
                    "[mdix] builder has no data — call set_* or with_* before build()".to_string()
                ));
            }
            let data = DixLoader::new()
                .load_from_str(&src, &DixLoadOptions::new())
                .map_err(|e| mdix_err("build", e))?;
            Ok(LuaMdixDatabase::from_data(data))
        });

        /// Clear only the tier-2 grouped data (tables and arrays).
        /// Flat properties, config, and enums are kept.
        /// Useful for re-using the builder with a different grouped section.
        methods.add_method_mut("reset_grouped", |_, this, ()| {
            this.tables.clear();
            this.arrays.clear();
            this.has_grouped = false;
            Ok(())
        });

        /// Reset everything: flat, grouped, config, and enums.
        methods.add_method_mut("reset", |_, this, ()| {
            *this = LuaMdixBuilder::new();
            Ok(())
        });

        // ── Meta ─────────────────────────────────────────────────────────────

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "MdixBuilder(flat={}, tables={}, arrays={})",
                this.flat.len(), this.tables.len(), this.arrays.len()
            ))
        });
    }
        }
