// mdix-lua/src/lib.rs
//
// Lua 5.4 module entrypoint — called by Lua's require("mdix").
//
// Exposes these module-level symbols:
//
//   mdix.version                                     -- string
//   mdix.load(path)                                  -- MdixDatabase
//   mdix.load_str(source)                            -- MdixDatabase
//   mdix.load_encrypted(enc_path [, key_path])       -- MdixDatabase
//   mdix.load_encrypted_password(enc_path, password) -- MdixDatabase
//   mdix.from_json(json)                             -- MdixDatabase
//   mdix.from_toml(toml)                             -- MdixDatabase
//   mdix.builder()                                   -- MdixBuilder
//   mdix.minify_source(source)                       -- string
//   mdix.format_source(source)                       -- string
//
// All database-access methods are on MdixDatabase userdata objects.
// All builder methods are on MdixBuilder userdata objects.
// See database.rs and builder.rs for the full method surfaces.
//
// Cargo.toml crate-type is ["cdylib", "rlib"].
//   cdylib → mdix.so / mdix.dylib / mdix.dll  (loaded by require)
//   rlib   → used if another Rust crate depends on this package
//
// Rename / symlink the output before placing on package.cpath:
//   Linux:  target/release/libmdix.so  → mdix.so
//   macOS:  target/release/libmdix.dylib → mdix.so
//   Windows: target/release/mdix.dll  (no rename needed)

mod builder;
mod database;
mod error;
mod value;

use mlua::prelude::*;
use dixscript::Runtime::{
    DixCompactor, DixConverter, DixFormatOptions, DixLoadOptions, DixLoader,
};

use builder::LuaMdixBuilder;
use database::LuaMdixDatabase;
use error::mdix_err;

