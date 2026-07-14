// MdixWatcher — "hot reload" for JS/TS, content-hash based.
//
// This is deliberately NOT the same mechanism as
// dixscript::Runtime::HotReloadWatcher (used by mdix-lua and
// mdix-python), which polls a file's mtime via std::fs::metadata.
// wasm32-unknown-unknown has no filesystem at all — not "a restricted
// one", *none* — so an mtime-polling watcher can never compile here, let
// alone run, regardless of whether the host happens to be Node (which
// does have a real fs) or a browser (which doesn't).
//
// The architecture this pushes you toward is the right one anyway: the
// HOST always already knows when its own file changed (Node's
// `fs.watch`/`chokidar`, or a browser polling its own `fetch()`/File
// System Access API) — it does not need wasm to tell it that. What wasm
// is actually useful for is the expensive part: deciding whether newly
// re-read text differs from what's already loaded, and re-parsing only
// when it does. Hashing instead of string-comparing means a multi-KB
// .mdix file isn't memcmp'd in JS on every poll tick.
//
// ```js
// import { MdixWatcher } from "midmanstudio-mdix";
//
// const watcher = new MdixWatcher();
//
// // Node — fs.watch tells you WHEN to re-read; this decides WHETHER to re-parse:
// fs.watch("config.mdix", async () => {
//   const text = await fs.promises.readFile("config.mdix", "utf8");
//   const outcome = watcher.check(text);
//   if (outcome.changed) applyNewConfig(outcome.database());
// });
//
// // Browser — poll your own source, e.g. fetch():
// setInterval(async () => {
//   const text = await (await fetch("/config.mdix")).text();
//   const outcome = watcher.check(text);
//   if (outcome.changed) applyNewConfig(outcome.database());
// }, 5000);
// ```

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use wasm_bindgen::prelude::*;
use crate::database::MdixDatabase;

#[wasm_bindgen]
pub struct MdixWatcher {
    last_hash: Option<u64>,
}

#[wasm_bindgen]
impl MdixWatcher {
    #[wasm_bindgen(constructor)]
    pub fn new() -> MdixWatcher {
        MdixWatcher { last_hash: None }
    }

    /// Returns true if `source` differs from the last content seen by
    /// `check()` (or unconditionally true if `check()` has never been
    /// called). Does not parse or update any state — use this for a
    /// cheap pre-check before doing anything more expensive than hashing.
    #[wasm_bindgen(js_name = hasChanged)]
    pub fn has_changed(&self, source: &str) -> bool {
        match self.last_hash {
            Some(prev) => prev != Self::hash(source),
            None       => true,
        }
    }

    /// Compares `source` against the last content seen, by hash. If it
    /// differs (or this is the first call), parses it and returns an
    /// outcome with `changed = true` and a usable database. If it is
    /// identical to last time, returns `changed = false` and does NOT
    /// parse — call `.database()` only when `changed` is true.
    pub fn check(&mut self, source: &str) -> Result<MdixWatchOutcome, JsValue> {
        let hash = Self::hash(source);
        if self.last_hash == Some(hash) {
            return Ok(MdixWatchOutcome { database: None, changed: false });
        }
        let db = MdixDatabase::load_str(source)?;
        self.last_hash = Some(hash);
        Ok(MdixWatchOutcome { database: Some(db), changed: true })
    }

    /// Forgets any previously seen content — the next `check()` call
    /// will always report `changed = true`, regardless of whether the
    /// content actually matches what was seen before this reset.
    pub fn reset(&mut self) {
        self.last_hash = None;
    }

    fn hash(source: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for MdixWatcher {
    fn default() -> Self {
        MdixWatcher::new()
    }
}

/// Returned by `MdixWatcher.check()`. Two fields instead of a tuple
/// since wasm-bindgen can't return a Rust tuple directly — same pattern
/// as `MdixMergeOutcome` in merge.rs.
#[wasm_bindgen]
pub struct MdixWatchOutcome {
    database: Option<MdixDatabase>,
    changed:  bool,
}

#[wasm_bindgen]
impl MdixWatchOutcome {
    #[wasm_bindgen(getter)]
    pub fn changed(&self) -> bool {
        self.changed
    }

    /// Consumes and returns the freshly parsed database. Only valid when
    /// `changed` is true — raises if called when nothing changed (there
    /// is nothing to take in that case) or if called a second time.
    pub fn database(&mut self) -> Result<MdixDatabase, JsValue> {
        self.database.take().ok_or_else(|| {
            JsValue::from_str(
                "[mdix] MdixWatchOutcome.database() unavailable — either \
                 changed was false, or database() was already called once",
            )
        })
    }
  }
