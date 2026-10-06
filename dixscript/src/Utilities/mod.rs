
pub mod keyword_definitions;
pub mod mid_logger;
pub mod mid_helper_functions;
pub mod utilities;
pub mod parser_collection_helper;
pub mod ast_debug_printer;
pub mod token_debug_printer;

// ── Hand-rolled replacements for external crates (dependency reduction) ──
// Base64, Hex, Hostname, LazyStatic and Uuid are `pub(crate)` on purpose --
// they replace dependencies that never reach this crate's public API. The
// three that do (AsyncTrait, Bitflags, RustcHash) are `pub` further down. See
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

// WIRED IN, and `pub` on purpose: `rustc-hash`, `bitflags` and `async-trait`
// all appear in this published crate's public signatures (`FxHashMap` fields
// and return types, `SectionFlags`, the `CloudStorageProvider` trait), so the
// replacement types have to be nameable from outside. The `allow`s cover the
// parts of each port that this crate does not itself call.
#[allow(dead_code, unused_imports, unused_macros)]
pub mod AsyncTrait;
#[allow(dead_code, unused_imports, unused_macros)]
pub mod Bitflags;
#[allow(dead_code, unused_imports, unused_macros)]
pub mod RustcHash;

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