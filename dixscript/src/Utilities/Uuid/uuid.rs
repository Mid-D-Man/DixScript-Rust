// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Uuid/uuid.rs"
// ============================================================================
//! Hand-rolled replacement for the `uuid` crate (locked at 1.19.0), covering
//! exactly the API this crate calls.
//!
//! ## What is used
//! - `Uuid::new_v4()` -- compilation ids, temp-file names (`loader.rs`,
//!   `auditor_trait.rs`) and the user-facing `Guid.new()`
//! - `Uuid::parse_str()` -- `Guid.parse` / `tryParse` / `validate` / `format`
//!   / `toBytes`, all user-facing
//! - `Uuid::nil()`, `Uuid::from_bytes()`, `.as_bytes()`, `.as_fields()`
//! - `.to_string()` (Display), `.simple()`, `.hyphenated()`
//!
//! ## Why `parse_str` is a careful port
//! `Guid.validate` and `Guid.tryParse` expose the accept/reject set directly
//! to `.mdix` authors, so it has to match the real crate. The real parser
//! dispatches on byte length:
//! - 32 -> 32 hex digits, no hyphens ("simple")
//! - 36 -> hyphenated, hyphens exactly at offsets 8, 13, 18, 23
//! - 38 -> hyphenated wrapped in `{` `}`
//! - 45 -> hyphenated preceded by the literal `urn:uuid:`
//! Hex digits are case-insensitive. Anything else -- wrong length, a hyphen
//! in the wrong place, a non-hex byte, a multi-byte UTF-8 character -- is
//! rejected. Differentially tested against the real crate; see the tests
//! and docs/dixscript/utilities.md.
//!
//! ## Randomness
//! `new_v4` draws 16 bytes from `rand::rngs::OsRng` (the OS entropy source,
//! the same class of source the real crate reaches through `getrandom`) and
//! sets the version (4) and RFC 4122 variant bits. `rand` is not a new
//! dependency -- the DLM key/salt/nonce generation already requires it.
//!
//! ## wasm32
//! `OsRng` reaches the browser's entropy through `getrandom` 0.2, which this
//! crate already enables with its `js` feature for `wasm32` (Cargo.toml,
//! `[target.'cfg(target_arch = "wasm32")'.dependencies]`) -- the same path the
//! DLM encryptors' key/IV generation already takes unconditionally. The real
//! `uuid` dependency's own `js` and `serde` features go away with the crate.
//! Not exercised on a wasm target here.
//!
//! ## Not covered
//! Other UUID versions, `Uuid::parse_str` error detail (every caller
//! discards it), serde, `Uuid::from_u128`/`as_u128`, `FromStr`, ordering
//! semantics beyond the derived byte-wise ones. Nothing here uses them.

use std::fmt;

/// A 128-bit UUID. Big-endian byte layout, as in RFC 4122.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Uuid([u8; 16]);

/// Returned by [`Uuid::parse_str`]. Carries no detail: every caller in the
/// crate discards it or reports its own message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UuidParseError;

impl fmt::Display for UuidParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid UUID")
    }
}

impl std::error::Error for UuidParseError {}

const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";

/// 0..=15 for a hex digit of either case, `0xff` for anything else.
const HEX_TABLE: [u8; 256] = {
    let mut table = [0xffu8; 256];
    let mut i = 0usize;
    while i < 256 {
        let b = i as u8;
        table[i] = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => 0xff,
        };
        i += 1;
    }
    table
};

impl Uuid {
    /// The all-zero UUID.
    pub(crate) const fn nil() -> Self {
        Uuid([0u8; 16])
    }

