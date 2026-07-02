// mdix-wasm/src/lib.rs
mod builder;
mod database;
mod error;
mod merge;
mod schema;
mod watch;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn init() {
    // Install unconditionally — no feature flag needed.
    // Without this, every Rust panic in WASM produces the useless
    // "unreachable executed" browser message with no file or line info.
    // With it, the browser console shows the exact panic location.
    console_error_panic_hook::set_once();
}

pub use builder::MdixBuilder;
pub use database::MdixDatabase;
pub use merge::{merge_sources, merge_sources_weighted, MdixMergeOutcome};
pub use schema::{MdixSchema, MdixValidationReport};
pub use watch::{MdixWatcher, MdixWatchOutcome};
