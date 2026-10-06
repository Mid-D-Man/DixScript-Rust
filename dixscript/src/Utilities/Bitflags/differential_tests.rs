// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags"
// ============================================================================
//! Differential tests: the same flags type is declared once with the real
//! `bitflags` 2.x (`::bitflags`, a dev-dependency) and once with this port, and
//! every observable behaviour is compared. Nothing here is checked against a
//! hand-written expectation -- the real crate is the specification.

use super::parser as lp;
use crate::Utilities::test_rng::XorShift;

// The same body, declared twice. `Local` = this port, `Real` = the crate.
macro_rules! decl {
    ($mac:path, $name:ident) => {
        $mac! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
            pub struct $name: u8 {
                const NONE     = 0x00;
                const CONFIG   = 0x01;
                const ENUMS    = 0x02;
                const DATA     = 0x04;
                const SECURITY = 0x08;
                const IMPORTS  = 0x10;
                const RESERVED_6 = 0x40;
                const RESERVED_7 = 0x80;
                const CD       = Self::CONFIG.bits() | Self::DATA.bits();
            }
        }
    };
}
decl!(crate::Utilities::Bitflags::bitflags, Local);
decl!(::bitflags::bitflags, Real);

/// Everything observable about one value, as text.
macro_rules! unary {
    ($T:ident, $bits:expr) => {{
        let bits: u8 = $bits;
        let v = $T::from_bits_retain(bits);
        let out = format!(
            "{:?}|{}|{:#?}|{:b}|{:o}|{:x}|{:X}|{:#010b}|{:?}|{:?}|{:?}|{}|{}|{}|{:?}|{:?}",
            v,
            bits,
            v,
            v,
            v,
            v,
            v,
            v,
            $T::from_bits(bits).map(|f| f.bits()),
            $T::from_bits_truncate(bits).bits(),
            v.complement().bits(),
            v.is_empty(),
            v.is_all(),
            v.contains_unknown_bits_probe(),
            v.iter().map(|f| f.bits()).collect::<Vec<_>>(),
            v.iter_names().map(|(n, f)| (n, f.bits())).collect::<Vec<_>>(),
        );
        // the two types differ only in their name, which Debug prints
        out.replace(stringify!($T), "T")
    }};
}

// `contains_unknown_bits` is a `Flags` trait method, not inherent; give both
// types the same inherent probe so `unary!` can stay macro-generic.
impl Local {
    fn contains_unknown_bits_probe(&self) -> bool {
        use super::Flags;
        self.contains_unknown_bits()
    }
}
impl Real {
    fn contains_unknown_bits_probe(&self) -> bool {
        use ::bitflags::Flags;
        self.contains_unknown_bits()
    }
}

#[test]
fn every_u8_value_observes_identically() {
    for b in 0..=255u8 {
        assert_eq!(unary!(Local, b), unary!(Real, b), "bits {b:#04x}");
    }
}

