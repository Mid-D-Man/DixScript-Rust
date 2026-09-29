
pub mod keyword_definitions;
pub mod mid_logger;
pub mod mid_helper_functions;
pub mod utilities;
pub mod parser_collection_helper;
pub mod ast_debug_printer;
pub mod token_debug_printer;

// ── Hand-rolled replacements for external crates (dependency reduction) ──
// Registered so they compile and their unit tests run, but deliberately NOT
// re-exported below and NOT yet used by any call site: swapping the ~200
// existing call sites over (and only then dropping the crates from
// Cargo.toml) is a separate, later pass. `pub(crate)` on purpose -- these
// replace dependencies, they are not part of this crate's public API.
// `allow(dead_code, unused_imports)` is temporary for exactly that reason:
// nothing calls them yet. Remove the allow when they're wired in.
// See docs/dixscript/utilities.md.
#[allow(dead_code, unused_imports, unused_macros)]
pub(crate) mod Bitflags;
#[allow(dead_code, unused_imports, unused_macros)]
pub(crate) mod Hex;
#[allow(dead_code, unused_imports, unused_macros)]
pub(crate) mod LazyStatic;
#[allow(dead_code, unused_imports, unused_macros)]
pub(crate) mod RustcHash;

pub use token_debug_printer::TokenDebugPrinter;
pub use keyword_definitions::Keywords;
pub use mid_logger::MID_Logger;
pub use mid_helper_functions::*;
pub use utilities::*;
pub use parser_collection_helper::*;
pub use ast_debug_printer::AstDebugPrinter;