// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags"
// ============================================================================
//! Formatting and parsing flags values as text.
//!
//! The grammar, as upstream documents it:
//! - _Flags:_ (_Whitespace_ _Flag_ _Whitespace_)`|`*
//! - _Flag:_ _Name_ | _Hex Number_
//! - _Name:_ the name of any defined flag (case-sensitive)
//! - _Hex Number:_ `0x` followed by hex digits
//!
//! So `Flags::A | Flags::B | 0x0c` round-trips as `A | B | 0x0c`. This is what
//! a derived `Debug` on a generated type prints.
//!
//! ## Attribution
//! Derived from the `bitflags` crate, v2.10.0
//! (<https://github.com/bitflags/bitflags>), licensed `MIT OR Apache-2.0`;
//! the algorithms follow upstream's `src/parser.rs`.
//!
//! ## One deliberate fixed choice: the `std`-less `ParseError`
//! Upstream's `ParseError` carries the offending text only when its optional
//! `std` feature is on. This crate depends on `bitflags` with default features
//! (no `std`), so the faithful behavior is the `std`-less one: the payload is
//! `()`, the `Display` text has no trailing `` `name` ``, and `ParseError` does
//! not implement `std::error::Error` (upstream only provides that with `std`).
//! That is what is ported.

use core::fmt::{self, Write};

use super::traits::{Bits, Flags};

/// Write a flags value as text. Any bits that aren't part of a contained flag
/// are written as a hex number, e.g. `A | B | 0xf6`.
pub fn to_writer<B: Flags>(flags: &B, mut writer: impl Write) -> Result<(), fmt::Error>
where
    B::Bits: WriteHex,
{
    // The names of set flags, bar-separated, then a hex number for any bits
    // that are set but don't correspond to a flag.
    let mut first = true;
    let mut iter = flags.iter_names();
    for (name, _) in &mut iter {
        if !first {
            writer.write_str(" | ")?;
        }

        first = false;
        writer.write_str(name)?;
    }

    let remaining = iter.remaining().bits();
    if remaining != B::Bits::EMPTY {
        if !first {
            writer.write_str(" | ")?;
        }

        writer.write_str("0x")?;
        remaining.write_hex(writer)?;
    }

    fmt::Result::Ok(())
}

/// Parse a flags value from text. Fails on any name that isn't a defined flag;
/// unknown bits (from hex) are retained.
pub fn from_str<B: Flags>(input: &str) -> Result<B, ParseError>
where
    B::Bits: ParseHex,
{
    let mut parsed_flags = B::empty();

    // Empty input is an empty set of flags.
    if input.trim().is_empty() {
        return Ok(parsed_flags);
    }

    for flag in input.split('|') {
        let flag = flag.trim();

        // An empty flag between bars is missing input.
        if flag.is_empty() {
            return Err(ParseError::empty_flag());
        }

        // `0x...` is a hex number, parsed straight to the bits type; anything
        // else is a name, and the generated type decides if it's valid.
        let parsed_flag = if let Some(flag) = flag.strip_prefix("0x") {
            let bits =
                <B::Bits>::parse_hex(flag).map_err(|_| ParseError::invalid_hex_flag(flag))?;

            B::from_bits_retain(bits)
        } else {
            B::from_name(flag).ok_or_else(|| ParseError::invalid_named_flag(flag))?
        };

        parsed_flags.insert(parsed_flag);
    }

    Ok(parsed_flags)
}

/// Write a flags value as text, ignoring any unknown bits.
pub fn to_writer_truncate<B: Flags>(flags: &B, writer: impl Write) -> Result<(), fmt::Error>
where
    B::Bits: WriteHex,
{
    to_writer(&B::from_bits_truncate(flags.bits()), writer)
}

/// Parse a flags value from text, ignoring any unknown bits.
pub fn from_str_truncate<B: Flags>(input: &str) -> Result<B, ParseError>
where
    B::Bits: ParseHex,
{
    Ok(B::from_bits_truncate(from_str::<B>(input)?.bits()))
}

/// Write only the contained, defined, named flags of a value as text.
pub fn to_writer_strict<B: Flags>(flags: &B, mut writer: impl Write) -> Result<(), fmt::Error> {
    let mut first = true;
    let mut iter = flags.iter_names();
    for (name, _) in &mut iter {
        if !first {
            writer.write_str(" | ")?;
        }

        first = false;
        writer.write_str(name)?;
    }

    fmt::Result::Ok(())
}

/// Parse a flags value from text. Fails on unknown names and on hex values.
pub fn from_str_strict<B: Flags>(input: &str) -> Result<B, ParseError> {
    let mut parsed_flags = B::empty();

    if input.trim().is_empty() {
        return Ok(parsed_flags);
    }

    for flag in input.split('|') {
        let flag = flag.trim();

        if flag.is_empty() {
            return Err(ParseError::empty_flag());
        }

        // Hex values aren't supported by the strict parser.
        if flag.starts_with("0x") {
            return Err(ParseError::invalid_hex_flag("unsupported hex flag value"));
        }

        let parsed_flag = B::from_name(flag).ok_or_else(|| ParseError::invalid_named_flag(flag))?;

        parsed_flags.insert(parsed_flag);
    }

    Ok(parsed_flags)
}

/// Encode a value as a hex string, without the `0x` prefix.
pub trait WriteHex {
    /// Write the value as hex.
    fn write_hex<W: fmt::Write>(&self, writer: W) -> fmt::Result;
}

/// Parse a value from a hex string, without the `0x` prefix.
pub trait ParseHex {
    /// Parse the value from hex.
    fn parse_hex(input: &str) -> Result<Self, ParseError>
    where
        Self: Sized;
}

/// An error encountered while parsing flags from text.
#[derive(Debug)]
pub struct ParseError(ParseErrorKind);

// The `got: ()` payloads are never read: they exist so the variant shapes match upstream's
// `std`-less build (see the module doc). `Debug` shows them, but rustc does not count that.
#[derive(Debug)]
#[allow(clippy::enum_variant_names, dead_code)]
enum ParseErrorKind {
    EmptyFlag,
    InvalidNamedFlag { got: () },
    InvalidHexFlag { got: () },
}

impl ParseError {
    /// An invalid hex flag was encountered.
    pub fn invalid_hex_flag(flag: impl fmt::Display) -> Self {
        let _flag = flag;
        ParseError(ParseErrorKind::InvalidHexFlag { got: () })
    }

    /// A named flag that doesn't correspond to any on the flags type was
    /// encountered.
    pub fn invalid_named_flag(flag: impl fmt::Display) -> Self {
        let _flag = flag;
        ParseError(ParseErrorKind::InvalidNamedFlag { got: () })
    }

    /// A hex or named flag wasn't found between separators.
    pub const fn empty_flag() -> Self {
        ParseError(ParseErrorKind::EmptyFlag)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ParseErrorKind::InvalidNamedFlag { .. } => {
                write!(f, "unrecognized named flag")?;
            }
            ParseErrorKind::InvalidHexFlag { .. } => {
                write!(f, "invalid hex flag")?;
            }
            ParseErrorKind::EmptyFlag => {
                write!(f, "encountered empty flag")?;
            }
        }

        Ok(())
    }
}