/// Module entry point — Lua calls luaopen_mdix when require("mdix") is evaluated.
///
/// Returns a table containing all factory functions and the version string.
/// Individual database/builder methods are registered via UserData, not here.
#[mlua::lua_module]
fn mdix(lua: &Lua) -> LuaResult<LuaTable> {
    let m = lua.create_table()?;

    // ── Version ────────────────────────────────────────────────────────────

    m.set("version", "1.0.0")?;

    // ── Plain .mdix loading ────────────────────────────────────────────────

    /// Load a plain .mdix file from disk.
    ///
    ///   local db = mdix.load("config.mdix")
    m.set("load", lua.create_function(|_, path: String| {
        if path.trim().is_empty() {
            return Err(LuaError::RuntimeError(
                "[mdix:load] path cannot be empty".into(),
            ));
        }
        DixLoader::new()
            .load_text(&path, &DixLoadOptions::new())
            .map(LuaMdixDatabase::from_data)
            .map_err(|e| mdix_err("load", e))
    })?)?;

    /// Load .mdix from a raw source string — no disk access.
    /// Useful for TextAssets (Unity), bundled resources, test strings.
    ///
    ///   local db = mdix.load_str('@DATA( port = 8080, host = "localhost" )')
    m.set("load_str", lua.create_function(|_, source: String| {
        if source.trim().is_empty() {
            return Err(LuaError::RuntimeError(
                "[mdix:load_str] source cannot be empty".into(),
            ));
        }
        DixLoader::new()
            .load_from_str(&source, &DixLoadOptions::new())
            .map(LuaMdixDatabase::from_data)
            .map_err(|e| mdix_err("load_str", e))
    })?)?;

    // ── Encrypted .mdix.enc loading ────────────────────────────────────────

    /// Load an encrypted .mdix.enc file.
    ///
    /// key_path is optional: omit or pass nil to auto-detect the .mdix.key
    /// file next to the .enc file.
    ///
    ///   local db = mdix.load_encrypted("secrets.mdix.enc")
    ///   local db = mdix.load_encrypted("secrets.mdix.enc", "/secure/secrets.mdix.key")
    m.set("load_encrypted",
        lua.create_function(|_, (enc_path, key_path): (String, Option<String>)| {
            if enc_path.trim().is_empty() {
                return Err(LuaError::RuntimeError(
                    "[mdix:load_encrypted] enc_path cannot be empty".into(),
                ));
            }
            let mut opts = DixLoadOptions::new();
            if let Some(kp) = key_path {
                let kp = kp.trim().to_string();
                if !kp.is_empty() {
                    opts.key_file_path = Some(kp);
                }
            }
            DixLoader::new()
                .load_encrypted(&enc_path, &opts)
                .map(LuaMdixDatabase::from_data)
                .map_err(|e| mdix_err("load_encrypted", e))
        })?
    )?;

    /// Load an encrypted .mdix.enc file using a password for key derivation.
    ///
    ///   local db = mdix.load_encrypted_password("secrets.mdix.enc", "my-password")
    m.set("load_encrypted_password",
        lua.create_function(|_, (enc_path, password): (String, String)| {
            if enc_path.trim().is_empty() {
                return Err(LuaError::RuntimeError(
                    "[mdix:load_encrypted_password] enc_path cannot be empty".into(),
                ));
            }
            if password.is_empty() {
                return Err(LuaError::RuntimeError(
                    "[mdix:load_encrypted_password] password cannot be empty".into(),
                ));
            }
            DixLoader::new()
                .load_encrypted(&enc_path, &DixLoadOptions::with_password(&password))
                .map(LuaMdixDatabase::from_data)
                .map_err(|e| mdix_err("load_encrypted_password", e))
        })?
    )?;
    

    // ── Foreign format import ──────────────────────────────────────────────

    /// Load from a JSON string.  Top level must be a JSON object.
    ///
    ///   local db = mdix.from_json('{"port": 7777, "host": "localhost"}')
    m.set("from_json", lua.create_function(|_, json: String| {
        if json.trim().is_empty() {
            return Err(LuaError::RuntimeError(
                "[mdix:from_json] json cannot be empty".into(),
            ));
        }
        let conv = DixConverter::new();
        let ast  = conv.from_json(&json)
            .map_err(|e| mdix_err("from_json:parse", e))?;
        let src  = conv.to_mdix(&ast, None)
            .map_err(|e| mdix_err("from_json:reserialize", e))?;
        DixLoader::new()
            .load_from_str(&src, &DixLoadOptions::new())
            .map(LuaMdixDatabase::from_data)
            .map_err(|e| mdix_err("from_json:load", e))
    })?)?;

    /// Load from a TOML string.  Top level must be a TOML table.
    ///
    ///   local db = mdix.from_toml('port = 7777\nhost = "localhost"\n')
    m.set("from_toml", lua.create_function(|_, toml: String| {
        if toml.trim().is_empty() {
            return Err(LuaError::RuntimeError(
                "[mdix:from_toml] toml cannot be empty".into(),
            ));
        }
        let conv = DixConverter::new();
        let ast  = conv.from_toml(&toml)
            .map_err(|e| mdix_err("from_toml:parse", e))?;
        let src  = conv.to_mdix(&ast, None)
            .map_err(|e| mdix_err("from_toml:reserialize", e))?;
        DixLoader::new()
            .load_from_str(&src, &DixLoadOptions::new())
            .map(LuaMdixDatabase::from_data)
            .map_err(|e| mdix_err("from_toml:load", e))
    })?)?;

    // ── Builder factory ────────────────────────────────────────────────────

    /// Create a new empty MdixBuilder.
    ///
    ///   local b = mdix.builder()
    ///   b:set_string("app_name", "AirStrike")
    ///   b:set_int("port", 7777)
    ///   b:with_table("server", {host = "localhost", port = 7777})
    ///   local db = b:build()
    m.set("builder", lua.create_function(|_, ()| {
        Ok(LuaMdixBuilder::new())
    })?)?;

    // ── Source text utilities ──────────────────────────────────────────────

    /// Minify a raw .mdix source string.
    /// Removes all unnecessary whitespace and comments.
    /// String literal contents are preserved.
    ///
    ///   local small = mdix.minify_source(source)
    m.set("minify_source", lua.create_function(|_, source: String| {
        Ok(DixCompactor::minify(&source))
    })?)?;

    /// Compact a raw .mdix source string.
    /// Removes trailing whitespace and collapses multiple blank lines.
    /// Less aggressive than minify — structure is preserved.
    ///
    ///   local neat = mdix.format_source(source)
    m.set("format_source", lua.create_function(|_, source: String| {
        Ok(DixCompactor::compact(&source))
    })?)?;

    Ok(m)
}
