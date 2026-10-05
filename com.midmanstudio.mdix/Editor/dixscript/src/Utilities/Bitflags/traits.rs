// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags"
// ============================================================================
//! The trait layer of the `bitflags` port: [`Flag`], [`Flags`], [`Bits`],
//! [`Primitive`] and [`PublicFlags`].
//!
//! ## Attribution
//! Derived from the `bitflags` crate, v2.10.0
//! (<https://github.com/bitflags/bitflags>), licensed `MIT OR Apache-2.0`.
//! The trait definitions and every default method body follow upstream's
//! `src/traits.rs`; explanatory comments are condensed. Upstream's
//! deprecated `BitFlags` alias trait and `ImplementedByBitFlagsMacro` marker
//! are not carried over.

use core::{
    fmt,
    ops::{BitAnd, BitOr, BitXor, Not},
};

use super::iter;
use super::parser::{ParseError, ParseHex, WriteHex};

/// A defined flags value that may be named or unnamed.
#[derive(Debug)]
pub struct Flag<B> {
    name: &'static str,
    value: B,
}

impl<B> Flag<B> {
    /// Define a flag. If `name` is non-empty the flag is named, otherwise it
    /// is unnamed.
    pub const fn new(name: &'static str, value: B) -> Self {
        Flag { name, value }
    }

    /// The flag's name; empty if the flag is unnamed.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The flag's value.
    pub const fn value(&self) -> &B {
        &self.value
    }

    /// Whether the flag is named.
    pub const fn is_named(&self) -> bool {
        !self.name.is_empty()
    }

    /// Whether the flag is unnamed.
    pub const fn is_unnamed(&self) -> bool {
        self.name.is_empty()
    }
}

/// A set of defined flags using a bits type as storage.
///
/// Implemented by the `bitflags!` macro; it can also be written by hand.
/// Everything with a default body here is also available, under the same name,
/// as an inherent method on the generated type, except `truncate`, `clear`,
/// `contains_unknown_bits` and `iter_defined_names`, which (as upstream) exist
/// only on this trait.
pub trait Flags: Sized + 'static {
    /// The set of defined flags.
    const FLAGS: &'static [Flag<Self>];

    /// The underlying bits type.
    type Bits: Bits;

    /// Get a flags value with all bits unset.
    fn empty() -> Self {
        Self::from_bits_retain(Self::Bits::EMPTY)
    }

    /// Get a flags value with all known bits set.
    fn all() -> Self {
        let mut truncated = Self::Bits::EMPTY;

        for flag in Self::FLAGS.iter() {
            truncated = truncated | flag.value().bits();
        }

        Self::from_bits_retain(truncated)
    }

    /// Whether any unknown bits are set.
    fn contains_unknown_bits(&self) -> bool {
        Self::all().bits() & self.bits() != self.bits()
    }

    /// Get the underlying bits value.
    fn bits(&self) -> Self::Bits;

    /// Convert from a bits value exactly; `None` if any unknown bit is set.
    fn from_bits(bits: Self::Bits) -> Option<Self> {
        let truncated = Self::from_bits_truncate(bits);

        if truncated.bits() == bits {
            Some(truncated)
        } else {
            None
        }
    }

    /// Convert from a bits value, unsetting any unknown bits.
    fn from_bits_truncate(bits: Self::Bits) -> Self {
        Self::from_bits_retain(bits & Self::all().bits())
    }

    /// Convert from a bits value, keeping every bit.
    fn from_bits_retain(bits: Self::Bits) -> Self;

    /// Get a flags value with the bits of a flag with the given name set.
    /// `None` if there's no flag with that name, or if `name` is empty.
    fn from_name(name: &str) -> Option<Self> {
        // Don't parse empty names as empty flags
        if name.is_empty() {
            return None;
        }

        for flag in Self::FLAGS {
            if flag.name() == name {
                return Some(Self::from_bits_retain(flag.value().bits()));
            }
        }

        None
    }

    /// Yield a set of contained flags values: defined flags first, then any
    /// leftover bits as one final value.
    fn iter(&self) -> iter::Iter<Self> {
        iter::Iter::new(self)
    }

    /// Yield a set of contained named flags values.
    fn iter_names(&self) -> iter::IterNames<Self> {
        iter::IterNames::new(self)
    }

    /// Yield a set of all named flags defined by [`Self::FLAGS`].
    fn iter_defined_names() -> iter::IterDefinedNames<Self> {
        iter::IterDefinedNames::new()
    }

    /// Whether all bits in this flags value are unset.
    fn is_empty(&self) -> bool {
        self.bits() == Self::Bits::EMPTY
    }

    /// Whether all known bits in this flags value are set.
    fn is_all(&self) -> bool {
        // NOTE: Checked against `Self::all`, not `Self::Bits::ALL`, because the
        // set of all flags may not use every bit.
        Self::all().bits() | self.bits() == self.bits()
    }

    /// Whether any set bits in a source flags value are also set in a target.
    fn intersects(&self, other: Self) -> bool
    where
        Self: Sized,
    {
        self.bits() & other.bits() != Self::Bits::EMPTY
    }

    /// Whether all set bits in a source flags value are also set in a target.
    fn contains(&self, other: Self) -> bool
    where
        Self: Sized,
    {
        self.bits() & other.bits() == other.bits()
    }

    /// Remove any unknown bits from the flags.
    fn truncate(&mut self)
    where
        Self: Sized,
    {
        *self = Self::from_bits_truncate(self.bits());
    }

    /// The bitwise or (`|`) of the bits in two flags values.
    fn insert(&mut self, other: Self)
    where
        Self: Sized,
    {
        *self = Self::from_bits_retain(self.bits()).union(other);
    }

    /// The intersection of a source flags value with the complement of a
    /// target flags value (`&!`). Unknown bits in `other` are not retained.
    fn remove(&mut self, other: Self)
    where
        Self: Sized,
    {
        *self = Self::from_bits_retain(self.bits()).difference(other);
    }

    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
    fn toggle(&mut self, other: Self)
    where
        Self: Sized,
    {
        *self = Self::from_bits_retain(self.bits()).symmetric_difference(other);
    }

    /// Call [`Flags::insert`] when `value` is `true` or [`Flags::remove`] when
    /// `value` is `false`.
    fn set(&mut self, other: Self, value: bool)
    where
        Self: Sized,
    {
        if value {
            self.insert(other);
        } else {
            self.remove(other);
        }
    }

    /// Unset all bits.
    fn clear(&mut self)
    where
        Self: Sized,
    {
        *self = Self::empty();
    }

    /// The bitwise and (`&`) of the bits in two flags values.
    #[must_use]
    fn intersection(self, other: Self) -> Self {
        Self::from_bits_retain(self.bits() & other.bits())
    }

    /// The bitwise or (`|`) of the bits in two flags values.
    #[must_use]
    fn union(self, other: Self) -> Self {
        Self::from_bits_retain(self.bits() | other.bits())
    }

    /// The intersection of a source flags value with the complement of a
    /// target flags value (`&!`).
    #[must_use]
    fn difference(self, other: Self) -> Self {
        Self::from_bits_retain(self.bits() & !other.bits())
    }

    /// The bitwise exclusive-or (`^`) of the bits in two flags values.
    #[must_use]
    fn symmetric_difference(self, other: Self) -> Self {
        Self::from_bits_retain(self.bits() ^ other.bits())
    }

    /// The bitwise negation (`!`) of the bits in a flags value, truncating the
    /// result.
    #[must_use]
    fn complement(self) -> Self {
        Self::from_bits_truncate(!self.bits())
    }
}

