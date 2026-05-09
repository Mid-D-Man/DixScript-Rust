// mdix-lsp/src/features/folding.rs
//! Folding provider — AST-driven with token depth-scanning for end positions.
//!
//! ## Strategy (per the LSP spec article)
//!
//! 1. Walk the AST for foldable nodes — sections, enum bodies, function bodies.
//! 2. Use depth-tracked token scanning to find exact END positions, starting
//!    from each node's AST start position.
//! 3. Apply the "closing brace adjustment": endLine = close_line − 1 so the
//!    closing `}` stays visible below the fold.
//! 4. @CONFIG has no tokens; use the pre-computed line range instead.
//!
//! ## Why depth-scanning beats a global token stack
//!
//! A global stack is confused by `} else {` (one line contains both a closer
//! AND an opener), interpolated-string `{}`, and any token the lexer might
//! emit differently than expected.  Starting a fresh depth-scan FROM a known
//! AST position means we always find the correct matching bracket regardless
//! of what came before it.
//!
//! ## Section-close fallback
//!
//! If `@DATA(` is tokenised as a single `SectionData` token (consuming the `(`),
//! there is no `Symbol('(')` in the stream.  `find_section_close` detects this
//! and falls back to "span until the line before the next section keyword or
//! the last token in the file", so section folds are produced regardless of
//! how the lexer handles the opening parenthesis.

use std::panic;
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};
use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::Tokenizer::TokenType;
use crate::document::Document;

// ── Public entry point ────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc))).unwrap_or_else(
        |payload| {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("folding panicked: {}", msg);
            None
        },
    )
}

fn provide_inner(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    let doc = doc?;
    let mut ranges: Vec<FoldingRange> = Vec::new();

    // ── @CONFIG fold ──────────────────────────────────────────────────────────
    // No tokens exist for @CONFIG — it is stripped before tokenisation.
    if let Some((cfg_start, cfg_raw_end)) = doc.config_line_range {
        let first_section_lsp = doc
            .tokens
            .iter()
            .filter(|t| t.token_type.is_section_keyword())
            .map(|t| t.line.saturating_sub(1) as u32)
            .min()
            .unwrap_or(u32::MAX);
        let cfg_end = cfg_raw_end.min(first_section_lsp.saturating_sub(1));
        if cfg_end > cfg_start {
            ranges.push(fold(cfg_start, cfg_end));
        }
    }

    if doc.tokens.is_empty() {
        return if ranges.is_empty() { None } else { Some(ranges) };
    }

    // ── Section-level folds (token stream scan) ───────────────────────────────
    // Scan for every section keyword in the raw token stream.  We do NOT rely
    // on AST positions for section starts — the token position is authoritative
    // and available even if the AST position is UNKNOWN.
    for tok in &doc.tokens {
        if !tok.token_type.is_section_keyword() {
            continue;
        }
        let start_lsp = tok.line.saturating_sub(1) as u32;
        if let Some(close_lsp) = find_section_close(&doc.tokens, start_lsp) {
            // Include the closing `)` in the fold — section collapses to one line.
            if close_lsp > start_lsp {
                ranges.push(fold(start_lsp, close_lsp));
            }
        }
    }

    // ── AST-driven content folds ──────────────────────────────────────────────
    // Use AST node positions for accurate START lines, then depth-scan tokens
    // for exact END lines.
    if let Some(ast) = &doc.ast {
        collect_content_folds(ast, &doc.tokens, &mut ranges);
    }

    // ── Finalise ──────────────────────────────────────────────────────────────
    ranges.sort_unstable_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then_with(|| b.end_line.cmp(&a.end_line))
    });
    ranges.dedup_by_key(|r| (r.start_line, r.end_line));
    ranges.retain(|r| r.end_line > r.start_line);

    tracing::debug!("folding: {} ranges produced", ranges.len());

    if ranges.is_empty() {
        None
    } else {
        Some(ranges)
    }
}

// ── Content fold collection ───────────────────────────────────────────────────

