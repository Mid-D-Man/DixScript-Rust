// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Hex/hex.rs"
// ============================================================================
//! Hand-rolled replacement for the `hex` crate.
//!
//! ## Why
//! Grepping every real call site in the crate found exactly two, both
//! `hex::encode(sha256_digest)` (`Compiler/DLM/Auditor/enhanced_auditor.rs`
//! and `diy_auditor.rs`) -- turning a SHA-256 digest into a lowercase hex
//! string for a checksum. That's the entire surface actually used. `decode`
//! is provided for symmetry and because a checksum module is the obvious
//! next place something would want to parse a hex string back into bytes,
//! but nothing calls it yet -- don't assume it's exercised until something
//! does.
//!
//! ## Scope
//! Lowercase output, no uppercase/mixed-case output option, no `0x` prefix
//! handling. That's all `hex::encode`'s call sites here ever needed.
//! `decode` accepts either case on input (a hex string someone hand-types
//! or pastes is as likely to be `DEADBEEF` as `deadbeef`), matching the
//! real crate's own default behavior.

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Lowercase hex string for `bytes`. Output is exactly `bytes.len() * 2`
/// ASCII characters.
pub(crate) fn encode(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX_DIGITS[(b >> 4) as usize] as char);
        out.push(HEX_DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

/// Parses a hex string (either case) back into bytes.
///
/// Errors on an odd-length input or any non-hex-digit character. Never
/// panics -- every branch returns `Result` rather than indexing past a
/// checked length or unwrapping a parse.
pub(crate) fn decode(s: impl AsRef<str>) -> Result<Vec<u8>, String> {
    let s = s.as_ref();
    let bytes = s.as_bytes();

    if bytes.len() % 2 != 0 {
        return Err(format!(
            "hex string must have an even number of characters, got {}",
            bytes.len()
        ));
    }

    fn nibble(c: u8) -> Result<u8, String> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(format!("invalid hex digit: '{}'", c as char)),
        }
    }

    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        out.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_matches_known_vectors() {
        assert_eq!(encode([]), "");
        assert_eq!(encode([0x00]), "00");
        assert_eq!(encode([0xff]), "ff");
        assert_eq!(encode([0xde, 0xad, 0xbe, 0xef]), "deadbeef");
        assert_eq!(encode(b"hi"), "6869");
    }

    #[test]
    fn decode_matches_known_vectors() {
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
        assert_eq!(decode("00").unwrap(), vec![0x00]);
        assert_eq!(decode("ff").unwrap(), vec![0xff]);
        assert_eq!(decode("deadbeef").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn decode_accepts_uppercase_and_mixed_case() {
        assert_eq!(decode("DEADBEEF").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(decode("DeAdBeEf").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn round_trips_arbitrary_bytes() {
        let original: Vec<u8> = (0..=255u8).collect();
        assert_eq!(decode(encode(&original)).unwrap(), original);
    }

    #[test]
    fn decode_rejects_odd_length() {
        assert!(decode("abc").is_err());
    }

    #[test]
    fn decode_rejects_non_hex_characters() {
        assert!(decode("zz").is_err());
        assert!(decode("0g").is_err());
    }
}