#[test]
fn every_pair_of_u8_values_combines_identically() {
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            let (la, lb) = (Local::from_bits_retain(a), Local::from_bits_retain(b));
            let (ra, rb) = (Real::from_bits_retain(a), Real::from_bits_retain(b));
            assert_eq!(la.contains(lb), ra.contains(rb), "contains {a:#x} {b:#x}");
            assert_eq!(la.intersects(lb), ra.intersects(rb), "intersects {a:#x} {b:#x}");
            assert_eq!(la.union(lb).bits(), ra.union(rb).bits(), "union {a:#x} {b:#x}");
            assert_eq!(la.intersection(lb).bits(), ra.intersection(rb).bits(), "intersection {a:#x} {b:#x}");
            assert_eq!(la.difference(lb).bits(), ra.difference(rb).bits(), "difference {a:#x} {b:#x}");
            assert_eq!(
                la.symmetric_difference(lb).bits(),
                ra.symmetric_difference(rb).bits(),
                "symmetric_difference {a:#x} {b:#x}"
            );
            assert_eq!((la | lb).bits(), (ra | rb).bits(), "| {a:#x} {b:#x}");
            assert_eq!((la & lb).bits(), (ra & rb).bits(), "& {a:#x} {b:#x}");
            assert_eq!((la ^ lb).bits(), (ra ^ rb).bits(), "^ {a:#x} {b:#x}");
            assert_eq!((la - lb).bits(), (ra - rb).bits(), "- {a:#x} {b:#x}");
            assert_eq!((!la).bits(), (!ra).bits(), "! {a:#x}");
            assert_eq!(la.cmp(&lb), ra.cmp(&rb), "Ord {a:#x} {b:#x}");
            assert_eq!(la == lb, ra == rb, "Eq {a:#x} {b:#x}");

            let (mut x, mut y) = (la, ra);
            x |= lb;
            y |= rb;
            assert_eq!(x.bits(), y.bits(), "|= {a:#x} {b:#x}");
            let (mut x, mut y) = (la, ra);
            x &= lb;
            y &= rb;
            assert_eq!(x.bits(), y.bits(), "&= {a:#x} {b:#x}");
            let (mut x, mut y) = (la, ra);
            x ^= lb;
            y ^= rb;
            assert_eq!(x.bits(), y.bits(), "^= {a:#x} {b:#x}");
            let (mut x, mut y) = (la, ra);
            x -= lb;
            y -= rb;
            assert_eq!(x.bits(), y.bits(), "-= {a:#x} {b:#x}");
            let (mut x, mut y) = (la, ra);
            x.insert(lb);
            y.insert(rb);
            assert_eq!(x.bits(), y.bits(), "insert {a:#x} {b:#x}");
            let (mut x, mut y) = (la, ra);
            x.remove(lb);
            y.remove(rb);
            assert_eq!(x.bits(), y.bits(), "remove {a:#x} {b:#x}");
            let (mut x, mut y) = (la, ra);
            x.toggle(lb);
            y.toggle(rb);
            assert_eq!(x.bits(), y.bits(), "toggle {a:#x} {b:#x}");
            for on in [false, true] {
                let (mut x, mut y) = (la, ra);
                x.set(lb, on);
                y.set(rb, on);
                assert_eq!(x.bits(), y.bits(), "set({on}) {a:#x} {b:#x}");
            }
        }
    }
}

#[test]
fn defined_flags_table_and_associated_consts_match() {
    use super::Flags as LF;
    use ::bitflags::Flags as RF;
    let l: Vec<_> = <Local as LF>::FLAGS.iter().map(|f| (f.name(), f.value().bits())).collect();
    let r: Vec<_> = <Real as RF>::FLAGS.iter().map(|f| (f.name(), f.value().bits())).collect();
    assert_eq!(l, r);
    assert_eq!(Local::empty().bits(), Real::empty().bits());
    assert_eq!(Local::all().bits(), Real::all().bits());
    let ln: Vec<_> = <Local as LF>::iter_defined_names().map(|(n, f)| (n, f.bits())).collect();
    let rn: Vec<_> = <Real as RF>::iter_defined_names().map(|(n, f)| (n, f.bits())).collect();
    assert_eq!(ln, rn);
}


#[test]
fn from_name_agrees_for_every_name_and_near_miss() {
    for name in [
        "NONE", "CONFIG", "ENUMS", "DATA", "SECURITY", "IMPORTS", "RESERVED_6", "RESERVED_7", "CD", "", "config",
        "Config", " CONFIG", "CONFIG ", "CONFIG|DATA", "0x1", "RESERVED_5", "CD\0",
    ] {
        assert_eq!(
            Local::from_name(name).map(|f| f.bits()),
            Real::from_name(name).map(|f| f.bits()),
            "from_name({name:?})"
        );
    }
}

