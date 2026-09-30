// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/AsyncTrait/boxed_future.rs"
// ============================================================================
//! Replacement for the `async-trait` attribute macro: a `BoxFuture` alias and
//! a hand-desugaring recipe. See `boxed_future.rs`; see
//! `docs/dixscript/utilities.md` for the dependency-reduction pass this is
//! part of.
//!
//! Crate-internal only (`pub(crate)`) -- this exists to replace a
//! dependency, not to grow the crate's own public API.

pub(crate) mod boxed_future;
pub(crate) use boxed_future::BoxFuture;