fn collect_content_folds(
    ast: &DixScript,
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
    ranges: &mut Vec<FoldingRange>,
) {
    // ── Enum declaration bodies ───────────────────────────────────────────────
    if let Some(enums_sec) = &ast.enums {
        // Compute the section's last line so we don't bleed into the next section.
        let sec_end_lsp = enums_sec_close_lsp(enums_sec, tokens);

        for decl in &enums_sec.enums {
            if !decl.position.is_valid() {
                continue;
            }
            let start_lsp = (decl.position.line.saturating_sub(1)) as u32;

            // Scan for the opening `{` from this line, then find its matching `}`.
            if let Some(close_lsp) = find_brace_close(tokens, start_lsp, sec_end_lsp) {
                // Closing-brace adjustment: keep `}` visible below the fold.
                if close_lsp > start_lsp {
                    let end_lsp = close_lsp.saturating_sub(1);
                    if end_lsp > start_lsp {
                        ranges.push(fold(start_lsp, end_lsp));
                    }
                }
            }
        }
    }

    // ── QuickFunc function bodies ─────────────────────────────────────────────
    if let Some(qf_sec) = &ast.quick_functions {
        let sec_end_lsp = qf_sec_close_lsp(qf_sec, tokens);

        for func in &qf_sec.functions {
            if !func.position.is_valid() {
                continue;
            }
            let start_lsp = (func.position.line.saturating_sub(1)) as u32;

            // Depth-scan for the function body `{ ... }`.
            // This correctly handles `} else {` inside the body because the
            // else-open `{` increments depth and its close `}` decrements it —
            // only the outermost close brings depth to 0.
            if let Some(close_lsp) = find_brace_close(tokens, start_lsp, sec_end_lsp) {
                if close_lsp > start_lsp {
                    let end_lsp = close_lsp.saturating_sub(1);
                    if end_lsp > start_lsp {
                        ranges.push(fold(start_lsp, end_lsp));
                    }
                }
            }
        }
    }

    // ── Data section object / brace folds ────────────────────────────────────
    if let Some(data_sec) = &ast.data {
        if data_sec.position.is_valid() {
            let data_start_lsp = (data_sec.position.line.saturating_sub(1)) as u32;
            let data_end_lsp = find_section_close(tokens, data_start_lsp);
            // Use a bounded stack WITHIN the data section to fold every
            // multi-line `{ }` (object literals, nested objects, etc.)
            collect_brace_folds_in_range(tokens, data_start_lsp + 1, data_end_lsp, ranges);
        }
    }

    // ── Security section block folds ──────────────────────────────────────────
    if let Some(sec_sec) = &ast.security {
        if sec_sec.position.is_valid() {
            let sec_start_lsp = (sec_sec.position.line.saturating_sub(1)) as u32;
            let sec_end_lsp = find_section_close(tokens, sec_start_lsp);
            collect_brace_folds_in_range(tokens, sec_start_lsp + 1, sec_end_lsp, ranges);
        }
    }
}

// ── Section-close finder ──────────────────────────────────────────────────────

/// Find the 0-based LSP line of the `)` that closes the section whose keyword
/// is on `section_start_lsp`.
///
/// PRIMARY: depth-tracked `(` / `)` scan starting from the section token.
/// FALLBACK: if no `Symbol('(')` exists after the keyword (lexer consumed it
///           as part of the keyword token), span to the line before the next
///           section keyword or to the last token in the file.
fn find_section_close(
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
    section_start_lsp: u32,
) -> Option<u32> {
    let section_line_1based = section_start_lsp + 1;

    // Find the section keyword token at this line.
    let sec_idx = tokens
        .iter()
        .position(|t| t.line == section_line_1based && t.token_type.is_section_keyword())?;

    // PRIMARY: scan for ( ... )
    {
        let mut depth: i32 = 0;
        let mut found_open = false;

        for tok in &tokens[sec_idx..] {
            match &tok.token_type {
                TokenType::EndOfFile => break,
                TokenType::Symbol('(') => {
                    depth += 1;
                    found_open = true;
                }
                TokenType::Symbol(')') if found_open => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(tok.line.saturating_sub(1) as u32);
                    }
                }
                _ => {}
            }
        }

        if found_open {
            // Opened but never closed (syntax error) — no fold.
            return None;
        }
    }

    // FALLBACK: no `Symbol('(')` was found after the section keyword.
    // Span from here to the line just before the next section keyword.
    let next_sec_line_1based = tokens
        .iter()
        .skip(sec_idx + 1)
        .filter(|t| t.token_type.is_section_keyword())
        .map(|t| t.line)
        .min();

    match next_sec_line_1based {
        Some(next_1based) => {
            // One line before the next section (convert: 1-based → 0-based → subtract 1)
            // next_1based is 1-based. The line before it in 0-based = next_1based - 2.
            if next_1based >= 2 {
                Some(next_1based - 2)
            } else {
                None
            }
        }
        None => {
            // This is the last section — use the last non-EOF token's line.
            tokens
                .iter()
                .rev()
                .find(|t| !matches!(t.token_type, TokenType::EndOfFile))
                .map(|t| t.line.saturating_sub(1) as u32)
        }
    }
}

