// mdix-lsp/src/features/folding.rs
//! Folding provider — single-pass stack-based bracket matching.
//!
//! ## Design
//!
//! One pass over the token stream with three independent stacks (parens,
//! braces, brackets) produces all folds. This is correct by construction:
//!
//!   `@SECTION( ... )`   → paren fold  (covers the whole section)
//!   `EnumName { ... }`  → brace fold  (per enum body)
//!   `~func(...){ ... }` → paren fold for params + brace fold for body
//!   `obj = { ... }`     → brace fold  (each object literal)
//!   `[ ... ]`           → bracket fold (multi-line explicit arrays)
//!   `@CONFIG`           → source-text fold (no tokens available)
//!
//! ## Why the old implementation was replaced
//!
//! The previous code mixed AST-position scanning, per-section token
//! filtering, and overlapping fold strategies. That caused:
//!   - Enum bodies bleeding into @DATA
//!   - QuickFunc bodies eating the next function's content
//!   - Object properties folding into siblings
//!   - Single-section files producing zero folds
//!   - Interpolated-string `{}` confusing the brace counter
//!
//! With a pure bracket stack these problems cannot occur: every opener
//! is matched to its exact closer, regardless of section or depth.
//!
//! ## Interpolated strings
//!
//! `$"Hello {name}"` is a single `InterpolatedString` token. The `{`
//! and `}` inside it are never emitted as `Symbol` tokens, so the brace
//! stack is never disturbed.
//!
//! ## Prefixed constructors
//!
//! `b:(...)`, `r:(...)`, `t:(...)` are each a single token
//! (`BlobConstructor` / `RegexConstructor` / `TupleConstructor`).
//! Their inner parentheses are not separate Symbol tokens.

use std::panic;
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use crate::document::Document;

// ── Public entry point ────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)))
        .unwrap_or_else(|payload| {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("folding panicked: {}", msg);
            None
        })
}

fn provide_inner(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    let doc = doc?;
    let mut ranges: Vec<FoldingRange> = Vec::new();

    // ── @CONFIG fold ──────────────────────────────────────────────────────────
    // @CONFIG is stripped before tokenisation — no tokens exist for it.
    // Use the pre-computed line range stored on the document.
    if let Some((cfg_start, cfg_raw_end)) = doc.config_line_range {
        // Clamp: never let the CONFIG fold overlap the first real section.
        let first_section_line = doc.tokens.iter()
            .filter(|t| t.token_type.is_section_keyword())
            .map(|t| t.line.saturating_sub(1) as u32)
            .min()
            .unwrap_or(u32::MAX);
        let cfg_end = cfg_raw_end.min(first_section_line.saturating_sub(1));
        if cfg_end > cfg_start {
            ranges.push(fold(cfg_start, cfg_end));
        }
    }

    if doc.tokens.is_empty() {
        return if ranges.is_empty() { None } else { Some(ranges) };
    }

    // ── Single-pass stack-based fold detection ────────────────────────────────
    //
    // Three independent stacks. Each entry is the 0-based LSP line of the
    // opening bracket. When we see a closer, pop and emit a fold if the
    // span is at least one line (same-line brackets never fold).
    let mut parens:   Vec<u32> = Vec::new(); //  (  )
    let mut braces:   Vec<u32> = Vec::new(); //  {  }
    let mut brackets: Vec<u32> = Vec::new(); //  [  ]

    for token in &doc.tokens {
        if matches!(token.token_type, TokenType::EndOfFile) {
            break;
        }

        // 1-based token line → 0-based LSP line.
        let line = token.line.saturating_sub(1) as u32;

        match &token.token_type {
            // ── Parens ───────────────────────────────────────────────────────
            TokenType::Symbol('(') => parens.push(line),
            TokenType::Symbol(')') => {
                if let Some(open) = parens.pop() {
                    if line > open {
                        ranges.push(fold(open, line));
                    }
                }
            }

            // ── Braces ───────────────────────────────────────────────────────
            TokenType::Symbol('{') => braces.push(line),
            TokenType::Symbol('}') => {
                if let Some(open) = braces.pop() {
                    if line > open {
                        ranges.push(fold(open, line));
                    }
                }
            }

            // ── Brackets ─────────────────────────────────────────────────────
            TokenType::Symbol('[') => brackets.push(line),
            TokenType::Symbol(']') => {
                if let Some(open) = brackets.pop() {
                    if line > open {
                        ranges.push(fold(open, line));
                    }
                }
            }

            // All other tokens are irrelevant for folding.
            _ => {}
        }
    }

    // ── Finalise ──────────────────────────────────────────────────────────────
    // Sort by start ascending, then by span descending (larger folds first
    // when two folds share a start line). Deduplicate exact (start, end)
    // pairs. Remove any zero-span stragglers.
    ranges.sort_unstable_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then_with(|| b.end_line.cmp(&a.end_line))
    });
    ranges.dedup_by_key(|r| (r.start_line, r.end_line));
    ranges.retain(|r| r.end_line > r.start_line);

    tracing::debug!("folding: {} folds produced", ranges.len());

    if ranges.is_empty() { None } else { Some(ranges) }
}

