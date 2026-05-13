// mdix-lsp/src/features/folding.rs
//!
//! Fold regions:
//! 1. Enum bodies         — collect_brace_folds_in_range on @ENUMS (same as DATA objects)
//! 2. QuickFunc bodies    — token-based: ~ → ( → ) → { → }
//! 3. QuickFunc params    — emitted when ~ line differs from { line
//! 4. DATA object literals — collect_brace_folds_in_range
//! 5. Table / group arrays — AST entry positions + token scan, comments excluded
//! 6. SECURITY objects    — collect_brace_folds_in_range
//!
//! Section-level folds intentionally omitted.

use std::panic;

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

use dixscript::Compiler::AST::{
    AstVisitorBase, DataEntry, DataSection, EnumsSection,
    QuickFuncsSection, SecuritySection,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;

use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

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

    if doc.tokens.is_empty() {
        return None;
    }

    if let Some(ast) = &doc.ast {
        let mut visitor = FoldingVisitor {
            tokens: &doc.tokens,
            ranges: &mut ranges,
        };
        visitor.visit(ast);
    }

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

// ── Visitor ───────────────────────────────────────────────────────────────────

struct FoldingVisitor<'a> {
    tokens: &'a [Token],
    ranges: &'a mut Vec<FoldingRange>,
}

impl<'a> AstVisitorBase for FoldingVisitor<'a> {
    type Result = ();
    fn default_result(&self) -> () {}

    // ── @ENUMS: same brace-fold method as DATA object literals ────────────────
    fn visit_enums_section(&mut self, section: &EnumsSection) -> () {
        if !section.position.is_valid() { return; }
        let start_lsp = section.position.line.saturating_sub(1) as u32;
        let end_lsp   = section_last_token_lsp(self.tokens, SectionId::Enums);
        // Start at +1 to skip the @ENUMS( line itself
        collect_brace_folds_in_range(self.tokens, start_lsp + 1, end_lsp, self.ranges);
    }

    // ── @QUICKFUNCS: pure token-based via ~ markers ───────────────────────────
    fn visit_quickfuncs_section(&mut self, _section: &QuickFuncsSection) -> () {
        collect_quickfunc_folds(self.tokens, self.ranges);
    }

    // ── @DATA: object literal { } folds + table/group entry folds ────────────
    fn visit_data_section(&mut self, section: &DataSection) -> () {
        if !section.position.is_valid() { return; }
        let data_start_lsp = section.position.line.saturating_sub(1) as u32;
        let data_end_lsp   = section_last_token_lsp(self.tokens, SectionId::Data);

        collect_brace_folds_in_range(
            self.tokens,
            data_start_lsp + 1,
            data_end_lsp,
            self.ranges,
        );

        collect_data_entry_folds(self.tokens, section, self.ranges);
    }

    // ── @SECURITY: object literal { } folds ───────────────────────────────────
    fn visit_security_section(&mut self, section: &SecuritySection) -> () {
        if !section.position.is_valid() { return; }
        let sec_start = section.position.line.saturating_sub(1) as u32;
        let sec_end   = section_last_token_lsp(self.tokens, SectionId::Security);
        collect_brace_folds_in_range(self.tokens, sec_start + 1, sec_end, self.ranges);
    }
}

// ── QuickFunc fold collection ─────────────────────────────────────────────────
//
// For each `~` token in @QUICKFUNCS:
//   1. Find opening `(` of the parameter list (first `(` after `~`)
//   2. Depth-track `(` / `)` to find the matching `)` closing the param list
//   3. Find first `{` in @QUICKFUNCS after that closing `)`  →  body open
//   4. Depth-track `{` / `}` to find the matching `}`         →  body close
//
// This correctly handles:
//   • Single-line params:  `~f<int>(x) {`   → body fold only
//   • Multi-line params:   `~f<int>(\n  x\n) {`  → param fold + body fold
//   • Scope declarations:  `~f => global(x) {`   → treated same way
//   • Object literals inside body don't confuse param detection because we
//     locate the body `{` only AFTER the param list `)`.