// ── Brace-close finder ────────────────────────────────────────────────────────

/// Starting from `from_lsp` (0-based), depth-scan tokens to find the `}` that
/// matches the FIRST `{` encountered at or after that line.
///
/// `upper_limit_lsp`: optional inclusive upper bound (0-based).  If the opening
/// `{` has not been found by the time we reach this line, we stop.  Once we
/// have started tracking depth, we continue past the limit until we find the
/// matching `}` (handles blocks whose content spills slightly past a sibling's
/// start estimate).
///
/// Returns the 0-based LSP line of the closing `}`, or `None`.
fn find_brace_close(
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
    from_lsp: u32,
    upper_limit_lsp: Option<u32>,
) -> Option<u32> {
    let from_1based = from_lsp + 1;
    let limit_1based = upper_limit_lsp.map(|l| l + 1);

    let mut depth: i32 = 0;
    let mut started = false;

    for tok in tokens {
        let line = tok.line;
        if line < from_1based {
            continue;
        }
        // If we haven't started yet and we're past the limit, stop.
        if !started {
            if let Some(lim) = limit_1based {
                if line > lim {
                    break;
                }
            }
        }

        match &tok.token_type {
            TokenType::EndOfFile => break,
            TokenType::Symbol('{') => {
                depth += 1;
                started = true;
            }
            TokenType::Symbol('}') if started => {
                depth -= 1;
                if depth == 0 {
                    return Some(tok.line.saturating_sub(1) as u32);
                }
            }
            _ => {}
        }
    }
    None
}

// ── Bounded brace-stack (for DATA / SECURITY object literals) ─────────────────

