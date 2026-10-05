// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags"
// ============================================================================
//! Port of the `bitflags` crate (2.10.0), `MIT OR Apache-2.0`. See
//! `bitflags_macro.rs` for what the macro generates and `docs/dixscript/
//! utilities.md` for the dependency-reduction pass this is part of.
//!
//! Unlike the other replacement modules, this one is **public**
//! (`dixscript::Utilities::Bitflags`): `SectionFlags` is a public type, and the
//! generated type's public methods return `Iter` / `IterNames` and implement
//! `Flags`, so those names must be reachable for the public API to be
//! nameable, exactly as with the real crate.

pub mod iter;
pub mod parser;
pub mod traits;

mod bitflags_macro;

pub(crate) use bitflags_macro::{__impl_flags_public_traits, bitflags};
pub use traits::{Bits, Flag, Flags, Primitive, PublicFlags};

#[cfg(test)]
mod tests;
