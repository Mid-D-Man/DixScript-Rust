// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags"
// ============================================================================
//! Differential tests: the `bitflags!` port against the real `bitflags` 2.10.0.
//!
//! Each suite defines the SAME flag list twice -- once with this port, once with
//! the real crate (called as `::bitflags`, since a same-named local macro
//! exists) -- and runs identical operations through both, requiring identical
//! results. What is compared: `Debug` text, `{:x} {:X} {:o} {:b}` (plain and
//! alternate), `bits`, `is_empty`, `is_all`, `iter`, `iter_names`,
//! `from_bits*`, `from_name`, the `Flags` trait's extras (`truncate`, `clear`,
//! `contains_unknown_bits`, `iter_defined_names`, the `FLAGS` table), text
//! round-trips and malformed text through the parser, every method and
//! operator over pairs of values, `Extend` / `FromIterator` / `IntoIterator`,
//! derived `Default` / `Ord` / `Hash`, and `const` evaluation.
//!
//! The five flag sets: the exact `SectionFlags` shape (a gap at `0x20`, two
//! reserved high bits); overlapping and multi-bit flags on `u16`; a signed
//! `i8` including `i8::MIN`; a `u32` with a high-byte mask; and a `u128`.
//!
//! When `bitflags` is dropped from `[dependencies]` it must stay as a
//! `[dev-dependencies]` oracle for these to keep running.

use crate::Utilities::test_rng::XorShift;
use crate::Utilities::Bitflags::{parser as mine_parser, Flags as MineFlags};
use ::bitflags::{parser as real_parser, Flags as RealFlags};

/// Defines the same flag list with both implementations.
macro_rules! flag_pair {
    ($m:ident, $T:ty, { $($body:tt)* }) => {
        mod $m {
            pub mod mine {
                use crate::Utilities::Bitflags::bitflags;
                bitflags! {
                    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
                    pub struct F: $T { $($body)* }
                }
            }
            pub mod real {
                ::bitflags::bitflags! {
                    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
                    pub struct F: $T { $($body)* }
                }
            }
        }
    };
}

flag_pair!(defs_section, u8, {
    const NONE       = 0x00;
    const CONFIG     = 0x01;
    const ENUMS      = 0x02;
    const DATA       = 0x04;
    const SECURITY   = 0x08;
    const IMPORTS    = 0x10;
    const RESERVED_6 = 0x40;
    const RESERVED_7 = 0x80;
});

flag_pair!(defs_wide, u16, {
    const NONE   = 0;
    const A      = 1;
    const B      = 2;
    const AB     = 0b11;
    const A_OR_B = Self::A.bits() | Self::B.bits();
    const C      = 0x100;
    const LOW    = 0xff;
    const HIGH_C = 0x0300;
});

flag_pair!(defs_signed, i8, {
    const ONE = 1;
    const TWO = 2;
    const MID = 0x40;
    const NEG = i8::MIN;
});

flag_pair!(defs_wide32, u32, {
    const A    = 1;
    const B    = 1 << 16;
    const C    = 1 << 31;
    const MASK = 0xff00_0000;
});

flag_pair!(defs_big128, u128, {
    const LO  = 1;
    const MID = 1 << 64;
    const HI  = 1 << 127;
});

/// Text of a value through this port's / the real crate's `to_writer`.
macro_rules! text_of {
    ($parser:ident, $v:expr) => {{
        let mut s = String::new();
        $parser::to_writer($v, &mut s).unwrap();
        s
    }};
}

/// The leading words of an error message, ignoring a trailing `` `name` ``
/// (the real crate adds it only with its optional `std` feature).
fn error_kind(e: &dyn core::fmt::Display) -> String {
    let s = e.to_string();
    s.split(" `").next().unwrap().to_string()
}

