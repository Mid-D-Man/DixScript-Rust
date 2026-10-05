// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/LazyStatic/lazy_static_macro.rs"
// ============================================================================
//! Hand-rolled replacement for the `lazy_static` crate. See
//! `lazy_static_macro.rs` for the implementation and reasoning; see
//! `docs/dixscript/utilities.md` for the dependency-reduction pass this is
//! part of.
//!
//! Crate-internal only (`pub(crate)`) -- this exists to replace a
//! dependency, not to grow the crate's own public API.

pub(crate) mod lazy_static_macro;
pub(crate) use lazy_static_macro::lazy_static;