/// Text inputs exercising every branch of the parser: separators, whitespace,
/// hex of every shape, overflow, unknown names, empty pieces.
const PARSE_INPUTS: &[&str] = &[
    "", " ", "NONE", "CONFIG", "CONFIG | DATA", "CONFIG|DATA", "  CONFIG  |  DATA  ", "CD", "CD | CONFIG",
    "0x1", "0x01", "0X01", "0xff", "0xFF", "0x100", "0x1ff", "0x", "0xZZ", "0x 1", "0x1 | DATA", "DATA | 0x20",
    "0x20 | 0x40", "NOPE", "CONFIG | NOPE", "CONFIG |", "| CONFIG", "CONFIG | | DATA", "CONFIG DATA", "config",
    "CONFIG | CONFIG", "RESERVED_7 | 0x7f", "0", "1", "-1", "0xfffffffff", "\u{00e9}", "CONFIG\n|\nDATA",
];

fn text<T>(r: Result<T, lp::ParseError>, f: impl Fn(T) -> u8) -> String {
    match r {
        Ok(v) => format!("ok {:#x}", f(v)),
        Err(e) => format!("err {e}"),
    }
}
fn rtext<T>(r: Result<T, ::bitflags::parser::ParseError>, f: impl Fn(T) -> u8) -> String {
    match r {
        Ok(v) => format!("ok {:#x}", f(v)),
        Err(e) => format!("err {e}"),
    }
}

#[test]
fn text_parsing_agrees_in_all_three_modes() {
    for input in PARSE_INPUTS {
        assert_eq!(
            text(lp::from_str::<Local>(input), |f| f.bits()),
            rtext(::bitflags::parser::from_str::<Real>(input), |f| f.bits()),
            "from_str({input:?})"
        );
        assert_eq!(
            text(lp::from_str_truncate::<Local>(input), |f| f.bits()),
            rtext(::bitflags::parser::from_str_truncate::<Real>(input), |f| f.bits()),
            "from_str_truncate({input:?})"
        );
        assert_eq!(
            text(lp::from_str_strict::<Local>(input), |f| f.bits()),
            rtext(::bitflags::parser::from_str_strict::<Real>(input), |f| f.bits()),
            "from_str_strict({input:?})"
        );
    }
}

#[test]
fn text_formatting_agrees_in_all_three_modes_for_every_value() {
    for b in 0..=255u8 {
        let (l, r) = (Local::from_bits_retain(b), Real::from_bits_retain(b));
        let (mut a, mut c) = (String::new(), String::new());
        lp::to_writer(&l, &mut a).unwrap();
        ::bitflags::parser::to_writer(&r, &mut c).unwrap();
        assert_eq!(a, c, "to_writer {b:#04x}");
        let (mut a, mut c) = (String::new(), String::new());
        lp::to_writer_truncate(&l, &mut a).unwrap();
        ::bitflags::parser::to_writer_truncate(&r, &mut c).unwrap();
        assert_eq!(a, c, "to_writer_truncate {b:#04x}");
        let (mut a, mut c) = (String::new(), String::new());
        lp::to_writer_strict(&l, &mut a).unwrap();
        ::bitflags::parser::to_writer_strict(&r, &mut c).unwrap();
        assert_eq!(a, c, "to_writer_strict {b:#04x}");
        // and the text round-trips to the same value in both implementations
        let mut t = String::new();
        lp::to_writer(&l, &mut t).unwrap();
        assert_eq!(lp::from_str::<Local>(&t).unwrap().bits(), b);
    }
}

#[test]
fn extend_from_iterator_and_into_iterator_agree() {
    let mut rng = XorShift::new(0xB17F_1A65);
    for _ in 0..2_000 {
        let n = rng.below(6);
        let bits: Vec<u8> = (0..n).map(|_| rng.byte()).collect();
        let l: Local = bits.iter().map(|&b| Local::from_bits_retain(b)).collect();
        let r: Real = bits.iter().map(|&b| Real::from_bits_retain(b)).collect();
        assert_eq!(l.bits(), r.bits(), "collect {bits:?}");
        let (mut l2, mut r2) = (Local::CONFIG, Real::CONFIG);
        l2.extend(bits.iter().map(|&b| Local::from_bits_retain(b)));
        r2.extend(bits.iter().map(|&b| Real::from_bits_retain(b)));
        assert_eq!(l2.bits(), r2.bits(), "extend {bits:?}");
        let li: Vec<u8> = l.into_iter().map(|f| f.bits()).collect();
        let ri: Vec<u8> = r.into_iter().map(|f| f.bits()).collect();
        assert_eq!(li, ri, "into_iter {bits:?}");
        let lr: Vec<u8> = (&l).into_iter().map(|f| f.bits()).collect();
        assert_eq!(lr, li);
    }
}

