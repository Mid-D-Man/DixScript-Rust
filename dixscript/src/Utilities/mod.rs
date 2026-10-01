
pub mod keyword_definitions;
pub mod mid_logger;
pub mod mid_helper_functions;
pub mod utilities;
pub mod parser_collection_helper;
pub mod ast_debug_printer;
pub mod token_debug_printer;

// ── Hand-rolled replacements for external crates (dependency reduction) ──
// `pub(crate)` on purpose -- these replace dependencies, they are not part of
// this crate's public API, and none is re-exported below. See
// docs/dixscript/utilities.md.
//
// WIRED IN (call sites now use these, the external crates are gone from
// `[dependencies]`): Base64, Hex, Hostname, LazyStatic, Uuid. They keep an
// `allow(dead_code, unused_imports)` because each deliberately offers a little
// more than the crate calls (e.g. `Hex::decode`, `Uuid::from_bytes`'s siblings).
#[allow(dead_code, unused_imports)]
pub(crate) mod Base64;
#[allow(dead_code, unused_imports)]
pub(crate) mod Hex;
#[allow(dead_code, unused_imports)]
pub(crate) mod Hostname;
#[allow(dead_code, unused_imports)]
pub(crate) mod LazyStatic;
#[allow(dead_code, unused_imports)]
pub(crate) mod Uuid;

// NOT WIRED IN, pending a decision on public API (see the "Decisions the
// wiring pass needs" section of the doc above): `rustc-hash`, `bitflags` and
// `async-trait` all appear in this published crate's public signatures, so
// swapping them is not a pure internal change. Nothing calls these yet, hence
// the broader allow. Remove it if/when they are wired in.
#[allow(dead_code, unused_imports, unused_macros)]
pub(crate) mod AsyncTrait;
#[allow(dead_code, unused_imports, unused_macros)]
pub(crate) mod Bitflags;
#[allow(dead_code, unused_imports, unused_macros)]
pub(crate) mod RustcHash;

// Test-only helper for the differential tests in the modules above.
#[cfg(test)]
pub(crate) mod test_rng;

pub use token_debug_printer::TokenDebugPrinter;
pub use keyword_definitions::Keywords;
pub use mid_logger::MID_Logger;
pub use mid_helper_functions::*;
pub use utilities::*;
pub use parser_collection_helper::*;
pub use ast_debug_printer::AstDebugPrinter;