/// A bits type that can be used as storage for a flags type.
pub trait Bits:
    Clone
    + Copy
    + PartialEq
    + BitAnd<Output = Self>
    + BitOr<Output = Self>
    + BitXor<Output = Self>
    + Not<Output = Self>
    + Sized
    + 'static
{
    /// A value with all bits unset.
    const EMPTY: Self;

    /// A value with all bits set.
    const ALL: Self;
}

/// Marks the primitive integer types the macro accepts as storage. Not
/// implementable outside this module, so a custom `Bits` type can't be fed to
/// the macro.
pub trait Primitive {}

macro_rules! impl_bits {
    ($($u:ty, $i:ty,)*) => {
        $(
            impl Bits for $u {
                const EMPTY: $u = 0;
                const ALL: $u = <$u>::MAX;
            }

            impl Bits for $i {
                const EMPTY: $i = 0;
                const ALL: $i = <$u>::MAX as $i;
            }

            impl ParseHex for $u {
                fn parse_hex(input: &str) -> Result<Self, ParseError> {
                    <$u>::from_str_radix(input, 16).map_err(|_| ParseError::invalid_hex_flag(input))
                }
            }

            impl ParseHex for $i {
                fn parse_hex(input: &str) -> Result<Self, ParseError> {
                    <$i>::from_str_radix(input, 16).map_err(|_| ParseError::invalid_hex_flag(input))
                }
            }

            impl WriteHex for $u {
                fn write_hex<W: fmt::Write>(&self, mut writer: W) -> fmt::Result {
                    write!(writer, "{:x}", self)
                }
            }

            impl WriteHex for $i {
                fn write_hex<W: fmt::Write>(&self, mut writer: W) -> fmt::Result {
                    write!(writer, "{:x}", self)
                }
            }

            impl Primitive for $i {}
            impl Primitive for $u {}
        )*
    }
}

impl_bits! {
    u8, i8,
    u16, i16,
    u32, i32,
    u64, i64,
    u128, i128,
    usize, isize,
}

/// A trait for referencing the `bitflags`-owned internal type without
/// exposing it publicly.
pub trait PublicFlags {
    /// The type of the underlying storage.
    type Primitive: Primitive;

    /// The type of the internal field on the generated flags type.
    type Internal;
}