macro_rules! differential_suite {
    (
        $suite:ident, $defs:ident, $T:ty, names: [$($name:literal),* $(,)?],
        values: $values:expr, pairs: $pairs:expr
    ) => {
        mod $suite {
            use super::*;
            use super::$defs::mine::F as M;
            use super::$defs::real::F as R;

            fn values() -> Vec<$T> { $values }
            fn pairs() -> Vec<($T, $T)> { $pairs }
            const NAMES: &[&str] = &[$($name),*];

            #[test]
            fn definitions_match() {
                assert_eq!(M::all().bits(), R::all().bits());
                assert_eq!(M::empty().bits(), R::empty().bits());
                assert_eq!(M::default().bits(), R::default().bits());

                let mf: Vec<(&str, $T)> = <M as MineFlags>::FLAGS.iter().map(|f| (f.name(), f.value().bits())).collect();
                let rf: Vec<(&str, $T)> = <R as RealFlags>::FLAGS.iter().map(|f| (f.name(), f.value().bits())).collect();
                assert_eq!(mf, rf, "the FLAGS table");

                let mn: Vec<(&str, $T)> = <M as MineFlags>::iter_defined_names().map(|(n, f)| (n, f.bits())).collect();
                let rn: Vec<(&str, $T)> = <R as RealFlags>::iter_defined_names().map(|(n, f)| (n, f.bits())).collect();
                assert_eq!(mn, rn, "iter_defined_names");

                // const evaluation: these only compile if the methods are const fn.
                const MINE_ALL: $T = M::all().bits();
                const MINE_EMPTY_IS_EMPTY: bool = M::empty().is_empty();
                const MINE_UNION: $T = M::all().union(M::empty()).bits();
                assert_eq!(MINE_ALL, R::all().bits());
                assert!(MINE_EMPTY_IS_EMPTY);
                assert_eq!(MINE_UNION, R::all().union(R::empty()).bits());
            }

            #[test]
            fn from_name_matches() {
                let mut probes: Vec<String> = NAMES.iter().map(|s| s.to_string()).collect();
                probes.extend(["", "NOPE", " ", "a", "Config", "NONE "].iter().map(|s| s.to_string()));
                probes.extend(NAMES.iter().map(|s| s.to_lowercase()));
                for p in &probes {
                    assert_eq!(M::from_name(p).map(|f| f.bits()), R::from_name(p).map(|f| f.bits()), "from_name({:?})", p);
                    assert_eq!(
                        <M as MineFlags>::from_name(p).map(|f| f.bits()),
                        <R as RealFlags>::from_name(p).map(|f| f.bits()),
                        "Flags::from_name({:?})", p
                    );
                }
            }

            #[test]
            fn single_value_behaviour_matches() {
                for v in values() {
                    let (m, r) = (M::from_bits_retain(v), R::from_bits_retain(v));

                    assert_eq!(format!("{:?}", m), format!("{:?}", r), "Debug {:#x}", v);
                    assert_eq!(format!("{:#?}", m), format!("{:#?}", r), "pretty Debug {:#x}", v);
                    assert_eq!(format!("{:x}", m), format!("{:x}", r));
                    assert_eq!(format!("{:X}", m), format!("{:X}", r));
                    assert_eq!(format!("{:o}", m), format!("{:o}", r));
                    assert_eq!(format!("{:b}", m), format!("{:b}", r));
                    assert_eq!(format!("{:#x}", m), format!("{:#x}", r));
                    assert_eq!(format!("{:#b}", m), format!("{:#b}", r));

                    assert_eq!(m.bits(), r.bits());
                    assert_eq!(m.is_empty(), r.is_empty(), "is_empty {:#x}", v);
                    assert_eq!(m.is_all(), r.is_all(), "is_all {:#x}", v);

                    let mi: Vec<$T> = m.iter().map(|f| f.bits()).collect();
                    let ri: Vec<$T> = r.iter().map(|f| f.bits()).collect();
                    assert_eq!(mi, ri, "iter {:#x}", v);

                    let mn: Vec<(&str, $T)> = m.iter_names().map(|(n, f)| (n, f.bits())).collect();
                    let rn: Vec<(&str, $T)> = r.iter_names().map(|(n, f)| (n, f.bits())).collect();
                    assert_eq!(mn, rn, "iter_names {:#x}", v);

                    assert_eq!(M::from_bits(v).map(|f| f.bits()), R::from_bits(v).map(|f| f.bits()), "from_bits {:#x}", v);
                    assert_eq!(M::from_bits_truncate(v).bits(), R::from_bits_truncate(v).bits(), "from_bits_truncate {:#x}", v);
                    assert_eq!(m.complement().bits(), r.complement().bits(), "complement {:#x}", v);
                    assert_eq!((!m).bits(), (!r).bits(), "! {:#x}", v);

                    // the Flags trait's extras
                    assert_eq!(
                        MineFlags::contains_unknown_bits(&m),
                        RealFlags::contains_unknown_bits(&r),
                        "contains_unknown_bits {:#x}", v
                    );
                    let (mut mt, mut rt) = (m, r);
                    MineFlags::truncate(&mut mt);
                    RealFlags::truncate(&mut rt);
                    assert_eq!(mt.bits(), rt.bits(), "truncate {:#x}", v);
                    let (mut mc, mut rc) = (m, r);
                    MineFlags::clear(&mut mc);
                    RealFlags::clear(&mut rc);
                    assert_eq!(mc.bits(), rc.bits(), "clear {:#x}", v);

                    // collection traits
                    let back_m: M = m.into_iter().collect();
                    let back_r: R = r.into_iter().collect();
                    assert_eq!(back_m.bits(), v, "mine into_iter/from_iter round trip {:#x}", v);
                    assert_eq!(back_r.bits(), v);
                    let mut ext = M::empty();
                    ext.extend(m);
                    assert_eq!(ext.bits(), v, "extend {:#x}", v);
                }
            }

            #[test]
            fn text_form_round_trips_and_matches() {
                for v in values() {
                    let (m, r) = (M::from_bits_retain(v), R::from_bits_retain(v));

                    let (tm, tr) = (text_of!(mine_parser, &m), text_of!(real_parser, &r));
                    assert_eq!(tm, tr, "to_writer {:#x}", v);
                    assert_eq!(text_of!(mine_parser, &M::from_bits_truncate(v)), text_of!(real_parser, &R::from_bits_truncate(v)));

                    let mut sm = String::new(); mine_parser::to_writer_strict(&m, &mut sm).unwrap();
                    let mut sr = String::new(); real_parser::to_writer_strict(&r, &mut sr).unwrap();
                    assert_eq!(sm, sr, "to_writer_strict {:#x}", v);

                    // parsing the text back
                    assert_eq!(mine_parser::from_str::<M>(&tm).map(|f| f.bits()).ok(), Some(v), "mine re-parse {:?}", tm);
                    assert_eq!(real_parser::from_str::<R>(&tr).map(|f| f.bits()).ok(), Some(v));
                    assert_eq!(
                        mine_parser::from_str_truncate::<M>(&tm).map(|f| f.bits()).ok(),
                        real_parser::from_str_truncate::<R>(&tr).map(|f| f.bits()).ok()
                    );
                }
            }

            #[test]
            fn malformed_and_unusual_text_is_handled_identically() {
                let mut inputs: Vec<String> = vec![
                    "".into(), " ".into(), "|".into(), " | ".into(), "0x".into(), "0xzz".into(), "0x1g".into(),
                    "NOPE".into(), "nope | NOPE".into(), "0xff".into(), "0xFF".into(), "0x0".into(),
                    "0X1".into(), "-1".into(), "0xffffffffffffffffffffffffffffffffff".into(),
                ];
                for n in NAMES {
                    inputs.push(n.to_string());
                    inputs.push(format!(" {} ", n));
                    inputs.push(format!("{} |", n));
                    inputs.push(format!("| {}", n));
                    inputs.push(format!("{n}||{n}"));
                    inputs.push(format!("{n} | 0x1"));
                    inputs.push(format!("{n}|0x10|{n}"));
                    inputs.push(n.to_lowercase());
                }
                if NAMES.len() >= 2 {
                    inputs.push(format!("{} | {}", NAMES[0], NAMES[1]));
                    inputs.push(format!("{}|{}", NAMES[1], NAMES[0]));
                }
                for s in &inputs {
                    match (mine_parser::from_str::<M>(s), real_parser::from_str::<R>(s)) {
                        (Ok(a), Ok(b)) => assert_eq!(a.bits(), b.bits(), "from_str({:?})", s),
                        (Err(a), Err(b)) => assert_eq!(error_kind(&a), error_kind(&b), "error kind for {:?}", s),
                        (a, b) => panic!("accept/reject disagreement for {:?}: mine ok={} real ok={}", s, a.is_ok(), b.is_ok()),
                    }
                    match (mine_parser::from_str_strict::<M>(s), real_parser::from_str_strict::<R>(s)) {
                        (Ok(a), Ok(b)) => assert_eq!(a.bits(), b.bits(), "from_str_strict({:?})", s),
                        (Err(a), Err(b)) => assert_eq!(error_kind(&a), error_kind(&b), "strict error kind for {:?}", s),
                        (a, b) => panic!("strict accept/reject disagreement for {:?}: mine ok={} real ok={}", s, a.is_ok(), b.is_ok()),
                    }
                    assert_eq!(
                        mine_parser::from_str_truncate::<M>(s).map(|f| f.bits()).ok(),
                        real_parser::from_str_truncate::<R>(s).map(|f| f.bits()).ok(),
                        "from_str_truncate({:?})", s
                    );
                }
            }

            #[test]
            fn every_method_and_operator_agrees_on_pairs() {
                for (a, b) in pairs() {
                    let (ma, mb) = (M::from_bits_retain(a), M::from_bits_retain(b));
                    let (ra, rb) = (R::from_bits_retain(a), R::from_bits_retain(b));
                    let ctx = format!("a={:#x} b={:#x}", a, b);

                    assert_eq!(ma.intersects(mb), ra.intersects(rb), "intersects {}", ctx);
                    assert_eq!(ma.contains(mb), ra.contains(rb), "contains {}", ctx);
                    assert_eq!(ma.union(mb).bits(), ra.union(rb).bits(), "union {}", ctx);
                    assert_eq!(ma.intersection(mb).bits(), ra.intersection(rb).bits(), "intersection {}", ctx);
                    assert_eq!(ma.difference(mb).bits(), ra.difference(rb).bits(), "difference {}", ctx);
                    assert_eq!(ma.symmetric_difference(mb).bits(), ra.symmetric_difference(rb).bits(), "symmetric_difference {}", ctx);
                    assert_eq!((ma | mb).bits(), (ra | rb).bits(), "| {}", ctx);
                    assert_eq!((ma & mb).bits(), (ra & rb).bits(), "& {}", ctx);
                    assert_eq!((ma ^ mb).bits(), (ra ^ rb).bits(), "^ {}", ctx);
                    assert_eq!((ma - mb).bits(), (ra - rb).bits(), "- {}", ctx);

                    // derived comparisons go through the hidden type
                    assert_eq!(ma.cmp(&mb), ra.cmp(&rb), "cmp {}", ctx);
                    assert_eq!(ma == mb, ra == rb, "eq {}", ctx);

                    let (mut m, mut r) = (ma, ra);
                    m.insert(mb); r.insert(rb);
                    assert_eq!(m.bits(), r.bits(), "insert {}", ctx);
                    let (mut m, mut r) = (ma, ra);
                    m.remove(mb); r.remove(rb);
                    assert_eq!(m.bits(), r.bits(), "remove {}", ctx);
                    let (mut m, mut r) = (ma, ra);
                    m.toggle(mb); r.toggle(rb);
                    assert_eq!(m.bits(), r.bits(), "toggle {}", ctx);
                    for value in [true, false] {
                        let (mut m, mut r) = (ma, ra);
                        m.set(mb, value); r.set(rb, value);
                        assert_eq!(m.bits(), r.bits(), "set({}) {}", value, ctx);
                    }
                    let (mut m, mut r) = (ma, ra);
                    m |= mb; r |= rb;
                    assert_eq!(m.bits(), r.bits(), "|= {}", ctx);
                    m &= mb; r &= rb;
                    assert_eq!(m.bits(), r.bits(), "&= {}", ctx);
                    m ^= mb; r ^= rb;
                    assert_eq!(m.bits(), r.bits(), "^= {}", ctx);
                    m -= mb; r -= rb;
                    assert_eq!(m.bits(), r.bits(), "-= {}", ctx);

                    // the Flags-trait spellings of the same operations
                    let (mut m, mut r) = (ma, ra);
                    MineFlags::insert(&mut m, mb); RealFlags::insert(&mut r, rb);
                    assert_eq!(m.bits(), r.bits(), "Flags::insert {}", ctx);
                    MineFlags::remove(&mut m, mb); RealFlags::remove(&mut r, rb);
                    assert_eq!(m.bits(), r.bits(), "Flags::remove {}", ctx);

                    // Extend over a pair
                    let (mut m, mut r) = (M::empty(), R::empty());
                    m.extend([ma, mb]); r.extend([ra, rb]);
                    assert_eq!(m.bits(), r.bits(), "extend {}", ctx);
                }
            }
        }
    };
}

