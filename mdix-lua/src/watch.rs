// mdix-lua/src/watch.rs
//
// Hot reload for Lua — thin binding over dixscript::Runtime::HotReloadWatcher.
// See dixscript/src/Runtime/hot_reload.rs for why this is poll-based rather
// than OS-event-based.
//
//   local watcher = mdix.watch("config.mdix")
//
//   -- in your game loop / tick / update:
//   local db, changed = watcher:check()
//   if changed then
//       apply_new_config(db)
//   end
//
// `db` is nil when nothing changed (changed == false) — keep using the
// previously loaded database instance in that case.

use mlua::{
    MetaMethod, Result as LuaResult, UserData, UserDataMethods,
};
use dixscript::Runtime::HotReloadWatcher;

use crate::database::LuaMdixDatabase;
use crate::error::mdix_err;

pub struct LuaMdixWatcher {
    inner: HotReloadWatcher,
}

impl LuaMdixWatcher {
    pub fn new(path: String) -> Self {
        LuaMdixWatcher { inner: HotReloadWatcher::new(path) }
    }
}

impl UserData for LuaMdixWatcher {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {

        /// Reloads only if the watched file's modified-time has changed
        /// since the last successful check (or since construction, on the
        /// first call). Returns `(database_or_nil, changed_bool)` — when
        /// `changed` is `false`, `database` is `nil` and you should keep
        /// using whatever database instance you already have.
        methods.add_method_mut("check", |_, this, ()| {
            match this.inner.check_and_reload() {
                Ok(Some(data)) => Ok((Some(LuaMdixDatabase::from_data(data)), true)),
                Ok(None)       => Ok((None, false)),
                Err(e)         => Err(mdix_err("watch:check", e)),
            }
        });

        /// Reloads unconditionally, regardless of whether the file changed.
        methods.add_method_mut("force_reload", |_, this, ()| {
            this.inner.force_reload()
                .map(LuaMdixDatabase::from_data)
                .map_err(|e| mdix_err("watch:force_reload", e))
        });

        /// Checks whether the file has changed without reloading it.
        methods.add_method("has_changed", |_, this, ()| {
            this.inner.has_changed().map_err(|e| mdix_err("watch:has_changed", e))
        });

        /// `true` once a successful reload has happened at least once.
        methods.add_method("has_loaded", |_, this, ()| {
            Ok(this.inner.has_loaded())
        });

        methods.add_method("path", |_, this, ()| {
            Ok(this.inner.path().to_string_lossy().to_string())
        });

        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("MdixWatcher(path=\"{}\")", this.inner.path().display()))
        });
    }
                        }