    pub(crate) const fn from_bytes(bytes: [u8; 16]) -> Self {
        Uuid(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// A random (version 4) UUID.
    pub(crate) fn new_v4() -> Self {
        use rand::RngCore;

        let mut bytes = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        // Version 4 in the high nibble of byte 6; RFC 4122 variant (10xx)
        // in the top two bits of byte 8.
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid(bytes)
    }

    /// `(time_low, time_mid, time_hi_and_version, clock_seq_and_node)`.
    pub(crate) fn as_fields(&self) -> (u32, u16, u16, &[u8; 8]) {
        let b = &self.0;
        let d1 = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        let d2 = u16::from_be_bytes([b[4], b[5]]);
        let d3 = u16::from_be_bytes([b[6], b[7]]);
        let d4: &[u8; 8] = b[8..16].try_into().expect("slice is exactly 8 bytes");
        (d1, d2, d3, d4)
    }

    /// 32 lowercase hex digits, no hyphens.
    pub(crate) fn simple(&self) -> Simple {
        Simple(*self)
    }

    /// Lowercase, `8-4-4-4-12`.
    pub(crate) fn hyphenated(&self) -> Hyphenated {
        Hyphenated(*self)
    }

    /// Parses simple, hyphenated, braced or URN form. See the module doc.
    pub(crate) fn parse_str(input: &str) -> Result<Uuid, UuidParseError> {
        let s = input.as_bytes();
        match (s.len(), s) {
            (32, s) => parse_simple(s),
            (36, s)
            | (38, [b'{', s @ .., b'}'])
            | (45, [b'u', b'r', b'n', b':', b'u', b'u', b'i', b'd', b':', s @ ..]) => {
                parse_hyphenated(s)
            }
            _ => Err(UuidParseError),
        }
    }
}

fn parse_simple(s: &[u8]) -> Result<Uuid, UuidParseError> {
    if s.len() != 32 {
        return Err(UuidParseError);
    }
    let mut buf = [0u8; 16];
    for i in 0..16 {
        let h1 = HEX_TABLE[s[i * 2] as usize];
        let h2 = HEX_TABLE[s[i * 2 + 1] as usize];
        if h1 == 0xff || h2 == 0xff {
            return Err(UuidParseError);
        }
        buf[i] = (h1 << 4) | h2;
    }
    Ok(Uuid(buf))
}

fn parse_hyphenated(s: &[u8]) -> Result<Uuid, UuidParseError> {
    if s.len() != 36 {
        return Err(UuidParseError);
    }
    // uuid : 936da01f-9abd-4d9d-80c7-02af85c822a8
    // hyphen offsets:     8    13   18   23
    if s[8] != b'-' || s[13] != b'-' || s[18] != b'-' || s[23] != b'-' {
        return Err(UuidParseError);
    }
    // Start offset of each 4-hex-digit group (each group is two bytes).
    const GROUP_STARTS: [usize; 8] = [0, 4, 9, 14, 19, 24, 28, 32];
    let mut buf = [0u8; 16];
    for (j, &i) in GROUP_STARTS.iter().enumerate() {
        let h1 = HEX_TABLE[s[i] as usize];
        let h2 = HEX_TABLE[s[i + 1] as usize];
        let h3 = HEX_TABLE[s[i + 2] as usize];
        let h4 = HEX_TABLE[s[i + 3] as usize];
        if h1 == 0xff || h2 == 0xff || h3 == 0xff || h4 == 0xff {
            return Err(UuidParseError);
        }
        buf[j * 2] = (h1 << 4) | h2;
        buf[j * 2 + 1] = (h3 << 4) | h4;
    }
    Ok(Uuid(buf))
}

fn write_hex_bytes(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    let mut buf = [0u8; 2];
    for &b in bytes {
        buf[0] = HEX_LOWER[(b >> 4) as usize];
        buf[1] = HEX_LOWER[(b & 0x0f) as usize];
        // Only ASCII hex digits were written, so this is always valid UTF-8.
        f.write_str(std::str::from_utf8(&buf).expect("hex digits are ASCII"))?;
    }
    Ok(())
}

/// `Display` adapter: 32 lowercase hex digits. Returned by [`Uuid::simple`].
#[derive(Clone, Copy)]
pub(crate) struct Simple(Uuid);

impl fmt::Display for Simple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex_bytes(f, &(self.0).0)
    }
}

/// `Display` adapter: lowercase `8-4-4-4-12`. Returned by [`Uuid::hyphenated`].
#[derive(Clone, Copy)]
pub(crate) struct Hyphenated(Uuid);

impl fmt::Display for Hyphenated {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &(self.0).0;
        write_hex_bytes(f, &b[0..4])?;
        f.write_str("-")?;
        write_hex_bytes(f, &b[4..6])?;
        f.write_str("-")?;
        write_hex_bytes(f, &b[6..8])?;
        f.write_str("-")?;
        write_hex_bytes(f, &b[8..10])?;
        f.write_str("-")?;
        write_hex_bytes(f, &b[10..16])
    }
}

