// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Hostname/hostname.rs"
// ============================================================================
//! Best-effort replacement for the `hostname` crate. See `hostname.rs` for
//! the implementation and, importantly, the ways it is NOT equivalent; see
//! `docs/dixscript/utilities.md` for the dependency-reduction pass this is
//! part of.
//!
//! Crate-internal only (`pub(crate)`) -- this exists to replace a
//! dependency, not to grow the crate's own public API.

pub(crate) mod hostname;
pub(crate) use hostname::get;