/// Collect all multi-line `{ ... }` folds WITHIN `[from_lsp, to_lsp]`.
/// Uses a stack so nested objects produce independent folds.
/// Applies the closing-brace adjustment (endLine = close − 1).
fn collect_brace_folds_in_range(
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
    from_lsp: u32,
    to_lsp: Option<u32>,
    ranges: &mut Vec<FoldingRange>,
) {
    let from_1based = from_lsp + 1;
    let to_1based = to_lsp.map(|l| l + 1).unwrap_or(u32::MAX);

    let mut stack: Vec<u32> = Vec::new(); // stores 0-based open-brace lines

    for tok in tokens {
        let line_1based = tok.line;
        if line_1based < from_1based {
            continue;
        }
        if line_1based > to_1based {
            break;
        }

        match &tok.token_type {
            TokenType::EndOfFile => break,
            TokenType::Symbol('{') => {
                stack.push(tok.line.saturating_sub(1) as u32);
            }
            TokenType::Symbol('}') => {
                if let Some(open_lsp) = stack.pop() {
                    let close_lsp = tok.line.saturating_sub(1) as u32;
                    if close_lsp > open_lsp {
                        // Keep `}` visible: fold ends at close - 1.
                        let end_lsp = close_lsp.saturating_sub(1);
                        if end_lsp > open_lsp {
                            ranges.push(fold(open_lsp, end_lsp));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// ── Section-end helpers ───────────────────────────────────────────────────────

fn enums_sec_close_lsp(
    sec: &dixscript::Compiler::AST::EnumsSection,
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
) -> Option<u32> {
    if !sec.position.is_valid() {
        return None;
    }
    find_section_close(tokens, (sec.position.line.saturating_sub(1)) as u32)
}

fn qf_sec_close_lsp(
    sec: &dixscript::Compiler::AST::QuickFuncsSection,
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
) -> Option<u32> {
    if !sec.position.is_valid() {
        return None;
    }
    find_section_close(tokens, (sec.position.line.saturating_sub(1)) as u32)
}

// ── Fold constructor ──────────────────────────────────────────────────────────

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

    // ── Single-section sanity ─────────────────────────────────────────────────

    #[test]
    fn single_data_section_folds() {
        let d = doc("@DATA(\n  x = 1\n  y = 2\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.is_empty(),
            "single @DATA section must produce at least one fold"
        );
    }

    #[test]
    fn single_enums_section_folds() {
        let d = doc("@ENUMS(\n  T { A = 0, B = 1 }\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(!folds.is_empty(), "single @ENUMS must fold: {:?}", folds);
    }

    #[test]
    fn single_quickfuncs_section_folds() {
        let d = doc("@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(!folds.is_empty(), "single @QUICKFUNCS must fold: {:?}", folds);
    }

    // ── Cross-section isolation ───────────────────────────────────────────────

    #[test]
    fn enums_fold_does_not_eat_data() {
        let src = "@ENUMS(\n  T { A = 0 }\n)\n@DATA(\n  x = 1\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // The @ENUMS section fold (start=0) must not reach @DATA (start=3).
        for f in folds.iter().filter(|f| f.start_line == 0) {
            assert!(f.end_line <= 2, "ENUMS bled into @DATA: {:?}", f);
        }
    }

    // ── Enum bodies ───────────────────────────────────────────────────────────

    #[test]
    fn multi_line_enum_body_keeps_closing_brace_visible() {
        let src = "@ENUMS(\n  ServerType {\n    DEV = 1,\n    PROD = 2\n  }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Body: starts at line 1 (`ServerType {`), closing `}` at line 4.
        // With the adjustment, endLine = 3 so line 4 (the `}`) stays visible.
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "enum body fold (1→3) missing, got: {:?}",
            folds
        );
    }

    #[test]
    fn single_line_enum_body_not_folded() {
        let src = "@ENUMS(\n  T { A = 0, B = 1 }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Single-line body — no body fold, only section fold.
        assert!(
            !folds.iter().any(|f| f.start_line == 1 && f.end_line == 1),
            "single-line enum body must not produce a fold: {:?}",
            folds
        );
    }

    #[test]
    fn multiple_enum_bodies_independent() {
        let src = concat!(
            "@ENUMS(\n",
            "  A {\n    X = 0\n  }\n",  // body lines 1-3 (end=2 with adjustment)
            "  B {\n    Y = 0\n  }\n",  // body lines 4-6 (end=5 with adjustment)
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1), "A fold: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "B fold: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line <= 3, "A bled into B: {:?}", f);
        }
    }

    // ── QuickFunc bodies ──────────────────────────────────────────────────────

    #[test]
    fn quickfunc_bodies_independent() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~a<int>(x) {\n    return x\n  }\n",
            "  ~b<int>(y) {\n    return y\n  }\n",
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // ~a body starts at line 1, ~b at line 4.
        assert!(folds.iter().any(|f| f.start_line == 1), "~a: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "~b: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line < 4, "~a ate ~b: {:?}", f);
        }
    }

    #[test]
    fn else_block_does_not_truncate_function_fold() {
        // The `} else {` pattern must not stop the function body fold prematurely.
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~check<int>(x) {\n",     // line 1
            "    if: x > 0 {\n",         // line 2
            "      return 1\n",          // line 3
            "    } else {\n",            // line 4  (} then {)
            "      return 0\n",          // line 5
            "    }\n",                   // line 6  (closes else)
            "  }\n",                     // line 7  (closes function body)
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Function body: start=1, close brace at line 7, endLine=6 (adjustment).
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 6),
            "function body fold must reach line 6+, got: {:?}",
            folds
        );
    }

    // ── Data section object folds ─────────────────────────────────────────────

    #[test]
    fn sibling_objects_independent() {
        let src = concat!(
            "@DATA(\n",
            "  a = {\n    x = 1\n  }\n",  // 1-3
            "  b = {\n    y = 2\n  }\n",  // 4-6
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1), "a: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "b: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line < 4, "`a` ate `b`: {:?}", f);
        }
    }

    #[test]
    fn nested_objects_correct() {
        let src = concat!(
            "@DATA(\n",
            "  outer = {\n",      // 1
            "    inner = {\n",    // 2
            "      x = 1\n",      // 3
            "    }\n",            // 4  inner close → fold (2, 3)
            "  }\n",              // 5  outer close → fold (1, 4)
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 2 && f.end_line == 3), "inner: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line == 4), "outer: {:?}", folds);
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
            assert!(f.end_line > f.start_line, "zero-span: {:?}", f);
        }
    }

    #[test]
    fn no_crash_on_none() {
        assert!(provide(None).is_none());
    }
}
