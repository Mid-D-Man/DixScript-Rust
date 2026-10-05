// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/AsyncTrait/boxed_future.rs"
// ============================================================================
//! Replacement for the `async-trait` attribute macro: a `BoxFuture` alias and a
//! recipe that reproduces the macro's expansion exactly. See `boxed_future.rs`;
//! see `docs/dixscript/utilities.md` for the dependency-reduction pass this is
//! part of.
//!
//! Public (`dixscript::Utilities::AsyncTrait`), unlike most replacement modules:
//! `CloudStorageProvider` is a public trait and its methods' signatures name
//! `BoxFuture`, so the alias must be nameable by implementors.

pub mod boxed_future;
pub use boxed_future::BoxFuture;
