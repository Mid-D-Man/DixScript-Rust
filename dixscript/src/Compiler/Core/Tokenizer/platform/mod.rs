//! Platform-specific whitespace-end finder used by the lexer's
//! `skip_whitespace` routine.
//!
//! ## Public surface
//!
//! One function: `find_whitespace_end(bytes, start) -> usize`.
//!
//! Returns the index of the first non-whitespace byte at or after `start`,
//! or `bytes.len()` if all remaining bytes are whitespace.  Only the four
//! DixScript whitespace bytes are recognised: space (0x20), tab (0x09),
//! carriage-return (0x0D), and line-feed (0x0A).
//!
//! ## Why split here instead of inside `skip_whitespace`?
//!
//! `skip_whitespace` needs to count newlines for accurate `line`/`column`
//! tracking.  Mixing that count into the SIMD fast-path complicates the
//! implementation without a meaningful throughput win (whitespace blocks are
//! short in config files; newlines within them are rarer still).
//!
//! The chosen split:
//! 1. **Platform module** — finds the *position* of the first non-whitespace
//!    byte as fast as possible (16 bytes at a time on SIMD targets).
//! 2. **Lexer** — counts `\n` bytes in the resulting slice with
//!    `memchr::memchr_iter` (itself SIMD-accelerated) in a single pass,
//!    then updates `line` and `column` arithmetically.
//!
//! ## Platform routing
//!
//! | Target          | Module       | Strategy                              |
//! |-----------------|--------------|---------------------------------------|
//! | `x86_64`        | `x86_64`     | SSE2 (guaranteed by ABI baseline)     |
//! | `aarch64`       | `aarch64`    | NEON (guaranteed by ABI baseline)     |
//! | `wasm32`        | `wasm32`     | SIMD128 when `+simd128` feature flag  |
//! | everything else | `scalar`     | byte-at-a-time match                  |

// Scalar is always compiled — used as the tail handler by SIMD modules.
pub mod scalar;

#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(target_arch = "aarch64")]
mod aarch64;

#[cfg(target_arch = "wasm32")]
mod wasm32;

/// Returns the byte offset of the first non-whitespace byte at or after
/// `start`, or `bytes.len()` if the remainder of the slice is all whitespace.
///
/// Recognises only ` ` (0x20), `\t` (0x09), `\r` (0x0D), `\n` (0x0A).
#[inline]
pub fn find_whitespace_end(bytes: &[u8], start: usize) -> usize {
    #[cfg(target_arch = "x86_64")]
    { return x86_64::find_whitespace_end(bytes, start); }

    #[cfg(target_arch = "aarch64")]
    { return aarch64::find_whitespace_end(bytes, start); }

    #[cfg(target_arch = "wasm32")]
    { return wasm32::find_whitespace_end(bytes, start); }

    // Scalar fallback for all other targets.
    #[allow(unreachable_code)]
    scalar::find_whitespace_end(bytes, start)
}