fn collect_quickfunc_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    // Collect all ~ positions in the QuickFuncs section
    let tilde_positions: Vec<(usize, u32)> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t.section == SectionId::QuickFuncs
                && matches!(t.token_type, TokenType::Symbol('~'))
        })
        .map(|(i, t)| (i, tok_lsp_line(t)))
        .collect();

    for (tilde_idx, tilde_lsp) in tilde_positions {
        // ── Step 1: find the opening ( of the parameter list ─────────────────
        let paren_open = tokens
            .iter()
            .enumerate()
            .skip(tilde_idx + 1)
            .find(|(_, t)| {
                t.section == SectionId::QuickFuncs
                    && matches!(t.token_type, TokenType::Symbol('('))
            });

        let (paren_open_idx, _) = match paren_open {
            Some(r) => r,
            None => continue,
        };

        // ── Step 2: depth-track to closing ) of param list ───────────────────
        let paren_close_idx = {
            let mut depth = 0i32;
            let mut found = None;
            for (idx, tok) in tokens.iter().enumerate().skip(paren_open_idx) {
                match &tok.token_type {
                    TokenType::Symbol('(') => depth += 1,
                    TokenType::Symbol(')') => {
                        depth -= 1;
                        if depth == 0 {
                            found = Some(idx);
                            break;
                        }
                    }
                    TokenType::EndOfFile => break,
                    _ => {}
                }
            }
            found
        };

        let paren_close_idx = match paren_close_idx {
            Some(i) => i,
            None => continue,
        };

        // ── Step 3: first { in @QUICKFUNCS after the closing ) ───────────────
        let body_open = tokens
            .iter()
            .enumerate()
            .skip(paren_close_idx + 1)
            .find(|(_, t)| {
                t.section == SectionId::QuickFuncs
                    && matches!(t.token_type, TokenType::Symbol('{'))
            });

        let (body_open_idx, body_open_tok) = match body_open {
            Some(r) => r,
            None => continue,
        };
        let open_lsp = tok_lsp_line(body_open_tok);

        // ── Step 4: depth-track { / } to find the matching } ─────────────────
        let close_lsp = {
            let mut depth = 0i32;
            let mut found = None;
            for tok in tokens.iter().skip(body_open_idx) {
                match &tok.token_type {
                    TokenType::Symbol('{') => depth += 1,
                    TokenType::Symbol('}') => {
                        depth -= 1;
                        if depth == 0 {
                            found = Some(tok_lsp_line(tok));
                            break;
                        }
                    }
                    TokenType::EndOfFile => break,
                    _ => {}
                }
            }
            found
        };

        let close_lsp = match close_lsp {
            Some(l) => l,
            None => continue,
        };

        // ── Body fold: { line → line before } ────────────────────────────────
        let body_end = close_lsp.saturating_sub(1);
        if body_end > open_lsp {
            ranges.push(make_fold(open_lsp, body_end));
        }

        // ── Param fold: ~ line → line before { (only when multiline) ─────────
        if tilde_lsp < open_lsp {
            let param_end = open_lsp.saturating_sub(1);
            if param_end > tilde_lsp {
                ranges.push(make_fold(tilde_lsp, param_end));
            }
        }
    }
}

// ── Table property / group array folds ───────────────────────────────────────
//
// For each TableProperty / GroupArray entry in the AST:
//   • start  = LSP line of the `:` or `::` token at the entry position
//   • end    = LSP line of the last DATA token that is:
//                - strictly below the delimiter line
//                - strictly before the next entry's start line (or section close)
//                - NOT a Comment token (comments directly above a subsequent
//                  entry are excluded so they don't get swallowed by the fold)
//
// Because blank lines have no tokens, trailing blanks are automatically excluded.
// Single-line entries have no tokens below the delimiter line → no fold produced.

