// mdix-lsp/src/features/folding.rs
//! Folding provider.
//!
//! Strategy (verified against lexer.rs):
//!
//!   1. @CONFIG  — source-text range, no tokens.
//!
//!   2. SECTION folds — scan token stream for every section keyword and
//!      depth-track the following `(` … `)`.  The lexer emits `@DATA` as
//!      `SectionData` and the `(` as a SEPARATE `Symbol('(')` token, so
//!      `find_section_close` always finds a matching parenthesis.
//!
//!   3. ENUM BODIES — implement `AstVisitorBase::visit_enum_declaration`:
//!      use the AST position for the start line, then depth-scan tokens
//!      for the first `{` at or after that line and its matching `}`.
//!      Closing-brace adjustment: endLine = close − 1 so `}` stays visible.
//!
//!   4. QUICKFUNC BODIES — same pattern via `visit_quick_function`.
//!      Depth tracking naturally handles `} else {` because:
//!        - `}` closes the if-then block  (depth 2 → 1)
//!        - `{` opens the else block      (depth 1 → 2)
//!        - only the function's own `}` brings depth back to 0.
//!
//!   5. DATA / SECURITY OBJECT LITERALS — bounded brace stack within the
//!      section's line range via `visit_data_section` / `visit_security_section`.
//!      Handles every nesting level of `{ }` without bleeding into siblings.
//!
//! Interpolated strings (`$"Hello {name}"`) are emitted as a single
//! `InterpolatedString` token by the lexer — the inner `{` and `}` are
//! stored as characters in the string value and never appear as Symbol
//! tokens.  The brace scanner is therefore never confused by them.

use std::panic;

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

use dixscript::Compiler::AST::{
    AstVisitorBase, DataSection, DixScript, EnumDeclaration,
    QuickFunction, SecuritySection,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};

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

    // ── 1. @CONFIG fold ───────────────────────────────────────────────────────
    if let Some((cfg_start, cfg_raw_end)) = doc.config_line_range {
        let first_section_lsp = doc
            .tokens
            .iter()
            .filter(|t| t.token_type.is_section_keyword())
            .map(tok_lsp_line)
            .min()
            .unwrap_or(u32::MAX);
        let cfg_end = cfg_raw_end.min(first_section_lsp.saturating_sub(1));
        if cfg_end > cfg_start {
            ranges.push(make_fold(cfg_start, cfg_end));
        }
    }

    if doc.tokens.is_empty() {
        return if ranges.is_empty() { None } else { Some(ranges) };
    }

    // ── 2. Section-level folds ────────────────────────────────────────────────
    // One scan of the token stream; every section keyword produces a fold.
    for tok in &doc.tokens {
        if !tok.token_type.is_section_keyword() {
            continue;
        }
        let start = tok_lsp_line(tok);
        if let Some(end) = find_section_close(&doc.tokens, start) {
            if end > start {
                ranges.push(make_fold(start, end));
            }
        }
    }

    // ── 3–5. AST-driven content folds ─────────────────────────────────────────
    if let Some(ast) = &doc.ast {
        let mut visitor = FoldingVisitor {
            tokens: &doc.tokens,
            ranges: &mut ranges,
        };
        visitor.visit(ast);
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

// ── AST Visitor ───────────────────────────────────────────────────────────────

struct FoldingVisitor<'a> {
    tokens: &'a [Token],
    ranges: &'a mut Vec<FoldingRange>,
}

impl<'a> FoldingVisitor<'a> {
    /// Find the `{` at or after `start_lsp`, depth-scan to its matching `}`,
    /// and push a fold with the closing-brace adjustment (end = close − 1).
    fn push_brace_fold(&mut self, ast_line_1based: usize) {
        if ast_line_1based == 0 {
            return;
        }
        let start = ast_lsp_line(ast_line_1based);
        if let Some(close) = find_brace_close(self.tokens, start) {
            if close > start {
                let end = close.saturating_sub(1); // keep closing `}` visible
                if end > start {
                    self.ranges.push(make_fold(start, end));
                }
            }
        }
    }
}

impl<'a> AstVisitorBase for FoldingVisitor<'a> {
    type Result = ();

    fn default_result(&self) -> () {}

    // ── 3. Enum declaration bodies ────────────────────────────────────────────
    fn visit_enum_declaration(&mut self, decl: &EnumDeclaration) -> () {
        if decl.position.is_valid() {
            self.push_brace_fold(decl.position.line);
        }
        // No need to visit fields — they add nothing for folding.
    }

    // ── 4. QuickFunc function bodies ──────────────────────────────────────────
    fn visit_quick_function(&mut self, func: &QuickFunction) -> () {
        if func.position.is_valid() {
            self.push_brace_fold(func.position.line);
        }
        // Do NOT visit sub-statements; `} else {` is handled by depth-tracking
        // inside `find_brace_close` rather than by the visitor.
    }