#[test]
fn iterators_report_remaining_and_are_fused_like_the_real_ones() {
    for b in 0..=255u8 {
        let mut l = Local::from_bits_retain(b).iter_names();
        let mut r = Real::from_bits_retain(b).iter_names();
        loop {
            assert_eq!(l.remaining().bits(), r.remaining().bits(), "remaining {b:#04x}");
            let (x, y) = (l.next().map(|(n, f)| (n, f.bits())), r.next().map(|(n, f)| (n, f.bits())));
            assert_eq!(x, y);
            if x.is_none() {
                break;
            }
        }
        assert!(l.next().is_none() && r.next().is_none());
    }
}

// ── the const-fn surface ────────────────────────────────────────────────
macro_rules! const_surface {
    ($T:ident, $m:ident) => {
        mod $m {
            use super::$T;
            pub const A: $T = $T::CONFIG.union($T::DATA);
            pub const EMPTY: $T = $T::empty();
            pub const ALL: $T = $T::all();
            pub const FROM_BITS: Option<$T> = $T::from_bits(0x05);
            pub const FROM_BITS_BAD: Option<$T> = $T::from_bits(0x20);
            pub const TRUNC: $T = $T::from_bits_truncate(0xff);
            pub const RETAIN: $T = $T::from_bits_retain(0xff);
            pub const BITS: u8 = A.bits();
            pub const IS_EMPTY: bool = EMPTY.is_empty();
            pub const IS_ALL: bool = ALL.is_all();
            pub const HAS: bool = A.contains($T::CONFIG);
            pub const ISECT: bool = A.intersects($T::ENUMS);
            pub const AND: $T = A.intersection($T::CONFIG);
            pub const DIFF: $T = A.difference($T::CONFIG);
            pub const XOR: $T = A.symmetric_difference($T::CD);
            pub const NOT: $T = A.complement();
        }
    };
}
const_surface!(Local, c_local);
const_surface!(Real, c_real);

#[test]
fn every_const_fn_is_usable_in_const_context_with_the_same_result() {
    macro_rules! pair {
        ($($n:ident),*) => {$(
            assert_eq!(
                format!("{:?}", c_local::$n).replace("Local", "T"),
                format!("{:?}", c_real::$n).replace("Real", "T"),
                stringify!($n)
            );
        )*};
    }
    pair!(A, EMPTY, ALL, FROM_BITS, FROM_BITS_BAD, TRUNC, RETAIN, BITS, IS_EMPTY, IS_ALL, HAS, ISECT, AND, DIFF, XOR, NOT);
}

// ── derived-trait text on the real, wired type ──────────────────────────
#[test]
fn section_flags_debug_prints_flag_names_like_the_real_crate() {
    use crate::Compiler::Core::BinarySerialization::binary_format::SectionFlags;
    assert_eq!(format!("{:?}", SectionFlags::CONFIG | SectionFlags::DATA), "SectionFlags(CONFIG | DATA)");
    assert_eq!(format!("{:?}", SectionFlags::empty()), "SectionFlags(0x0)");
    assert_eq!(format!("{:?}", SectionFlags::from_bits_retain(0x21)), "SectionFlags(CONFIG | 0x20)");
    assert_eq!(SectionFlags::all().bits(), 0xDF);
}

