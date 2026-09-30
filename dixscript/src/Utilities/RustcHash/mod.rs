// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/RustcHash"
// ============================================================================
//! Hand-rolled replacement for the `rustc-hash` crate.
//!
//! The default exports (`FxHasher`, `FxBuildHasher`, `FxHashMap`,
//! `FxHashSet`) come from `rustc_hash_v2.rs`, a port of `rustc-hash` 2.1.1 --
//! the version this crate actually uses -- so swapping it in changes neither
//! hash values, nor `HashMap` iteration order, nor speed.
//!
//! `classic` is the FxHash copied from `mid-engine` (`fx_hash.rs`). It is a
//! different, older, slower-on-strings algorithm and is deliberately not the
//! default; see the header of `fx_hash.rs` for the measurements.
//!
//! To flip the default to the classic one, change the `pub(crate) use` below
//! to `fx_hash::{..}`. Nothing else needs to change.
//!
//! Crate-internal only (`pub(crate)`). NOTE for the wiring pass: several
//! `FxHashMap<String, DixValue>` types appear in this crate's PUBLIC
//! signatures (see docs/dixscript/utilities.md), so `pub(crate)` here will
//! need revisiting before those are switched over.

pub(crate) mod fx_hash;
pub(crate) mod rustc_hash_v2;

/// The mid-engine classic FxHash. Not the default; see the module doc.
pub(crate) use self::fx_hash as classic;

pub(crate) use rustc_hash_v2::{FxBuildHasher, FxHashMap, FxHashSet, FxHasher};
