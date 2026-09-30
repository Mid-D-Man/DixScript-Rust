// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Uuid/uuid.rs"
// ============================================================================
//! Hand-rolled replacement for the `uuid` crate. See `uuid.rs` for the
//! implementation and the parser reasoning; see `docs/dixscript/utilities.md`
//! for the dependency-reduction pass this is part of.
//!
//! Crate-internal only (`pub(crate)`) -- this exists to replace a
//! dependency, not to grow the crate's own public API.

pub(crate) mod uuid;
pub(crate) use uuid::{Hyphenated, Simple, Uuid, UuidParseError};
