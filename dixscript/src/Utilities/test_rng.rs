// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/test_rng.rs"
// ============================================================================
//! Test-only. A tiny deterministic PRNG for the differential tests in the
//! hand-rolled `Utilities/*` modules, which compare each replacement against
//! the real crate it replaces on thousands of generated inputs.
//!
//! Deterministic on purpose: a failing case must reproduce from the same seed
//! on every machine, and a test that is sometimes green by luck is worse than
//! none. Not for anything but tests -- `Uuid::new_v4` uses the OS entropy
//! source, not this.

/// xorshift64. Fixed seed per test, so failures replay exactly.
pub(crate) struct XorShift(u64);

impl XorShift {
    pub(crate) fn new(seed: u64) -> Self {
        // xorshift must never be seeded with 0 (it would stay 0 forever).
        XorShift(if seed == 0 { 0x9e37_79b9_7f4a_7c15 } else { seed })
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform-enough value in `0..n` (`n` must be non-zero).
    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    pub(crate) fn byte(&mut self) -> u8 {
        (self.next_u64() >> 24) as u8
    }

    pub(crate) fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.byte()).collect()
    }

    pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}
