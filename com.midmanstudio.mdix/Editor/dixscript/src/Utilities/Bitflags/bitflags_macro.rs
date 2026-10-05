// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags"
// ============================================================================
//! The `bitflags!` macro, ported from `bitflags` 2.10.0.
//!
//! ## Attribution
//! Derived from the `bitflags` crate, v2.10.0
//! (<https://github.com/bitflags/bitflags>), licensed `MIT OR Apache-2.0`. The
//! generated structure, method set, const-ness, iteration and text format
//! follow upstream's `src/lib.rs`, `src/public.rs` and `src/internal.rs`. The
//! macro layering is flattened (one macro plus one helper, instead of
//! upstream's chain of `__declare_*` / `__impl_*` helpers), which changes how
//! it is written, not what it generates.
//!
//! ## What is generated (as upstream)
//! For `bitflags! { pub struct F: u8 { const A = 1; ... } }`:
//! - `pub struct F(<F as PublicFlags>::Internal)` -- a public wrapper around a
//!   hidden `InternalBitFlags(u8)`. The two-layer shape is what makes a derived
//!   `Debug` print flag *names* (`F(A | B)`, `F(0x0)`, `F(A | 0xf0)`): the
//!   derive runs on the wrapper and delegates to the hidden type's
//!   hand-written `Debug`.
//! - the flag constants, `impl Flags for F`, and the full inherent API
//!   (`empty/all/bits/from_bits/from_bits_truncate/from_bits_retain/from_name/
//!   is_empty/is_all/intersects/contains/insert/remove/toggle/set/intersection/
//!   union/difference/symmetric_difference/complement/iter/iter_names`), with
//!   upstream's `const fn` set.
//! - `Binary`/`Octal`/`LowerHex`/`UpperHex`, `| & ^ - !` and their `-Assign`
//!   forms, `Extend`, `FromIterator`, `IntoIterator`.
//!
//! The hidden type additionally implements `Debug`, `Display`, `FromStr`,
//! `Default`, `AsRef`, `From`, and the derives `Clone/Copy/PartialEq/Eq/
//! PartialOrd/Ord/Hash`, so those derives are available on the wrapper.
//!
//! ## Not ported (and why that is safe here)
//! - Unnamed flags (`const _ = ...;`) and `#[cfg(..)]` attributes on flag
//!   constants. The one invocation in this crate (`SectionFlags`) uses neither.
//! - The `serde` / `arbitrary` / `bytemuck` / `zerocopy` / `bitflags_match!`
//!   integrations, and the deprecated `BitFlags` alias trait.
//! - Operators, formatting and iteration on the *hidden* type (upstream has
//!   them; nothing can name it, and the wrapper's versions forward to bits).
//!
//! ## Verification
//! See `tests.rs` in this directory: differential tests against the real
//! `bitflags` 2.10.0 across several flag sets.

/// Implements the formatting, operator, `Extend`, `FromIterator` and
/// `IntoIterator` traits on the public wrapper. Internal helper of `bitflags!`.
macro_rules! __impl_flags_public_traits {
    ($Public:ident, $T:ty) => {
        impl ::core::fmt::Binary for $Public {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Binary::fmt(&self.bits(), f)
            }
        }

        impl ::core::fmt::Octal for $Public {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Octal::fmt(&self.bits(), f)
            }
        }

        impl ::core::fmt::LowerHex for $Public {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::LowerHex::fmt(&self.bits(), f)
            }
        }

        impl ::core::fmt::UpperHex for $Public {
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::UpperHex::fmt(&self.bits(), f)
            }
        }

        impl ::core::ops::BitOr for $Public {
            type Output = Self;

            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            fn bitor(self, other: $Public) -> Self {
                self.union(other)
            }
        }

        impl ::core::ops::BitOrAssign for $Public {
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            fn bitor_assign(&mut self, other: Self) {
                self.insert(other);
            }
        }

        impl ::core::ops::BitXor for $Public {
            type Output = Self;

            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            fn bitxor(self, other: Self) -> Self {
                self.symmetric_difference(other)
            }
        }

        impl ::core::ops::BitXorAssign for $Public {
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            fn bitxor_assign(&mut self, other: Self) {
                self.toggle(other);
            }
        }

        impl ::core::ops::BitAnd for $Public {
            type Output = Self;

            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            fn bitand(self, other: Self) -> Self {
                self.intersection(other)
            }
        }

        impl ::core::ops::BitAndAssign for $Public {
            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            fn bitand_assign(&mut self, other: Self) {
                *self = Self::from_bits_retain(self.bits()).intersection(other);
            }
        }

        impl ::core::ops::Sub for $Public {
            type Output = Self;

            /// The intersection of a source flags value with the complement of a
            /// target flags value (`&!`).
            #[inline]
            fn sub(self, other: Self) -> Self {
                self.difference(other)
            }
        }

        impl ::core::ops::SubAssign for $Public {
            /// The intersection of a source flags value with the complement of a
            /// target flags value (`&!`).
            #[inline]
            fn sub_assign(&mut self, other: Self) {
                self.remove(other);
            }
        }

        impl ::core::ops::Not for $Public {
            type Output = Self;

            /// The bitwise negation (`!`) of the bits in a flags value, truncating
            /// the result.
            #[inline]
            fn not(self) -> Self {
                self.complement()
            }
        }

        impl ::core::iter::Extend<$Public> for $Public {
            /// The bitwise or (`|`) of the bits in each flags value.
            fn extend<T: ::core::iter::IntoIterator<Item = Self>>(&mut self, iterator: T) {
                for item in iterator {
                    self.insert(item)
                }
            }
        }

        impl ::core::iter::FromIterator<$Public> for $Public {
            /// The bitwise or (`|`) of the bits in each flags value.
            fn from_iter<T: ::core::iter::IntoIterator<Item = Self>>(iterator: T) -> Self {
                use ::core::iter::Extend;

                let mut result = Self::empty();
                result.extend(iterator);
                result
            }
        }

        impl ::core::iter::IntoIterator for $Public {
            type Item = $Public;
            type IntoIter = $crate::Utilities::Bitflags::iter::Iter<$Public>;

            fn into_iter(self) -> Self::IntoIter {
                self.iter()
            }
        }
    };
}

