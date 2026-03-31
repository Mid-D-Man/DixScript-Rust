// mdix-lua/src/database.rs
//
// MdixDatabase — a loaded, read-only DixScript database.
// Wraps DixData with Lua UserData so Lua scripts can call typed getters,
// inspect structure, and export to JSON / TOML / .mdix.

use mlua::{
    Error as LuaError, Lua, MetaMethod, Result as LuaResult,
    Table as LuaTable, UserData, UserDataMethods, Value as LuaValue,
};
use dixscript::Runtime::{
    DixConverter, DixData, DixFormatOptions, DixValue,
};
use crate::error::*;
use crate::value::dix_to_lua;

pub struct LuaMdixDatabase {
    pub(crate) inner: Option<DixData>,
}

impl LuaMdixDatabase {
    pub fn from_data(data: DixData) -> Self {
        LuaMdixDatabase { inner: Some(data) }
    }

    fn data(&self) -> LuaResult<&DixData> {
        self.inner.as_ref().ok_or_else(closed_err)
    }
}

impl UserData for LuaMdixDatabase {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {

        // ── Lifecycle ──────────────────────────────────────────────────────

        /// Close the database early. After this all methods raise an error.
        /// Not required — the GC will clean up automatically.
        methods.add_method_mut("close", |_, this, ()| {
            this.inner = None;
            Ok(())
        });

        // ── Metadata ───────────────────────────────────────────────────────

        /// Total number of entries in the flattened data store.
        methods.add_method("entry_count", |_, this, ()| {
            Ok(this.data()?.entry_count() as i64)
        });

        // ── Inspection ─────────────────────────────────────────────────────

        /// Returns true if the dotted path exists.
        methods.add_method("exists", |_, this, path: String| {
            Ok(this.data()?.exists(&path))
        });

        /// Returns the type name of the value at path:
        /// "int", "string", "bool", "float", "double", "array", "object",
        /// "enum", "date", "timestamp", "hex_color", "blob", "regex",
        /// "tuple", "null", or "unknown" (path not found).
        methods.add_method("get_type", |_, this, path: String| {
            let data = this.data()?;
            let t = match data.get_value(&path) {
                None                         => "unknown",
                Some(DixValue::Null)         => "null",
                Some(DixValue::Bool(_))      => "bool",
                Some(DixValue::Int(_))       => "int",
                Some(DixValue::Float(_))     => "float",
                Some(DixValue::Double(_))    => "double",
                Some(DixValue::String(_))    => "string",
                Some(DixValue::Date(_))      => "date",
                Some(DixValue::Timestamp(_)) => "timestamp",
                Some(DixValue::HexColor(_))  => "hex_color",
                Some(DixValue::Blob(_))      => "blob",
                Some(DixValue::Regex(_))     => "regex",
                Some(DixValue::Array(_))     => "array",
                Some(DixValue::Object(_))    => "object",
                Some(DixValue::Tuple(_))     => "tuple",
                Some(DixValue::Enum { .. })  => "enum",
            };
            Ok(t)
        });

        /// Number of items in the array at path. Returns -1 if not an array.
        methods.add_method("array_length", |_, this, path: String| {
            let data = this.data()?;
            Ok(match data.get_value(&path) {
                Some(DixValue::Array(a)) => a.len() as i64,
                _                        => -1i64,
            })
        });

        /// Direct child key names under prefix. Pass "" or no arg for top-level keys.
        ///
        ///   local top   = db:keys()           -- top-level keys
        ///   local inner = db:keys("server")   -- children of "server"
        methods.add_method("keys", |lua, this, prefix: Option<String>| {
            let data = this.data()?;
            let prefix_str = prefix.as_deref().unwrap_or("");
            let keys = data.get_keys(prefix_str);
            let t: LuaTable = lua.create_table()?;
            for (i, k) in keys.into_iter().enumerate() {
                t.set(i + 1, k)?;
            }
            Ok(t)
        });

        // ── Typed getters ───────────────────────────────────────────────────

        /// Get any value, auto-converted to the best Lua type.
        /// Returns nil if path does not exist.
        ///
        ///   local port = db:get("port")          -- integer
        ///   local name = db:get("app_name")      -- string
        ///   local cfg  = db:get("server")        -- table
        ///   local arr  = db:get("enemies")       -- table (sequence)
        ///   local lv   = db:get("log_level")     -- table { enum_name, field, value }
        methods.add_method("get", |lua, this, path: String| {
            let data = this.data()?;
            match data.get_value(&path) {
                None    => Ok(LuaValue::Nil),
                Some(v) => dix_to_lua(lua, v),
            }
        });

