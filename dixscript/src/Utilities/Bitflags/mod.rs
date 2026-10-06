// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags"
// ============================================================================
//! Hand-rolled replacement for the `bitflags` crate (2.x), ported from the
//! real 2.10.0 source rather than re-derived from its documentation.
//!
//! The one place this crate uses it is `SectionFlags` in
//! `Compiler/Core/BinarySerialization/binary_format.rs`. That type is public,
//! so it has to keep the real crate's full public shape: the generated
//! methods and operators, the `Flags` trait impl, `iter()` / `iter_names()`,
//! `from_name()`, the text format (`Debug` prints `SectionFlags(CONFIG | DATA)`)
//! and the two-layer type (a public wrapper over a hidden internal bitflags
//! type). This module is therefore `pub`; the `bitflags!` macro itself stays
//! crate-private.
//!
//! Not ported: serde / arbitrary / bytemuck glue (`external`), the
//! `bitflags_match!` macro, and the deprecated `BitFlags` trait.

pub use traits::{Bits, Flag, Flags};

pub mod iter;
pub mod parser;
mod traits;

#[doc(hidden)]
pub mod __private {
    pub use super::traits::__private::*;
    pub use core;
}

mod bitflags_macro;
#[allow(unused_imports)]
pub(crate) use bitflags_macro::*;

#[cfg(test)]
mod differential_tests;
#[cfg(test)]
mod upstream_tests;
