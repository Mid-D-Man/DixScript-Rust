// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Base64/base64_codec.rs"
// ============================================================================
//! Hand-rolled replacement for the `base64` crate (locked at 0.21.7).
//!
//! ## What is replaced -- exactly one configuration
//! The crate is used in 11 files (35 calls), and every call is
//! `general_purpose::STANDARD.encode(..)` or `.decode(..)`: standard
//! alphabet (`A-Za-z0-9+/`), padded. No URL-safe alphabet, no no-pad mode,
//! no streaming, no `encode_slice`. That single configuration is all this
//! file implements.
//!
//! ## Why the decoder is a careful port, not a sketch
//! Encode is trivial. Decode is not, because most call sites use it as a
//! **validity check** on user-supplied text, not just as a converter:
//! `data_section_analyzer.rs` rejects a `.mdix` blob whose payload fails
//! `decode(..).is_err()`, `binary_format.rs` uses `.is_ok()`, and the DLM
//! encryptors fall back to a generated key when a key string fails to
//! decode. A decoder even slightly looser or stricter than the real one
//! silently changes which files are accepted. So the accept/reject
//! behavior -- and the error variant and offset, since `dix_value.rs`
//! formats the error into a user-visible message -- follows the real
//! 0.21.7 decoder's own rules:
//!
//! 1. A length remainder of 1 (mod 4) is `InvalidLength`, checked FIRST --
//!    unless the last byte is itself an invalid non-padding symbol, in
//!    which case that is reported as `InvalidByte` (the real crate does
//!    this so stray trailing whitespace gets a better message).
//! 2. Everything before the final (possibly partial) 8-byte group must be
//!    plain alphabet symbols; a `=` there is `InvalidByte`.
//! 3. In the final group, padding is only accepted at positions 2 or 3 of a
//!    4-symbol block, must run to the end, and anything after it is
//!    `InvalidByte` pointing at the FIRST padding byte.
//! 4. Padding must be canonical (symbols + padding a multiple of 4),
//!    otherwise `InvalidPadding`.
//! 5. Non-zero bits left over in the last symbol (non-canonical encoding,
//!    e.g. `"QR=="`) are `InvalidLastSymbol`.
//!
//! Precedence between those is the order above, and it is tested
//! differentially against the real crate -- see the tests and
//! docs/dixscript/utilities.md.
//!
//! ## Drop-in surface
//! `general_purpose::STANDARD`, an `Engine` trait, and `DecodeError` are
//! reproduced under the same names, so a call site's change is just its
//! `use` line. `DecodeError`'s `Display` text matches the real crate's
//! word for word, because that text reaches users.

use std::fmt;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const PAD: u8 = b'=';
const INVALID: u8 = 0xff;

const DECODE_TABLE: [u8; 256] = {
    let mut table = [INVALID; 256];
    let mut i = 0;
    while i < 64 {
        table[ALPHABET[i] as usize] = i as u8;
        i += 1;
    }
    table
};