/// Like the real crate, `Uuid`'s own `Display` is the hyphenated form.
impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.hyphenated(), f)
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "936da01f-9abd-4d9d-80c7-02af85c822a8";

    #[test]
    fn nil_is_all_zero_and_displays_as_such() {
        assert_eq!(Uuid::nil().to_string(), "00000000-0000-0000-0000-000000000000");
        assert_eq!(Uuid::nil().as_bytes(), &[0u8; 16]);
    }

    #[test]
    fn parse_and_display_round_trip_the_hyphenated_form() {
        let u = Uuid::parse_str(SAMPLE).unwrap();
        assert_eq!(u.to_string(), SAMPLE);
        assert_eq!(u.hyphenated().to_string(), SAMPLE);
        assert_eq!(u.simple().to_string(), "936da01f9abd4d9d80c702af85c822a8");
    }

    #[test]
    fn parse_accepts_all_four_shapes_and_either_case() {
        let expected = Uuid::parse_str(SAMPLE).unwrap();
        assert_eq!(Uuid::parse_str("936da01f9abd4d9d80c702af85c822a8").unwrap(), expected);
        assert_eq!(Uuid::parse_str("{936da01f-9abd-4d9d-80c7-02af85c822a8}").unwrap(), expected);
        assert_eq!(Uuid::parse_str("urn:uuid:936da01f-9abd-4d9d-80c7-02af85c822a8").unwrap(), expected);
        assert_eq!(Uuid::parse_str(&SAMPLE.to_uppercase()).unwrap(), expected);
    }

    #[test]
    fn parse_rejects_bad_shapes() {
        for bad in [
            "",
            "not a uuid",
            "936da01f-9abd-4d9d-80c7-02af85c822a",   // one short
            "936da01f-9abd-4d9d-80c7-02af85c822a88", // one long
            "936da01f9abd-4d9d-80c7-02af85c822a8",   // hyphens missing
            "936da01f-9abd-4d9d-80c702af85c822a8-",  // hyphen in the wrong place
            "936da01f-9abd-4d9d-80c7-02af85c822ag",  // non-hex digit
            "{936da01f-9abd-4d9d-80c7-02af85c822a8",  // unbalanced brace
            "(936da01f-9abd-4d9d-80c7-02af85c822a8)", // parentheses are not accepted
            "urn:uuid:936da01f9abd4d9d80c702af85c822a8", // urn needs the hyphenated form
        ] {
            assert!(Uuid::parse_str(bad).is_err(), "should reject {:?}", bad);
        }
    }

    #[test]
    fn as_fields_splits_big_endian() {
        let u = Uuid::parse_str(SAMPLE).unwrap();
        let (d1, d2, d3, d4) = u.as_fields();
        assert_eq!(d1, 0x936da01f);
        assert_eq!(d2, 0x9abd);
        assert_eq!(d3, 0x4d9d);
        assert_eq!(d4, &[0x80, 0xc7, 0x02, 0xaf, 0x85, 0xc8, 0x22, 0xa8]);
    }

    #[test]
    fn from_bytes_round_trips_as_bytes() {
        let bytes: [u8; 16] = std::array::from_fn(|i| i as u8);
        assert_eq!(Uuid::from_bytes(bytes).as_bytes(), &bytes);
    }

    #[test]
    fn new_v4_sets_version_and_variant_and_is_not_constant() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        assert_ne!(a, b, "two fresh v4 UUIDs colliding means the RNG is not being used");
        for u in [a, b] {
            assert_eq!(u.as_bytes()[6] >> 4, 0x4, "version nibble");
            assert_eq!(u.as_bytes()[8] >> 6, 0b10, "RFC 4122 variant bits");
            // and what it prints must parse back to itself
            assert_eq!(Uuid::parse_str(&u.to_string()).unwrap(), u);
        }
    }
}


/// Differential tests against the real `uuid` 1.19.0. `Guid.parse` /
/// `validate` / `tryParse` expose the accepted spellings directly to `.mdix`
/// authors, so accept/reject and the parsed bytes must agree exactly, and the
/// text forms must match character for character.
///
/// The real crate is called as `::uuid` (leading `::` = the extern crate; a
/// same-named local module exists). When `uuid` is dropped from
/// `[dependencies]` in the wiring pass, move it to `[dev-dependencies]` so
/// these keep guarding this file.
#[cfg(test)]
mod differential_against_real_crate {
    use super::*;
    use crate::Utilities::test_rng::XorShift;
    use ::uuid::Uuid as Real;