fn all_u8() -> Vec<u8> { (0..=255u8).collect() }
fn all_i8() -> Vec<i8> { (0..=255u8).map(|v| v as i8).collect() }
fn all_u16() -> Vec<u16> { (0..=65535u16).collect() }

fn all_pairs_u8() -> Vec<(u8, u8)> {
    let mut out = Vec::with_capacity(65536);
    for a in 0..=255u8 { for b in 0..=255u8 { out.push((a, b)); } }
    out
}
fn all_pairs_i8() -> Vec<(i8, i8)> {
    all_pairs_u8().into_iter().map(|(a, b)| (a as i8, b as i8)).collect()
}

fn random_u16_pairs() -> Vec<(u16, u16)> {
    let mut rng = XorShift::new(0x16);
    (0..100_000).map(|_| (rng.next_u64() as u16, rng.next_u64() as u16)).collect()
}
fn random_u32(n: usize, seed: u64) -> Vec<u32> {
    let mut rng = XorShift::new(seed);
    let mut v: Vec<u32> = vec![0, 1, u32::MAX, 1 << 31, 0xff00_0000, 0x0001_0000, 0xdead_beef];
    v.extend((0..n).map(|_| rng.next_u64() as u32));
    v
}
fn random_u32_pairs() -> Vec<(u32, u32)> {
    let v = random_u32(400, 0x32);
    let mut rng = XorShift::new(0x33);
    (0..50_000).map(|_| (v[rng.below(v.len())], v[rng.below(v.len())])).collect()
}
fn random_u128(n: usize, seed: u64) -> Vec<u128> {
    let mut rng = XorShift::new(seed);
    let mut v: Vec<u128> = vec![0, 1, u128::MAX, 1 << 64, 1 << 127, (1 << 64) | 1];
    v.extend((0..n).map(|_| ((rng.next_u64() as u128) << 64) | rng.next_u64() as u128));
    v
}
fn random_u128_pairs() -> Vec<(u128, u128)> {
    let v = random_u128(200, 0x128);
    let mut rng = XorShift::new(0x129);
    (0..30_000).map(|_| (v[rng.below(v.len())], v[rng.below(v.len())])).collect()
}

