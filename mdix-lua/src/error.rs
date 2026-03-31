// mdix-lua/src/error.rs

use mlua::Error as LuaError;

/// Wrap a runtime error with a [mdix:context] prefix.
pub fn mdix_err(context: &str, detail: impl std::fmt::Display) -> LuaError {
    LuaError::RuntimeError(format!("[mdix:{}] {}", context, detail))
}

/// Raised when a dotted path does not exist in the database.
pub fn not_found_err(path: &str) -> LuaError {
    LuaError::RuntimeError(format!("[mdix] path not found: '{}'", path))
}

/// Raised when a method is called on a closed database.
pub fn closed_err() -> LuaError {
    LuaError::RuntimeError("[mdix] database has been closed".to_string())
}

/// Raised when the two-tier DATA ordering rule is violated in the builder.
pub fn two_tier_err(prop: &str) -> LuaError {
    LuaError::RuntimeError(format!(
        "[mdix] cannot add flat property '{}' after with_table() or with_array() \
         — all flat (set_*) properties must come before grouped (with_*) calls",
        prop
    ))
}
