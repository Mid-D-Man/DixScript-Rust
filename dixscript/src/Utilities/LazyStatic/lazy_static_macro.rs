// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/LazyStatic/lazy_static_macro.rs"
// ============================================================================
//! Hand-rolled replacement for the `lazy_static` crate's `lazy_static!` macro.
//!
//! Each `static ref NAME: Type = expr;` expands to a `static` of the crate's own
//! [`Lazy`] type, which fills a `std::sync::OnceLock` on first access. The macro
//! keeps the `lazy_static` call syntax so call sites do not change. `OnceLock` is
//! stable since Rust 1.70, which is why this does not use `std::sync::LazyLock`
//! (1.80).

use std::ops::Deref;
use std::sync::OnceLock;

/// A value computed on first access and then shared by every thread. A `static`
/// holds the empty cell and the function that fills it.
pub(crate) struct Lazy<T> {
    cell: OnceLock<T>,
    init: fn() -> T,
}

impl<T> Lazy<T> {
    /// `init` must not capture anything, so a `|| expr` closure is accepted.
    pub(crate) const fn new(init: fn() -> T) -> Self {
        Lazy { cell: OnceLock::new(), init }
    }
}

impl<T> Deref for Lazy<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.cell.get_or_init(self.init)
    }
}

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
        $vis static $name: $crate::Utilities::LazyStatic::Lazy<$ty> =
            $crate::Utilities::LazyStatic::Lazy::new(|| $init);
        $crate::Utilities::LazyStatic::lazy_static! { $($rest)* }
    };

    // Last item with no trailing semicolon, which the real macro also accepts.
    ($(#[$attr:meta])* $vis:vis static ref $name:ident : $ty:ty = $init:expr) => {
        $(#[$attr])*
        #[allow(dead_code)]
        $vis static $name: $crate::Utilities::LazyStatic::Lazy<$ty> =
            $crate::Utilities::LazyStatic::Lazy::new(|| $init);
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
