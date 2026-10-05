// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Base64/base64_codec.rs"
// ============================================================================
//! Hand-rolled replacement for the `base64` crate. See `base64_codec.rs` for
//! the implementation and the decode-strictness reasoning; see
//! `docs/dixscript/utilities.md` for the dependency-reduction pass this is
//! part of.
//!
//! Crate-internal only (`pub(crate)`) -- this exists to replace a
//! dependency, not to grow the crate's own public API.

pub(crate) mod base64_codec;
pub(crate) use base64_codec::{decode, encode, general_purpose, DecodeError, Engine, StandardEngine};
