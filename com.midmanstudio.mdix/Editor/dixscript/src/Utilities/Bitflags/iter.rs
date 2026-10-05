// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/Bitflags"
// ============================================================================
//! Iterators over the contained flags of a value: [`Iter`], [`IterNames`] and
//! [`IterDefinedNames`].
//!
//! ## Attribution
//! Derived from the `bitflags` crate, v2.10.0
//! (<https://github.com/bitflags/bitflags>), licensed `MIT OR Apache-2.0`;
//! the logic follows upstream's `src/iter.rs`, comments condensed.

use super::traits::{Flag, Flags};

/// An iterator over flags values.
///
/// Yields flags values for contained, defined flags first, with any remaining
/// bits yielded as one final flags value, so `into_iter` / `from_iter`
/// round-trip.
pub struct Iter<B: 'static> {
    inner: IterNames<B>,
    done: bool,
}

impl<B: Flags> Iter<B> {
    pub(crate) fn new(flags: &B) -> Self {
        Iter {
            inner: IterNames::new(flags),
            done: false,
        }
    }
}

impl<B: 'static> Iter<B> {
    /// Used by the `bitflags!` macro.
    #[doc(hidden)]
    pub const fn __private_const_new(flags: &'static [Flag<B>], source: B, remaining: B) -> Self {
        Iter {
            inner: IterNames::__private_const_new(flags, source, remaining),
            done: false,
        }
    }
}

impl<B: Flags> Iterator for Iter<B> {
    type Item = B;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next() {
            Some((_, flag)) => Some(flag),
            None if !self.done => {
                self.done = true;

                // After iterating the valid names, any bits left over come out
                // as one final value, which makes `into_iter` and `from_iter`
                // round-trip.
                if !self.inner.remaining().is_empty() {
                    Some(B::from_bits_retain(self.inner.remaining.bits()))
                } else {
                    None
                }
            }
            None => None,
        }
    }
}

/// An iterator over contained, defined, *named* flags values. Remaining
/// (unnamed or unknown) bits aren't yielded but are available from
/// [`IterNames::remaining`].
pub struct IterNames<B: 'static> {
    flags: &'static [Flag<B>],
    idx: usize,
    source: B,
    remaining: B,
}

impl<B: Flags> IterNames<B> {
    pub(crate) fn new(flags: &B) -> Self {
        IterNames {
            flags: B::FLAGS,
            idx: 0,
            remaining: B::from_bits_retain(flags.bits()),
            source: B::from_bits_retain(flags.bits()),
        }
    }
}

impl<B: 'static> IterNames<B> {
    /// Used by the `bitflags!` macro.
    #[doc(hidden)]
    pub const fn __private_const_new(flags: &'static [Flag<B>], source: B, remaining: B) -> Self {
        IterNames {
            flags,
            idx: 0,
            remaining,
            source,
        }
    }

    /// The bits not yet yielded: unnamed flags and unknown bits.
    pub fn remaining(&self) -> &B {
        &self.remaining
    }
}

impl<B: Flags> Iterator for IterNames<B> {
    type Item = (&'static str, B);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(flag) = self.flags.get(self.idx) {
            // Short-circuit if our state is empty
            if self.remaining.is_empty() {
                return None;
            }

            self.idx += 1;

            // Skip unnamed flags
            if flag.name().is_empty() {
                continue;
            }

            let bits = flag.value().bits();

            // Yield the flag if it is set in the source AND has bits no earlier
            // flag has covered yet. For multi-bit flags that means: partially
            // overlapping flags (0b001 and 0b101) both yield; a flag that fully
            // overlaps earlier ones (a convenience shorthand) does not.
            if self.source.contains(B::from_bits_retain(bits))
                && self.remaining.intersects(B::from_bits_retain(bits))
            {
                self.remaining.remove(B::from_bits_retain(bits));

                return Some((flag.name(), B::from_bits_retain(bits)));
            }
        }

        None
    }
}

/// An iterator over all defined named flags, whether or not a particular flags
/// value contains them.
pub struct IterDefinedNames<B: 'static> {
    flags: &'static [Flag<B>],
    idx: usize,
}

impl<B: Flags> IterDefinedNames<B> {
    pub(crate) fn new() -> Self {
        IterDefinedNames {
            flags: B::FLAGS,
            idx: 0,
        }
    }
}

impl<B: Flags> Iterator for IterDefinedNames<B> {
    type Item = (&'static str, B);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(flag) = self.flags.get(self.idx) {
            self.idx += 1;

            // Only yield named flags
            if flag.is_named() {
                return Some((flag.name(), B::from_bits_retain(flag.value().bits())));
            }
        }

        None
    }
}