        /// Get a string value.
        /// Raises an error if path does not exist (unless a default is provided).
        ///
        ///   local name = db:get_string("app_name")
        ///   local host = db:get_string("server.host", "localhost")
        methods.add_method("get_string", |_, this, (path, default): (String, Option<String>)| {
            let data = this.data()?;
            match data.get::<String>(&path) {
                Ok(v)  => Ok(v),
                Err(e) => match default {
                    Some(d) => Ok(d),
                    None    => Err(mdix_err("get_string", e)),
                },
            }
        });

        /// Get an integer value (returned as Lua integer / i64).
        ///
        ///   local port = db:get_int("port")
        ///   local cap  = db:get_int("max_players", 100)
        methods.add_method("get_int", |_, this, (path, default): (String, Option<i64>)| {
            let data = this.data()?;
            match data.get::<i32>(&path) {
                Ok(v)  => Ok(v as i64),
                Err(e) => match default {
                    Some(d) => Ok(d),
                    None    => Err(mdix_err("get_int", e)),
                },
            }
        });

        /// Get a float or double value (returned as Lua number / f64).
        ///
        ///   local gravity = db:get_number("gravity")
        ///   local scale   = db:get_number("ui.scale", 1.0)
        methods.add_method("get_number", |_, this, (path, default): (String, Option<f64>)| {
            let data = this.data()?;
            match data.get::<f64>(&path) {
                Ok(v)  => Ok(v),
                Err(e) => match default {
                    Some(d) => Ok(d),
                    None    => Err(mdix_err("get_number", e)),
                },
            }
        });

        /// Get a boolean value.
        ///
        ///   local debug = db:get_bool("debug")
        ///   local ssl   = db:get_bool("server.ssl", false)
        methods.add_method("get_bool", |_, this, (path, default): (String, Option<bool>)| {
            let data = this.data()?;
            match data.get::<bool>(&path) {
                Ok(v)  => Ok(v),
                Err(e) => match default {
                    Some(d) => Ok(d),
                    None    => Err(mdix_err("get_bool", e)),
                },
            }
        });

        /// Serialize the value at path to a compact JSON string.
        /// Useful for complex nested values or blobs.
        methods.add_method("get_json", |_, this, path: String| {
            let data = this.data()?;
            match data.get_value(&path) {
                None    => Err(not_found_err(&path)),
                Some(v) => serde_json::to_string(v).map_err(|e| mdix_err("get_json", e)),
            }
        });

        // ── Export ──────────────────────────────────────────────────────────

        /// Export all entries as a JSON string.
        /// Pass false for compact output (default: pretty-printed).
        methods.add_method("to_json", |_, this, indented: Option<bool>| {
            let data    = this.data()?;
            let entries = data.to_hashmap();
            let conv    = DixConverter::new();
            let ast     = conv.from_hashmap(entries).map_err(|e| mdix_err("to_json", e))?;
            let map     = conv.to_hashmap(&ast);
            if indented.unwrap_or(true) {
                serde_json::to_string_pretty(&map)
            } else {
                serde_json::to_string(&map)
            }
            .map_err(|e| mdix_err("to_json", e))
        });

        /// Export all entries as a TOML string.
        methods.add_method("to_toml", |_, this, ()| {
            let data    = this.data()?;
            let entries = data.to_hashmap();
            let conv    = DixConverter::new();
            let ast     = conv.from_hashmap(entries).map_err(|e| mdix_err("to_toml", e))?;
            conv.to_toml(&ast).map_err(|e| mdix_err("to_toml", e))
        });

        /// Serialize back to a .mdix source string.
        methods.add_method("to_mdix", |_, this, ()| {
            let data    = this.data()?;
            let entries = data.to_hashmap();
            let conv    = DixConverter::new();
            let ast     = conv.from_hashmap(entries).map_err(|e| mdix_err("to_mdix", e))?;
            conv.to_mdix(&ast, Some(&DixFormatOptions::pretty()))
                .map_err(|e| mdix_err("to_mdix", e))
        });

        // ── Meta ────────────────────────────────────────────────────────────

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(match &this.inner {
                Some(d) => format!("MdixDatabase(entries={})", d.entry_count()),
                None    => "MdixDatabase(closed)".to_string(),
            })
        });
    }
              }