/// Why a decode failed. Variants, payloads and `Display` text match the real
/// crate's `DecodeError`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DecodeError {
    /// A byte outside the alphabet at this offset. A misplaced `=` counts.
    InvalidByte(usize, u8),
    /// The length can't be a valid encoding (a lone trailing symbol).
    InvalidLength,
    /// The last symbol has non-zero bits that decoding would discard.
    InvalidLastSymbol(usize, u8),
    /// Padding is absent or wrong where it must be canonical.
    InvalidPadding,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::InvalidByte(index, byte) => write!(f, "Invalid byte {}, offset {}.", byte, index),
            Self::InvalidLength => write!(f, "Encoded text cannot have a 6-bit remainder."),
            Self::InvalidLastSymbol(index, byte) => {
                write!(f, "Invalid last symbol {}, offset {}.", byte, index)
            }
            Self::InvalidPadding => write!(f, "Invalid padding"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Standard-alphabet, padded base64.
pub(crate) fn encode(input: impl AsRef<[u8]>) -> String {
    let input = input.as_ref();
    let mut out = String::with_capacity(((input.len() + 2) / 3) * 4);

    let mut chunks = input.chunks_exact(3);
    for c in &mut chunks {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(ALPHABET[(n >> 6) as usize & 63] as char);
        out.push(ALPHABET[n as usize & 63] as char);
    }

    match *chunks.remainder() {
        [] => {}
        [a] => {
            let n = (a as u32) << 16;
            out.push(ALPHABET[(n >> 18) as usize & 63] as char);
            out.push(ALPHABET[(n >> 12) as usize & 63] as char);
            out.push(PAD as char);
            out.push(PAD as char);
        }
        [a, b] => {
            let n = ((a as u32) << 16) | ((b as u32) << 8);
            out.push(ALPHABET[(n >> 18) as usize & 63] as char);
            out.push(ALPHABET[(n >> 12) as usize & 63] as char);
            out.push(ALPHABET[(n >> 6) as usize & 63] as char);
            out.push(PAD as char);
        }
        _ => unreachable!("chunks_exact(3) leaves a remainder of 0, 1 or 2"),
    }

    out
}

/// Standard-alphabet, canonically padded base64 decode. See the module doc
/// for the exact accept/reject rules and their precedence.
pub(crate) fn decode(input: impl AsRef<[u8]>) -> Result<Vec<u8>, DecodeError> {
    let input = input.as_ref();
    let len = input.len();

    // Rule 1: checked before looking at any other byte.
    if len % 4 == 1 {
        if let Some(&b) = input.last() {
            if b != PAD && DECODE_TABLE[b as usize] == INVALID {
                return Err(DecodeError::InvalidByte(len - 1, b));
            }
        }
        return Err(DecodeError::InvalidLength);
    }

    // The final (possibly partial) group of up to 8 bytes is where padding
    // may legally appear; everything before it may not contain a '='.
    let suffix_start = if len == 0 { 0 } else { ((len - 1) / 8) * 8 };

    let mut out = Vec::with_capacity(len / 4 * 3 + 3);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;

    // Feeds one 6-bit value in; emits a byte whenever 8 are available.
    // Whatever is left in `acc` afterwards is the discarded trailing bits.
    macro_rules! push_morsel {
        ($m:expr) => {{
            acc = (acc << 6) | ($m as u32);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
                acc &= (1u32 << bits) - 1;
            }
        }};
    }

    // Rule 2.
    for (i, &b) in input[..suffix_start].iter().enumerate() {
        let m = DECODE_TABLE[b as usize];
        if m == INVALID {
            return Err(DecodeError::InvalidByte(i, b));
        }
        push_morsel!(m);
    }

    // Rule 3.
    let mut padding_bytes = 0usize;
    let mut first_padding_index = 0usize;
    let mut morsels = 0usize;
    let mut last_symbol = 0u8;

    for (i, &b) in input[suffix_start..].iter().enumerate() {
        if b == PAD {
            // Padding can only sit at position 2 or 3 of a 4-symbol block.
            if i % 4 < 2 {
                let bad = suffix_start + if padding_bytes > 0 { first_padding_index } else { i };
                return Err(DecodeError::InvalidByte(bad, b));
            }
            if padding_bytes == 0 {
                first_padding_index = i;
            }
            padding_bytes += 1;
            continue;
        }

        // A non-padding byte after padding has started. Reported as the
        // padding's position, and checked before whether `b` itself is valid.
        if padding_bytes > 0 {
            return Err(DecodeError::InvalidByte(suffix_start + first_padding_index, PAD));
        }

        last_symbol = b;
        let m = DECODE_TABLE[b as usize];
        if m == INVALID {
            return Err(DecodeError::InvalidByte(suffix_start + i, b));
        }
        morsels += 1;
        push_morsel!(m);
    }

    // Rule 4.
    if (padding_bytes + morsels) % 4 != 0 {
        return Err(DecodeError::InvalidPadding);
    }

    // Rule 5.
    if acc != 0 {
        return Err(DecodeError::InvalidLastSymbol(suffix_start + morsels - 1, last_symbol));
    }

    Ok(out)
}

/// Mirrors `base64::Engine` for the two methods the crate calls, so a call
/// site can keep writing `general_purpose::STANDARD.encode(..)` unchanged.
pub(crate) trait Engine {
    fn encode<T: AsRef<[u8]>>(&self, input: T) -> String;
    fn decode<T: AsRef<[u8]>>(&self, input: T) -> Result<Vec<u8>, DecodeError>;
}

/// The standard alphabet, padded -- the only engine configuration in use.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StandardEngine;

impl Engine for StandardEngine {
    #[inline]
    fn encode<T: AsRef<[u8]>>(&self, input: T) -> String {
        encode(input)
    }

    #[inline]
    fn decode<T: AsRef<[u8]>>(&self, input: T) -> Result<Vec<u8>, DecodeError> {
        decode(input)
    }
}

