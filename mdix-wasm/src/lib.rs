mod builder;
mod database;
mod error;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

// Re-export the public JS-facing types so wasm-bindgen
// sees them all from a single crate root.
pub use builder::MdixBuilder;
pub use database::MdixDatabase;
