// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/AsyncTrait"
// ============================================================================
//! Replacement for the `async-trait` attribute macro: a `BoxFuture` alias and
//! a hand-desugaring recipe. See `boxed_future.rs`; see
//! `docs/dixscript/utilities.md` for the dependency-reduction pass this is
//! part of.
//!
//! `pub`: the `CloudStorageProvider` trait is part of this crate's public API
//! and its methods return [`BoxFuture`].

pub mod boxed_future;
pub use boxed_future::BoxFuture;