differential_suite!(
    section, defs_section, u8,
    names: ["NONE", "CONFIG", "ENUMS", "DATA", "SECURITY", "IMPORTS", "RESERVED_6", "RESERVED_7"],
    values: all_u8(), pairs: all_pairs_u8()
);

differential_suite!(
    wide, defs_wide, u16,
    names: ["NONE", "A", "B", "AB", "A_OR_B", "C", "LOW", "HIGH_C"],
    values: all_u16(), pairs: random_u16_pairs()
);

differential_suite!(
    signed, defs_signed, i8,
    names: ["ONE", "TWO", "MID", "NEG"],
    values: all_i8(), pairs: all_pairs_i8()
);

differential_suite!(
    wide32, defs_wide32, u32,
    names: ["A", "B", "C", "MASK"],
    values: random_u32(20_000, 0x3232), pairs: random_u32_pairs()
);

differential_suite!(
    big128, defs_big128, u128,
    names: ["LO", "MID", "HI"],
    values: random_u128(5_000, 0x1280), pairs: random_u128_pairs()
);

/// Hashing goes through the hidden type's derive; equal values must hash equal
/// and the hash must be a function of the value alone, as with the real type.
#[test]
fn derived_hash_is_consistent() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use self::defs_section::mine::F as M;

    fn h<T: Hash>(t: &T) -> u64 {
        let mut s = DefaultHasher::new();
        t.hash(&mut s);
        s.finish()
    }
    assert_eq!(h(&M::CONFIG), h(&M::from_bits_retain(1)));
    assert_ne!(h(&M::CONFIG), h(&M::DATA));
}
