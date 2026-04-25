//! WebAssembly SIMD128 whitespace-end finder.
//!
//! SIMD128 is an *optional* WebAssembly feature.  Enable it by passing
//! `-C target-feature=+simd128` (e.g. via `RUSTFLAGS` or `.cargo/config.toml`).
//!
//! When the feature is not active this module compiles to a thin wrapper
//! that delegates directly to the scalar fallback — no dead SIMD code is
//! emitted and no `wasm-opt` post-pass is required.
//!
//! ## Algorithm (SIMD128 path)
//!
//! Mirrors the SSE2 approach: `i8x16_bitmask` extracts the sign bit of each
//! byte, giving a 16-bit mask where bit i == 1 iff byte[pos+i] is whitespace
//! (since our equality-compare result is 0xFF = sign bit set).  Trailing zeros
//! of the complement locate the first non-whitespace position.

/// Public entry point.
#[cfg(target_arch = "wasm32")]
#[inline]
pub fn find_whitespace_end(bytes: &[u8], start: usize) -> usize {
    #[cfg(target_feature = "simd128")]
    {
        // SAFETY: guarded by target_feature = "simd128".
        return unsafe { find_whitespace_end_simd128(bytes, start) };
    }

    // Scalar fallback when SIMD128 is not enabled.
    #[cfg(not(target_feature = "simd128"))]
    {
        return super::scalar::find_whitespace_end(bytes, start);
    }

    // Unreachable — one of the two cfg branches above always matches on wasm32.
    #[allow(unreachable_code)]
    super::scalar::find_whitespace_end(bytes, start)
}

// ── SIMD128 implementation — compiled only when the feature is active ──────────

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
use std::arch::wasm32::*;

/// SIMD128 fast path — 16 bytes per iteration.
///
/// # Safety
/// Requires `target_feature = "simd128"`.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
unsafe fn find_whitespace_end_simd128(bytes: &[u8], start: usize) -> usize {
    let len = bytes.len();
    let mut pos = start;

    let v_space = u8x16_splat(b' ');
    let v_tab   = u8x16_splat(b'\t');
    let v_cr    = u8x16_splat(b'\r');
    let v_nl    = u8x16_splat(b'\n');

    while pos + 16 <= len {
        // Unaligned 16-byte load.
        let chunk = v128_load(bytes.as_ptr().add(pos) as *const v128);

        // Lane == 0xFF iff byte equals that whitespace character.
        let eq_sp = u8x16_eq(chunk, v_space);
        let eq_tb = u8x16_eq(chunk, v_tab);
        let eq_cr = u8x16_eq(chunk, v_cr);
        let eq_nl = u8x16_eq(chunk, v_nl);

        // Combine.
        let is_ws = v128_or(v128_or(eq_sp, eq_tb), v128_or(eq_cr, eq_nl));

        // i8x16_bitmask: extracts the sign bit (bit 7) of each of the 16 bytes.
        // Our 0xFF lanes have bit 7 == 1; 0x00 lanes have bit 7 == 0.
        // Result is i32; only the low 16 bits are meaningful.
        let mask = i8x16_bitmask(is_ws) as u32 & 0xFFFF;

        if mask != 0xFFFF {
            let non_ws = (!mask) & 0xFFFF;
            pos += non_ws.trailing_zeros() as usize;
            return pos;
        }

        pos += 16;
    }

    super::scalar::find_whitespace_end(bytes, pos)
          }