    #[test]
    fn text_forms_and_fields_match_real_on_random_bytes() {
        let mut rng = XorShift::new(0x0011_D);
        for _ in 0..20_000 {
            let bytes: [u8; 16] = rng.bytes(16).try_into().unwrap();
            let (mine, real) = (Uuid::from_bytes(bytes), Real::from_bytes(bytes));

            assert_eq!(mine.to_string(), real.to_string(), "{:?}", bytes);
            assert_eq!(mine.hyphenated().to_string(), real.hyphenated().to_string());
            assert_eq!(mine.simple().to_string(), real.simple().to_string());
            assert_eq!(mine.as_bytes(), real.as_bytes());

            let (a1, a2, a3, a4) = mine.as_fields();
            let (b1, b2, b3, b4) = real.as_fields();
            assert_eq!((a1, a2, a3, a4), (b1, b2, b3, b4), "{:?}", bytes);
        }
        assert_eq!(Uuid::nil().to_string(), Real::nil().to_string());
    }

    /// Builds a valid UUID string in one of the accepted spellings, then
    /// maybe damages it, so the corpus is dense around the accept/reject edge.
    fn candidate(rng: &mut XorShift) -> String {
        let bytes: [u8; 16] = rng.bytes(16).try_into().unwrap();
        let real = Real::from_bytes(bytes);
        let mut s = match rng.below(7) {
            0 => real.simple().to_string(),
            1 => format!("{{{}}}", real.hyphenated()),
            2 => format!("urn:uuid:{}", real.hyphenated()),
            3 => real.hyphenated().to_string().to_uppercase(),
            4 => format!("({})", real.hyphenated()),
            5 => format!("{}-", real.simple()),
            _ => real.hyphenated().to_string(),
        };

        // 0..=2 single-character edits, drawn from characters that are
        // near-misses: hex, hyphen, braces, space, a non-hex letter, and a
        // multi-byte character (which changes the byte length).
        const EDITS: &[char] = &['0', '9', 'a', 'F', '-', '{', '}', ' ', 'g', 'x', ':', '\u{e9}'];
        for _ in 0..rng.below(3) {
            let mut chars: Vec<char> = s.chars().collect();
            if chars.is_empty() {
                break;
            }
            let i = rng.below(chars.len());
            match rng.below(3) {
                0 => chars[i] = *rng.pick(EDITS),
                1 => {
                    chars.remove(i);
                }
                _ => chars.insert(i, *rng.pick(EDITS)),
            }
            s = chars.into_iter().collect();
        }
        s
    }

    #[test]
    fn parse_str_matches_real_on_valid_and_damaged_inputs() {
        let mut rng = XorShift::new(0xFACE);
        let (mut accepted, mut rejected) = (0usize, 0usize);

        for _ in 0..200_000 {
            let s = candidate(&mut rng);
            let mine = Uuid::parse_str(&s).map(|u| *u.as_bytes());
            let real = Real::parse_str(&s).map(|u| *u.as_bytes());
            assert_eq!(mine.is_ok(), real.is_ok(), "accept/reject disagreement for {:?}", s);
            if let (Ok(a), Ok(b)) = (mine, real) {
                assert_eq!(a, b, "parsed bytes differ for {:?}", s);
                accepted += 1;
            } else {
                rejected += 1;
            }
        }

        assert!(accepted > 10_000, "corpus should hit many accepted inputs, got {}", accepted);
        assert!(rejected > 10_000, "corpus should hit many rejected inputs, got {}", rejected);
    }

    #[test]
    fn parse_str_matches_real_on_arbitrary_short_strings() {
        let mut rng = XorShift::new(0xB0B);
        const POOL: &[char] = &['0', '1', 'a', 'f', 'A', 'F', '-', '{', '}', 'g', ' ', '\u{e9}'];
        for _ in 0..100_000 {
            let len = rng.below(48);
            let s: String = (0..len).map(|_| *rng.pick(POOL)).collect();
            assert_eq!(
                Uuid::parse_str(&s).is_ok(),
                Real::parse_str(&s).is_ok(),
                "accept/reject disagreement for {:?}",
                s
            );
        }
    }
}