fn collect_data_entry_folds(
    tokens:  &[Token],
    section: &DataSection,
    ranges:  &mut Vec<FoldingRange>,
) {
    let entries      = &section.entries;
    let sec_close    = section_last_token_lsp(tokens, SectionId::Data);

    for (i, entry) in entries.iter().enumerate() {
        let entry_pos = match entry {
            DataEntry::TableProperty { position, .. }
            | DataEntry::GroupArray   { position, .. } => *position,
            _ => continue,
        };

        if !entry_pos.is_valid() || entry_pos.line == 0 { continue; }

        let entry_line_1based = entry_pos.line;

        // Locate the : or :: token at or within 1 line of the entry position.
        let delim_tok = tokens.iter().find(|t| {
            t.section == SectionId::Data
                && t.line >= entry_line_1based
                && t.line <= entry_line_1based + 1
                && matches!(t.token_type, TokenType::Symbol(':') | TokenType::DoubleColon)
        });

        let delim_lsp = match delim_tok {
            Some(t) => tok_lsp_line(t),
            None => continue,
        };

        // Upper bound: next entry's 1-based AST line (exclusive).
        let bound_1based: Option<usize> = entries
            .get(i + 1)
            .map(|e| e.position())
            .filter(|p| p.is_valid() && p.line > 0)
            .map(|p| p.line);

        // Last DATA token below the delimiter, before the bound, excluding comments.
        let end_lsp = tokens
            .iter()
            .filter(|t| {
                t.section == SectionId::Data
                    && !matches!(
                        t.token_type,
                        TokenType::EndOfFile | TokenType::Comment(_)
                    )
                    && tok_lsp_line(t) > delim_lsp
                    && match bound_1based {
                        Some(b) => t.line < b,
                        None    => sec_close
                            .map(|sc| tok_lsp_line(t) < sc)
                            .unwrap_or(true),
                    }
            })
            .map(tok_lsp_line)
            .max();

        if let Some(e) = end_lsp {
            if e > delim_lsp {
                ranges.push(make_fold(delim_lsp, e));
            }
        }
    }
}

// ── Brace { } fold collector ──────────────────────────────────────────────────
//
// Used for: @ENUMS enum bodies, @DATA object literals, @SECURITY objects.
// Scans all tokens in the given 0-based LSP line range and emits a fold for
// every balanced { } pair found.
// End = close_lsp - 1  so the closing `}` remains visible below the fold.
// Single-line pairs (open and close on same line) produce no fold.

