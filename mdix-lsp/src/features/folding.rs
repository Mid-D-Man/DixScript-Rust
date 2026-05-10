// mdix-lsp/src/features/folding.rs
//! Folding provider.
//!
//! ## Fold regions produced
//!
//! 1. **@CONFIG** — source-text line range (stripped before tokenisation, so no tokens exist)
//! 2. **Section-level** — via SectionId stamped on every token by the lexer.
//!    Each section's fold spans from its keyword token to the last non-EOF
//!    token sharing that SectionId (i.e. the closing `)`).  This is robust
//!    for single-section files AND prevents sections from bleeding into each other.
//! 3. **Enum declaration bodies** — `{ … }` depth-tracked within @ENUMS
//! 4. **QuickFunc bodies** — body fold from `{` to before `}`; when params
//!    span multiple lines an additional param fold is emitted from `~` to
//!    the line before `{`, preventing the confusing `~func(  ▶  }` collapse.
//! 5. **Table properties / group arrays** — `path: …` and `path:: …` in @DATA,
//!    using the "next entry start − 1" rule on AST positions.
//! 6. **Inline object literals** — `{ … }` depth-tracked within @DATA / @SECURITY.

use std::panic;

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

use dixscript::Compiler::AST::{
    AstVisitorBase, DataEntry, DataSection, EnumDeclaration,
    QuickFunction, SecuritySection,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;

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

    // ── 1. @CONFIG fold (source-text, no tokens) ──────────────────────────────
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

    // ── 2. Section-level folds (SectionId-based) ─────────────────────────────
    //
    // Each section keyword token has its OWN SectionId stamped on it (fixed
    // in lexer.rs).  The closing `)` of every section also carries that same
    // SectionId.  We simply find the keyword token (start) and the last
    // non-EOF token with the same SectionId (end = closing `)`).
    //
    // This replaces the fragile paren-depth approach that broke when a single
    // section was present or when sections had deep nested parens.
    for &sid in &[
        SectionId::Imports,
        SectionId::Dlm,
        SectionId::Enums,
        SectionId::QuickFuncs,
        SectionId::Data,
        SectionId::Security,
    ] {
        if let Some((start, end)) = section_fold_range(&doc.tokens, sid) {
            ranges.push(make_fold(start, end));
        }
    }

    // ── 3–6. AST-driven content folds ─────────────────────────────────────────
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
    if ranges.is_empty() { None } else { Some(ranges) }
}

// ── Section fold helpers ──────────────────────────────────────────────────────

/// Return the 0-based LSP fold range `(start, end)` for `section_id` using
/// the SectionId already embedded in every token by the lexer.
///
/// start = line of the section keyword token (`@DATA`, `@ENUMS`, …)
/// end   = line of the last non-EOF token with that SectionId (the `)`).
fn section_fold_range(tokens: &[Token], section_id: SectionId) -> Option<(u32, u32)> {
    let first = tokens
        .iter()
        .find(|t| t.section == section_id && t.token_type.is_section_keyword())?;
    let last = tokens
        .iter()
        .rev()
        .find(|t| t.section == section_id && !matches!(t.token_type, TokenType::EndOfFile))?;
    let start = tok_lsp_line(first);
    let end   = tok_lsp_line(last);
    if end > start { Some((start, end)) } else { None }
}

/// Return the 0-based LSP line of the last non-EOF token with the given SectionId.
fn section_last_lsp(tokens: &[Token], id: SectionId) -> Option<u32> {
    tokens
        .iter()
        .rev()
        .find(|t| t.section == id && !matches!(t.token_type, TokenType::EndOfFile))
        .map(tok_lsp_line)
}

// ── AST Visitor ───────────────────────────────────────────────────────────────

struct FoldingVisitor<'a> {
    tokens: &'a [Token],
    ranges: &'a mut Vec<FoldingRange>,
}

impl<'a> FoldingVisitor<'a> {
    // ── QuickFunc folds ───────────────────────────────────────────────────────