    // ── 5a. DATA section — bounded brace scan for all object literals ─────────
    fn visit_data_section(&mut self, section: &DataSection) -> () {
        if !section.position.is_valid() {
            return;
        }
        let data_start = ast_lsp_line(section.position.line);
        let data_end   = find_section_close(self.tokens, data_start);
        // Scan for every `{ }` pair within the DATA section, however deeply
        // nested.  The bounded range prevents bleeding into adjacent sections.
        collect_brace_folds_in_range(self.tokens, data_start + 1, data_end, self.ranges);
        // Do NOT call the base — visiting entries via the visitor path is
        // unnecessary because the bounded brace scan covers everything.
    }

    // ── 5b. SECURITY section ──────────────────────────────────────────────────
    fn visit_security_section(&mut self, section: &SecuritySection) -> () {
        if !section.position.is_valid() {
            return;
        }
        let sec_start = ast_lsp_line(section.position.line);
        let sec_end   = find_section_close(self.tokens, sec_start);
        collect_brace_folds_in_range(self.tokens, sec_start + 1, sec_end, self.ranges);
    }
}

// ── Section-close finder ──────────────────────────────────────────────────────
//
// Confirmed by lexer.rs analysis:
//   `@DATA(` → SectionData token  +  Symbol('(') token  (two separate tokens)
// So there IS always a Symbol('(') after the section keyword in doc.tokens.
//
// FALLBACK: if (for whatever reason) no `(` follows the keyword, span to the
// line before the next section keyword or the last token.

fn find_section_close(tokens: &[Token], section_start_lsp: u32) -> Option<u32> {
    let section_line_1based = section_start_lsp + 1; // convert LSP 0-based → token 1-based

    // Locate the section-keyword token on this line.
    let sec_idx = tokens.iter().position(|t| {
        (t.line as u32) == section_line_1based && t.token_type.is_section_keyword()
    });

    // PRIMARY: depth-track `(` … `)`
    if let Some(idx) = sec_idx {
        let mut depth: i32 = 0;
        let mut found_open = false;

        for tok in &tokens[idx..] {
            match &tok.token_type {
                TokenType::EndOfFile => break,
                TokenType::Symbol('(') => {
                    depth += 1;
                    found_open = true;
                }
                TokenType::Symbol(')') if found_open => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(tok_lsp_line(tok));
                    }
                }
                _ => {}
            }
        }
        // If we found an opener but never closed, no fold.
        if found_open {
            return None;
        }
    }

    // FALLBACK: span to the line before the next section keyword.
    let next_sec_line_1based = tokens
        .iter()
        .filter(|t| {
            (t.line as u32) > section_line_1based && t.token_type.is_section_keyword()
        })
        .map(|t| t.line as u32)
        .min();

    match next_sec_line_1based {
        Some(next_1based) => {
            // Line before next section, converted to 0-based LSP:
            //   next_1based is 1-based → the line before it is next_1based − 1 (still 1-based)
            //   convert to 0-based: next_1based − 1 − 1 = next_1based − 2
            if next_1based >= 2 {
                Some(next_1based - 2)
            } else {
                None
            }
        }
        None => tokens
            .iter()
            .rev()
            .find(|t| !matches!(t.token_type, TokenType::EndOfFile))
            .map(tok_lsp_line),
    }
}

// ── Brace-close finder ────────────────────────────────────────────────────────
//
// Depth-scans from `from_lsp` (0-based) forward.  Finds the FIRST `{` at or
// after `from_lsp` and returns the line of its matching `}`.
//
// Correctly handles `} else {`:
//   the `}` decrements depth  →  depth 2 → 1
//   the `{` increments depth  →  depth 1 → 2
//   neither reaches 0, so the scan continues to the function's own `}`.

fn find_brace_close(tokens: &[Token], from_lsp: u32) -> Option<u32> {
    let from_1based = from_lsp + 1;
    let mut depth: i32 = 0;
    let mut started = false;

    for tok in tokens {
        if (tok.line as u32) < from_1based {
            continue;
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
                    return Some(tok_lsp_line(tok));
                }
            }
            _ => {}
        }
    }
    None
}

// ── Bounded brace-fold collector ──────────────────────────────────────────────
//
// Collects all multi-line `{ … }` folds whose opening `{` falls within
// [from_lsp, to_lsp].  Handles arbitrary nesting.  Applies the closing-brace
// adjustment (end = close − 1) so `}` stays visible.

