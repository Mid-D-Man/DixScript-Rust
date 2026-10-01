// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/RustcHash/rustc_hash_v2.rs"
// ============================================================================
//! Port of the hasher in `rustc-hash` **2.1.1**, the version this crate's
//! `Cargo.lock` resolves. This is the default export of `Utilities::RustcHash`.
//!
//! ## Attribution
//! Derived from the `rustc-hash` crate, v2.1.1
//! (<https://github.com/rust-lang/rustc-hash>), which is licensed
//! `Apache-2.0 OR MIT`. The constants, `hash_bytes`, `multiply_mix` and the
//! `Hasher` methods follow upstream's source; the comments explaining *why*
//! they are what they are are condensed from upstream's. Upstream's
//! `no_std`, `rand`, `nightly` and seeded-state machinery is not carried over
//! (none of it is used here).
//!
//! ## Why this exists alongside `fx_hash.rs`
//! `fx_hash.rs` is the classic FxHash copied from `mid-engine`. It is a
//! different algorithm from `rustc-hash` 2.x, and measuring showed it is not a
//! safe drop-in for it: different hash values (so a different `HashMap`
//! iteration order), roughly a third slower on string keys -- this crate's
//! dominant key type -- and about 44x slower on integer keys with a
//! power-of-two stride (see docs/dixscript/utilities.md for the numbers).
//! The 2.x rewrite exists precisely to fix that last weakness. So a
//! replacement for `rustc-hash` 2.1.1 has to *be* the 2.1.1 algorithm to be
//! behavior-preserving, and this file is that.
//!
//! ## What "identical" means and how it is checked
//! For every input, `FxHasher::finish()` here equals upstream's, and so a
//! `FxHashMap`/`FxHashSet` built here iterates in the same order as one built
//! with `rustc-hash` given the same operations. The tests compare against the
//! real crate directly (`::rustc_hash`) on random write sequences and
//! random map operations, and carry upstream's own known-answer vectors.
//!
//! ## 32-bit targets
//! Both pointer widths are ported (`wasm32` is 32-bit and is a target of this
//! crate, and this module is compiled for it even though nothing calls it yet).
//! The 32-bit branches were forced on, one at a time, in an isolated copy and
//! type-checked with the real compiler, so they compile. Upstream's 32-bit
//! known-answer vectors are included but could only be *run* on a 32-bit target,
//! which was not possible here: their behavior there is unverified.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};

/// The hasher. Almost never named directly -- use [`FxHashMap`] / [`FxHashSet`].
#[derive(Clone)]
pub(crate) struct FxHasher {
    hash: usize,
}

// A multiplicative hash, with a constant found good for a multiplicative
// congruential generator in Steele & Vigna, "Computationally Easy, Spectrally
// Good Multipliers for Congruential Pseudorandom Number Generators".
#[cfg(target_pointer_width = "64")]
const K: usize = 0xf1357aea2e62a9c5;
#[cfg(target_pointer_width = "32")]
const K: usize = 0x93d765dd;

impl FxHasher {
    /// A hasher starting from `seed` instead of zero.
    #[allow(dead_code)]
    pub(crate) const fn with_seed(seed: usize) -> FxHasher {
        FxHasher { hash: seed }
    }

    #[inline]
    fn add_to_hash(&mut self, i: usize) {
        self.hash = self.hash.wrapping_add(i).wrapping_mul(K);
    }
}

impl Default for FxHasher {
    #[inline]
    fn default() -> FxHasher {
        FxHasher { hash: 0 }
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // Compress the byte string to a single u64 and add it to the hash.
        self.write_u64(hash_bytes(bytes));
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add_to_hash(i as usize);
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add_to_hash(i as usize);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add_to_hash(i as usize);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add_to_hash(i as usize);
        #[cfg(target_pointer_width = "32")]
        self.add_to_hash((i >> 32) as usize);
    }

    #[inline]
    fn write_u128(&mut self, i: u128) {
        self.add_to_hash(i as usize);
        #[cfg(target_pointer_width = "32")]
        self.add_to_hash((i >> 32) as usize);
        self.add_to_hash((i >> 64) as usize);
        #[cfg(target_pointer_width = "32")]
        self.add_to_hash((i >> 96) as usize);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add_to_hash(i);
    }