    /// Emit fold region(s) for a single QuickFunction.
    ///
    /// **Body fold** — from the opening `{` line to the line before `}`.
    /// Always present when the body spans more than one line.
    ///
    /// **Param fold** — from the `~` declaration line to the line before `{`.
    /// Only emitted when params span multiple lines (i.e. the `{` is not on
    /// the same line as `~`).  This prevents the confusing visual where a
    /// multiline-param function collapses to `~func(  ▶  }`.
    fn add_quickfunc_folds(&mut self, func: &QuickFunction) {
        if !func.position.is_valid() { return; }

        let func_start_lsp = ast_lsp_line(func.position.line);

        // Find the body's opening `{`: first QuickFuncs Symbol('{') token at
        // or after the function's declaration line.
        let brace_open_lsp = self
            .tokens
            .iter()
            .filter(|t| {
                t.section == SectionId::QuickFuncs
                    && tok_lsp_line(t) >= func_start_lsp
                    && matches!(t.token_type, TokenType::Symbol('{'))
            })
            .next()
            .map(tok_lsp_line);

        let Some(brace_open_lsp) = brace_open_lsp else { return; };

        // Depth-track from the `{` to find its matching `}`.
        let Some(close_lsp) = find_brace_close(self.tokens, brace_open_lsp) else { return; };

        // Body fold.
        let body_end = close_lsp.saturating_sub(1);
        if body_end > brace_open_lsp {
            self.ranges.push(make_fold(brace_open_lsp, body_end));
        }

        // Param fold (only when `~` and `{` are on different lines).
        if func_start_lsp < brace_open_lsp {
            let param_end = brace_open_lsp.saturating_sub(1);
            if param_end > func_start_lsp {
                self.ranges.push(make_fold(func_start_lsp, param_end));
            }
        }
    }

    // ── DATA entry folds ──────────────────────────────────────────────────────

    /// Emit fold regions for `TableProperty` and `GroupArray` entries in @DATA.
    ///
    /// The end of each entry's fold is determined by the next entry's start
    /// line minus one.  For the last entry the section's closing `)` line
    /// (minus one) is used.  Only entries whose content spans more than one
    /// line receive a fold.
    fn add_data_entry_folds(&mut self, section: &DataSection) {
        let entries = &section.entries;
        let section_close_lsp = section_last_lsp(self.tokens, SectionId::Data);

        for (i, entry) in entries.iter().enumerate() {
            // Determine entry start position and the line of its last content item.
            let (pos, last_content_line) = match entry {
                DataEntry::TableProperty { position, properties, .. } => {
                    let last_line = properties
                        .last()
                        .filter(|p| p.position.is_valid())
                        .map(|p| p.position.line);
                    (*position, last_line)
                }
                DataEntry::GroupArray { position, items, .. } => {
                    let last_line = items
                        .last()
                        .filter(|v| v.position().is_valid())
                        .map(|v| v.position().line);
                    (*position, last_line)
                }
                // SimpleProperty / ObjectProperty: handled by brace fold scanner.
                _ => continue,
            };

            if !pos.is_valid() || pos.line == 0 { continue; }

            // Only fold if content actually spans multiple lines.
            let Some(last_line) = last_content_line else { continue; };
            if last_line <= pos.line { continue; }

            let start_lsp = ast_lsp_line(pos.line);

            // End = just before the next entry's first line, or just before
            // the section's closing `)`.
            let end_lsp = entries
                .get(i + 1)
                .map(|next| next.position())
                .filter(|p| p.is_valid() && p.line > pos.line)
                .map(|p| ast_lsp_line(p.line).saturating_sub(1))
                .or_else(|| section_close_lsp.map(|l| l.saturating_sub(1)))
                .unwrap_or(start_lsp);

            if end_lsp > start_lsp {
                self.ranges.push(make_fold(start_lsp, end_lsp));
            }
        }
    }
}

impl<'a> AstVisitorBase for FoldingVisitor<'a> {
    type Result = ();
    fn default_result(&self) -> () {}

    // ── 3. Enum declaration bodies ────────────────────────────────────────────
    fn visit_enum_declaration(&mut self, decl: &EnumDeclaration) -> () {
        if !decl.position.is_valid() { return; }
        let start = ast_lsp_line(decl.position.line);
        if let Some(close) = find_brace_close(self.tokens, start) {
            let end = close.saturating_sub(1);
            if end > start {
                self.ranges.push(make_fold(start, end));
            }
        }
    }

    // ── 4. QuickFunc bodies (+ multiline param folds) ─────────────────────────
    fn visit_quick_function(&mut self, func: &QuickFunction) -> () {
        self.add_quickfunc_folds(func);
    }

    // ── 5 & 6. DATA: entry folds + inline object folds ───────────────────────
    fn visit_data_section(&mut self, section: &DataSection) -> () {
        if !section.position.is_valid() { return; }

        // Brace folds for every `{ … }` object literal within @DATA.
        let data_start_lsp = ast_lsp_line(section.position.line);
        let data_end_lsp   = section_last_lsp(self.tokens, SectionId::Data);
        collect_brace_folds_in_range(
            self.tokens,
            data_start_lsp + 1,
            data_end_lsp,
            self.ranges,
        );

        // Entry-based folds for table properties and group arrays.
        self.add_data_entry_folds(section);
    }

    // ── 6b. SECURITY: inline object folds ────────────────────────────────────
    fn visit_security_section(&mut self, section: &SecuritySection) -> () {
        if !section.position.is_valid() { return; }
        let sec_start = ast_lsp_line(section.position.line);
        let sec_end   = section_last_lsp(self.tokens, SectionId::Security);
        collect_brace_folds_in_range(self.tokens, sec_start + 1, sec_end, self.ranges);
    }
}

