// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Hex/hex.rs"
// ============================================================================
//! Hand-rolled replacement for the `hex` crate. See `hex.rs` for the
//! implementation and reasoning; see `docs/dixscript/utilities.md` for the
//! dependency-reduction pass this is part of.
//!
//! Crate-internal only (`pub(crate)`) -- this exists to replace a
//! dependency, not to grow the crate's own public API.

pub(crate) mod hex;
pub(crate) use hex::{decode, encode};
