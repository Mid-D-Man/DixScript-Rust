// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Utilities/parser_collection_helper.rs"
// ============================================================================
//! Up-front capacities for the vectors the section parsers build.
//!
//! A parser knows how long its whole token stream is, but not how many tokens the
//! construct it is parsing will use. An estimate taken from the whole stream is close
//! for the one vector that holds a whole section ([`section_estimate`], not capped). For
//! every nested call argument list, array, object and statement block it is far too
//! large, and the parsers make one such vector per construct. Those estimates are
//! capped at [`MAX_RESERVED_ELEMENTS`]. A vector that ends up larger grows by doubling,
//! which costs a handful of reallocations.

/// Largest number of elements any parser reserves up front.
pub const MAX_RESERVED_ELEMENTS: usize = 16;

/// For the vector that holds a whole section's items: `token_count / divisor`, at least
/// `min`, not capped. `token_count` is the section's own size here, so the estimate is
/// close and the reservation happens once per section.
#[inline]
pub fn section_estimate(min: usize, token_count: usize, divisor: usize) -> usize {
    usize::max(min, token_count / divisor)
}

/// For vectors inside a construct: `token_count / divisor`, at least `min`, at most
/// [`MAX_RESERVED_ELEMENTS`].
#[inline]
pub fn bounded_estimate(min: usize, token_count: usize, divisor: usize) -> usize {
    usize::max(min, token_count / divisor).min(MAX_RESERVED_ELEMENTS)
}

/// Estimate properties count from token count
/// Typical ratio: 1 property per 4-6 tokens
#[inline]
pub fn estimate_properties_count(token_count: usize) -> usize {
    bounded_estimate(8, token_count, 5)
}

/// Estimate array items from token count
/// Typical ratio: 1 item per 8-10 tokens
#[inline]
pub fn estimate_array_items_count(token_count: usize) -> usize {
    bounded_estimate(4, token_count, 9)
}

/// Estimate statements count from token count
/// Typical ratio: 1 statement per 10-15 tokens
#[inline]
pub fn estimate_statements_count(token_count: usize) -> usize {
    bounded_estimate(4, token_count, 12)
}

/// Estimate enum fields from token count
/// Typical ratio: 1 field per 2-3 tokens
#[inline]
pub fn estimate_enum_fields_count(token_count: usize) -> usize {
    bounded_estimate(4, token_count, 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimates_have_minimums() {
        assert_eq!(estimate_properties_count(0), 8);
        assert_eq!(estimate_array_items_count(0), 4);
        assert_eq!(estimate_statements_count(0), 4);
        assert_eq!(estimate_enum_fields_count(0), 4);
    }

    #[test]
    fn test_estimates_scale_with_tokens() {
        // 100 tokens
        assert!(estimate_properties_count(100) > 8);
        assert!(estimate_array_items_count(100) > 4);
        assert!(estimate_statements_count(100) > 4);
        assert!(estimate_enum_fields_count(100) > 4);
    }

    #[test]
    fn test_enum_fields_estimate() {
        // 1 field per 2 tokens: 20 tokens -> 10 fields, below the cap
        assert_eq!(estimate_enum_fields_count(20), 10);
        // 50 tokens would be 25 fields, which the cap holds to 16
        assert_eq!(estimate_enum_fields_count(50), MAX_RESERVED_ELEMENTS);
    }

    #[test]
    fn test_section_estimate_is_not_capped() {
        assert_eq!(section_estimate(2, 0, 50), 2);
        assert_eq!(section_estimate(2, 5_000, 50), 100);
        assert!(section_estimate(8, 100_000, 5) > MAX_RESERVED_ELEMENTS);
    }

    #[test]
    fn test_estimates_never_exceed_the_cap() {
        for tokens in [0, 1, 100, 10_000, 1_000_000, usize::MAX] {
            assert!(estimate_properties_count(tokens) <= MAX_RESERVED_ELEMENTS);
            assert!(estimate_array_items_count(tokens) <= MAX_RESERVED_ELEMENTS);
            assert!(estimate_statements_count(tokens) <= MAX_RESERVED_ELEMENTS);
            assert!(estimate_enum_fields_count(tokens) <= MAX_RESERVED_ELEMENTS);
            assert!(bounded_estimate(2, tokens, 50) <= MAX_RESERVED_ELEMENTS);
        }
    }
}