/// Usage (identical to the real crate's basic form):
///
/// ```ignore
/// bitflags! {
///     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
///     pub struct SectionFlags: u8 {
///         const NONE   = 0x00;
///         const CONFIG = 0x01;
///         const ENUMS  = 0x02;
///     }
/// }
/// ```
macro_rules! bitflags {
    () => {};

    (
        $(#[$outer:meta])*
        $vis:vis struct $Public:ident : $T:ty {
            $(
                $(#[$inner:meta])*
                const $Flag:ident = $value:expr;
            )*
        }

        $($rest:tt)*
    ) => {
        // The public wrapper. Its field type goes through `PublicFlags` so the
        // hidden internal type never appears by name.
        $(#[$outer])*
        $vis struct $Public(<$Public as $crate::Utilities::Bitflags::PublicFlags>::Internal);

        // The flag constants.
        #[allow(dead_code, deprecated, unused_doc_comments, non_upper_case_globals)]
        impl $Public {
            $(
                $(#[$inner])*
                pub const $Flag: Self = Self::from_bits_retain($value);
            )*
        }

        impl $crate::Utilities::Bitflags::Flags for $Public {
            const FLAGS: &'static [$crate::Utilities::Bitflags::Flag<$Public>] = &[
                $(
                    $crate::Utilities::Bitflags::Flag::new(::core::stringify!($Flag), $Public::$Flag),
                )*
            ];

            type Bits = $T;

            fn bits(&self) -> $T {
                $Public::bits(self)
            }

            fn from_bits_retain(bits: $T) -> $Public {
                $Public::from_bits_retain(bits)
            }
        }

        $crate::Utilities::Bitflags::__impl_flags_public_traits! { $Public, $T }

        #[allow(
            dead_code,
            deprecated,
            unused_doc_comments,
            unused_attributes,
            unused_mut,
            unused_imports,
            non_upper_case_globals
        )]
        const _: () = {
            // The hidden storage type: raw bits plus the hand-written
            // Debug/Display/FromStr that give the wrapper its text form.
            #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
            #[repr(transparent)]
            $vis struct InternalBitFlags($T);

            impl $crate::Utilities::Bitflags::PublicFlags for $Public {
                type Primitive = $T;
                type Internal = InternalBitFlags;
            }

            impl ::core::default::Default for InternalBitFlags {
                #[inline]
                fn default() -> Self {
                    InternalBitFlags::empty()
                }
            }

            impl ::core::fmt::Debug for InternalBitFlags {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    if self.is_empty() {
                        // An empty value has no names to list.
                        ::core::write!(
                            f,
                            "{:#x}",
                            <$T as $crate::Utilities::Bitflags::Bits>::EMPTY
                        )
                    } else {
                        ::core::fmt::Display::fmt(self, f)
                    }
                }
            }

            impl ::core::fmt::Display for InternalBitFlags {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    $crate::Utilities::Bitflags::parser::to_writer(&$Public(*self), f)
                }
            }

            impl ::core::str::FromStr for InternalBitFlags {
                type Err = $crate::Utilities::Bitflags::parser::ParseError;

                fn from_str(s: &str) -> ::core::result::Result<Self, Self::Err> {
                    $crate::Utilities::Bitflags::parser::from_str::<$Public>(s).map(|flags| flags.0)
                }
            }

            impl ::core::convert::AsRef<$T> for InternalBitFlags {
                fn as_ref(&self) -> &$T {
                    &self.0
                }
            }

            impl ::core::convert::From<$T> for InternalBitFlags {
                fn from(bits: $T) -> Self {
                    Self::from_bits_retain(bits)
                }
            }

            // The real implementation, on the hidden type.
            impl InternalBitFlags {
                /// Get a flags value with all bits unset.
                #[inline]
                pub const fn empty() -> Self {
                    InternalBitFlags(<$T as $crate::Utilities::Bitflags::Bits>::EMPTY)
                }

                /// Get a flags value with all known bits set.
                #[inline]
                pub const fn all() -> Self {
                    InternalBitFlags(
                        <$T as $crate::Utilities::Bitflags::Bits>::EMPTY
                        $(| $Public::$Flag.bits())*
                    )
                }

                /// Get the underlying bits value.
                #[inline]
                pub const fn bits(&self) -> $T {
                    self.0
                }

                /// Convert from a bits value exactly.
                #[inline]
                pub const fn from_bits(bits: $T) -> ::core::option::Option<Self> {
                    let truncated = Self::from_bits_truncate(bits).0;

                    if truncated == bits {
                        ::core::option::Option::Some(InternalBitFlags(bits))
                    } else {
                        ::core::option::Option::None
                    }
                }

                /// Convert from a bits value, unsetting any unknown bits.
                #[inline]
                pub const fn from_bits_truncate(bits: $T) -> Self {
                    InternalBitFlags(bits & Self::all().0)
                }

                /// Convert from a bits value, keeping every bit.
                #[inline]
                pub const fn from_bits_retain(bits: $T) -> Self {
                    InternalBitFlags(bits)
                }

                /// The flag with the given name, if any.
                #[inline]
                pub fn from_name(name: &str) -> ::core::option::Option<Self> {
                    $(
                        if name == ::core::stringify!($Flag) {
                            return ::core::option::Option::Some(Self($Public::$Flag.bits()));
                        }
                    )*

                    let _ = name;
                    ::core::option::Option::None
                }

                /// Whether all bits in this flags value are unset.
                #[inline]
                pub const fn is_empty(&self) -> bool {
                    self.0 == <$T as $crate::Utilities::Bitflags::Bits>::EMPTY
                }

                /// Whether all known bits in this flags value are set.
                #[inline]
                pub const fn is_all(&self) -> bool {
                    // NOTE: against `all()`, not the bits type's max, because
                    // the set of all flags may not use every bit.
                    Self::all().0 | self.0 == self.0
                }

                /// Whether any set bits in a source flags value are also set in a target.
                #[inline]
                pub const fn intersects(&self, other: Self) -> bool {
                    self.0 & other.0 != <$T as $crate::Utilities::Bitflags::Bits>::EMPTY
                }

                /// Whether all set bits in a source flags value are also set in a target.
                #[inline]
                pub const fn contains(&self, other: Self) -> bool {
                    self.0 & other.0 == other.0
                }

                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                pub fn insert(&mut self, other: Self) {
                    *self = Self(self.0).union(other);
                }

                /// The intersection with the complement of a target (`&!`).
                #[inline]
                pub fn remove(&mut self, other: Self) {
                    *self = Self(self.0).difference(other);
                }

                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                pub fn toggle(&mut self, other: Self) {
                    *self = Self(self.0).symmetric_difference(other);
                }

                /// Insert when `value` is `true`, remove when it is `false`.
                #[inline]
                pub fn set(&mut self, other: Self, value: bool) {
                    if value {
                        self.insert(other);
                    } else {
                        self.remove(other);
                    }
                }

                /// The bitwise and (`&`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn intersection(self, other: Self) -> Self {
                    Self(self.0 & other.0)
                }

                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn union(self, other: Self) -> Self {
                    Self(self.0 | other.0)
                }

                /// The intersection with the complement of a target (`&!`).
                #[inline]
                #[must_use]
                pub const fn difference(self, other: Self) -> Self {
                    Self(self.0 & !other.0)
                }

                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn symmetric_difference(self, other: Self) -> Self {
                    Self(self.0 ^ other.0)
                }

                /// The bitwise negation (`!`) of the bits, truncating the result.
                #[inline]
                #[must_use]
                pub const fn complement(self) -> Self {
                    Self::from_bits_truncate(!self.0)
                }

                /// Mutable access to the underlying bits.
                #[inline]
                pub fn bits_mut(&mut self) -> &mut $T {
                    &mut self.0
                }
            }

            // The public API: thin forwards to the hidden type, as upstream.
            impl $Public {
                /// Get a flags value with all bits unset.
                #[inline]
                pub const fn empty() -> Self {
                    Self(InternalBitFlags::empty())
                }

                /// Get a flags value with all known bits set.
                #[inline]
                pub const fn all() -> Self {
                    Self(InternalBitFlags::all())
                }

                /// Get the underlying bits value.
                #[inline]
                pub const fn bits(&self) -> $T {
                    self.0.bits()
                }

                /// Convert from a bits value exactly.
                #[inline]
                pub const fn from_bits(bits: $T) -> ::core::option::Option<Self> {
                    match InternalBitFlags::from_bits(bits) {
                        ::core::option::Option::Some(bits) => ::core::option::Option::Some(Self(bits)),
                        ::core::option::Option::None => ::core::option::Option::None,
                    }
                }

                /// Convert from a bits value, unsetting any unknown bits.
                #[inline]
                pub const fn from_bits_truncate(bits: $T) -> Self {
                    Self(InternalBitFlags::from_bits_truncate(bits))
                }

                /// Convert from a bits value, keeping every bit.
                #[inline]
                pub const fn from_bits_retain(bits: $T) -> Self {
                    Self(InternalBitFlags::from_bits_retain(bits))
                }

                /// The flag with the given name, if any.
                #[inline]
                pub fn from_name(name: &str) -> ::core::option::Option<Self> {
                    match InternalBitFlags::from_name(name) {
                        ::core::option::Option::Some(bits) => ::core::option::Option::Some(Self(bits)),
                        ::core::option::Option::None => ::core::option::Option::None,
                    }
                }

                /// Whether all bits in this flags value are unset.
                #[inline]
                pub const fn is_empty(&self) -> bool {
                    self.0.is_empty()
                }

                /// Whether all known bits in this flags value are set.
                #[inline]
                pub const fn is_all(&self) -> bool {
                    self.0.is_all()
                }

                /// Whether any set bits in a source flags value are also set in a target.
                #[inline]
                pub const fn intersects(&self, other: Self) -> bool {
                    self.0.intersects(other.0)
                }

                /// Whether all set bits in a source flags value are also set in a target.
                #[inline]
                pub const fn contains(&self, other: Self) -> bool {
                    self.0.contains(other.0)
                }

                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                pub fn insert(&mut self, other: Self) {
                    self.0.insert(other.0)
                }

                /// The intersection with the complement of a target (`&!`).
                #[inline]
                pub fn remove(&mut self, other: Self) {
                    self.0.remove(other.0)
                }

                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                pub fn toggle(&mut self, other: Self) {
                    self.0.toggle(other.0)
                }

                /// Insert when `value` is `true`, remove when it is `false`.
                #[inline]
                pub fn set(&mut self, other: Self, value: bool) {
                    self.0.set(other.0, value)
                }

                /// The bitwise and (`&`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn intersection(self, other: Self) -> Self {
                    Self(self.0.intersection(other.0))
                }

                /// The bitwise or (`|`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn union(self, other: Self) -> Self {
                    Self(self.0.union(other.0))
                }

                /// The intersection with the complement of a target (`&!`).
                #[inline]
                #[must_use]
                pub const fn difference(self, other: Self) -> Self {
                    Self(self.0.difference(other.0))
                }

                /// The bitwise exclusive-or (`^`) of the bits in two flags values.
                #[inline]
                #[must_use]
                pub const fn symmetric_difference(self, other: Self) -> Self {
                    Self(self.0.symmetric_difference(other.0))
                }

                /// The bitwise negation (`!`) of the bits, truncating the result.
                #[inline]
                #[must_use]
                pub const fn complement(self) -> Self {
                    Self(self.0.complement())
                }

                /// Yield a set of contained flags values: defined flags first,
                /// then any leftover bits as one final value.
                #[inline]
                pub const fn iter(&self) -> $crate::Utilities::Bitflags::iter::Iter<$Public> {
                    $crate::Utilities::Bitflags::iter::Iter::__private_const_new(
                        <$Public as $crate::Utilities::Bitflags::Flags>::FLAGS,
                        $Public::from_bits_retain(self.bits()),
                        $Public::from_bits_retain(self.bits()),
                    )
                }

                /// Yield a set of contained named flags values.
                #[inline]
                pub const fn iter_names(&self) -> $crate::Utilities::Bitflags::iter::IterNames<$Public> {
                    $crate::Utilities::Bitflags::iter::IterNames::__private_const_new(
                        <$Public as $crate::Utilities::Bitflags::Flags>::FLAGS,
                        $Public::from_bits_retain(self.bits()),
                        $Public::from_bits_retain(self.bits()),
                    )
                }
            }
        };

        $crate::Utilities::Bitflags::bitflags! { $($rest)* }
    };
}

pub(crate) use __impl_flags_public_traits;
pub(crate) use bitflags;
