// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags/bitflags_macro.rs"
// ============================================================================
//! Hand-rolled replacement for the `bitflags` crate. See
//! `bitflags_macro.rs` for the implementation and reasoning; see
//! `docs/dixscript/utilities.md` for the dependency-reduction pass this is
//! part of.
//!
//! Crate-internal only (`pub(crate)`) -- this exists to replace a
//! dependency, not to grow the crate's own public API.

pub(crate) mod bitflags_macro;
pub(crate) use bitflags_macro::bitflags;