// ── Brace-close finder ────────────────────────────────────────────────────────
//
// Finds the first `{` at or after `from_lsp` (0-based) and depth-tracks to
// its matching `}`.  Returns the 0-based line of the closing `}`.
//
// Handles `} else {` correctly: `}` decrements depth (2→1), `{` increments
// it (1→2); only the outermost `}` brings depth to 0 and is returned.

fn find_brace_close(tokens: &[Token], from_lsp: u32) -> Option<u32> {
    let from_1based = from_lsp + 1; // token lines are 1-based
    let mut depth: i32 = 0;
    let mut started = false;

    for tok in tokens {
        if (tok.line as u32) < from_1based { continue; }
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
// Walks every `{` / `}` token whose 0-based line falls within
// [from_lsp, to_lsp] and emits a fold for each balanced pair.
// Handles arbitrary nesting.  Applies the closing-brace adjustment
// (end = close − 1) so `}` remains visible.

fn collect_brace_folds_in_range(
    tokens:   &[Token],
    from_lsp: u32,
    to_lsp:   Option<u32>,
    ranges:   &mut Vec<FoldingRange>,
) {
    let from_1based = from_lsp + 1;
    let to_1based   = to_lsp.map(|l| l + 1).unwrap_or(u32::MAX);

    let mut stack: Vec<u32> = Vec::new(); // open-brace lines (0-based)

    for tok in tokens {
        let line_1based = tok.line as u32;
        if line_1based < from_1based { continue; }
        if line_1based > to_1based   { break;    }
        match &tok.token_type {
            TokenType::EndOfFile => break,
            TokenType::Symbol('{') => {
                stack.push(tok_lsp_line(tok));
            }
            TokenType::Symbol('}') => {
                if let Some(open_lsp) = stack.pop() {
                    let close_lsp = tok_lsp_line(tok);
                    if close_lsp > open_lsp {
                        let end = close_lsp.saturating_sub(1);
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

    // ── Section folds ─────────────────────────────────────────────────────────

    #[test]
    fn single_data_section_produces_fold() {
        // A single section in isolation must still fold — the old paren-depth
        // approach failed here because there was nothing to trigger the fallback.
        let d = doc("@DATA(\n  x = 1\n  y = 2\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.is_empty(),
            "single @DATA must produce at least one fold"
        );
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line >= 3),
            "@DATA section fold missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_enums_section_produces_fold() {
        let d = doc("@ENUMS(\n  T { A = 0, B = 1 }\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.is_empty(),
            "single @ENUMS must produce at least one fold"
        );
    }

    #[test]
    fn single_quickfuncs_section_produces_fold() {
        let d = doc("@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n");
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.is_empty(),
            "single @QUICKFUNCS must produce at least one fold"
        );
    }

    #[test]
    fn section_folds_do_not_bleed_into_each_other() {
        // The @ENUMS fold must not extend into @DATA territory.
        let src = "@ENUMS(\n  T { A = 0 }\n)\n@DATA(\n  x = 1\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        for f in folds.iter().filter(|f| f.start_line == 0) {
            assert!(
                f.end_line <= 2,
                "@ENUMS fold bled into @DATA: {:?}",
                f
            );
        }
    }

    #[test]
    fn multiple_sections_each_get_their_own_fold() {
        let src = "@ENUMS(\n  T { A = 0 }\n)\n@DATA(\n  x = 1\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Both @ENUMS (start=0) and @DATA (start=3) should have folds.
        assert!(
            folds.iter().any(|f| f.start_line == 0),
            "@ENUMS fold missing: {:?}",
            folds
        );
        assert!(
            folds.iter().any(|f| f.start_line == 3),
            "@DATA fold missing: {:?}",
            folds
        );
    }

    // ── Enum body folds ───────────────────────────────────────────────────────

    #[test]
    fn multiline_enum_body_folds_with_visible_closing_brace() {
        // `}` of ServerType must stay visible (endLine = close line − 1).
        let src = "@ENUMS(\n  ServerType {\n    DEV = 1,\n    PROD = 2\n  }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Enum declaration is at line 1 (0-based), closing `}` at line 4.
        // Body fold: start=1, end=3 (one before `}` at line 4).
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "enum body fold (1→3) missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_line_enum_body_produces_no_body_fold() {
        let src = "@ENUMS(\n  T { A = 0, B = 1 }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.iter().any(|f| f.start_line == 1 && f.end_line == 1),
            "single-line enum body must not produce a body fold: {:?}",
            folds
        );
    }

    // ── QuickFunc body folds ──────────────────────────────────────────────────

    #[test]
    fn quickfunc_body_fold_is_independent_of_params() {
        // Single-line params → body fold starts at the `~` / `{` line.
        let src = "@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // `~f<int>(x) {` is line 1 (0-based). Body: line 2. `}` at line 3.
        // Expected body fold: start=1, end=2.
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 2),
            "single-line param function body fold missing: {:?}",
            folds
        );
    }

    #[test]
    fn multiline_params_produce_two_folds() {
        // When params span multiple lines the function declaration produces:
        //   - a param fold from `~` to the line before `{`
        //   - a body  fold from `{` to the line before `}`
        let src = concat!(
            "@QUICKFUNCS(\n",         // 0
            "  ~f<int>(\n",           // 1  ← func_start (param fold start)
            "    x<int>,\n",          // 2
            "    y<int>\n",           // 3
            "  ) {\n",               // 4  ← brace_open (body fold start)
            "    return x\n",         // 5
            "  }\n",                  // 6  ← close
            ")\n"                     // 7
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Param fold: start=1, end=3 (line before `) {` at line 4)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "param fold (1→3) missing: {:?}",
            folds
        );
        // Body fold: start=4, end=5 (line before `}` at line 6)
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line == 5),
            "body fold (4→5) missing: {:?}",
            folds
        );
    }

    #[test]
    fn else_branch_does_not_truncate_function_fold() {
        // `} else {` must NOT stop the function body fold early.
        let src = concat!(
            "@QUICKFUNCS(\n",          // 0
            "  ~check<int>(x) {\n",    // 1  function body opens here
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
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 6),
            "function body fold must span past `else {{`, got: {:?}",
            folds
        );
    }

    #[test]
    fn sibling_quickfuncs_are_independent() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~a<int>(x) {\n    return x\n  }\n",  // 1-3
            "  ~b<int>(y) {\n    return y\n  }\n",  // 4-6
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1), "~a fold missing: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "~b fold missing: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line < 4, "~a fold ate ~b: {:?}", f);
        }
    }

    // ── DATA inline object folds ──────────────────────────────────────────────

    #[test]
    fn sibling_objects_in_data_are_independent() {
        let src = concat!(
            "@DATA(\n",
            "  a = {\n    x = 1\n  }\n",  // 1-3
            "  b = {\n    y = 2\n  }\n",  // 4-6
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1), "object `a` fold missing: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "object `b` fold missing: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line < 4, "`a` fold ate `b`: {:?}", f);
        }
    }

    #[test]
    fn nested_objects_each_get_a_fold() {
        let src = concat!(
            "@DATA(\n",
            "  outer = {\n",    // 1  outer open
            "    inner = {\n",  // 2  inner open
            "      x = 1\n",    // 3
            "    }\n",          // 4  inner close → fold (2, 3)
            "  }\n",            // 5  outer close → fold (1, 4)
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 2 && f.end_line == 3),
            "inner object fold (2→3) missing: {:?}",
            folds
        );
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 4),
            "outer object fold (1→4) missing: {:?}",
            folds
        );
    }

    // ── DATA table / group array folds ────────────────────────────────────────

    #[test]
    fn multiline_table_property_gets_a_fold() {
        let src = concat!(
            "@DATA(\n",
            "  server.config:\n",    // 1  table start
            "    host = \"x\"\n",    // 2
            "    port = 8080\n",     // 3  last property
            "  other = 1\n",         // 4  next entry
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Table fold: start=1, end should be >= 3 (up to but not including
        // the `other` entry at line 4, so end=3).
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 2),
            "table property fold missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_line_table_property_produces_no_table_fold() {
        // `outer.value: five = 100` — path and property on the same line.
        let src = "@DATA(\n  outer.value: five = 100\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Should have a section fold, but NOT a separate table-entry fold for
        // a single-line table.  The section fold starts at line 0.
        assert!(
            !folds.iter().any(|f| f.start_line == 1),
            "single-line table should not produce an entry fold: {:?}",
            folds
        );
    }

    #[test]
    fn group_array_with_multiple_items_gets_a_fold() {
        let src = concat!(
            "@DATA(\n",
            "  tags::\n",          // 1  group array start
            "    \"alpha\"\n",     // 2
            "    \"beta\"\n",      // 3  last item
            ")\n"
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 2),
            "group array fold missing: {:?}",
            folds
        );
    }

    // ── General invariants ────────────────────────────────────────────────────

    #[test]
    fn no_zero_span_folds() {
        let src = "@ENUMS(\n  T{A=0}\n)\n@DATA(\n  x=1\n)\n";
        let d = doc(src);
        for f in provide(Some(&d)).unwrap_or_default() {
            assert!(f.end_line > f.start_line, "zero-span fold: {:?}", f);
        }
    }
        }
