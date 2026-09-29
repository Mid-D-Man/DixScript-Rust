// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/RustcHash/fx_hash.rs"
// ============================================================================
//! Hand-rolled replacement for the `rustc-hash` crate. See `fx_hash.rs` for
//! the implementation, its provenance (copied from `mid-engine`'s
//! `mid-collections`), and reasoning; see `docs/dixscript/utilities.md` for
//! the dependency-reduction pass this is part of.
//!
//! Crate-internal only (`pub(crate)`) -- this exists to replace a
//! dependency, not to grow the crate's own public API.

pub(crate) mod fx_hash;
pub(crate) use fx_hash::{FxBuildHasher, FxHashMap, FxHashSet, FxHasher};
