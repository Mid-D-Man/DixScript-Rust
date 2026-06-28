
//! Poll-based hot reload for `.mdix` files.
//!
//! ```rust,ignore
//! use dixscript::Runtime::HotReloadWatcher;
//!
//! let mut watcher = HotReloadWatcher::new("config.mdix");
//!
//! // in your game loop / tick / update:
//! match watcher.check_and_reload() {
//!     Ok(Some(data)) => apply_new_config(data),  // file changed, reloaded
//!     Ok(None)       => {}                       // unchanged, nothing to do
//!     Err(e)         => log::warn!("hot reload failed: {e}"),
//! }
//! ```
//!
//! Deliberately polling-based rather than OS-event-based (no `notify` /
//! inotify / FSEvents / ReadDirectoryChangesW dependency): a single
//! `std::fs::metadata` call per check is cheap enough to run every frame,
//! and it behaves identically across every platform DixScript ships native
//! bindings for — desktop, mobile, and embedded-Lua targets that may not
//! have a filesystem-event backend available at all.
//!
//! On reload failure (e.g. the file was saved mid-write and is briefly
//! invalid), the watcher's `last_modified` stamp is NOT updated, so the next
//! `check_and_reload()` call will retry against the same file state rather
//! than silently giving up on that change.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::dix_data::DixData;
use super::load_options::DixLoadOptions;
use super::loader::DixLoader;

/// Watches a single `.mdix` file on disk and reloads it through the full
/// `DixLoader` pipeline only when its modified-time has changed.
pub struct HotReloadWatcher {
    path: PathBuf,
    options: DixLoadOptions,
    last_modified: Option<SystemTime>,
}

impl HotReloadWatcher {
    /// Start watching `path`. Does not read the file yet — the first
    /// `check_and_reload()` / `has_changed()` call always reports a change.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        HotReloadWatcher {
            path: path.into(),
            options: DixLoadOptions::new(),
            last_modified: None,
        }
    }

    /// Use custom load options (e.g. for an encrypted `.mdix`) instead of
    /// the defaults.
    pub fn with_options(mut self, options: DixLoadOptions) -> Self {
        self.options = options;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `true` once a successful reload has happened at least once.
    pub fn has_loaded(&self) -> bool {
        self.last_modified.is_some()
    }

    fn current_mtime(&self) -> Result<SystemTime, String> {
        std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .map_err(|e| format!(
                "hot_reload: cannot stat '{}': {}",
                self.path.display(), e
            ))
    }

    /// Checks whether the file's modified-time differs from the last
    /// successful reload, without reloading it.
    pub fn has_changed(&self) -> Result<bool, String> {
        let mtime = self.current_mtime()?;
        Ok(match self.last_modified {
            Some(prev) => mtime != prev,
            None => true,
        })
    }

    /// Reloads unconditionally — regardless of whether the file has
    /// changed — and updates the internal mtime stamp on success.
    pub fn force_reload(&mut self) -> Result<DixData, String> {
        let mtime = self.current_mtime()?;
        let data = DixLoader::new()
            .load_text(self.path.to_string_lossy().as_ref(), &self.options)?;
        self.last_modified = Some(mtime);
        Ok(data)
    }

    /// Reloads only if the file has changed since the last successful
    /// reload (or since construction, on the first call).
    ///
    /// Returns `Ok(Some(data))` when a fresh reload happened,
    /// `Ok(None)` when the file is unchanged. A reload failure leaves the
    /// mtime stamp untouched so the next call retries.
    pub fn check_and_reload(&mut self) -> Result<Option<DixData>, String> {
        if self.has_changed()? {
            self.force_reload().map(Some)
        } else {
            Ok(None)
        }
    }
}