    #[inline]
    fn finish(&self) -> u64 {
        // The hash is multiplicative, so its TOP bits carry the most entropy,
        // but hash tables (hashbrown included) pick the bucket from the BOTTOM
        // bits. Rotating moves entropy from the top down. Ideally the rotation
        // would equal the table's size in bits; that isn't known here, so 26
        // (good up to 2^26 entries) on 64-bit and 15 on 32-bit.
        #[cfg(target_pointer_width = "64")]
        const ROTATE: u32 = 26;
        #[cfg(target_pointer_width = "32")]
        const ROTATE: u32 = 15;

        self.hash.rotate_left(ROTATE) as u64
    }
}

// Nothing special, digits of pi.
const SEED1: u64 = 0x243f6a8885a308d3;
const SEED2: u64 = 0x13198a2e03707344;
const PREVENT_TRIVIAL_ZERO_COLLAPSE: u64 = 0xa4093822299f31d0;

#[inline]
fn multiply_mix(x: u64, y: u64) -> u64 {
    #[cfg(target_pointer_width = "64")]
    {
        // Full u64 x u64 -> u128 product; the middle bits fluctuate most with
        // small input changes, which are the top of `lo` and bottom of `hi`,
        // so XORing the halves makes the whole output move with the input.
        let full = (x as u128) * (y as u128);
        let lo = full as u64;
        let hi = (full >> 64) as u64;
        lo ^ hi
    }

    #[cfg(target_pointer_width = "32")]
    {
        // A u64 x u64 -> u128 product is prohibitively expensive on 32-bit,
        // so decompose into 32-bit parts.
        let lx = x as u32;
        let ly = y as u32;
        let hx = (x >> 32) as u32;
        let hy = (y >> 32) as u32;

        let afull = (lx as u64) * (hy as u64);
        let bfull = (hx as u64) * (ly as u64);

        afull ^ bfull.rotate_right(32)
    }
}

/// Compresses a byte string to a `u64`.
#[inline]
fn hash_bytes(bytes: &[u8]) -> u64 {
    let len = bytes.len();
    let mut s0 = SEED1;
    let mut s1 = SEED2;

    if len <= 16 {
        // XOR the input into s0, s1.
        if len >= 8 {
            s0 ^= u64::from_le_bytes(bytes[0..8].try_into().unwrap());
            s1 ^= u64::from_le_bytes(bytes[len - 8..].try_into().unwrap());
        } else if len >= 4 {
            s0 ^= u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as u64;
            s1 ^= u32::from_le_bytes(bytes[len - 4..].try_into().unwrap()) as u64;
        } else if len > 0 {
            let lo = bytes[0];
            let mid = bytes[len / 2];
            let hi = bytes[len - 1];
            s0 ^= lo as u64;
            s1 ^= ((hi as u64) << 8) | mid as u64;
        }
    } else {
        // Bulk (may partially overlap the suffix below).
        let mut off = 0;
        while off < len - 16 {
            let x = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
            let y = u64::from_le_bytes(bytes[off + 8..off + 16].try_into().unwrap());

            // Two independent streams, so the compiler can unroll. Zeroes are a
            // common input, so a constant is XORed into `y` to stop an
            // immediate trivial collapse.
            let t = multiply_mix(s0 ^ x, PREVENT_TRIVIAL_ZERO_COLLAPSE ^ y);
            s0 = s1;
            s1 = t;
            off += 16;
        }

        let suffix = &bytes[len - 16..];
        s0 ^= u64::from_le_bytes(suffix[0..8].try_into().unwrap());
        s1 ^= u64::from_le_bytes(suffix[8..16].try_into().unwrap());
    }

    multiply_mix(s0, s1) ^ (len as u64)
}

/// Builds [`FxHasher`]s. A unit struct, as upstream's is.
#[derive(Copy, Clone, Default)]
pub(crate) struct FxBuildHasher;

impl BuildHasher for FxBuildHasher {
    type Hasher = FxHasher;
    fn build_hasher(&self) -> FxHasher {
        FxHasher::default()
    }
}

/// Drop-in replacement for `rustc_hash::FxHashMap`.
pub(crate) type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

