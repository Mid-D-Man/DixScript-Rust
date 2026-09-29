// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/RustcHash/fx_hash.rs"
// ============================================================================
//! Hand-rolled replacement for the `rustc-hash` crate.
//!
//! ## Provenance
//! Copied, not reinvented: this is the same `FxHasher` already hand-rolled
//! in this repo's sibling project, `Mid-D-Man/mid-engine`'s
//! `crates/mid-collections/src/fx_hash.rs` (that copy's own doc comment
//! traces it back to rustc's own internal hasher, itself ported from
//! Firefox). Copied here nearly verbatim -- only the top doc comment
//! changed, since mid-collections' version explains itself in terms of
//! `SpatialHash` cell keys, which don't exist in this crate. The algorithm,
//! the seed constant, and every method are unchanged.
//!
//! ## Why it's safe to swap in
//! Not cryptographic and not DoS-hardened the way std's default SipHasher
//! is -- deliberately, same reasoning as rustc's own internal maps and as
//! mid-collections': every `FxHashMap`/`FxHashSet` in this crate keys on
//! values this compiler computes for itself while parsing/analyzing a
//! `.mdix` file it already fully controls (symbol names, section ids,
//! table paths, ...), not attacker-supplied keys arriving over a network.
//! There's no adversarial-input threat model here to defend against, so
//! SipHash's HashDoS resistance is pure overhead this crate doesn't need.
//!
//! ## What the crate actually calls
//! Grepped every call site before writing this: only `FxHashMap::default()`,
//! `FxHashMap::with_capacity_and_hasher(cap, hasher)`, and the `FxHashSet`
//! equivalents (178 sites total, 13 files) -- both are inherent
//! `HashMap`/`HashSet` methods that fall out for free once the type alias
//! and `BuildHasherDefault<FxHasher>: Default` line up, so no wrapper
//! functions beyond the aliases themselves are needed. `rustc-hash`'s own
//! public surface is reproduced exactly (`FxHasher`, `FxBuildHasher`,
//! `FxHashMap`, `FxHashSet`) so the eventual call-site sweep is a pure
//! import-path change, nothing else.
//!
//! ## Not verified by compilation
//! `Hasher`/`BuildHasherDefault` are both long-stable, well under this
//! sandbox's rustc 1.75 ceiling, so unlike `LazyStatic` there's no toolchain
//! gap here -- but this file was still only parse-checked standalone, not
//! exercised through a real `HashMap`/`HashSet` in this sandbox. The unit
//! tests below do exercise it end-to-end and should be run for real once
//! wired in.

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
