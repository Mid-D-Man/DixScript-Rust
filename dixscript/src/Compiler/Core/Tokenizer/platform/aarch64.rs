//! AArch64 NEON whitespace-end finder.
//!
//! NEON (Advanced SIMD) is a mandatory part of the AArch64 base architecture
//! — it is unconditionally present on every AArch64 target compiled by Rust.
//!
//! ## Algorithm
//!
//! Load 16 bytes with `vld1q_u8`.  For each of the four whitespace characters,
//! use `vceqq_u8` to produce a lane of 0xFF where the byte matches.  OR all
//! four results together.  Reduce with `vminvq_u8`: if the minimum lane is
//! 0xFF, every byte was whitespace and we advance 16 positions.  Otherwise,
//! the exact byte position is found by falling back to the scalar tail — NEON
//! lacks a direct `movemask` equivalent, and the scalar scan of a single
//! 16-byte chunk is negligible compared to the cost of manufacturing a bitmask
//! via `vshrn`/`vget_lane` tricks.

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

/// NEON fast path — 16 bytes per iteration, scalar tail for < 16 bytes.
///
/// # Safety
/// NEON is unconditionally available on all AArch64 targets.
#[cfg(target_arch = "aarch64")]
unsafe fn find_whitespace_end_neon(bytes: &[u8], start: usize) -> usize {
    let len = bytes.len();
    let mut pos = start;

    // Broadcast each whitespace character into a 16-byte vector.
    let v_space = vdupq_n_u8(b' ');
    let v_tab   = vdupq_n_u8(b'\t');
    let v_cr    = vdupq_n_u8(b'\r');
    let v_nl    = vdupq_n_u8(b'\n');

    while pos + 16 <= len {
        // Unaligned load — vld1q_u8 accepts any pointer alignment on AArch64.
        let chunk = vld1q_u8(bytes.as_ptr().add(pos));

        // Lane == 0xFF iff byte equals that whitespace character.
        let eq_sp = vceqq_u8(chunk, v_space);
        let eq_tb = vceqq_u8(chunk, v_tab);
        let eq_cr = vceqq_u8(chunk, v_cr);
        let eq_nl = vceqq_u8(chunk, v_nl);

        // Combine: lane is 0xFF iff byte is any whitespace character.
        let is_ws = vorrq_u8(vorrq_u8(eq_sp, eq_tb), vorrq_u8(eq_cr, eq_nl));

        // vminvq_u8 reduces to the minimum byte across all 16 lanes.
        // min == 0xFF  →  every lane matched a whitespace byte.
        // min == 0x00  →  at least one non-whitespace byte present.
        if vminvq_u8(is_ws) != 0xFF {
            // Some byte is non-whitespace; find the exact position with
            // the scalar scanner (only up to 15 extra bytes scanned).
            return super::scalar::find_whitespace_end(bytes, pos);
        }

        pos += 16;
    }

    // Scalar tail for the remaining < 16 bytes.
    super::scalar::find_whitespace_end(bytes, pos)
}

/// Public entry point — always delegates to the NEON implementation.
#[cfg(target_arch = "aarch64")]
#[inline]
pub fn find_whitespace_end(bytes: &[u8], start: usize) -> usize {
    // SAFETY: NEON is unconditionally available on AArch64.
    unsafe { find_whitespace_end_neon(bytes, start) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neon_finds_end_of_spaces() {
        let s: Vec<u8> = [b' '; 9].into_iter().chain([b'X']).collect();
        assert_eq!(find_whitespace_end(&s, 0), 9);
    }

    #[test]
    fn neon_handles_cross_boundary() {
        let mut s = vec![b' '; 17];
        s.push(b'X');
        assert_eq!(find_whitespace_end(&s, 0), 17);
    }

    #[test]
    fn neon_all_whitespace() {
        let s = vec![b'\n'; 64];
        assert_eq!(find_whitespace_end(&s, 0), 64);
    }

    #[test]
    fn neon_no_whitespace() {
        assert_eq!(find_whitespace_end(b"hello", 0), 0);
    }
  }
