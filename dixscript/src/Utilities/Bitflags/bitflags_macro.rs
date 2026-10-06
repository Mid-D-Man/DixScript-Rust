// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags"
// ============================================================================
// Derived from `bitflags` 2.10.0 (https://github.com/bitflags/bitflags),
// Copyright (c) The Rust Project Developers, licensed MIT OR Apache-2.0.
// Port of the macros in src/{lib,internal,public}.rs. `bitflags_match!` is not ported.
//
//! The `bitflags!` macro and the helper macros it expands to, ported from
//! upstream `lib.rs`, `internal.rs` and `public.rs`. All of them are
//! crate-private (`pub(crate) use`) and every `$crate::` path points at
//! `$crate::Utilities::Bitflags::`.

macro_rules! bitflags {
    (
        $(#[$outer:meta])*
        $vis:vis struct $BitFlags:ident: $T:ty {
            $(
                $(#[$inner:ident $($args:tt)*])*
                const $Flag:tt = $value:expr;
            )*
        }

        $($t:tt)*
    ) => {
        // Declared in the scope of the `bitflags!` call
        // This type appears in the end-user's API
        $crate::Utilities::Bitflags::__declare_public_bitflags! {
            $(#[$outer])*
            $vis struct $BitFlags
        }

        // Workaround for: https://github.com/bitflags/bitflags/issues/320
        $crate::Utilities::Bitflags::__impl_public_bitflags_consts! {
            $BitFlags: $T {
                $(
                    $(#[$inner $($args)*])*
                    const $Flag = $value;
                )*
            }
        }

        #[allow(
            dead_code,
            deprecated,
            unused_doc_comments,
            unused_attributes,
            unused_mut,
            unused_imports,
            non_upper_case_globals,
            clippy::assign_op_pattern,
            clippy::indexing_slicing,
            clippy::same_name_method,
            clippy::iter_without_into_iter,
        )]
        const _: () = {
            // Declared in a "hidden" scope that can't be reached directly
            // These types don't appear in the end-user's API
            $crate::Utilities::Bitflags::__declare_internal_bitflags! {
                $vis struct InternalBitFlags: $T
            }

            $crate::Utilities::Bitflags::__impl_internal_bitflags! {
                InternalBitFlags: $T, $BitFlags {
                    $(
                        $(#[$inner $($args)*])*
                        const $Flag = $value;
                    )*
                }
            }

            // This is where new library trait implementations can be added
            $crate::Utilities::Bitflags::__impl_external_bitflags! {
                InternalBitFlags: $T, $BitFlags {
                    $(
                        $(#[$inner $($args)*])*
                        const $Flag;
                    )*
                }
            }

            $crate::Utilities::Bitflags::__impl_public_bitflags_forward! {
                $BitFlags: $T, InternalBitFlags
            }

            $crate::Utilities::Bitflags::__impl_public_bitflags_ops! {
                $BitFlags
            }

            $crate::Utilities::Bitflags::__impl_public_bitflags_iter! {
                $BitFlags: $T, $BitFlags
            }
        };

        $crate::Utilities::Bitflags::bitflags! {
            $($t)*
        }
    };
    (
        $(#[$outer:meta])*
        impl $BitFlags:ident: $T:ty {
            $(
                $(#[$inner:ident $($args:tt)*])*
                const $Flag:tt = $value:expr;
            )*
        }

        $($t:tt)*
    ) => {
        $crate::Utilities::Bitflags::__impl_public_bitflags_consts! {
            $BitFlags: $T {
                $(
                    $(#[$inner $($args)*])*
                    const $Flag = $value;
                )*
            }
        }

        #[allow(
            dead_code,
            deprecated,
            unused_doc_comments,
            unused_attributes,
            unused_mut,
            unused_imports,
            non_upper_case_globals,
            clippy::assign_op_pattern,
            clippy::iter_without_into_iter,
        )]
        const _: () = {
            $crate::Utilities::Bitflags::__impl_public_bitflags! {
                $(#[$outer])*
                $BitFlags: $T, $BitFlags {
                    $(
                        $(#[$inner $($args)*])*
                        const $Flag = $value;
                    )*
                }
            }

            $crate::Utilities::Bitflags::__impl_public_bitflags_ops! {
                $BitFlags
            }

            $crate::Utilities::Bitflags::__impl_public_bitflags_iter! {
                $BitFlags: $T, $BitFlags
            }
        };

        $crate::Utilities::Bitflags::bitflags! {
            $($t)*
        }
    };
    () => {};
}
pub(crate) use bitflags;

macro_rules! __impl_bitflags {
    (
        // These param names must be passed in to make the macro work.
        // Just use `params: self, bits, name, other, value;`.
        params: $self:ident, $bits:ident, $name:ident, $other:ident, $value:ident;
        $(#[$outer:meta])*
        $PublicBitFlags:ident: $T:ty {
            fn empty() $empty_body:block
            fn all() $all_body:block
            fn bits(&self) $bits_body:block
            fn from_bits(bits) $from_bits_body:block
            fn from_bits_truncate(bits) $from_bits_truncate_body:block
            fn from_bits_retain(bits) $from_bits_retain_body:block
            fn from_name(name) $from_name_body:block
            fn is_empty(&self) $is_empty_body:block
            fn is_all(&self) $is_all_body:block
            fn intersects(&self, other) $intersects_body:block
            fn contains(&self, other) $contains_body:block
            fn insert(&mut self, other) $insert_body:block
            fn remove(&mut self, other) $remove_body:block
            fn toggle(&mut self, other) $toggle_body:block
            fn set(&mut self, other, value) $set_body:block
            fn intersection(self, other) $intersection_body:block
            fn union(self, other) $union_body:block
            fn difference(self, other) $difference_body:block
            fn symmetric_difference(self, other) $symmetric_difference_body:block
            fn complement(self) $complement_body:block
        }
    ) => {
        #[allow(dead_code, deprecated, unused_attributes)]
        $(#[$outer])*
        impl $PublicBitFlags {
            /// Get a flags value with all bits unset.
            #[inline]
            pub const fn empty() -> Self
                $empty_body

            /// Get a flags value with all known bits set.
            #[inline]
            pub const fn all() -> Self
                $all_body

            /// Get the underlying bits value.
            ///
            /// The returned value is exactly the bits set in this flags value.
            #[inline]
            pub const fn bits(&$self) -> $T
                $bits_body

            /// Convert from a bits value.
            ///
            /// This method will return `None` if any unknown bits are set.
            #[inline]
            pub const fn from_bits($bits: $T) -> $crate::Utilities::Bitflags::__private::core::option::Option<Self>
                $from_bits_body

            /// Convert from a bits value, unsetting any unknown bits.
            #[inline]
            pub const fn from_bits_truncate($bits: $T) -> Self
                $from_bits_truncate_body

            /// Convert from a bits value exactly.
            #[inline]
            pub const fn from_bits_retain($bits: $T) -> Self
                $from_bits_retain_body

            /// Get a flags value with the bits of a flag with the given name set.
            ///
            /// This method will return `None` if `name` is empty or doesn't
            /// correspond to any named flag.
            #[inline]
            pub fn from_name($name: &str) -> $crate::Utilities::Bitflags::__private::core::option::Option<Self>
                $from_name_body

            /// Whether all bits in this flags value are unset.
            #[inline]
            pub const fn is_empty(&$self) -> bool
                $is_empty_body

            /// Whether all known bits in this flags value are set.
            #[inline]
            pub const fn is_all(&$self) -> bool
                $is_all_body

            /// Whether any set bits in a source flags value are also set in a target flags value.
            #[inline]
            pub const fn intersects(&$self, $other: Self) -> bool
                $intersects_body

            /// Whether all set bits in a source flags value are also set in a target flags value.
            #[inline]
            pub const fn contains(&$self, $other: Self) -> bool
                $contains_body

            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            pub fn insert(&mut $self, $other: Self)
                $insert_body

            /// The intersection of a source flags value with the complement of a target flags
            /// value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `remove` won't truncate `other`, but the `!` operator will.
            #[inline]
            pub fn remove(&mut $self, $other: Self)
                $remove_body

            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            pub fn toggle(&mut $self, $other: Self)
                $toggle_body

            /// Call `insert` when `value` is `true` or `remove` when `value` is `false`.
            #[inline]
            pub fn set(&mut $self, $other: Self, $value: bool)
                $set_body

            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            #[must_use]
            pub const fn intersection($self, $other: Self) -> Self
                $intersection_body

            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            #[must_use]
            pub const fn union($self, $other: Self) -> Self
                $union_body

            /// The intersection of a source flags value with the complement of a target flags
            /// value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `difference` won't truncate `other`, but the `!` operator will.
            #[inline]
            #[must_use]
            pub const fn difference($self, $other: Self) -> Self
                $difference_body

            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            #[must_use]
            pub const fn symmetric_difference($self, $other: Self) -> Self
                $symmetric_difference_body

            /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
            #[inline]
            #[must_use]
            pub const fn complement($self) -> Self
                $complement_body
        }
    };
}
pub(crate) use __impl_bitflags;

macro_rules! __bitflags_expr_safe_attrs {
    // Entrypoint: Move all flags and all attributes into `unprocessed` lists
    // where they'll be munched one-at-a-time
    (
        $(#[$inner:ident $($args:tt)*])*
        { $e:expr }
    ) => {
        $crate::Utilities::Bitflags::__bitflags_expr_safe_attrs! {
            expr: { $e },
            attrs: {
                // All attributes start here
                unprocessed: [$(#[$inner $($args)*])*],
                // Attributes that are safe on expressions go here
                processed: [],
            },
        }
    };
    // Process the next attribute on the current flag
    // `cfg`: The next flag should be propagated to expressions
    // NOTE: You can copy this rules block and replace `cfg` with
    // your attribute name that should be considered expression-safe
    (
        expr: { $e:expr },
            attrs: {
            unprocessed: [
                // cfg matched here
                #[cfg $($args:tt)*]
                $($attrs_rest:tt)*
            ],
            processed: [$($expr:tt)*],
        },
    ) => {
        $crate::Utilities::Bitflags::__bitflags_expr_safe_attrs! {
            expr: { $e },
            attrs: {
                unprocessed: [
                    $($attrs_rest)*
                ],
                processed: [
                    $($expr)*
                    // cfg added here
                    #[cfg $($args)*]
                ],
            },
        }
    };
    // Process the next attribute on the current flag
    // `$other`: The next flag should not be propagated to expressions
    (
        expr: { $e:expr },
            attrs: {
            unprocessed: [
                // $other matched here
                #[$other:ident $($args:tt)*]
                $($attrs_rest:tt)*
            ],
            processed: [$($expr:tt)*],
        },
    ) => {
        $crate::Utilities::Bitflags::__bitflags_expr_safe_attrs! {
            expr: { $e },
                attrs: {
                unprocessed: [
                    $($attrs_rest)*
                ],
                processed: [
                    // $other not added here
                    $($expr)*
                ],
            },
        }
    };
    // Once all attributes on all flags are processed, generate the actual code
    (
        expr: { $e:expr },
        attrs: {
            unprocessed: [],
            processed: [$(#[$expr:ident $($exprargs:tt)*])*],
        },
    ) => {
        $(#[$expr $($exprargs)*])*
        { $e }
    }
}
pub(crate) use __bitflags_expr_safe_attrs;

macro_rules! __bitflags_flag {
    (
        {
            name: _,
            named: { $($named:tt)* },
            unnamed: { $($unnamed:tt)* },
        }
    ) => {
        $($unnamed)*
    };
    (
        {
            name: $Flag:ident,
            named: { $($named:tt)* },
            unnamed: { $($unnamed:tt)* },
        }
    ) => {
        $($named)*
    };
}
pub(crate) use __bitflags_flag;

macro_rules! __declare_internal_bitflags {
    (
        $vis:vis struct $InternalBitFlags:ident: $T:ty
    ) => {
        // NOTE: The ABI of this type is _guaranteed_ to be the same as `T`
        // This is relied on by some external libraries like `bytemuck` to make
        // its `unsafe` trait impls sound.
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        $vis struct $InternalBitFlags($T);
    };
}
pub(crate) use __declare_internal_bitflags;

macro_rules! __impl_internal_bitflags {
    (
        $InternalBitFlags:ident: $T:ty, $PublicBitFlags:ident {
            $(
                $(#[$inner:ident $($args:tt)*])*
                const $Flag:tt = $value:expr;
            )*
        }
    ) => {
        // NOTE: This impl is also used to prevent using bits types from non-primitive types
        // in the `bitflags` macro. If this approach is changed, this guard will need to be
        // retained somehow
        impl $crate::Utilities::Bitflags::__private::PublicFlags for $PublicBitFlags {
            type Primitive = $T;
            type Internal = $InternalBitFlags;
        }

        impl $crate::Utilities::Bitflags::__private::core::default::Default for $InternalBitFlags {
            #[inline]
            fn default() -> Self {
                $InternalBitFlags::empty()
            }
        }

        impl $crate::Utilities::Bitflags::__private::core::fmt::Debug for $InternalBitFlags {
            fn fmt(&self, f: &mut $crate::Utilities::Bitflags::__private::core::fmt::Formatter<'_>) -> $crate::Utilities::Bitflags::__private::core::fmt::Result {
                if self.is_empty() {
                    // If no flags are set then write an empty hex flag to avoid
                    // writing an empty string. In some contexts, like serialization,
                    // an empty string is preferable, but it may be unexpected in
                    // others for a format not to produce any output.
                    //
                    // We can remove this `0x0` and remain compatible with `FromStr`,
                    // because an empty string will still parse to an empty set of flags,
                    // just like `0x0` does.
                    $crate::Utilities::Bitflags::__private::core::write!(f, "{:#x}", <$T as $crate::Utilities::Bitflags::Bits>::EMPTY)
                } else {
                    $crate::Utilities::Bitflags::__private::core::fmt::Display::fmt(self, f)
                }
            }
        }

        impl $crate::Utilities::Bitflags::__private::core::fmt::Display for $InternalBitFlags {
            fn fmt(&self, f: &mut $crate::Utilities::Bitflags::__private::core::fmt::Formatter<'_>) -> $crate::Utilities::Bitflags::__private::core::fmt::Result {
                $crate::Utilities::Bitflags::parser::to_writer(&$PublicBitFlags(*self), f)
            }
        }

        impl $crate::Utilities::Bitflags::__private::core::str::FromStr for $InternalBitFlags {
            type Err = $crate::Utilities::Bitflags::parser::ParseError;

            fn from_str(s: &str) -> $crate::Utilities::Bitflags::__private::core::result::Result<Self, Self::Err> {
                $crate::Utilities::Bitflags::parser::from_str::<$PublicBitFlags>(s).map(|flags| flags.0)
            }
        }

        impl $crate::Utilities::Bitflags::__private::core::convert::AsRef<$T> for $InternalBitFlags {
            fn as_ref(&self) -> &$T {
                &self.0
            }
        }

        impl $crate::Utilities::Bitflags::__private::core::convert::From<$T> for $InternalBitFlags {
            fn from(bits: $T) -> Self {
                Self::from_bits_retain(bits)
            }
        }

        // The internal flags type offers a similar API to the public one

        $crate::Utilities::Bitflags::__impl_public_bitflags! {
            $InternalBitFlags: $T, $PublicBitFlags {
                $(
                    $(#[$inner $($args)*])*
                    const $Flag = $value;
                )*
            }
        }

        $crate::Utilities::Bitflags::__impl_public_bitflags_ops! {
            $InternalBitFlags
        }

        $crate::Utilities::Bitflags::__impl_public_bitflags_iter! {
            $InternalBitFlags: $T, $PublicBitFlags
        }

        impl $InternalBitFlags {
            /// Returns a mutable reference to the raw value of the flags currently stored.
            #[inline]
            pub fn bits_mut(&mut self) -> &mut $T {
                &mut self.0
            }
        }
    };
}
pub(crate) use __impl_internal_bitflags;

macro_rules! __declare_public_bitflags {
    (
        $(#[$outer:meta])*
        $vis:vis struct $PublicBitFlags:ident
    ) => {
        $(#[$outer])*
        $vis struct $PublicBitFlags(<$PublicBitFlags as $crate::Utilities::Bitflags::__private::PublicFlags>::Internal);
    };
}
pub(crate) use __declare_public_bitflags;

macro_rules! __impl_public_bitflags_forward {
    (
        $(#[$outer:meta])*
        $PublicBitFlags:ident: $T:ty, $InternalBitFlags:ident
    ) => {
        $crate::Utilities::Bitflags::__impl_bitflags! {
            params: self, bits, name, other, value;
            $(#[$outer])*
            $PublicBitFlags: $T {
                fn empty() {
                    Self($InternalBitFlags::empty())
                }

                fn all() {
                    Self($InternalBitFlags::all())
                }

                fn bits(&self) {
                    self.0.bits()
                }

                fn from_bits(bits) {
                    match $InternalBitFlags::from_bits(bits) {
                        $crate::Utilities::Bitflags::__private::core::option::Option::Some(bits) => $crate::Utilities::Bitflags::__private::core::option::Option::Some(Self(bits)),
                        $crate::Utilities::Bitflags::__private::core::option::Option::None => $crate::Utilities::Bitflags::__private::core::option::Option::None,
                    }
                }

                fn from_bits_truncate(bits) {
                    Self($InternalBitFlags::from_bits_truncate(bits))
                }

                fn from_bits_retain(bits) {
                    Self($InternalBitFlags::from_bits_retain(bits))
                }

                fn from_name(name) {
                    match $InternalBitFlags::from_name(name) {
                        $crate::Utilities::Bitflags::__private::core::option::Option::Some(bits) => $crate::Utilities::Bitflags::__private::core::option::Option::Some(Self(bits)),
                        $crate::Utilities::Bitflags::__private::core::option::Option::None => $crate::Utilities::Bitflags::__private::core::option::Option::None,
                    }
                }

                fn is_empty(&self) {
                    self.0.is_empty()
                }

                fn is_all(&self) {
                    self.0.is_all()
                }

                fn intersects(&self, other) {
                    self.0.intersects(other.0)
                }

                fn contains(&self, other) {
                    self.0.contains(other.0)
                }

                fn insert(&mut self, other) {
                    self.0.insert(other.0)
                }

                fn remove(&mut self, other) {
                    self.0.remove(other.0)
                }

                fn toggle(&mut self, other) {
                    self.0.toggle(other.0)
                }

                fn set(&mut self, other, value) {
                    self.0.set(other.0, value)
                }

                fn intersection(self, other) {
                    Self(self.0.intersection(other.0))
                }

                fn union(self, other) {
                    Self(self.0.union(other.0))
                }

                fn difference(self, other) {
                    Self(self.0.difference(other.0))
                }

                fn symmetric_difference(self, other) {
                    Self(self.0.symmetric_difference(other.0))
                }

                fn complement(self) {
                    Self(self.0.complement())
                }
            }
        }
    };
}
pub(crate) use __impl_public_bitflags_forward;

macro_rules! __impl_public_bitflags {
    (
        $(#[$outer:meta])*
        $BitFlags:ident: $T:ty, $PublicBitFlags:ident {
            $(
                $(#[$inner:ident $($args:tt)*])*
                const $Flag:tt = $value:expr;
            )*
        }
    ) => {
        $crate::Utilities::Bitflags::__impl_bitflags! {
            params: self, bits, name, other, value;
            $(#[$outer])*
            $BitFlags: $T {
                fn empty() {
                    Self(<$T as $crate::Utilities::Bitflags::Bits>::EMPTY)
                }

                fn all() {
                    let mut truncated = <$T as $crate::Utilities::Bitflags::Bits>::EMPTY;
                    let mut i = 0;

                    $(
                        $crate::Utilities::Bitflags::__bitflags_expr_safe_attrs!(
                            $(#[$inner $($args)*])*
                            {{
                                let flag = <$PublicBitFlags as $crate::Utilities::Bitflags::Flags>::FLAGS[i].value().bits();

                                truncated = truncated | flag;
                                i += 1;
                            }}
                        );
                    )*

                    let _ = i;
                    Self(truncated)
                }

                fn bits(&self) {
                    self.0
                }

                fn from_bits(bits) {
                    let truncated = Self::from_bits_truncate(bits).0;

                    if truncated == bits {
                        $crate::Utilities::Bitflags::__private::core::option::Option::Some(Self(bits))
                    } else {
                        $crate::Utilities::Bitflags::__private::core::option::Option::None
                    }
                }

                fn from_bits_truncate(bits) {
                    Self(bits & Self::all().0)
                }

                fn from_bits_retain(bits) {
                    Self(bits)
                }

                fn from_name(name) {
                    $(
                        $crate::Utilities::Bitflags::__bitflags_flag!({
                            name: $Flag,
                            named: {
                                $crate::Utilities::Bitflags::__bitflags_expr_safe_attrs!(
                                    $(#[$inner $($args)*])*
                                    {
                                        if name == $crate::Utilities::Bitflags::__private::core::stringify!($Flag) {
                                            return $crate::Utilities::Bitflags::__private::core::option::Option::Some(Self($PublicBitFlags::$Flag.bits()));
                                        }
                                    }
                                );
                            },
                            unnamed: {},
                        });
                    )*

                    let _ = name;
                    $crate::Utilities::Bitflags::__private::core::option::Option::None
                }

                fn is_empty(&self) {
                    self.0 == <$T as $crate::Utilities::Bitflags::Bits>::EMPTY
                }

                fn is_all(&self) {
                    // NOTE: We check against `Self::all` here, not `Self::Bits::ALL`
                    // because the set of all flags may not use all bits
                    Self::all().0 | self.0 == self.0
                }

                fn intersects(&self, other) {
                    self.0 & other.0 != <$T as $crate::Utilities::Bitflags::Bits>::EMPTY
                }

                fn contains(&self, other) {
                    self.0 & other.0 == other.0
                }

                fn insert(&mut self, other) {
                    *self = Self(self.0).union(other);
                }

                fn remove(&mut self, other) {
                    *self = Self(self.0).difference(other);
                }

                fn toggle(&mut self, other) {
                    *self = Self(self.0).symmetric_difference(other);
                }

                fn set(&mut self, other, value) {
                    if value {
                        self.insert(other);
                    } else {
                        self.remove(other);
                    }
                }

                fn intersection(self, other) {
                    Self(self.0 & other.0)
                }

                fn union(self, other) {
                    Self(self.0 | other.0)
                }

                fn difference(self, other) {
                    Self(self.0 & !other.0)
                }

                fn symmetric_difference(self, other) {
                    Self(self.0 ^ other.0)
                }

                fn complement(self) {
                    Self::from_bits_truncate(!self.0)
                }
            }
        }
    };
}
pub(crate) use __impl_public_bitflags;

macro_rules! __impl_public_bitflags_iter {
    (
        $(#[$outer:meta])*
        $BitFlags:ident: $T:ty, $PublicBitFlags:ident
    ) => {
        $(#[$outer])*
        impl $BitFlags {
            /// Yield a set of contained flags values.
            ///
            /// Each yielded flags value will correspond to a defined named flag. Any unknown bits
            /// will be yielded together as a final flags value.
            #[inline]
            pub const fn iter(&self) -> $crate::Utilities::Bitflags::iter::Iter<$PublicBitFlags> {
                $crate::Utilities::Bitflags::iter::Iter::__private_const_new(
                    <$PublicBitFlags as $crate::Utilities::Bitflags::Flags>::FLAGS,
                    $PublicBitFlags::from_bits_retain(self.bits()),
                    $PublicBitFlags::from_bits_retain(self.bits()),
                )
            }

            /// Yield a set of contained named flags values.
            ///
            /// This method is like [`iter`](#method.iter), except only yields bits in contained named flags.
            /// Any unknown bits, or bits not corresponding to a contained flag will not be yielded.
            #[inline]
            pub const fn iter_names(&self) -> $crate::Utilities::Bitflags::iter::IterNames<$PublicBitFlags> {
                $crate::Utilities::Bitflags::iter::IterNames::__private_const_new(
                    <$PublicBitFlags as $crate::Utilities::Bitflags::Flags>::FLAGS,
                    $PublicBitFlags::from_bits_retain(self.bits()),
                    $PublicBitFlags::from_bits_retain(self.bits()),
                )
            }
        }

        $(#[$outer:meta])*
        impl $crate::Utilities::Bitflags::__private::core::iter::IntoIterator for $BitFlags {
            type Item = $PublicBitFlags;
            type IntoIter = $crate::Utilities::Bitflags::iter::Iter<$PublicBitFlags>;

            fn into_iter(self) -> Self::IntoIter {
                self.iter()
            }
        }
    };
}
pub(crate) use __impl_public_bitflags_iter;

macro_rules! __impl_public_bitflags_ops {
    (
        $(#[$outer:meta])*
        $PublicBitFlags:ident
    ) => {

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::fmt::Binary for $PublicBitFlags {
            fn fmt(
                &self,
                f: &mut $crate::Utilities::Bitflags::__private::core::fmt::Formatter,
            ) -> $crate::Utilities::Bitflags::__private::core::fmt::Result {
                let inner = self.0;
                $crate::Utilities::Bitflags::__private::core::fmt::Binary::fmt(&inner, f)
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::fmt::Octal for $PublicBitFlags {
            fn fmt(
                &self,
                f: &mut $crate::Utilities::Bitflags::__private::core::fmt::Formatter,
            ) -> $crate::Utilities::Bitflags::__private::core::fmt::Result {
                let inner = self.0;
                $crate::Utilities::Bitflags::__private::core::fmt::Octal::fmt(&inner, f)
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::fmt::LowerHex for $PublicBitFlags {
            fn fmt(
                &self,
                f: &mut $crate::Utilities::Bitflags::__private::core::fmt::Formatter,
            ) -> $crate::Utilities::Bitflags::__private::core::fmt::Result {
                let inner = self.0;
                $crate::Utilities::Bitflags::__private::core::fmt::LowerHex::fmt(&inner, f)
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::fmt::UpperHex for $PublicBitFlags {
            fn fmt(
                &self,
                f: &mut $crate::Utilities::Bitflags::__private::core::fmt::Formatter,
            ) -> $crate::Utilities::Bitflags::__private::core::fmt::Result {
                let inner = self.0;
                $crate::Utilities::Bitflags::__private::core::fmt::UpperHex::fmt(&inner, f)
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::ops::BitOr for $PublicBitFlags {
            type Output = Self;

            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            fn bitor(self, other: $PublicBitFlags) -> Self {
                self.union(other)
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::ops::BitOrAssign for $PublicBitFlags {
            /// The bitwise or (`|`) of the bits in two flags values.
            #[inline]
            fn bitor_assign(&mut self, other: Self) {
                self.insert(other);
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::ops::BitXor for $PublicBitFlags {
            type Output = Self;

            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            fn bitxor(self, other: Self) -> Self {
                self.symmetric_difference(other)
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::ops::BitXorAssign for $PublicBitFlags {
            /// The bitwise exclusive-or (`^`) of the bits in two flags values.
            #[inline]
            fn bitxor_assign(&mut self, other: Self) {
                self.toggle(other);
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::ops::BitAnd for $PublicBitFlags {
            type Output = Self;

            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            fn bitand(self, other: Self) -> Self {
                self.intersection(other)
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::ops::BitAndAssign for $PublicBitFlags {
            /// The bitwise and (`&`) of the bits in two flags values.
            #[inline]
            fn bitand_assign(&mut self, other: Self) {
                *self = Self::from_bits_retain(self.bits()).intersection(other);
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::ops::Sub for $PublicBitFlags {
            type Output = Self;

            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `difference` won't truncate `other`, but the `!` operator will.
            #[inline]
            fn sub(self, other: Self) -> Self {
                self.difference(other)
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::ops::SubAssign for $PublicBitFlags {
            /// The intersection of a source flags value with the complement of a target flags value (`&!`).
            ///
            /// This method is not equivalent to `self & !other` when `other` has unknown bits set.
            /// `difference` won't truncate `other`, but the `!` operator will.
            #[inline]
            fn sub_assign(&mut self, other: Self) {
                self.remove(other);
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::ops::Not for $PublicBitFlags {
            type Output = Self;

            /// The bitwise negation (`!`) of the bits in a flags value, truncating the result.
            #[inline]
            fn not(self) -> Self {
                self.complement()
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::iter::Extend<$PublicBitFlags> for $PublicBitFlags {
            /// The bitwise or (`|`) of the bits in each flags value.
            fn extend<T: $crate::Utilities::Bitflags::__private::core::iter::IntoIterator<Item = Self>>(
                &mut self,
                iterator: T,
            ) {
                for item in iterator {
                    self.insert(item)
                }
            }
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::__private::core::iter::FromIterator<$PublicBitFlags> for $PublicBitFlags {
            /// The bitwise or (`|`) of the bits in each flags value.
            fn from_iter<T: $crate::Utilities::Bitflags::__private::core::iter::IntoIterator<Item = Self>>(
                iterator: T,
            ) -> Self {
                use $crate::Utilities::Bitflags::__private::core::iter::Extend;

                let mut result = Self::empty();
                result.extend(iterator);
                result
            }
        }
    };
}
pub(crate) use __impl_public_bitflags_ops;

macro_rules! __impl_public_bitflags_consts {
    (
        $(#[$outer:meta])*
        $PublicBitFlags:ident: $T:ty {
            $(
                $(#[$inner:ident $($args:tt)*])*
                const $Flag:tt = $value:expr;
            )*
        }
    ) => {
        $(#[$outer])*
        impl $PublicBitFlags {
            $(
                $crate::Utilities::Bitflags::__bitflags_flag!({
                    name: $Flag,
                    named: {
                        $(#[$inner $($args)*])*
                        #[allow(
                            deprecated,
                            non_upper_case_globals,
                        )]
                        pub const $Flag: Self = Self::from_bits_retain($value);
                    },
                    unnamed: {},
                });
            )*
        }

        $(#[$outer])*
        impl $crate::Utilities::Bitflags::Flags for $PublicBitFlags {
            const FLAGS: &'static [$crate::Utilities::Bitflags::Flag<$PublicBitFlags>] = &[
                $(
                    $crate::Utilities::Bitflags::__bitflags_flag!({
                        name: $Flag,
                        named: {
                            $crate::Utilities::Bitflags::__bitflags_expr_safe_attrs!(
                                $(#[$inner $($args)*])*
                                {
                                    #[allow(
                                        deprecated,
                                        non_upper_case_globals,
                                    )]
                                    $crate::Utilities::Bitflags::Flag::new($crate::Utilities::Bitflags::__private::core::stringify!($Flag), $PublicBitFlags::$Flag)
                                }
                            )
                        },
                        unnamed: {
                            $crate::Utilities::Bitflags::__bitflags_expr_safe_attrs!(
                                $(#[$inner $($args)*])*
                                {
                                    #[allow(
                                        deprecated,
                                        non_upper_case_globals,
                                    )]
                                    $crate::Utilities::Bitflags::Flag::new("", $PublicBitFlags::from_bits_retain($value))
                                }
                            )
                        },
                    }),
                )*
            ];

            type Bits = $T;

            fn bits(&self) -> $T {
                $PublicBitFlags::bits(self)
            }

            fn from_bits_retain(bits: $T) -> $PublicBitFlags {
                $PublicBitFlags::from_bits_retain(bits)
            }
        }
    };
}
pub(crate) use __impl_public_bitflags_consts;

// Upstream's `external` module (serde / arbitrary / bytemuck glue) is not
// ported. The `bitflags!` macro still calls this hook, so it is kept as a
// no-op that accepts the same input.
macro_rules! __impl_external_bitflags {
    (
        $InternalBitFlags:ident: $T:ty, $PublicBitFlags:ident {
            $(
                $(#[$inner:ident $($args:tt)*])*
                const $Flag:tt;
            )*
        }
    ) => {};
}
pub(crate) use __impl_external_bitflags;
