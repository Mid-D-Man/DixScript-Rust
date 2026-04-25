//! x86_64 SSE2 whitespace-end finder.
//!
//! SSE2 is mandated by both the System V AMD64 ABI and the Windows x64 ABI,
//! so it is unconditionally present on every x86_64 target compiled by Rust.
//! No runtime feature detection is required.
//!
//! ## Algorithm
//!
//! Load 16 bytes at a time.  Compare each lane against all four whitespace
//! characters with `_mm_cmpeq_epi8`, OR the results together so that a lane
//! is 0xFF iff it matches any whitespace byte, then extract a 16-bit mask
//! with `_mm_movemask_epi8`.  A mask of 0xFFFF means all 16 bytes are
//! whitespace; any other mask means the position of the first non-whitespace
//! byte can be found with `trailing_zeros` on the bitwise complement.
//! The final < 16-byte tail is handled by the scalar fallback.

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// SSE2 fast path — 16 bytes per iteration.
///
/// # Safety
/// Caller must ensure SSE2 is available (guaranteed on all x86_64 targets).
/// All loads use `_mm_loadu_si128` (unaligned), so pointer alignment is not
/// required.
#[cfg(target_arch = "x86_64")]
unsafe fn find_whitespace_end_sse2(bytes: &[u8], start: usize) -> usize {
    let len = bytes.len();
    let mut pos = start;

    // Broadcast each whitespace character into a full 128-bit register.
    let v_space = _mm_set1_epi8(b' '  as i8);
    let v_tab   = _mm_set1_epi8(b'\t' as i8);
    let v_cr    = _mm_set1_epi8(b'\r' as i8);
    let v_nl    = _mm_set1_epi8(b'\n' as i8);

    while pos + 16 <= len {
        // Unaligned 16-byte load — safe because pos + 16 <= len.
        let chunk = _mm_loadu_si128(bytes.as_ptr().add(pos) as *const __m128i);

        // Lane == 0xFF iff the byte equals that whitespace character.
        let eq_sp = _mm_cmpeq_epi8(chunk, v_space);
        let eq_tb = _mm_cmpeq_epi8(chunk, v_tab);
        let eq_cr = _mm_cmpeq_epi8(chunk, v_cr);
        let eq_nl = _mm_cmpeq_epi8(chunk, v_nl);

        // Combine: lane is 0xFF iff byte is ANY of the four whitespace chars.
        let is_ws = _mm_or_si128(
            _mm_or_si128(eq_sp, eq_tb),
            _mm_or_si128(eq_cr, eq_nl),
        );

        // _mm_movemask_epi8 extracts the high bit of each byte into a 16-bit
        // integer.  Bit i == 1  ⟺  byte[pos + i] is whitespace.
        let mask = _mm_movemask_epi8(is_ws) as u32;

        if mask != 0xFFFF {
            // At least one non-whitespace byte in this chunk.
            // Invert: 1-bit now means non-whitespace.
            // trailing_zeros gives the index of the first non-whitespace byte.
            let non_ws = (!mask) & 0xFFFF;
            pos += non_ws.trailing_zeros() as usize;
            return pos;
        }

        pos += 16;
    }

    // Scalar tail for the remaining < 16 bytes.
    super::scalar::find_whitespace_end(bytes, pos)
}

/// Public entry point — always delegates to the SSE2 implementation.
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn find_whitespace_end(bytes: &[u8], start: usize) -> usize {
    // SAFETY: SSE2 is unconditionally available on x86_64.
    unsafe { find_whitespace_end_sse2(bytes, start) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse2_finds_end_of_spaces() {
        let s: Vec<u8> = b"         X".to_vec(); // 9 spaces then X
        assert_eq!(find_whitespace_end(&s, 0), 9);
    }

    #[test]
    fn sse2_handles_17_byte_block() {
        // Spans the 16-byte boundary; tests scalar tail.
        let mut s = vec![b' '; 17];
        s.push(b'X');
        assert_eq!(find_whitespace_end(&s, 0), 17);
    }

    #[test]
    fn sse2_mixed_whitespace_types() {
        let s = b" \t\r\n \t\r\n \t\r\n \t\r\nZ";
        assert_eq!(find_whitespace_end(s, 0), 16);
    }

    #[test]
    fn sse2_empty() {
        assert_eq!(find_whitespace_end(b"", 0), 0);
    }

    #[test]
    fn sse2_no_leading_whitespace() {
        assert_eq!(find_whitespace_end(b"hello world", 0), 0);
    }

    #[test]
    fn sse2_all_whitespace() {
        let s = vec![b' '; 64];
        assert_eq!(find_whitespace_end(&s, 0), 64);
    }
      }
