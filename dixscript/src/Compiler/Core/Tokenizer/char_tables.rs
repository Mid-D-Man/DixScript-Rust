//! Compile-time character classification lookup tables for the DixScript lexer.
//!
//! ## Why lookup tables instead of `char` methods?
//!
//! `char::is_alphanumeric()` walks Unicode category tables — correct for
//! general text but wasteful for a format whose identifiers are ASCII-only.
//! A 256-element `[bool; 256]` fits in a single cache line: lookup is one
//! memory read with no branching, no Unicode overhead, no method-call overhead.
//!
//! All tables are evaluated by `const fn` at compile time; runtime cost is zero.
//!
//! ## Note on Double / f64 precision
//!
//! DixScript's `Double` type (and `ScientificNotation`) are genuine IEEE 754
//! 64-bit doubles — `f64` end-to-end from lexer through to `DixValue::Double`.
//! Only literals that carry an explicit `f` / `F` suffix are stored as `f32`
//! (`DixValue::Float`).  The tables here have no impact on numeric precision;
//! they classify only the ASCII byte characters that make up literals.

/// Whitespace bytes: space (0x20), horizontal tab (0x09),
/// carriage return (0x0D), line feed (0x0A).
pub static WHITESPACE: [bool; 256] = {
    let mut t = [false; 256];
    t[0x09] = true; // \t
    t[0x0A] = true; // \n
    t[0x0D] = true; // \r
    t[0x20] = true; // ' '
    t
};

/// Valid *start* characters for a DixScript identifier: `[A-Za-z_]`.
pub static IDENT_START: [bool; 256] = make_ident_start();

/// Valid *continuation* characters for a DixScript identifier: `[A-Za-z0-9_]`.
///
/// Kebab-case hyphens (`-`) are **not** included; the scanner handles them
/// separately so that arithmetic minus in `@QUICKFUNCS` is unambiguous.
pub static IDENT_CONT: [bool; 256] = make_ident_cont();

/// `[A-Za-z_]` — identical to `IDENT_START`; offered as a named alias for
/// call sites that specifically need "alphabetic or underscore" semantics.
pub static ALPHA_UNDERSCORE: [bool; 256] = make_ident_start();

/// ASCII decimal digits: `[0-9]`.
pub static DIGIT: [bool; 256] = make_digit();

/// ASCII hexadecimal digits: `[0-9A-Fa-f]`.
pub static HEX_DIGIT: [bool; 256] = make_hex_digit();

// ── Const constructors ────────────────────────────────────────────────────────
// Pure integer arithmetic — no heap, no iterators — evaluates on any
// stable Rust ≥ 1.70 toolchain (the project's declared MSRV).

const fn make_ident_start() -> [bool; 256] {
    let mut t = [false; 256];
    let mut i = b'A';
    while i <= b'Z' { t[i as usize] = true; i += 1; }
    i = b'a';
    while i <= b'z' { t[i as usize] = true; i += 1; }
    t[b'_' as usize] = true;
    t
}

const fn make_ident_cont() -> [bool; 256] {
    // Start with alpha + underscore, then add digits.
    let mut t = make_ident_start();
    let mut i = b'0';
    while i <= b'9' { t[i as usize] = true; i += 1; }
    t
}

const fn make_digit() -> [bool; 256] {
    let mut t = [false; 256];
    let mut i = b'0';
    while i <= b'9' { t[i as usize] = true; i += 1; }
    t
}

const fn make_hex_digit() -> [bool; 256] {
    let mut t = [false; 256];
    let mut i = b'0';
    while i <= b'9' { t[i as usize] = true; i += 1; }
    i = b'a';
    while i <= b'f' { t[i as usize] = true; i += 1; }
    i = b'A';
    while i <= b'F' { t[i as usize] = true; i += 1; }
    t
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ident_start_covers_alpha_and_underscore() {
        for b in b'A'..=b'Z' { assert!(IDENT_START[b as usize], "missing {}", b as char); }
        for b in b'a'..=b'z' { assert!(IDENT_START[b as usize], "missing {}", b as char); }
        assert!(IDENT_START[b'_' as usize]);
        assert!(!IDENT_START[b'0' as usize]);
        assert!(!IDENT_START[b'-' as usize]);
        assert!(!IDENT_START[b'@' as usize]);
    }

    #[test]
    fn ident_cont_includes_digits_but_not_hyphen() {
        for b in b'0'..=b'9' { assert!(IDENT_CONT[b as usize]); }
        assert!(IDENT_CONT[b'a' as usize]);
        assert!(IDENT_CONT[b'_' as usize]);
        assert!(!IDENT_CONT[b'-' as usize]);
        assert!(!IDENT_CONT[b'.' as usize]);
    }

    #[test]
    fn digit_table_only_decimal() {
        for b in b'0'..=b'9' { assert!(DIGIT[b as usize]); }
        assert!(!DIGIT[b'a' as usize]);
        assert!(!DIGIT[b'A' as usize]);
        assert!(!DIGIT[b' ' as usize]);
    }

    #[test]
    fn hex_digit_covers_all_forms() {
        for b in b'0'..=b'9' { assert!(HEX_DIGIT[b as usize]); }
        for b in b'a'..=b'f' { assert!(HEX_DIGIT[b as usize]); }
        for b in b'A'..=b'F' { assert!(HEX_DIGIT[b as usize]); }
        assert!(!HEX_DIGIT[b'g' as usize]);
        assert!(!HEX_DIGIT[b'G' as usize]);
        assert!(!HEX_DIGIT[b'x' as usize]);
    }

    #[test]
    fn whitespace_table_exactly_four_bytes() {
        assert!(WHITESPACE[b' '  as usize]);
        assert!(WHITESPACE[b'\t' as usize]);
        assert!(WHITESPACE[b'\n' as usize]);
        assert!(WHITESPACE[b'\r' as usize]);
        assert!(!WHITESPACE[b'a' as usize]);
        assert!(!WHITESPACE[0x0B_usize]); // vertical tab — NOT whitespace in DixScript
    }

    #[test]
    fn alpha_underscore_is_alias_for_ident_start() {
        for i in 0..256_usize {
            assert_eq!(ALPHA_UNDERSCORE[i], IDENT_START[i]);
        }
    }
  }