fn collect_brace_folds_in_range(
    tokens:   &[Token],
    from_lsp: u32,
    to_lsp:   Option<u32>,
    ranges:   &mut Vec<FoldingRange>,
) {
    let from_1based = from_lsp + 1;
    let to_1based   = to_lsp.map(|l| l + 1).unwrap_or(u32::MAX);
    let mut stack: Vec<u32> = Vec::new();

    for tok in tokens {
        let tl = tok.line as u32;
        if tl < from_1based { continue; }
        if tl > to_1based   { break;    }
        match &tok.token_type {
            TokenType::EndOfFile => break,
            TokenType::Symbol('{') => stack.push(tok_lsp_line(tok)),
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Token 1-based line → 0-based LSP line.
#[inline]
fn tok_lsp_line(tok: &Token) -> u32 {
    tok.line.saturating_sub(1) as u32
}

/// LSP line of the last non-EOF token with the given SectionId.
fn section_last_token_lsp(tokens: &[Token], id: SectionId) -> Option<u32> {
    tokens
        .iter()
        .rev()
        .find(|t| t.section == id && !matches!(t.token_type, TokenType::EndOfFile))
        .map(tok_lsp_line)
}

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

    fn make_doc(src: &str) -> Document {
        let mut d = Document::new(
            Url::parse("file:///test.mdix").unwrap(),
            src.to_string(),
            0,
        );
        run_pipeline(&mut d);
        d
    }

    #[test]
    fn no_crash_on_none() {
        assert!(provide(None).is_none());
    }

    #[test]
    fn no_zero_span_folds() {
        let src = concat!(
            "@ENUMS(\n  T { A=0, B=1 }\n)\n",
            "@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n",
            "@DATA(\n  x=1\n  srv:\n    host=\"x\"\n  tags::\n    \"a\"\n    \"b\"\n)\n"
        );
        let d = make_doc(src);
        for f in provide(Some(&d)).unwrap_or_default() {
            assert!(f.end_line > f.start_line, "zero-span fold: {:?}", f);
        }
    }

    // ── Enum ──────────────────────────────────────────────────────────────────

    #[test]
    fn single_line_enum_does_not_fold() {
        let src = "@ENUMS(\n  T { A = 0, B = 1 }\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // { and } on same line → no fold
        assert!(
            !folds.iter().any(|f| f.start_line == 1),
            "single-line enum must not fold: {:?}", folds
        );
    }

    #[test]
    fn multiline_enum_folds() {
        // { on line 1 (0-based), } on line 4
        let src = "@ENUMS(\n  AIType {\n    PASSIVE = 0,\n    BOSS = 1\n  }\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // fold(1, 3): { at lsp1, } at lsp4 → end = 4-1 = 3
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "enum fold (1→3) missing: {:?}", folds
        );
    }

    #[test]
    fn enum_with_brace_on_decl_line_folds_correctly() {
        // { on same line as the name, members on next lines
        let src = "@ENUMS(\n  ServerType {DEVELOPMENT = 1,\n    STAGING = 2,\n    PRODUCTION = 3}\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // { at lsp1, } at lsp4 → fold(1, 3)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "enum with brace on decl line fold (1→3) missing: {:?}", folds
        );
    }

    #[test]
    fn sibling_enums_fold_independently() {
        let src = "@ENUMS(\n  A {\n    X = 0\n  }\n  B {\n    Y = 0\n  }\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1), "enum A missing: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "enum B missing: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line < 4, "enum A ate B: {:?}", f);
        }
    }

    // ── QuickFunc ─────────────────────────────────────────────────────────────

    #[test]
    fn single_line_params_gives_body_fold_only() {
        let src = "@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n";
        //         lsp:         0             1              2         3
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // { at lsp1, } at lsp3 → body fold (1, 2)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 2),
            "body fold (1→2) missing: {:?}", folds
        );
        // ~ and { on same line → no param fold
        assert!(
            !folds.iter().any(|f| f.start_line == 1 && f.end_line == 1),
            "zero-span fold found: {:?}", folds
        );
    }

    #[test]
    fn multiline_params_gives_both_param_and_body_folds() {
        let src = concat!(
            "@QUICKFUNCS(\n",    // lsp 0
            "  ~f<int>(\n",      // lsp 1  ← ~ here
            "    x<int>,\n",     // lsp 2
            "    y<int>\n",      // lsp 3
            "  ) {\n",           // lsp 4  ← { here
            "    return x\n",    // lsp 5
            "  }\n",             // lsp 6  ← } here
            ")\n"                // lsp 7
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        // Param fold: ~ (lsp1) → line before { (lsp3)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "param fold (1→3) missing: {:?}", folds
        );
        // Body fold: { (lsp4) → line before } (lsp5)
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line == 5),
            "body fold (4→5) missing: {:?}", folds
        );
    }

    #[test]
    fn scoped_multiline_params_gives_both_folds() {
        // Mirrors the real pattern: ~func<type> => global(\n  params\n) {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~build<string> => global(\n",  // lsp 1  ← ~ here
            "    host<string>,\n",             // lsp 2
            "    port<int>\n",                 // lsp 3
            "  ) {\n",                         // lsp 4  ← { here
            "    return host\n",               // lsp 5
            "  }\n",                           // lsp 6  ← } here
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "param fold (1→3) missing for scoped func: {:?}", folds
        );
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line == 5),
            "body fold (4→5) missing for scoped func: {:?}", folds
        );
    }

    #[test]
    fn else_branch_does_not_break_body_fold() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~check<int>(x) {\n",  // lsp 1
            "    if: x > 0 {\n",     // lsp 2
            "      return 1\n",      // lsp 3
            "    } else {\n",        // lsp 4
            "      return 0\n",      // lsp 5
            "    }\n",               // lsp 6
            "  }\n",                 // lsp 7
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 6),
            "body fold must cover past else: {:?}", folds
        );
    }

    #[test]
    fn sibling_funcs_fold_independently() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~a<int>(x) {\n    return x\n  }\n",  // lsp 1-3
            "  ~b<int>(y) {\n    return y\n  }\n",  // lsp 4-6
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1), "~a missing: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "~b missing: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line < 4, "~a ate ~b: {:?}", f);
        }
    }

    // ── DATA object literals ──────────────────────────────────────────────────

    #[test]
    fn data_object_literals_fold() {
        let src = concat!(
            "@DATA(\n",
            "  a = {\n    x = 1\n  }\n",  // lsp 1-3
            "  b = {\n    y = 2\n  }\n",  // lsp 4-6
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line == 2),
            "object a fold missing: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4 && f.end_line == 5),
            "object b fold missing: {:?}", folds);
    }

    // ── DATA table properties ─────────────────────────────────────────────────

    #[test]
    fn single_line_table_does_not_fold() {
        let src = "@DATA(\n  server: host = \"x\", port = 80\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.iter().any(|f| f.start_line == 1),
            "single-line table must not fold: {:?}", folds
        );
    }

    #[test]
    fn multiline_table_folds_without_trailing_blank() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",         // lsp 1
            "    host = \"x\"\n",  // lsp 2
            "    port = 80\n",     // lsp 3
            "\n",                  // lsp 4 — blank, no tokens
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        let f = folds.iter().find(|f| f.start_line == 1)
            .expect("table fold missing");
        assert_eq!(f.end_line, 3, "fold must end at last token lsp3, not blank: {:?}", folds);
    }

    #[test]
    fn two_tables_fold_independently_no_blank_between() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",            // lsp 1
            "    host = \"x\"\n",     // lsp 2
            "    port = 80\n",        // lsp 3
            "  cache:\n",             // lsp 4
            "    host = \"r\"\n",     // lsp 5
            "    port = 6379\n",      // lsp 6
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "first table fold (1→3) missing: {:?}", folds
        );
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line >= 5),
            "second table fold (start=4) missing: {:?}", folds
        );
    }

    #[test]
    fn comment_above_next_table_not_included_in_fold() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",         // lsp 1
            "    host = \"x\"\n",  // lsp 2
            "    port = 80\n",     // lsp 3
            "\n",                  // lsp 4 — blank
            "  // cache section\n",// lsp 5 — comment
            "  cache:\n",          // lsp 6
            "    port = 6379\n",   // lsp 7
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        let first = folds.iter().find(|f| f.start_line == 1)
            .expect("first table fold missing");
        // Must end at lsp 3, not at comment lsp 5
        assert_eq!(
            first.end_line, 3,
            "first fold must end at lsp3 (not comment lsp5): {:?}", folds
        );

        assert!(
            folds.iter().any(|f| f.start_line == 6 && f.end_line >= 7),
            "second table fold (start=6) missing: {:?}", folds
        );
    }

    // ── DATA group arrays ─────────────────────────────────────────────────────

    #[test]
    fn single_line_group_array_does_not_fold() {
        let src = "@DATA(\n  tags:: \"a\", \"b\", \"c\"\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            !folds.iter().any(|f| f.start_line == 1),
            "single-line group must not fold: {:?}", folds
        );
    }

    #[test]
    fn multiline_group_array_folds() {
        let src = concat!(
            "@DATA(\n",
            "  tags::\n",      // lsp 1
            "    \"alpha\"\n", // lsp 2
            "    \"beta\"\n",  // lsp 3
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 2),
            "group array fold missing: {:?}", folds
        );
    }

    #[test]
    fn comment_above_group_not_included_in_table_fold() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",         // lsp 1
            "    host = \"x\"\n",  // lsp 2
            "  // items below\n",  // lsp 3 — comment
            "  tags::\n",          // lsp 4
            "    \"a\"\n",         // lsp 5
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        let table_fold = folds.iter().find(|f| f.start_line == 1)
            .expect("table fold missing");
        // Must end at lsp 2, comment at lsp 3 must be excluded
        assert_eq!(
            table_fold.end_line, 2,
            "table fold must end at lsp2 (not comment lsp3): {:?}", folds
        );
    }
    }