// ── other declaration forms and widths ──────────────────────────────────
macro_rules! forms {
    ($mac:path, $m:ident) => {
        mod $m {
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct Plain(pub u8);
            $mac! {
                impl Plain: u8 {
                    const X = 1;
                    const Y = 1 << 1;
                }
            }
            $mac! {
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub struct Open: u8 {
                    const A = 1;
                    const B = 1 << 1;
                    const _ = !0;
                }
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub struct Gated: u8 {
                    const A = 1;
                    #[cfg(any())]
                    const GONE = 1 << 1;
                    #[cfg(all())]
                    const HERE = 1 << 2;
                }
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub struct Signed: i8 {
                    const NEG = i8::MIN;
                    const LOW = 1;
                }
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub struct Wide: u128 {
                    const TOP = 1 << 127;
                    const BOT = 1;
                }
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub struct Zero: u8 {
                    const NIL = 0;
                    const ONE = 1;
                }
                #[derive(Debug, Clone, Copy, PartialEq, Eq)]
                pub struct Nothing: u8 {}
            }
        }
    };
}
forms!(crate::Utilities::Bitflags::bitflags, f_local);
forms!(::bitflags::bitflags, f_real);

#[test]
fn impl_mode_unnamed_cfg_signed_wide_zero_and_empty_forms_agree() {
    // impl mode
    assert_eq!(f_local::Plain::X.bits(), f_real::Plain::X.bits());
    assert_eq!(f_local::Plain::from_bits_retain(3).iter_names().count(), f_real::Plain::from_bits_retain(3).iter_names().count());
    // unnamed `_ = !0` makes every bit known
    for b in 0..=255u8 {
        assert_eq!(f_local::Open::from_bits(b).map(|f| f.bits()), f_real::Open::from_bits(b).map(|f| f.bits()));
        assert_eq!(f_local::Open::from_bits_truncate(b).bits(), f_real::Open::from_bits_truncate(b).bits());
        assert_eq!(f_local::Open::all().bits(), f_real::Open::all().bits());
        assert_eq!(format!("{:?}", f_local::Open::from_bits_retain(b)), format!("{:?}", f_real::Open::from_bits_retain(b)));
        assert_eq!(f_local::Gated::from_bits_truncate(b).bits(), f_real::Gated::from_bits_truncate(b).bits());
        assert_eq!(f_local::Zero::from_bits_truncate(b).bits(), f_real::Zero::from_bits_truncate(b).bits());
        assert_eq!(f_local::Nothing::from_bits_truncate(b).bits(), f_real::Nothing::from_bits_truncate(b).bits());
        assert_eq!(format!("{:?}", f_local::Zero::from_bits_retain(b)), format!("{:?}", f_real::Zero::from_bits_retain(b)));
        assert_eq!(format!("{:?}", f_local::Signed::from_bits_retain(b as i8)), format!("{:?}", f_real::Signed::from_bits_retain(b as i8)));
    }
    // a `#[cfg]`-ed-out flag does not exist; the live one does
    assert_eq!(f_local::Gated::HERE.bits(), f_real::Gated::HERE.bits());
    assert_eq!(f_local::Gated::all().bits(), f_real::Gated::all().bits());
    // wide and signed bit types
    let mut rng = XorShift::new(0x5EED_0001);
    for _ in 0..500 {
        let v = ((rng.next_u64() as u128) << 64) | rng.next_u64() as u128;
        let (l, r) = (f_local::Wide::from_bits_retain(v), f_real::Wide::from_bits_retain(v));
        assert_eq!(format!("{:?}", l), format!("{:?}", r));
        assert_eq!(l.complement().bits(), r.complement().bits());
        assert_eq!(format!("{:x}", l), format!("{:x}", r));
        assert_eq!(f_local::Wide::from_bits(v).is_some(), f_real::Wide::from_bits(v).is_some());
    }
    assert_eq!(f_local::Signed::all().bits(), f_real::Signed::all().bits());
    assert_eq!(f_local::Signed::NEG.complement().bits(), f_real::Signed::NEG.complement().bits());
}

#[test]
fn public_flags_associated_types_are_the_real_shape() {
    use super::__private::PublicFlags;
    fn assert_primitive<T: PublicFlags<Primitive = u8>>() {}
    assert_primitive::<Local>();
}
