// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/RustcHash/fx_hash.rs"
// ============================================================================
//! The classic FxHash, copied from `Mid-D-Man/mid-engine`. **Not** the default
//! export of `Utilities::RustcHash` -- see below for why.
//!
//! ## Provenance
//! Copied, not reinvented: this is the `FxHasher` already hand-rolled in this
//! repo's sibling project, `mid-engine`'s
//! `crates/mid-collections/src/fx_hash.rs` (that copy's own doc comment traces
//! it back to rustc's original internal hasher, itself ported from Firefox).
//! Copied nearly verbatim -- only the top doc comment changed, since
//! mid-collections' version explains itself in terms of `SpatialHash` cell
//! keys, which don't exist in this crate. The algorithm, the seed constant and
//! every method are unchanged. `FxHashMap`/`FxHashSet` aliases were added.
//!
//! ## It is NOT a drop-in for `rustc-hash` 2.x
//! This crate's `Cargo.lock` resolves `rustc-hash` 2.1.1, which replaced this
//! algorithm. Comparing the two directly (release build, 100k entries):
//! - **Different hash values**, so a `HashMap`/`HashSet` built with this hasher
//!   iterates in a different order than one built with `rustc-hash` 2.1.1.
//! - **About a third slower on string keys** (6.5 ms vs 4.9 ms), which is
//!   ~100 of this crate's ~125 `FxHashMap`/`FxHashSet` sites.
//! - **About 44x slower on integer keys with a power-of-two stride** (77 ms vs
//!   1.75 ms at stride 4096): this algorithm's low bits are weak, and hash
//!   tables pick their bucket from the low bits. That is the weakness the 2.x
//!   rewrite was made to fix. (This crate has only three `i32`-keyed maps, so
//!   it is unlikely to bite here; it is real for anyone reusing this file.)
//!
//! So `RustcHash/mod.rs` exports `rustc_hash_v2.rs` -- a port of the 2.1.1
//! algorithm -- as the default. This file stays available as
//! `Utilities::RustcHash::classic` (it is the copy you asked for, and it is
//! tested), for anything that wants the classic behavior on purpose.
//!
//! ## Why the classic algorithm is still fine for what it is
//! Not cryptographic and not DoS-hardened, deliberately, same reasoning as
//! rustc's own maps: keys are values this compiler computes for itself while
//! parsing a file it already controls (symbol names, section ids, table
//! paths), not attacker-supplied input over a network.
//!
//! ## Verification
//! Unit-tested end to end through a real `HashMap`/`HashSet`. There is no
//! differential test against `rustc-hash` here because the two are different
//! algorithms by design; the benchmark numbers above were taken in a scratch
//! crate on one machine and are indicative, not a guarantee for other hardware.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// The hasher itself. Almost never named directly -- use `FxHashMap`/
/// `FxHashSet` below, or `FxBuildHasher` for a type that needs the
/// `BuildHasher` directly (e.g. `HashMap::with_hasher`).
#[derive(Default)]
pub(crate) struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, w: u64) {
        self.hash = (self.hash.rotate_left(5) ^ w).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, mut bytes: &[u8]) {
        while bytes.len() >= 8 {
            self.add(u64::from_ne_bytes(bytes[..8].try_into().unwrap()));
            bytes = &bytes[8..];
        }
        if bytes.len() >= 4 {
            self.add(u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as u64);
            bytes = &bytes[4..];
        }
        if bytes.len() >= 2 {
            self.add(u16::from_ne_bytes(bytes[..2].try_into().unwrap()) as u64);
            bytes = &bytes[2..];
        }
        if let Some(&b) = bytes.first() {
            self.add(b as u64);
        }
    }

    #[inline] fn write_u8(&mut self, i: u8)       { self.add(i as u64); }
    #[inline] fn write_u16(&mut self, i: u16)     { self.add(i as u64); }
    #[inline] fn write_u32(&mut self, i: u32)     { self.add(i as u64); }
    #[inline] fn write_u64(&mut self, i: u64)     { self.add(i); }
    #[inline] fn write_usize(&mut self, i: usize) { self.add(i as u64); }
    #[inline] fn write_i8(&mut self, i: i8)       { self.add(i as u64); }
    #[inline] fn write_i16(&mut self, i: i16)     { self.add(i as u64); }
    #[inline] fn write_i32(&mut self, i: i32)     { self.add(i as u64); }
    #[inline] fn write_i64(&mut self, i: i64)     { self.add(i as u64); }
    #[inline] fn write_isize(&mut self, i: isize) { self.add(i as u64); }

    #[inline]
    fn finish(&self) -> u64 { self.hash }
}