/// Mirrors `base64::engine::general_purpose`.
pub(crate) mod general_purpose {
    pub(crate) const STANDARD: super::StandardEngine = super::StandardEngine;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_test_vectors_encode() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"foo"), "Zm9v");
        assert_eq!(encode(b"foob"), "Zm9vYg==");
        assert_eq!(encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn rfc4648_test_vectors_decode() {
        assert_eq!(decode("").unwrap(), b"");
        assert_eq!(decode("Zg==").unwrap(), b"f");
        assert_eq!(decode("Zm8=").unwrap(), b"fo");
        assert_eq!(decode("Zm9v").unwrap(), b"foo");
        assert_eq!(decode("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(decode("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn round_trips_every_length_and_byte_value() {
        let all: Vec<u8> = (0..=255u8).collect();
        for n in 0..=all.len() {
            let enc = encode(&all[..n]);
            assert_eq!(decode(&enc).unwrap(), &all[..n], "length {}", n);
        }
    }

    #[test]
    fn engine_trait_matches_free_functions() {
        use general_purpose::STANDARD;
        assert_eq!(STANDARD.encode(b"hello"), encode(b"hello"));
        assert_eq!(STANDARD.decode("aGVsbG8=").unwrap(), b"hello");
    }

    #[test]
    fn rejects_the_shapes_the_call_sites_rely_on_rejecting() {
        // missing padding
        assert_eq!(decode("Zg"), Err(DecodeError::InvalidPadding));
        // stray whitespace, including a trailing newline
        assert_eq!(decode("Zm9v\n"), Err(DecodeError::InvalidByte(4, b'\n')));
        assert_eq!(decode("Zm 9"), Err(DecodeError::InvalidByte(2, b' ')));
        // ...but a length remainder of 1 wins over an earlier bad byte, because
        // the real decoder checks length first: "Zm 9v" is 5 bytes.
        assert_eq!(decode("Zm 9v"), Err(DecodeError::InvalidLength));
        // lone trailing symbol
        assert_eq!(decode("Zm9vY"), Err(DecodeError::InvalidLength));
        // non-canonical trailing bits
        assert_eq!(decode("Zh=="), Err(DecodeError::InvalidLastSymbol(1, b'h')));
        // padding in the middle
        assert_eq!(decode("Zg==Zg=="), Err(DecodeError::InvalidByte(2, b'=')));
        // URL-safe alphabet is not the standard alphabet
        assert_eq!(decode("-_-_"), Err(DecodeError::InvalidByte(0, b'-')));
    }

    #[test]
    fn display_text_matches_the_real_crate() {
        assert_eq!(DecodeError::InvalidByte(3, 33).to_string(), "Invalid byte 33, offset 3.");
        assert_eq!(DecodeError::InvalidLength.to_string(), "Encoded text cannot have a 6-bit remainder.");
        assert_eq!(DecodeError::InvalidLastSymbol(1, 104).to_string(), "Invalid last symbol 104, offset 1.");
        assert_eq!(DecodeError::InvalidPadding.to_string(), "Invalid padding");
    }
}


/// Differential tests: this decoder against the real `base64` 0.21.7 it
/// replaces, on generated input. These exist because the decode call sites
/// use the result as a validity check on user text, so "close" is not good
/// enough -- accept/reject, the error variant, its offset and its byte must
/// all agree.
///
/// They call the real crate as `::base64` (the leading `::` forces the
/// extern crate, since a same-named local module exists). When `base64` is
/// dropped from `[dependencies]` in the wiring pass, move it to
/// `[dev-dependencies]` rather than deleting it: these tests then keep
/// guarding this file forever.
#[cfg(test)]
mod differential_against_real_crate {
    use super::*;
    use crate::Utilities::test_rng::XorShift;
    use ::base64::engine::general_purpose::STANDARD as REAL;
    use ::base64::Engine as _;
    use std::collections::BTreeMap;

    /// Compares one input. Returns the real crate's outcome label so callers
    /// can count which branches were actually exercised.
    fn check(input: &[u8]) -> String {
        let mine = decode(input);
        let real = REAL.decode(input);
        match (&mine, &real) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a, b, "decoded bytes differ for input {:?}", String::from_utf8_lossy(input));
                "Ok".to_string()
            }
            (Err(a), Err(b)) => {
                assert_eq!(
                    format!("{:?}", a),
                    format!("{:?}", b),
                    "error differs for input {:?} ({:?})",
                    String::from_utf8_lossy(input),
                    input
                );
                assert_eq!(a.to_string(), b.to_string(), "Display differs for input {:?}", input);
                format!("{:?}", b).split('(').next().unwrap().to_string()
            }
            _ => panic!(
                "accept/reject disagreement for input {:?} ({:?}): mine={:?}, real={:?}",
                String::from_utf8_lossy(input),
                input,
                mine,
                real
            ),
        }
    }

    fn assert_every_outcome_seen(seen: &BTreeMap<String, usize>) {
        for label in ["Ok", "InvalidByte", "InvalidLength", "InvalidLastSymbol", "InvalidPadding"] {
            assert!(
                seen.get(label).copied().unwrap_or(0) > 0,
                "the test never produced a {:?} outcome, so it proves nothing about that branch: {:?}",
                label,
                seen
            );
        }
    }

    #[test]
    fn encode_matches_real_on_random_bytes() {
        let mut rng = XorShift::new(0xBA5E_64);
        for _ in 0..20_000 {
            let len = rng.below(100);
            let data = rng.bytes(len);
            assert_eq!(encode(&data), REAL.encode(&data), "input {:?}", data);
        }
    }

    #[test]
    fn decode_matches_real_on_valid_encodings_with_single_edits() {
        let mut rng = XorShift::new(0xDEC0_DE);
        let noise: Vec<u8> = ALPHABET
            .iter()
            .copied()
            .chain(*b"= \n\r\t-_!\0")
            .chain([0x80u8, 0xff])
            .collect();
        let mut seen = BTreeMap::new();

        for _ in 0..150_000 {
            let len = rng.below(60);
            let mut enc = REAL.encode(rng.bytes(len)).into_bytes();

            match rng.below(6) {
                0 => {} // untouched, must decode
                1 if !enc.is_empty() => {
                    let i = rng.below(enc.len());
                    enc[i] = *rng.pick(&noise);
                }
                2 if !enc.is_empty() => {
                    let i = rng.below(enc.len());
                    enc.remove(i);
                }
                3 => {
                    let i = rng.below(enc.len() + 1);
                    enc.insert(i, *rng.pick(&noise));
                }
                4 if !enc.is_empty() => {
                    let keep = rng.below(enc.len());
                    enc.truncate(keep);
                }
                _ => enc.push(*rng.pick(&noise)),
            }

            *seen.entry(check(&enc)).or_insert(0usize) += 1;
        }

        assert_every_outcome_seen(&seen);
    }

    #[test]
    fn decode_matches_real_on_random_symbol_soup() {
        let mut rng = XorShift::new(0x50_0F);
        // Weighted toward valid symbols so long inputs still reach the
        // padding and trailing-bit rules instead of dying on the first byte.
        let mut pool: Vec<u8> = Vec::new();
        for _ in 0..6 { pool.extend_from_slice(b"ABQgz09+/"); }
        pool.extend_from_slice(b"====  \n-_!\xc3\xa9");
        let mut seen = BTreeMap::new();

        for _ in 0..300_000 {
            let len = rng.below(41);
            let input: Vec<u8> = (0..len).map(|_| *rng.pick(&pool)).collect();
            *seen.entry(check(&input)).or_insert(0usize) += 1;
        }

        assert_every_outcome_seen(&seen);
    }

    /// Every string up to `max_len` over `alphabet`. Small alphabets, so this
    /// is exhaustive rather than sampled: every padding position and every
    /// length around the 8-byte group boundary where the real decoder
    /// switches from its fast path to its suffix handling.
    fn exhaustive(alphabet: &[u8], max_len: usize, seen: &mut BTreeMap<String, usize>) {
        let mut buf: Vec<u8> = Vec::new();
        fn rec(alphabet: &[u8], max_len: usize, buf: &mut Vec<u8>, seen: &mut BTreeMap<String, usize>) {
            *seen.entry(check(buf)).or_insert(0usize) += 1;
            if buf.len() == max_len {
                return;
            }
            for &c in alphabet {
                buf.push(c);
                rec(alphabet, max_len, buf, seen);
                buf.pop();
            }
        }
        rec(alphabet, max_len, &mut buf, seen);
    }

    #[test]
    fn decode_matches_real_exhaustively_over_small_alphabets() {
        let mut seen = BTreeMap::new();
        // 'B' has non-zero low bits, so it is what triggers InvalidLastSymbol.
        exhaustive(b"AB=!", 8, &mut seen);
        // Longer, over fewer symbols: reaches lengths 9..=12 where the
        // suffix group starts after a full first group.
        exhaustive(b"A=!", 12, &mut seen);
        assert_every_outcome_seen(&seen);
    }
}
