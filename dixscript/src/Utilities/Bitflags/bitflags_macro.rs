// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags/bitflags_macro.rs"
// ============================================================================
//! Hand-rolled replacement for the `bitflags` crate's `bitflags!` macro.
//!
//! ## Why
//! Exactly one call site in the whole crate:
//! `Compiler/Core/BinarySerialization/binary_format.rs`'s `SectionFlags`, an
//! 8-bit set of section-presence flags. The real crate is a mature, general
//! library (custom `Debug`/iteration/serde support, const-generic-friendly
//! internals, ...); this replaces only the specific subset that call site
//! actually uses, checked directly against every `SectionFlags::` and
//! `.method()` call before writing this:
//! - Associated consts (`SectionFlags::CONFIG`, `::NONE`, ...)
//! - `.bits()`, `.contains()`, `.insert()`, `SectionFlags::from_bits_truncate()`
//! - Combining flags with `|`
//!
//! `.remove()`/`.is_empty()`/`|=` are included too even though nothing calls
//! them today -- a flag set this small naturally wants them, and the user
//! has flagged more feature-gating work (which tends to want exactly this
//! kind of flag set) coming later.
//!
//! ## What's deliberately NOT here
//! No custom `Debug` (derive it, like the real invocation already does), no
//! iteration over set bits, no serde support, no macro-level validation that
//! flag values don't overlap. None of that is exercised by the one real
//! caller, and the real crate's own docs don't promise flag values are
//! disjoint either -- that's the macro user's responsibility either way.
//!
//! ## Not verified by compilation
//! This sandbox's rustc is 1.75; `$vis:vis` and tuple-field access in a
//! const context are both long-stable (well under 1.75), so that part isn't
//! in question, but the macro itself was never actually expanded and
//! type-checked here -- only read back by hand against the exact
//! `SectionFlags` definition it needs to reproduce. Confirm with a real
//! `cargo test` once wired in.

/// Usage (identical to the real crate's basic form):
///
/// ```ignore
/// bitflags! {
///     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
///     pub struct SectionFlags: u8 {
///         const NONE     = 0x00;
///         const CONFIG   = 0x01;
///         const ENUMS    = 0x02;
///     }
/// }
/// ```
macro_rules! bitflags {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident : $repr:ty {
            $(const $fname:ident = $fval:expr;)*
        }
    ) => {
        $(#[$meta])*
        #[repr(transparent)]
        $vis struct $name($repr);

        impl $name {
            $(pub const $fname: $name = $name($fval);)*

            /// Every bit any of the flags above actually sets, computed at
            /// macro-expansion time -- this is what `from_bits_truncate`
            /// masks against, exactly like the real crate does.
            #[allow(dead_code)]
            const __ALL_BITS: $repr = 0 $(| Self::$fname.0)*;

            /// The raw bit pattern.
            #[inline]
            #[allow(dead_code)]
            pub const fn bits(&self) -> $repr {
                self.0
            }

            /// Builds a value from raw bits, silently dropping any bit that
            /// isn't one of the flags declared above -- never fails, same
            /// contract as the real crate's `from_bits_truncate`.
            #[inline]
            #[allow(dead_code)]
            pub const fn from_bits_truncate(bits: $repr) -> Self {
                $name(bits & Self::__ALL_BITS)
            }

            /// Whether every bit set in `other` is also set in `self`.
            #[inline]
            #[allow(dead_code)]
            pub fn contains(&self, other: Self) -> bool {
                (self.0 & other.0) == other.0
            }

            /// Sets every bit `other` has, in addition to whatever `self`
            /// already had.
            #[inline]
            #[allow(dead_code)]
            pub fn insert(&mut self, other: Self) {
                self.0 |= other.0;
            }

            /// Clears every bit `other` has.
            #[inline]
            #[allow(dead_code)]
            pub fn remove(&mut self, other: Self) {
                self.0 &= !other.0;
            }

            /// Whether no flag is set at all.
            #[inline]
            #[allow(dead_code)]
            pub fn is_empty(&self) -> bool {
                self.0 == 0
            }
        }

        impl ::std::ops::BitOr for $name {
            type Output = $name;
            #[inline]
            fn bitor(self, rhs: $name) -> $name {
                $name(self.0 | rhs.0)
            }
        }

        impl ::std::ops::BitOrAssign for $name {
            #[inline]
            fn bitor_assign(&mut self, rhs: $name) {
                self.0 |= rhs.0;
            }
        }
    };
}

pub(crate) use bitflags;

#[cfg(test)]
mod tests {
    // Deliberately mirrors SectionFlags shape-for-shape, at a size small
    // enough to read the whole test in one glance.
    bitflags! {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct TestFlags: u8 {
            const NONE = 0x00;
            const A    = 0x01;
            const B    = 0x02;
            const C    = 0x04;
        }
    }

    #[test]
    fn bits_round_trips_through_from_bits_truncate() {
        assert_eq!(TestFlags::from_bits_truncate(0x03).bits(), 0x03);
    }

    #[test]
    fn from_bits_truncate_drops_unknown_bits() {
        // 0x08 isn't any declared flag -- must be masked away, not kept.
        assert_eq!(TestFlags::from_bits_truncate(0x0f).bits(), 0x07);
    }

    #[test]
    fn contains_checks_every_bit_in_other() {
        let both = TestFlags(TestFlags::A.bits() | TestFlags::B.bits());
        assert!(both.contains(TestFlags::A));
        assert!(both.contains(TestFlags::B));
        assert!(!both.contains(TestFlags::C));
    }

    #[test]
    fn insert_adds_without_clearing_existing_bits() {
        let mut flags = TestFlags::A;
        flags.insert(TestFlags::B);
        assert!(flags.contains(TestFlags::A));
        assert!(flags.contains(TestFlags::B));
    }

    #[test]
    fn remove_clears_only_the_given_bits() {
        let mut flags = TestFlags::from_bits_truncate(TestFlags::A.bits() | TestFlags::B.bits());
        flags.remove(TestFlags::A);
        assert!(!flags.contains(TestFlags::A));
        assert!(flags.contains(TestFlags::B));
    }

    #[test]
    fn is_empty_is_true_only_for_no_bits_set() {
        assert!(TestFlags::NONE.is_empty());
        assert!(!TestFlags::A.is_empty());
    }

    #[test]
    fn bitor_combines_flags() {
        let combined = TestFlags::A | TestFlags::C;
        assert!(combined.contains(TestFlags::A));
        assert!(combined.contains(TestFlags::C));
        assert!(!combined.contains(TestFlags::B));
    }
}
