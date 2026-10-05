
pub mod keyword_definitions;
pub mod mid_logger;
pub mod mid_helper_functions;
pub mod utilities;
pub mod parser_collection_helper;
pub mod ast_debug_printer;
pub mod token_debug_printer;

// ── Hand-rolled replacements for external crates (dependency reduction) ──
// All eight are WIRED IN: call sites use them and the external crates are no
// longer runtime dependencies (see Cargo.toml; most stay as dev-dependency
// oracles for the differential tests). See docs/dixscript/utilities.md.
//
// Three are `pub` because this crate's public API names their types:
//   Bitflags   -- `SectionFlags` is a public type (its methods return `Iter`, ...)
//   AsyncTrait -- `CloudStorageProvider` is a public trait (names `BoxFuture`)
//   RustcHash  -- 11 public items take or return `FxHashMap<String, _>`
// The other five are `pub(crate)`: they replace dependencies, and are not part
// of the public API. They keep `allow(dead_code, unused_imports)` because each
// deliberately offers a little more than the crate calls (`Hex::decode`, ...).
pub mod AsyncTrait;
pub mod Bitflags;
pub mod RustcHash;
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