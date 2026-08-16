// Lua 5.4 module entrypoint — called by Lua's require("mdix").
//
// Primary deployment target: a Rust host process that embeds Lua itself
// via `mlua` (typically with the "vendored" feature, e.g. a game's
// modding layer) for running user-supplied .lua scripts. Mod scripts
// running inside that embedded interpreter can then `require("mdix")` to
// read/write .mdix config and save data — this module is THAT side of the
// boundary, not the embedding side. It is built with mlua's "module"
// feature specifically because of that: "module" does not vendor its own
// copy of the Lua C library — it dynamically resolves Lua symbols against
// whatever interpreter loaded it via require(), which is exactly the host's
// already-running (possibly "vendored") Lua 5.4 instance. Building this
// crate with "vendored" instead would statically link a second, separate
// Lua runtime into the same process and break in confusing ways the moment
// two different Lua states try to coexist.
//
// This DOES require the host and this module to agree on the Lua version
// (both must be lua54 — mixing 5.1/5.4 or PUC-Lua/LuaJIT across the
// boundary is not supported and not checked at compile time, only at
// require() time when it fails to load). Not yet verified on a real
// `cargo apk`/Android build — see mdix-lua/Cargo.toml.
//
// Exposes these module-level symbols:
//
//   mdix.version                                     -- string
//   mdix.load(path)                                  -- MdixDatabase
//   mdix.load_str(source)                             -- MdixDatabase
//   mdix.load_encrypted(enc_path [, key_path])       -- MdixDatabase
//   mdix.load_encrypted_password(enc_path, password) -- MdixDatabase
//   mdix.from_json(json)                             -- MdixDatabase
//   mdix.from_toml(toml)                             -- MdixDatabase
//   mdix.from_table(table)                           -- MdixBuilder
//   mdix.builder()                                   -- MdixBuilder
//   mdix.schema()                                     -- MdixSchema
//   mdix.watch(path)                                  -- MdixWatcher
//   mdix.merge_files(paths [, strategy [, array_strategy]])
//                                                      -- MdixDatabase, conflicts
//   mdix.merge_files_weighted(entries [, strategy [, array_strategy]])
//                                                      -- MdixDatabase, conflicts
//   mdix.query(table)                                 -- MdixQuery
//   mdix.minify_source(source)                       -- string
//   mdix.format_source(source)                       -- string
//   mdix.strip_comments(source)                      -- string
//
// MdixDatabase additionally exposes :to_table(), :validate_schema(schema),
// :query(path), :query_many(pattern), and
// :merge_with(other [, strategy [, array_strategy [, temp_dir]]]).
// See database.rs, builder.rs, schema.rs, merge.rs, query.rs, and watch.rs
// for the full method surfaces.
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
mod merge;
mod query;
mod schema;
mod value;
mod watch;

use mlua::prelude::*;
use dixscript::Runtime::{
    DixCompactor, DixConverter, DixFormatOptions, DixLoadOptions, DixLoader,
};

use builder::LuaMdixBuilder;
use database::LuaMdixDatabase;
use error::mdix_err;
use query::LuaMdixQuery;
use schema::LuaMdixSchema;
use watch::LuaMdixWatcher;

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

    /// Build an MdixBuilder from a single nested Lua table in one shot —
    /// the reverse of MdixDatabase:to_table(). See builder.rs for exactly
    /// how nested arrays/tables map onto DixScript's section kinds.
    ///
    ///   local b  = mdix.from_table({app_name = "AirStrike", port = 7777})
    ///   local db = b:build()
    m.set("from_table", lua.create_function(|_, table: LuaTable| {
        LuaMdixBuilder::from_table(&table)
    })?)?;

    // ── Schema validation ───────────────────────────────────────────────────

    /// Create a new empty MdixSchema. See schema.rs for the full
    /// require_*/optional_* method surface.
    ///
    ///   local schema = mdix.schema():require_string("app_name"):require_int("port")
    ///   local report = db:validate_schema(schema)
    m.set("schema", lua.create_function(|_, ()| {
        Ok(LuaMdixSchema::new())
    })?)?;

    // ── Hot reload ───────────────────────────────────────────────────────────

    /// Start watching a .mdix file for changes. See watch.rs.
    ///
    ///   local watcher = mdix.watch("config.mdix")
    ///   local db, changed = watcher:check()
    m.set("watch", lua.create_function(|_, path: String| {
        if path.trim().is_empty() {
            return Err(LuaError::RuntimeError(
                "[mdix:watch] path cannot be empty".into(),
            ));
        }
        Ok(LuaMdixWatcher::new(path))
    })?)?;

    // ── Merging ──────────────────────────────────────────────────────────────

    /// Merge two or more .mdix files. Files are weighted in descending
    /// order (first = highest priority). strategy defaults to "weighted";
    /// array_strategy defaults to "concat_dedup". See merge.rs.
    /// Returns (database, conflicts).
    ///
    ///   local db, conflicts = mdix.merge_files({"base.mdix", "patch.mdix"})
    m.set("merge_files", lua.create_function(merge::merge_files)?)?;

    /// Merge .mdix files with explicit per-file weights.
    /// Returns (database, conflicts). See merge.rs.
    ///
    ///   local db, conflicts = mdix.merge_files_weighted(
    ///       {{"base.mdix", 1.0}, {"patch.mdix", 0.8}}, "weighted")
    m.set("merge_files_weighted", lua.create_function(merge::merge_files_weighted)?)?;

    // ── Querying ─────────────────────────────────────────────────────────────

    /// Wrap an arbitrary Lua sequence table for querying — see query.rs
    /// for the full where/select/order_by/group_by/... method surface.
    /// For querying a loaded Database's own fields, prefer
    /// MdixDatabase:query(path)/:query_many(pattern) instead — this is
    /// for querying data that didn't come from a Database at all.
    ///
    ///   local q = mdix.query({1, 5, 3, 2, 4})
    ///   local total = q:sum_int()
    m.set("query", lua.create_function(|_, table: LuaTable| {
        LuaMdixQuery::from_table(&table)
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
    /// Less aggressive than minify — structure is preserved. Despite the
    /// name, this does not do full AST-based pretty-printing with mode
    /// control (Default/Pretty/Compact/Minified) the way the FFI-based
    /// bindings' format_source does — the underlying mdix_format_source
    /// itself only actually differentiates Minified from everything
    /// else right now (checked directly in mdix-ffi/src/lib.rs; Default/
    /// Pretty/Compact all currently fall through to the same
    /// DixCompactor::compact this calls), so this is exactly equivalent,
    /// just without the mode parameter those bindings expose for a
    /// distinction that isn't implemented yet either way.
    ///
    ///   local neat = mdix.format_source(source)
    m.set("format_source", lua.create_function(|_, source: String| {
        Ok(DixCompactor::compact(&source))
    })?)?;

    /// Strip comments from a raw .mdix source string, leaving
    /// structure and whitespace otherwise untouched.
    ///
    ///   local uncommented = mdix.strip_comments(source)
    m.set("strip_comments", lua.create_function(|_, source: String| {
        Ok(DixCompactor::remove_comments(&source))
    })?)?;

    Ok(m)
}