fn collect_brace_folds_in_range(
    tokens:    &[Token],
    from_lsp:  u32,
    to_lsp:    Option<u32>,
    ranges:    &mut Vec<FoldingRange>,
) {
    let from_1based = from_lsp + 1;
    let to_1based   = to_lsp.map(|l| l + 1).unwrap_or(u32::MAX);

    let mut stack: Vec<u32> = Vec::new(); // 0-based open-brace lines

    for tok in tokens {
        let line_1based = tok.line as u32;
        if line_1based < from_1based {
            continue;
        }
        if line_1based > to_1based {
            break;
        }
        match &tok.token_type {
            TokenType::EndOfFile => break,
            TokenType::Symbol('{') => {
                stack.push(tok_lsp_line(tok));
            }
            TokenType::Symbol('}') => {
                if let Some(open_lsp) = stack.pop() {
                    let close_lsp = tok_lsp_line(tok);
                    if close_lsp > open_lsp {
                        let end = close_lsp.saturating_sub(1); // keep } visible
                        if end > open_lsp {
                            ranges.push(make_fold(open_lsp, end));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// ── Line-conversion helpers ───────────────────────────────────────────────────

/// Token line (1-based) → LSP line (0-based).
#[inline]
fn tok_lsp_line(tok: &Token) -> u32 {
    tok.line.saturating_sub(1) as u32
}

/// AST position line (1-based) → LSP line (0-based).
#[inline]
fn ast_lsp_line(ast_line_1based: usize) -> u32 {
    ast_line_1based.saturating_sub(1) as u32
}

// ── Fold constructor ──────────────────────────────────────────────────────────

#[inline]
fn make_fold(start: u32, end: u32) -> FoldingRange {
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

    #[test]
    fn no_crash_none() {
        assert!(provide(None).is_none());
    }

    #[test]
    fn single_data_section() {
        let d = doc("@DATA(\n  x = 1\n  y = 2\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.is_empty(),
            "single @DATA must produce a fold, got none"
        );
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line >= 3),
            "section fold missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_enums_section() {
        let d = doc("@ENUMS(\n  T { A = 0, B = 1 }\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(!folds.is_empty(), "single @ENUMS must produce a fold");
    }

    #[test]
    fn single_quickfuncs_section() {
        let d = doc("@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.is_empty(),
            "single @QUICKFUNCS must produce a fold"
        );
    }

    #[test]
    fn enums_does_not_eat_data() {
        let src = "@ENUMS(\n  T { A = 0 }\n)\n@DATA(\n  x = 1\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        for f in folds.iter().filter(|f| f.start_line == 0) {
            assert!(
                f.end_line <= 2,
                "ENUMS fold bled into @DATA: {:?}",
                f
            );
        }
    }

    #[test]
    fn multiline_enum_body_visible_brace() {
        // `}` of ServerType must stay visible (endLine < line of `}`)
        let src = "@ENUMS(\n  ServerType {\n    DEV = 1,\n    PROD = 2\n  }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Body: start=1, close brace at line 4 (0-based), endLine should be 3
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "enum body fold (1→3) missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_line_enum_no_body_fold() {
        let src = "@ENUMS(\n  T { A = 0, B = 1 }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.iter().any(|f| f.start_line == 1 && f.end_line == 1),
            "single-line enum body must not fold: {:?}",
            folds
        );
    }

    #[test]
    fn quickfunc_else_does_not_truncate_fold() {
        // `} else {` must NOT stop the function body fold early.
        // The depth tracker: } → depth 2→1, { → depth 1→2, only function's own } → 0.
        let src = concat!(
            "@QUICKFUNCS(\n",          // 0
            "  ~check<int>(x) {\n",    // 1  function body open
            "    if: x > 0 {\n",       // 2
            "      return 1\n",        // 3
            "    } else {\n",          // 4  } closes if-then, { opens else
            "      return 0\n",        // 5
            "    }\n",                 // 6  closes else
            "  }\n",                   // 7  closes function body
            ")\n"                      // 8
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Function body: start=1, close brace at line 7, endLine should be 6
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 6),
            "function body fold must span past `else {{`, got: {:?}",
            folds
        );
    }

    #[test]
    fn sibling_quickfuncs_independent() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~a<int>(x) {\n    return x\n  }\n",  // 1-3
            "  ~b<int>(y) {\n    return y\n  }\n",  // 4-6
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1), "~a: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "~b: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line < 4, "~a fold ate ~b: {:?}", f);
        }
    }

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
    fn nested_objects() {
        let src = concat!(
            "@DATA(\n",
            "  outer = {\n",    // 1
            "    inner = {\n",  // 2
            "      x = 1\n",    // 3
            "    }\n",          // 4  inner close → fold (2, 3)
            "  }\n",            // 5  outer close → fold (1, 4)
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 2 && f.end_line == 3),
            "inner: {:?}",
            folds
        );
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 4),
            "outer: {:?}",
            folds
        );
    }

    #[test]
    fn no_zero_span_folds() {
        let src = "@ENUMS(\n  T{A=0}\n)\n@DATA(\n  x=1\n)\n";
        let d = doc(src);
        for f in provide(Some(&d)).unwrap_or_default() {
            assert!(f.end_line > f.start_line, "zero-span: {:?}", f);
        }
    }
}
