//! Scalar (non-SIMD) whitespace scanner.
//!
//! Used directly on non-SIMD targets and as the tail handler for the
//! remaining < 16-byte chunks in the SIMD implementations.

/// Returns the index of the first non-whitespace byte at or after `start`,
/// or `bytes.len()` if all remaining bytes are whitespace.
///
/// Recognises only the four DixScript whitespace bytes: space, tab,
/// carriage-return, newline.
#[inline]
pub fn find_whitespace_end(bytes: &[u8], start: usize) -> usize {
    let mut pos = start;
    while pos < bytes.len() {
        match bytes[pos] {
            b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
            _ => break,
        }
    }
    pos
}

/// Predicate version — `true` iff `b` is a DixScript whitespace byte.
#[inline(always)]
pub fn is_whitespace_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_end_of_pure_spaces() {
        let s = b"   hello";
        assert_eq!(find_whitespace_end(s, 0), 3);
    }

    #[test]
    fn handles_mixed_whitespace() {
        let s = b" \t\r\nX";
        assert_eq!(find_whitespace_end(s, 0), 4);
    }

    #[test]
    fn returns_start_when_no_whitespace() {
        let s = b"abc";
        assert_eq!(find_whitespace_end(s, 0), 0);
    }

    #[test]
    fn returns_len_when_all_whitespace() {
        let s = b"   ";
        assert_eq!(find_whitespace_end(s, 0), 3);
    }

    #[test]
    fn start_offset_respected() {
        let s = b"abc   def";
        assert_eq!(find_whitespace_end(s, 3), 6);
    }

    #[test]
    fn empty_slice() {
        assert_eq!(find_whitespace_end(b"", 0), 0);
    }
}
