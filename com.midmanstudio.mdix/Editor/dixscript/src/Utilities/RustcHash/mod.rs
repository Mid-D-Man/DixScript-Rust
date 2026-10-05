// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/RustcHash"
// ============================================================================
//! Hand-rolled replacement for the `rustc-hash` crate.
//!
//! The default exports (`FxHasher`, `FxBuildHasher`, `FxHashMap`, `FxHashSet`)
//! come from `rustc_hash_v2.rs`, a port of `rustc-hash` 2.1.1 -- the version this
//! crate actually used -- so swapping it in changed neither hash values, nor
//! `HashMap` iteration order, nor speed.
//!
//! `classic` is the FxHash copied from `mid-engine` (`fx_hash.rs`). It is a
//! different, older, slower-on-strings algorithm and is deliberately not the
//! default; see the header of `fx_hash.rs` for the measurements. To flip the
//! default to the classic one, change the `pub use` below to `fx_hash::{..}`.
//!
//! **Public**, unlike most replacement modules: 11 items in this crate's public
//! API (`ExecutionContext::variables`, `FunctionInterpreter::new`/`execute`,
//! `DataSectionAnalyzer::get_indexes`, ...) take or return `FxHashMap<String, _>`,
//! so these types must be nameable by callers. That is a deliberate, approved
//! change to those signatures: a caller that built a `rustc_hash::FxHashMap` to
//! pass in must now build `dixscript::Utilities::RustcHash::FxHashMap` (same
//! `HashMap<_, _, _>` shape, different hasher type).

// Kept on purpose (it is the copy from mid-engine) but unused by default, so it would warn.
#[allow(dead_code)]
pub(crate) mod fx_hash;
pub mod rustc_hash_v2;

/// The mid-engine classic FxHash. Not the default; see the module doc.
#[allow(unused_imports)]
pub(crate) use self::fx_hash as classic;

pub use rustc_hash_v2::{FxBuildHasher, FxHashMap, FxHashSet, FxHasher};
