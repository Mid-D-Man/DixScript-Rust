// mdix-lua/src/value.rs
//
// Conversions between DixValue (Rust) and Lua values, plus a formatter that
// turns Lua values into DixScript literal strings for use in @DATA source.

use mlua::{
    Error as LuaError, Lua, Result as LuaResult,
    Table as LuaTable, Value as LuaValue,
};
use dixscript::Runtime::DixValue;

// ── DixValue → Lua ────────────────────────────────────────────────────────────

/// Convert a loaded DixScript value to the most natural Lua representation.
///
/// - Null           → nil
/// - Bool           → boolean
/// - Int            → integer
/// - Float / Double → number
/// - String, Date, Timestamp, HexColor, Blob, Regex → string
/// - Array / Tuple  → table (1-indexed sequence)
/// - Object         → table (string-keyed hash)
/// - Enum           → table { enum_name, field, value }
pub fn dix_to_lua(lua: &Lua, value: &DixValue) -> LuaResult<LuaValue> {
    match value {
        DixValue::Null => Ok(LuaValue::Nil),
        DixValue::Bool(b) => Ok(LuaValue::Boolean(*b)),
        DixValue::Int(i) => Ok(LuaValue::Integer(*i as i64)),
        // Lua 5.4 integers are already i64, so Long maps over with no loss.
        DixValue::Long(i) => Ok(LuaValue::Integer(*i)),
        DixValue::Float(f) => Ok(LuaValue::Number(*f as f64)),
        DixValue::Double(d) => Ok(LuaValue::Number(*d)),

        DixValue::String(s)
        | DixValue::Date(s)
        | DixValue::Timestamp(s)
        | DixValue::HexColor(s)
        | DixValue::Blob(s)
        | DixValue::Regex(s) => Ok(LuaValue::String(lua.create_string(s.as_bytes())?)),

        DixValue::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.set(i + 1, dix_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }

        DixValue::Object(obj) => {
            let t = lua.create_table()?;
            for (k, v) in obj {
                t.set(k.as_str(), dix_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }

        DixValue::Tuple(items) => {
            let t = lua.create_table()?;
            for (i, v) in items.iter().enumerate() {
                t.set(i + 1, dix_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }

        DixValue::Enum { enum_name, field_name, value } => {
            let t = lua.create_table()?;
            t.set("enum_name", lua.create_string(enum_name.as_bytes())?)?;
            t.set("field",     lua.create_string(field_name.as_bytes())?)?;
            t.set("value",     *value)?;
            Ok(LuaValue::Table(t))
        }
    }
}

// ── Lua → DixScript literal ───────────────────────────────────────────────────

/// Escape a string for embedding inside a DixScript double-quoted literal.
pub fn escape_mdix(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('"',  "\\\"")
     .replace('\n', "\\n")
     .replace('\r', "\\r")
     .replace('\t', "\\t")
}

/// Convert a Lua value to a DixScript literal string that can be embedded
/// directly in @DATA source (e.g. as a flat property value or array item).
///
/// - nil     → `null`
/// - boolean → `true` / `false`
/// - integer → `42`
/// - number  → `3.14`
/// - string  → `"hello"`
/// - table (sequence) → `["a", "b", "c"]`
/// - table (hash)     → `{ key = "val", n = 1 }`
pub fn lua_to_mdix(value: &LuaValue) -> LuaResult<String> {
    match value {
        LuaValue::Nil         => Ok("null".to_string()),
        LuaValue::Boolean(b)  => Ok(if *b { "true".to_string() } else { "false".to_string() }),
        LuaValue::Integer(i)  => Ok(i.to_string()),
        LuaValue::Number(n)   => Ok(n.to_string()),

        LuaValue::String(s) => {
            let str_val = s.to_str()?;
            // FIX: Borrow str_val here to convert BorrowedStr to &str
            Ok(format!("\"{}\"", escape_mdix(&str_val)))
        }

        LuaValue::Table(t) => {
            let t = t.clone();
            if table_is_sequence(&t)? {
                // Emit as a DixScript array literal
                let len = t.len()? as i64;
                let mut items = Vec::with_capacity(len as usize);
                for i in 1..=len {
                    let v: LuaValue = t.get(i)?;
                    items.push(lua_to_mdix(&v)?);
                }
                Ok(format!("[{}]", items.join(", ")))
            } else {
                // Emit as a DixScript inline object literal
                let mut pairs_vec = Vec::new();
                for pair in t.clone().pairs::<LuaValue, LuaValue>() {
                    let (k, v) = pair?;
                    let key = match &k {
                        LuaValue::String(s)  => s.to_str()?.to_string(),
                        LuaValue::Integer(i) => i.to_string(),
                        other => {
                            return Err(LuaError::RuntimeError(format!(
                                "[mdix] object keys must be strings or integers, got {}",
                                other.type_name()
                            )));
                        }
                    };
                    pairs_vec.push(format!("{} = {}", key, lua_to_mdix(&v)?));
                }
                Ok(format!("{{ {} }}", pairs_vec.join(", ")))
            }
        }

        other => Err(LuaError::RuntimeError(format!(
            "[mdix] cannot convert Lua {} to a mdix value",
            other.type_name()
        ))),
    }
}

/// Returns true if the Lua table is a sequence (1-indexed integer keys, no gaps).
/// An empty table is treated as an empty sequence.
pub fn table_is_sequence(table: &LuaTable) -> LuaResult<bool> {
    let len = table.len()? as i64;
    let mut count = 0i64;
    for pair in table.clone().pairs::<LuaValue, LuaValue>() {
        let (k, _) = pair?;
        match k {
            LuaValue::Integer(i) if i >= 1 && i <= len => count += 1,
            _ => return Ok(false),
        }
    }
    Ok(count == len)
            }