// ── Constructor helper ────────────────────────────────────────────────────────

#[inline]
fn fold(start: u32, end: u32) -> FoldingRange {
    FoldingRange {
        start_line:      start,
        end_line:        end,
        kind:            Some(FoldingRangeKind::Region),
        start_character: None,
        end_character:   None,
        collapsed_text:  None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::run_pipeline;
    use crate::document::Document;
    use tower_lsp::lsp_types::Url;

    fn doc(src: &str) -> Document {
        let mut d = Document::new(
            Url::parse("file:///test.mdix").unwrap(),
            src.to_string(),
            0,
        );
        run_pipeline(&mut d);
        d
    }

    // ── Sanity ────────────────────────────────────────────────────────────────

    #[test]
    fn no_crash_on_none() {
        assert!(provide(None).is_none());
    }

    #[test]
    fn empty_file_returns_none() {
        let d = doc("");
        assert!(provide(Some(&d)).is_none());
    }

    // ── Single section ────────────────────────────────────────────────────────

    #[test]
    fn single_data_section_folds() {
        let d = doc("@DATA(\n  x = 1\n  y = 2\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(!folds.is_empty(), "single section must produce at least one fold");
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line >= 3),
            "DATA section fold missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_enums_section_folds() {
        let d = doc("@ENUMS(\n  T { A = 0, B = 1 }\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line >= 2),
            "ENUMS section fold missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_quickfuncs_section_folds() {
        let d = doc("@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line >= 4),
            "QUICKFUNCS section fold missing: {:?}",
            folds
        );
    }

    // ── Cross-section isolation ───────────────────────────────────────────────

    #[test]
    fn enums_fold_does_not_bleed_into_data() {
        let src = "@ENUMS(\n  T { A = 0 }\n)\n@DATA(\n  x = 1\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // The ENUMS section fold starts at 0 and must end before @DATA (line 3)
        for f in folds.iter().filter(|f| f.start_line == 0) {
            assert!(
                f.end_line <= 2,
                "ENUMS fold bled into @DATA: {:?}",
                f
            );
        }
    }

    // ── Enum bodies ───────────────────────────────────────────────────────────

    #[test]
    fn multi_line_enum_body_folds() {
        let src = "@ENUMS(\n  ServerType {\n    DEV = 1,\n    PROD = 2\n  }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Body of ServerType: lines 1-4
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 4),
            "enum body fold missing: {:?}",
            folds
        );
        // Section: lines 0-5
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line >= 5),
            "ENUMS section fold missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_line_enum_body_does_not_fold() {
        let src = "@ENUMS(\n  T { A = 0, B = 1 }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // `T { ... }` is all on line 1 — no body fold, only section fold
        assert!(
            !folds.iter().any(|f| f.start_line == 1 && f.end_line == 1),
            "single-line enum body should not fold: {:?}",
            folds
        );
    }

    #[test]
    fn multiple_enum_bodies_independent() {
        let src = concat!(
            "@ENUMS(\n",
            "  A {\n    X = 0\n  }\n",  // lines 1-3
            "  B {\n    Y = 0\n  }\n",  // lines 4-6
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line == 3), "A fold: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4 && f.end_line == 6), "B fold: {:?}", folds);
        // A must not reach B
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line <= 3, "A enum fold bled into B: {:?}", f);
        }
    }

    // ── QuickFunc bodies ──────────────────────────────────────────────────────

    #[test]
    fn quickfunc_bodies_independent() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~a<int>(x) {\n    return x\n  }\n",  // body lines 1-3
            "  ~b<int>(y) {\n    return y\n  }\n",  // body lines 4-6
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line == 3), "~a: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4 && f.end_line == 6), "~b: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line <= 3, "~a fold ate ~b: {:?}", f);
        }
    }

    #[test]
    fn interpolated_string_braces_not_counted() {
        // The { } inside $"..." must NOT affect the brace stack.
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~greet<string>(name) {\n",
            "    return $\"Hello {name}!\"\n",
            "  }\n",
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Body fold: lines 1-3
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "greet body fold missing (interpolated string confused brace stack?): {:?}",
            folds
        );
    }

    // ── Data section object literals ──────────────────────────────────────────

    #[test]
    fn sibling_objects_fold_independently() {
        let src = concat!(
            "@DATA(\n",
            "  a = {\n    x = 1\n  }\n",   // lines 1-3
            "  b = {\n    y = 2\n  }\n",   // lines 4-6
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line == 3), "a: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4 && f.end_line == 6), "b: {:?}", folds);
        // `a` must not eat `b`
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line <= 3, "`a` fold ate `b`: {:?}", f);
        }
    }

    #[test]
    fn nested_objects_fold_correctly() {
        let src = concat!(
            "@DATA(\n",
            "  outer = {\n",          // line 1
            "    inner = {\n",        // line 2
            "      x = 1\n",          // line 3
            "    }\n",                // line 4  → inner fold (2-4)
            "  }\n",                  // line 5  → outer fold (1-5)
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 2 && f.end_line == 4), "inner: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line == 5), "outer: {:?}", folds);
    }

    #[test]
    fn deeply_nested_objects() {
        let src = concat!(
            "@DATA(\n",
            "  a = {\n",              // 1
            "    b = {\n",            // 2
            "      c = {\n",          // 3
            "        x = 1\n",        // 4
            "      }\n",              // 5  c (3-5)
            "    }\n",                // 6  b (2-6)
            "  }\n",                  // 7  a (1-7)
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 3 && f.end_line == 5), "c: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 2 && f.end_line == 6), "b: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line == 7), "a: {:?}", folds);
    }

    // ── Invariants ────────────────────────────────────────────────────────────

    #[test]
    fn no_zero_span_folds() {
        let src = concat!(
            "@ENUMS(\n  T { A = 0 }\n)\n",
            "@DATA(\n  x = 1\n)\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        for f in &folds {
            assert!(f.end_line > f.start_line, "zero-span fold: {:?}", f);
        }
    }

    #[test]
    fn all_sections_fold_in_multi_section_file() {
        let src = concat!(
            "@ENUMS(\n  T { A = 0 }\n)\n",
            "@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n",
            "@DATA(\n  y = 1\n)\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Each section should produce at least one fold.
        // ENUMS starts at line 0, QUICKFUNCS at 3, DATA at 8.
        assert!(folds.iter().any(|f| f.start_line == 0), "ENUMS fold: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 3), "QUICKFUNCS fold: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 8 || f.start_line == 9), "DATA fold: {:?}", folds);
    }
        }