/// `HashMap<K, V, FxBuildHasher>::default()` to use it directly; almost
/// everything should reach for `FxHashMap`/`FxHashSet` below instead.
pub(crate) type FxBuildHasher = BuildHasherDefault<FxHasher>;

/// Drop-in replacement for `rustc_hash::FxHashMap`.
pub(crate) type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

/// Drop-in replacement for `rustc_hash::FxHashSet`.
pub(crate) type FxHashSet<T> = HashSet<T, FxBuildHasher>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fx_hash_map_default_constructs_and_stores() {
        let mut m: FxHashMap<&str, i32> = FxHashMap::default();
        m.insert("a", 1);
        m.insert("b", 2);
        assert_eq!(m.get("a"), Some(&1));
        assert_eq!(m.get("b"), Some(&2));
        assert_eq!(m.get("c"), None);
    }

    #[test]
    fn fx_hash_map_with_capacity_and_hasher_matches_default_behavior() {
        let mut m: FxHashMap<i32, i32> = FxHashMap::with_capacity_and_hasher(16, FxBuildHasher::default());
        for i in 0..16 {
            m.insert(i, i * i);
        }
        for i in 0..16 {
            assert_eq!(m.get(&i), Some(&(i * i)));
        }
    }

    #[test]
    fn fx_hash_set_default_constructs_and_stores() {
        let mut s: FxHashSet<&str> = FxHashSet::default();
        s.insert("x");
        s.insert("y");
        assert!(s.contains("x"));
        assert!(s.contains("y"));
        assert!(!s.contains("z"));
    }

    #[test]
    fn fx_hash_set_with_capacity_and_hasher_matches_default_behavior() {
        let mut s: FxHashSet<i32> = FxHashSet::with_capacity_and_hasher(8, FxBuildHasher::default());
        for i in 0..8 {
            s.insert(i);
        }
        for i in 0..8 {
            assert!(s.contains(&i));
        }
        assert!(!s.contains(&100));
    }

    #[test]
    fn same_key_hashes_the_same_every_time() {
        let mut h1 = FxHasher::default();
        let mut h2 = FxHasher::default();
        "database.primary.host".hash_into(&mut h1);
        "database.primary.host".hash_into(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn different_keys_are_very_likely_to_hash_differently() {
        // Not a cryptographic guarantee (never the point of FxHash) --
        // just confirms `add()` is actually mixing bits, not silently
        // returning a constant or ignoring input.
        let mut h1 = FxHasher::default();
        let mut h2 = FxHasher::default();
        "database.primary.host".hash_into(&mut h1);
        "database.primary.port".hash_into(&mut h2);
        assert_ne!(h1.finish(), h2.finish());
    }

    /// Tiny local helper so the two tests above read naturally -- equivalent
    /// to `std::hash::Hash::hash(&self, state)` for `&str`, without pulling
    /// in the `Hash` trait bound just for a two-test file.
    trait HashInto {
        fn hash_into(&self, hasher: &mut FxHasher);
    }
    impl HashInto for str {
        fn hash_into(&self, hasher: &mut FxHasher) {
            hasher.write(self.as_bytes());
        }
    }
}
