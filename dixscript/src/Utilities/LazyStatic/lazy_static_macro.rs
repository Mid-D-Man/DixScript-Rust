// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/LazyStatic/lazy_static_macro.rs"
// ============================================================================
//! Hand-rolled replacement for the `lazy_static` crate's `lazy_static!`
//! macro, built on `std::sync::LazyLock` (stable since Rust 1.80; this
//! crate's `rust-version` is already 1.85).
//!
//! ## Why this is barely "hand-rolled" at all
//! `LazyLock` already does everything `lazy_static` exists to do -- a
//! `static` whose initializer runs once, on first access, thread-safely.
//! `lazy_static` predates it. This file only exists so the 18 existing
//! `static ref NAME: Type = expr;` items (in 6 `lazy_static!` blocks, one per
//! file) keep their exact current syntax: the eventual call-site sweep changes
//! one `use` line per file, not 18 declarations.
//!
//! ## What the crate actually uses
//! Checked before writing: only the macro itself -- no
//! `lazy_static::initialize()`, no `pub static ref` -- and every block is a
//! plain `static ref NAME: Type = expr;` (sometimes with a doc comment,
//! sometimes with a multi-statement `{ ... }` initializer, sometimes several
//! per block). Visibility and the no-trailing-semicolon last-item form are
//! supported anyway, for parity with the real macro's grammar.
//!
//! ## `#[allow(dead_code)]` on every generated static
//! The real macro wraps each static in a hidden type carrying
//! `#[allow(dead_code)]`, so an unused `static ref` never warned. A bare
//! `LazyLock` static does warn, so omitting this would have made the swap *add*
//! warnings -- and it did, exposing two dead statics in
//! `quickfuncs_section_parser.rs` (`INTERP_METHOD_CALL_RE`, `INTERP_PROPERTY_RE`)
//! that the old macro had been hiding. The attribute keeps the swap behavior-
//! preserving; the two dead statics are recorded in docs/dixscript/utilities.md
//! as a cleanup candidate instead of being deleted here.
//!
//! ## The recursive step is path-based on purpose
//! A block with several items expands one item at a time by calling the macro
//! again. That inner call is written `$crate::Utilities::LazyStatic::lazy_static!`
//! rather than the bare `lazy_static!`, because a bare name only resolves where
//! the caller has imported the macro. One call site invokes it by full path with
//! no import (`quickfuncs_section_parser.rs`), and even a single-item block
//! makes a final empty recursive call.
//!
//! ## One real behavioral difference
//! `lazy_static!` generates a distinct hidden struct per static that derefs
//! to the inner type; `LazyLock<T>` is a single std type doing the same via
//! `Deref`. Every use in the crate is either `NAME.method()` or `&*NAME`,
//! both of which work identically through either. Anything that named the
//! generated type itself would break -- nothing does.
//!
//! ## Verification
//! Wired into all six `lazy_static!` sites and compiled and tested as part of
//! the real crate on Rust 1.85.1 (the crate's declared `rust-version`) against
//! the genuine `std::sync::LazyLock`: this file's four tests pass, as does the
//! rest of the suite. (An earlier version of this note said it could not be
//! compiled at all; that was true only of the sandbox's default rustc 1.75,
//! where `LazyLock` is unstable.)

/// Usage (identical to the real crate's form):
///
/// ```ignore
/// lazy_static! {
///     /// Doc comments and other attributes carry over.
///     static ref NAME: Regex = Regex::new("...").unwrap();
///
///     static ref TABLE: FxHashMap<&'static str, i32> = {
///         let mut m = FxHashMap::default();
///         m.insert("a", 1);
///         m
///     };
/// }
/// ```
macro_rules! lazy_static {
    () => {};

    ($(#[$attr:meta])* $vis:vis static ref $name:ident : $ty:ty = $init:expr; $($rest:tt)*) => {
        $(#[$attr])*
        #[allow(dead_code)]
        $vis static $name: ::std::sync::LazyLock<$ty> =
            ::std::sync::LazyLock::new(|| $init);
        $crate::Utilities::LazyStatic::lazy_static! { $($rest)* }
    };

    // Last item with no trailing semicolon, which the real macro also accepts.
    ($(#[$attr:meta])* $vis:vis static ref $name:ident : $ty:ty = $init:expr) => {
        $(#[$attr])*
        #[allow(dead_code)]
        $vis static $name: ::std::sync::LazyLock<$ty> =
            ::std::sync::LazyLock::new(|| $init);
    };
}

pub(crate) use lazy_static;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static INIT_COUNT: AtomicUsize = AtomicUsize::new(0);

    lazy_static! {
        /// Doc comments must pass through the `$(#[$attr:meta])*` matcher.
        static ref SIMPLE: i32 = 42;

        static ref COUNTED: i32 = {
            INIT_COUNT.fetch_add(1, Ordering::SeqCst);
            7
        };

        static ref TABLE: HashMap<&'static str, i32> = {
            let mut m = HashMap::new();
            m.insert("a", 1);
            m.insert("b", 2);
            m
        };

        static ref LAST_NO_SEMICOLON: &'static str = "tail"
    }

    #[test]
    fn simple_value_derefs() {
        assert_eq!(*SIMPLE, 42);
    }

    #[test]
    fn multi_statement_initializer_block_works() {
        assert_eq!(TABLE.get("a"), Some(&1));
        assert_eq!(TABLE.get("b"), Some(&2));
        assert_eq!(TABLE.get("c"), None);
    }

    #[test]
    fn initializer_runs_exactly_once_however_many_times_it_is_read() {
        assert_eq!(*COUNTED, 7);
        assert_eq!(*COUNTED, 7);
        assert_eq!(*COUNTED, 7);
        assert_eq!(INIT_COUNT.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn last_item_without_trailing_semicolon_is_accepted() {
        assert_eq!(*LAST_NO_SEMICOLON, "tail");
    }
}