/// Drop-in replacement for `rustc_hash::FxHashSet`.
pub(crate) type FxHashSet<V> = HashSet<V, FxBuildHasher>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    // Upstream's own known-answer vectors (rustc-hash 2.1.1 src/lib.rs tests).
    macro_rules! test_hash {
        ($( hash($value:expr) == $result:expr, )*) => {
            $( assert_eq!(FxBuildHasher.hash_one($value), $result); )*
        };
    }

    const B32: bool = cfg!(target_pointer_width = "32");

    #[test]
    fn unsigned_known_answers() {
        test_hash! {
            hash(0_u8) == 0,
            hash(1_u8) == if B32 { 3001993707 } else { 12157901119326311915 },
            hash(100_u8) == if B32 { 3844759569 } else { 16751747135202103309 },
            hash(u8::MAX) == if B32 { 999399879 } else { 1211781028898739645 },

            hash(0_u32) == 0,
            hash(1_u32) == if B32 { 3001993707 } else { 12157901119326311915 },
            hash(u32::MAX) == if B32 { 1293006356 } else { 7729994835221066939 },

            hash(0_u64) == 0,
            hash(1_u64) == if B32 { 275023839 } else { 12157901119326311915 },
            hash(u64::MAX) == if B32 { 1017982517 } else { 6288842954450348564 },

            hash(1_u128) == if B32 { 1860738631 } else { 13032756267696824044 },
            hash(u128::MAX) == if B32 { 2156022013 } else { 11702830760530184999 },

            hash(1_usize) == if B32 { 3001993707 } else { 12157901119326311915 },
            hash(usize::MAX) == if B32 { 1293006356 } else { 6288842954450348564 },
        }
    }

    // Avoids depending on std's `Hash` impls for slices.
    struct HashBytes(&'static [u8]);
    impl Hash for HashBytes {
        fn hash<H: Hasher>(&self, state: &mut H) {
            state.write(self.0);
        }
    }

    #[test]
    fn byte_string_known_answers() {
        test_hash! {
            hash(HashBytes(&[])) == if B32 { 2673204745 } else { 17606491139363777937 },
            hash(HashBytes(&[0])) == if B32 { 2948228584 } else { 5448590020104574886 },
            hash(HashBytes(&[1])) == if B32 { 2943445104 } else { 5922447956811044110 },
            hash(HashBytes(b"uwu")) == if B32 { 2699662140 } else { 7168164714682931527 },
            hash(HashBytes(b"These are some bytes for testing rustc_hash.")) == if B32 { 2303640537 } else { 2349210501944688211 },
        }
    }

    #[test]
    fn with_seed_changes_the_hash() {
        let mut a = FxHasher::with_seed(1);
        let mut b = FxHasher::with_seed(2);
        a.write_u64(7);
        b.write_u64(7);
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn map_and_set_construct_the_ways_the_crate_does() {
        let mut m: FxHashMap<String, u32> = FxHashMap::default();
        m.insert("a".into(), 1);
        let mut m2: FxHashMap<String, u32> = FxHashMap::with_capacity_and_hasher(8, Default::default());
        m2.insert("b".into(), 2);
        assert_eq!(m.get("a"), Some(&1));
        assert_eq!(m2.get("b"), Some(&2));

        let mut s: FxHashSet<&str> = FxHashSet::default();
        s.insert("x");
        let mut s2: FxHashSet<&str> = FxHashSet::with_capacity_and_hasher(8, Default::default());
        s2.insert("y");
        assert!(s.contains("x") && s2.contains("y"));
    }
}

/// Differential tests against the real `rustc-hash` (called as `::rustc_hash`,
/// the extern crate). The claim being tested is stronger than "same
/// algorithm": identical `finish()` values, therefore identical iteration
/// order for identical operations. When `rustc-hash` is dropped from
/// `[dependencies]` in the wiring pass, move it to `[dev-dependencies]` so
/// these keep guarding this file.
#[cfg(test)]
mod differential_against_real_crate {
    use super::*;
    use crate::Utilities::test_rng::XorShift;
    use std::hash::Hash;

    #[test]
    fn finish_matches_on_random_write_sequences() {
        let mut rng = XorShift::new(0xF00D);
        for _ in 0..50_000 {
            let mut mine = FxHasher::default();
            let mut real = ::rustc_hash::FxHasher::default();
            for _ in 0..rng.below(8) {
                match rng.below(7) {
                    0 => { let v = rng.byte(); mine.write_u8(v); real.write_u8(v); }
                    1 => { let v = rng.next_u64() as u16; mine.write_u16(v); real.write_u16(v); }
                    2 => { let v = rng.next_u64() as u32; mine.write_u32(v); real.write_u32(v); }
                    3 => { let v = rng.next_u64(); mine.write_u64(v); real.write_u64(v); }
                    4 => { let v = ((rng.next_u64() as u128) << 64) | rng.next_u64() as u128; mine.write_u128(v); real.write_u128(v); }
                    5 => { let v = rng.next_u64() as usize; mine.write_usize(v); real.write_usize(v); }
                    _ => {
                        // 0..=70 bytes: covers <4, 4..8, 8..=16, and the bulk loop.
                        let len = rng.below(71);
                        let b = rng.bytes(len);
                        mine.write(&b);
                        real.write(&b);
                    }
                }
            }
            assert_eq!(mine.finish(), real.finish());
        }
    }

    #[test]
    fn hash_one_matches_for_the_key_types_the_crate_uses() {
        let mut rng = XorShift::new(0xC0FFEE);
        for _ in 0..20_000 {
            let len = rng.below(40);
            let s: String = (0..len).map(|_| (b'a' + (rng.byte() % 26)) as char).collect();
            let n = rng.next_u64() as i32;

            assert_eq!(FxBuildHasher.hash_one(&s), ::rustc_hash::FxBuildHasher.hash_one(&s), "String {:?}", s);
            assert_eq!(FxBuildHasher.hash_one(s.as_str()), ::rustc_hash::FxBuildHasher.hash_one(s.as_str()));
            assert_eq!(FxBuildHasher.hash_one(n), ::rustc_hash::FxBuildHasher.hash_one(n), "i32 {}", n);
            assert_eq!(
                FxBuildHasher.hash_one((s.as_str(), n)),
                ::rustc_hash::FxBuildHasher.hash_one((s.as_str(), n))
            );
        }
        // a derived `Hash` on an enum, like `DLMModuleSubtype`
        #[derive(Hash)]
        enum E { A, B(u8) }
        assert_eq!(FxBuildHasher.hash_one(E::A), ::rustc_hash::FxBuildHasher.hash_one(E::A));
        assert_eq!(FxBuildHasher.hash_one(E::B(3)), ::rustc_hash::FxBuildHasher.hash_one(E::B(3)));
    }

    #[test]
    fn maps_iterate_in_the_same_order_after_the_same_operations() {
        let mut rng = XorShift::new(0x0DDBA11);
        for round in 0..300 {
            let mut mine: FxHashMap<String, u32> = FxHashMap::default();
            let mut real: ::rustc_hash::FxHashMap<String, u32> = ::rustc_hash::FxHashMap::default();
            for step in 0..rng.below(120) {
                let key = format!("k{}", rng.below(60));
                if rng.below(4) == 0 {
                    assert_eq!(mine.remove(&key), real.remove(&key));
                } else {
                    assert_eq!(mine.insert(key.clone(), step as u32), real.insert(key, step as u32));
                }
            }
            let a: Vec<_> = mine.iter().collect();
            let b: Vec<_> = real.iter().collect();
            assert_eq!(a, b, "iteration order diverged in round {}", round);
        }
    }

    #[test]
    fn sets_iterate_in_the_same_order_after_the_same_operations() {
        let mut rng = XorShift::new(0xBEEF);
        for round in 0..300 {
            let mut mine: FxHashSet<i32> = FxHashSet::default();
            let mut real: ::rustc_hash::FxHashSet<i32> = ::rustc_hash::FxHashSet::default();
            for _ in 0..rng.below(120) {
                let v = (rng.next_u64() % 500) as i32 - 250;
                if rng.below(4) == 0 { mine.remove(&v); real.remove(&v); } else { mine.insert(v); real.insert(v); }
            }
            let a: Vec<_> = mine.iter().collect();
            let b: Vec<_> = real.iter().collect();
            assert_eq!(a, b, "iteration order diverged in round {}", round);
        }
    }
}
