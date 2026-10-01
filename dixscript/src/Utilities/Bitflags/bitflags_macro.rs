// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags/bitflags_macro.rs"
// ============================================================================
//! Hand-rolled replacement for the `bitflags` crate's `bitflags!` macro.
//!
//! ## Why
//! Exactly one invocation in the whole crate:
//! `Compiler/Core/BinarySerialization/binary_format.rs`'s `SectionFlags`, an
//! 8-bit set of section-presence flags. Inside the crate only a handful of
//! methods are ever called on it (`.bits()`, `.contains()`, `.insert()`,
//! `from_bits_truncate()`, the associated consts, `|`).
//!
//! ## Why it covers more than the crate calls
//! `SectionFlags` is a **public** type in a public module
//! (`dixscript::Compiler::Core::BinarySerialization::binary_format`), it is
//! used in public signatures (`BinaryHeader::add_section`, `has_section`, and
//! the public field `BinaryHeader::flags`), and the real `bitflags` 2 macro
//! generates a much larger public API than the crate itself uses. Anyone
//! outside this repo could be calling `SectionFlags::empty()`, `!flags`,
//! `flags & other`, `from_bits(..)` and so on, and nothing in this workspace
//! can rule that out. So this macro reproduces bitflags 2's set-algebra API
//! -- `empty`/`all`/`from_bits`/`from_bits_truncate`/`from_bits_retain`,
//! `is_empty`/`is_all`/`contains`/`intersects`, `insert`/`remove`/`toggle`/
//! `set`, `union`/`intersection`/`difference`/`symmetric_difference`/
//! `complement`, and the operators `| & ^ - !` plus their `-Assign` forms --
//! and every one of them is checked against the real crate over all 256 byte
//! values (see the differential tests).
//!
//! ## Known differences from the real crate (still)
//! - **`Debug` output.** The invocation derives `Debug`, which here prints the
//!   raw number (`SectionFlags(5)`); the real crate prints flag names
//!   (`SectionFlags(CONFIG | DATA)`). Anything that formats a `SectionFlags`
//!   with `{:?}` and compares the text would notice. Nothing in this
//!   workspace does.
//! - **No `iter()` / `iter_names()` / `from_name()`**, and no `Flags` trait
//!   (nothing in this workspace uses them).
//! - No serde support; no overlap checking between flag values -- the real
//!   crate does not promise disjointness either.
//!
//! ## Verification
//! Differential tests against the real `bitflags` 2.x over every one of the 256
//! values (and every pair for the binary operations), run inside the real crate
//! on Rust 1.85.1. The macro is not yet used by `SectionFlags` itself -- that
//! swap is held back pending a decision on public API (see
//! docs/dixscript/utilities.md).

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

            /// The value with no flags set.
            #[inline]
            #[allow(dead_code)]
            pub const fn empty() -> Self {
                $name(0)
            }

            /// The value with every declared flag set.
            #[inline]
            #[allow(dead_code)]
            pub const fn all() -> Self {
                $name(Self::__ALL_BITS)
            }

            /// `Some` only if every set bit is a declared flag; `None` if any
            /// unknown bit is present.
            #[inline]
            #[allow(dead_code)]
            pub const fn from_bits(bits: $repr) -> ::std::option::Option<Self> {
                if bits & !Self::__ALL_BITS == 0 {
                    ::std::option::Option::Some($name(bits))
                } else {
                    ::std::option::Option::None
                }
            }

            /// Keeps every bit, including undeclared ones.
            #[inline]
            #[allow(dead_code)]
            pub const fn from_bits_retain(bits: $repr) -> Self {
                $name(bits)
            }

            /// Whether every declared flag is set.
            #[inline]
            #[allow(dead_code)]
            pub fn is_all(&self) -> bool {
                self.0 & Self::__ALL_BITS == Self::__ALL_BITS
            }

            /// Whether at least one bit of `other` is also set in `self`.
            #[inline]
            #[allow(dead_code)]
            pub fn intersects(&self, other: Self) -> bool {
                self.0 & other.0 != 0
            }

            /// Flips every bit `other` has.
            #[inline]
            #[allow(dead_code)]
            pub fn toggle(&mut self, other: Self) {
                self.0 ^= other.0;
            }

            /// Sets or clears every bit `other` has, depending on `value`.
            #[inline]
            #[allow(dead_code)]
            pub fn set(&mut self, other: Self, value: bool) {
                if value {
                    self.0 |= other.0;
                } else {
                    self.0 &= !other.0;
                }
            }

            /// Bits set in `self` or `other`.
            #[inline]
            #[allow(dead_code)]
            #[must_use]
            pub const fn union(self, other: Self) -> Self {
                $name(self.0 | other.0)
            }

            /// Bits set in both.
            #[inline]
            #[allow(dead_code)]
            #[must_use]
            pub const fn intersection(self, other: Self) -> Self {
                $name(self.0 & other.0)
            }

            /// Bits set in `self` but not in `other`.
            #[inline]
            #[allow(dead_code)]
            #[must_use]
            pub const fn difference(self, other: Self) -> Self {
                $name(self.0 & !other.0)
            }

            /// Bits set in exactly one of the two.
            #[inline]
            #[allow(dead_code)]
            #[must_use]
            pub const fn symmetric_difference(self, other: Self) -> Self {
                $name(self.0 ^ other.0)
            }

            /// Every declared flag NOT set in `self` (undeclared bits are dropped).
            #[inline]
            #[allow(dead_code)]
            #[must_use]
            pub const fn complement(self) -> Self {
                $name(!self.0 & Self::__ALL_BITS)
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

        impl ::std::ops::BitAnd for $name {
            type Output = $name;
            #[inline]
            fn bitand(self, rhs: $name) -> $name {
                self.intersection(rhs)
            }
        }

        impl ::std::ops::BitAndAssign for $name {
            #[inline]
            fn bitand_assign(&mut self, rhs: $name) {
                self.0 &= rhs.0;
            }
        }

        impl ::std::ops::BitXor for $name {
            type Output = $name;
            #[inline]
            fn bitxor(self, rhs: $name) -> $name {
                self.symmetric_difference(rhs)
            }
        }

        impl ::std::ops::BitXorAssign for $name {
            #[inline]
            fn bitxor_assign(&mut self, rhs: $name) {
                self.0 ^= rhs.0;
            }
        }

        impl ::std::ops::Sub for $name {
            type Output = $name;
            #[inline]
            fn sub(self, rhs: $name) -> $name {
                self.difference(rhs)
            }
        }

        impl ::std::ops::SubAssign for $name {
            #[inline]
            fn sub_assign(&mut self, rhs: $name) {
                self.0 &= !rhs.0;
            }
        }

        impl ::std::ops::Not for $name {
            type Output = $name;
            #[inline]
            fn not(self) -> $name {
                self.complement()
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


/// Differential test against the real `bitflags` 2.x, using the exact flag set
/// `SectionFlags` declares in `binary_format.rs` (including the gap at 0x20 and
/// the reserved high bits), over every one of the 256 possible byte values.
/// `SectionFlags::from_bits_truncate` is what the binary unpacker feeds an
/// untrusted header byte through, so this is the behavior that must not drift.
///
/// The real macro is called as `::bitflags::bitflags!` (the extern crate; a
/// same-named local macro exists). When `bitflags` is dropped from
/// `[dependencies]` in the wiring pass, move it to `[dev-dependencies]` so this
/// keeps guarding the macro.
#[cfg(test)]
mod differential_against_real_crate {
    mod mine {
        use super::super::bitflags;
        bitflags! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct Flags: u8 {
                const NONE       = 0x00;
                const CONFIG     = 0x01;
                const ENUMS      = 0x02;
                const DATA       = 0x04;
                const SECURITY   = 0x08;
                const IMPORTS    = 0x10;
                const RESERVED_6 = 0x40;
                const RESERVED_7 = 0x80;
            }
        }
    }

    mod real {
        ::bitflags::bitflags! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct Flags: u8 {
                const NONE       = 0x00;
                const CONFIG     = 0x01;
                const ENUMS      = 0x02;
                const DATA       = 0x04;
                const SECURITY   = 0x08;
                const IMPORTS    = 0x10;
                const RESERVED_6 = 0x40;
                const RESERVED_7 = 0x80;
            }
        }
    }

    #[test]
    fn from_bits_truncate_and_bits_agree_for_every_byte() {
        for b in 0..=255u8 {
            assert_eq!(
                mine::Flags::from_bits_truncate(b).bits(),
                real::Flags::from_bits_truncate(b).bits(),
                "byte {:#04x}",
                b
            );
        }
    }

    #[test]
    fn contains_agrees_for_every_pair_of_bytes() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                assert_eq!(
                    mine::Flags::from_bits_truncate(a).contains(mine::Flags::from_bits_truncate(b)),
                    real::Flags::from_bits_truncate(a).contains(real::Flags::from_bits_truncate(b)),
                    "a={:#04x} b={:#04x}",
                    a,
                    b
                );
            }
        }
    }

    #[test]
    fn insert_agrees_with_the_real_crate() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                let mut m = mine::Flags::from_bits_truncate(a);
                m.insert(mine::Flags::from_bits_truncate(b));
                let mut r = real::Flags::from_bits_truncate(a);
                r.insert(real::Flags::from_bits_truncate(b));
                assert_eq!(m.bits(), r.bits(), "a={:#04x} b={:#04x}", a, b);
            }
        }
    }

    #[test]
    fn empty_all_from_bits_and_predicates_agree_for_every_byte() {
        assert_eq!(mine::Flags::empty().bits(), real::Flags::empty().bits());
        assert_eq!(mine::Flags::all().bits(), real::Flags::all().bits());
        for b in 0..=255u8 {
            assert_eq!(
                mine::Flags::from_bits(b).map(|f| f.bits()),
                real::Flags::from_bits(b).map(|f| f.bits()),
                "from_bits {:#04x}",
                b
            );
            assert_eq!(
                mine::Flags::from_bits_retain(b).bits(),
                real::Flags::from_bits_retain(b).bits(),
                "from_bits_retain {:#04x}",
                b
            );
            let (m, r) = (mine::Flags::from_bits_truncate(b), real::Flags::from_bits_truncate(b));
            assert_eq!(m.is_empty(), r.is_empty(), "is_empty {:#04x}", b);
            assert_eq!(m.is_all(), r.is_all(), "is_all {:#04x}", b);
            assert_eq!(m.complement().bits(), r.complement().bits(), "complement {:#04x}", b);
            assert_eq!((!m).bits(), (!r).bits(), "! {:#04x}", b);
        }
    }

    #[test]
    fn set_algebra_agrees_for_every_pair_of_bytes() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                let (ma, mb) = (mine::Flags::from_bits_truncate(a), mine::Flags::from_bits_truncate(b));
                let (ra, rb) = (real::Flags::from_bits_truncate(a), real::Flags::from_bits_truncate(b));
                let ctx = format!("a={:#04x} b={:#04x}", a, b);

                assert_eq!(ma.intersects(mb), ra.intersects(rb), "intersects {}", ctx);
                assert_eq!(ma.union(mb).bits(), ra.union(rb).bits(), "union {}", ctx);
                assert_eq!(ma.intersection(mb).bits(), ra.intersection(rb).bits(), "intersection {}", ctx);
                assert_eq!(ma.difference(mb).bits(), ra.difference(rb).bits(), "difference {}", ctx);
                assert_eq!(
                    ma.symmetric_difference(mb).bits(),
                    ra.symmetric_difference(rb).bits(),
                    "symmetric_difference {}",
                    ctx
                );
                assert_eq!((ma | mb).bits(), (ra | rb).bits(), "| {}", ctx);
                assert_eq!((ma & mb).bits(), (ra & rb).bits(), "& {}", ctx);
                assert_eq!((ma ^ mb).bits(), (ra ^ rb).bits(), "^ {}", ctx);
                assert_eq!((ma - mb).bits(), (ra - rb).bits(), "- {}", ctx);

                let (mut m, mut r) = (ma, ra);
                m.toggle(mb);
                r.toggle(rb);
                assert_eq!(m.bits(), r.bits(), "toggle {}", ctx);

                for value in [true, false] {
                    let (mut m, mut r) = (ma, ra);
                    m.set(mb, value);
                    r.set(rb, value);
                    assert_eq!(m.bits(), r.bits(), "set({}) {}", value, ctx);
                }

                let (mut m, mut r) = (ma, ra);
                m.remove(mb);
                r.remove(rb);
                assert_eq!(m.bits(), r.bits(), "remove {}", ctx);

                let (mut m, mut r) = (ma, ra);
                m |= mb; r |= rb;
                m &= mb; r &= rb;
                m ^= mb; r ^= rb;
                m -= mb; r -= rb;
                assert_eq!(m.bits(), r.bits(), "assign-ops chain {}", ctx);
            }
        }
    }

    #[test]
    fn named_constants_carry_the_same_bits() {
        assert_eq!(mine::Flags::NONE.bits(), real::Flags::NONE.bits());
        assert_eq!(mine::Flags::CONFIG.bits(), real::Flags::CONFIG.bits());
        assert_eq!(mine::Flags::ENUMS.bits(), real::Flags::ENUMS.bits());
        assert_eq!(mine::Flags::DATA.bits(), real::Flags::DATA.bits());
        assert_eq!(mine::Flags::SECURITY.bits(), real::Flags::SECURITY.bits());
        assert_eq!(mine::Flags::IMPORTS.bits(), real::Flags::IMPORTS.bits());
        assert_eq!(mine::Flags::RESERVED_6.bits(), real::Flags::RESERVED_6.bits());
        assert_eq!(mine::Flags::RESERVED_7.bits(), real::Flags::RESERVED_7.bits());
    }
}
