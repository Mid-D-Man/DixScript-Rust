// mdix-lsp/src/features/folding.rs

use std::panic;

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

use dixscript::Compiler::AST::{
    AstVisitorBase, DataEntry, DataSection, EnumDeclaration,
    QuickFunction, SecuritySection,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;

use crate::document::Document;

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

impl<'a> FoldingVisitor<'a> {
    // ── QuickFunc ─────────────────────────────────────────────────────────────
    //
    // Always produces a body fold. Additionally produces a param fold when
    // the param list spans multiple lines (~ line differs from { line).
    //
    // Uses 1-based token line numbers directly to avoid conversion errors.
    fn add_quickfunc_folds(&mut self, func: &QuickFunction) {
        if !func.position.is_valid() { return; }

        // func.position.line is 1-based (AST).
        let func_line_1based = func.position.line;

        // Find the opening `{` of the function body.
        // Must be in @QUICKFUNCS section at or after the ~ line.
        let open = self
            .tokens
            .iter()
            .enumerate()
            .find(|(_, t)| {
                t.section == SectionId::QuickFuncs
                    && t.line >= func_line_1based
                    && matches!(t.token_type, TokenType::Symbol('{'))
            });

        let (open_idx, open_tok) = match open {
            Some(r) => r,
            None => return,
        };

        // lsp (0-based) line of the opening `{`
        let open_lsp = tok_lsp_line(open_tok);

        // Depth-track from the `{` index (inclusive) to find the matching `}`.
        // We do NOT filter by section here — inner braces in object literals
        // or if/else blocks inside the body must be counted correctly.
        let close_lsp = {
            let mut depth = 0i32;
            let mut found = None;
            for tok in self.tokens.iter().skip(open_idx) {
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
            None => return,
        };

        let func_lsp = func_line_1based.saturating_sub(1) as u32;

        // Body fold: { line → line before } (so `}` remains visible)
        let body_end = close_lsp.saturating_sub(1);
        if body_end > open_lsp {
            self.ranges.push(make_fold(open_lsp, body_end));
        }

        // Param fold: ~ line → line before {
        // Only when params are on different lines from the body open brace.
        if func_lsp < open_lsp {
            let param_end = open_lsp.saturating_sub(1);
            if param_end > func_lsp {
                self.ranges.push(make_fold(func_lsp, param_end));
            }
        }
    }

    // ── Table property / group array ──────────────────────────────────────────
    //
    // Strategy: for each TableProperty/GroupArray entry, find the `:` or `::`
    // token that opens it, then find the last @DATA token that lies strictly
    // below the delimiter line and strictly above the next entry (or section
    // close).  Token lines are never blank lines, so trailing blank lines are
    // automatically excluded.  Single-line entries have no tokens below the
    // delimiter → no fold.
    fn add_data_entry_folds(&mut self, section: &DataSection) {
        let entries = &section.entries;
        let sec_close_lsp = section_last_token_lsp(self.tokens, SectionId::Data);

        for (i, entry) in entries.iter().enumerate() {
            let entry_pos = match entry {
                DataEntry::TableProperty { position, .. }
                | DataEntry::GroupArray   { position, .. } => *position,
                _ => continue,
            };

            if !entry_pos.is_valid() || entry_pos.line == 0 { continue; }

            // 1-based line of the first identifier in this entry (e.g. "server")
            let entry_line_1based = entry_pos.line;

            // Find the `:` or `::` delimiter token at or near the entry line.
            // Allow +1 line tolerance in case the parser positions differ slightly.
            let delim_tok = self
                .tokens
                .iter()
                .find(|t| {
                    t.section == SectionId::Data
                        && t.line >= entry_line_1based
                        && t.line <= entry_line_1based + 1
                        && matches!(
                            t.token_type,
                            TokenType::Symbol(':') | TokenType::DoubleColon
                        )
                });

            let delim_lsp = match delim_tok {
                Some(t) => tok_lsp_line(t),
                None => continue,
            };

            // Upper bound (exclusive): next entry's 1-based line, or section close.
            let bound_1based: Option<usize> = entries
                .get(i + 1)
                .map(|e| e.position())
                .filter(|p| p.is_valid() && p.line > 0)
                .map(|p| p.line);

            // Last DATA token strictly below the delimiter and strictly before
            // the next entry (or section close).  Blank lines have no tokens,
            // so trailing blanks are naturally excluded.
            let end_lsp = self
                .tokens
                .iter()
                .filter(|t| {
                    t.section == SectionId::Data
                        && !matches!(t.token_type, TokenType::EndOfFile)
                        && tok_lsp_line(t) > delim_lsp
                        && match bound_1based {
                            Some(b) => t.line < b,
                            // No next entry — stop before section close token
                            None => sec_close_lsp
                                .map(|sc| tok_lsp_line(t) < sc)
                                .unwrap_or(true),
                        }
                })
                .map(tok_lsp_line)
                .max();

            if let Some(e) = end_lsp {
                if e > delim_lsp {
                    self.ranges.push(make_fold(delim_lsp, e));
                }
            }
        }
    }
}

impl<'a> AstVisitorBase for FoldingVisitor<'a> {
    type Result = ();
    fn default_result(&self) -> () {}

    // ── Enum bodies ───────────────────────────────────────────────────────────
    //
    // Fold end = `}` line (INCLUSIVE) so the closing brace is hidden with the
    // members rather than floating on its own line below the collapse point.
    fn visit_enum_declaration(&mut self, decl: &EnumDeclaration) -> () {
        if !decl.position.is_valid() { return; }

        let decl_line_1based = decl.position.line;
        let decl_lsp = decl_line_1based.saturating_sub(1) as u32;

        // Find `{` at or after the declaration line (1-based comparison).
        let open = self.tokens.iter().enumerate().find(|(_, t)| {
            t.section == SectionId::Enums
                && t.line >= decl_line_1based
                && matches!(t.token_type, TokenType::Symbol('{'))
        });

        let (open_idx, _) = match open {
            Some(r) => r,
            None => return,
        };

        // Depth-track to find matching `}`.
        let close_lsp = {
            let mut depth = 0i32;
            let mut found = None;
            for tok in self.tokens.iter().skip(open_idx) {
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
            None => return,
        };

        // Include the `}` in the fold (end = close_lsp, not close_lsp - 1)
        // so the closing brace is hidden along with the members.
        if close_lsp > decl_lsp {
            self.ranges.push(make_fold(decl_lsp, close_lsp));
        }
    }

    fn visit_quick_function(&mut self, func: &QuickFunction) -> () {
        self.add_quickfunc_folds(func);
    }

    // ── DATA section ──────────────────────────────────────────────────────────
    fn visit_data_section(&mut self, section: &DataSection) -> () {
        if !section.position.is_valid() { return; }

        let data_start_lsp = section.position.line.saturating_sub(1) as u32;
        let data_end_lsp   = section_last_token_lsp(self.tokens, SectionId::Data);

        // Object literal { } folds (e.g. weapon = { id = 1, damage = 35 })
        collect_brace_folds_in_range(
            self.tokens,
            data_start_lsp + 1,
            data_end_lsp,
            self.ranges,
        );

        // Table property and group array multi-line folds
        self.add_data_entry_folds(section);
    }

    // ── SECURITY: inline object folds ─────────────────────────────────────────
    fn visit_security_section(&mut self, section: &SecuritySection) -> () {
        if !section.position.is_valid() { return; }
        let sec_start = section.position.line.saturating_sub(1) as u32;
        let sec_end   = section_last_token_lsp(self.tokens, SectionId::Security);
        collect_brace_folds_in_range(self.tokens, sec_start + 1, sec_end, self.ranges);
    }
}

// ── Brace { } fold collector for object literals ──────────────────────────────
//
// Emits a fold for every balanced { } pair whose opening brace falls within
// the 0-based lsp line range (from_lsp, to_lsp).
// End = close_lsp - 1 so the `}` remains visible (standard object fold).
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

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Token's 1-based line → 0-based LSP line.
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

    // ── Sanity ────────────────────────────────────────────────────────────────

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
    fn multiline_enum_folds_and_includes_closing_brace() {
        // `}` should be HIDDEN (included in fold end), not floating below.
        let src = "@ENUMS(\n  AIType {\n    PASSIVE = 0,\n    BOSS = 1\n  }\n)\n";
        //         line:  0      1              2              3             4   5
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Decl at lsp 1, `}` at lsp 4 → fold(1, 4)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 4),
            "enum fold (1→4, `}}` inclusive) missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_line_enum_does_not_fold() {
        let src = "@ENUMS(\n  T { A = 0, B = 1 }\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // decl and `}` are on the same line → close_lsp == decl_lsp → no fold
        assert!(
            !folds.iter().any(|f| f.start_line == 1),
            "single-line enum must not fold: {:?}",
            folds
        );
    }

    // ── QuickFunc ─────────────────────────────────────────────────────────────

    #[test]
    fn single_line_params_gives_body_fold_only() {
        // `~f<int>(x) {` all on one line → only body fold, no param fold.
        let src = "@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n";
        //         lsp:         0             1              2         3   4
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // `{` at lsp 1, `}` at lsp 3 → body fold (1, 2)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 2),
            "body fold (1→2) missing: {:?}",
            folds
        );
        // No zero-span fold
        assert!(
            !folds.iter().any(|f| f.start_line == f.end_line),
            "zero-span fold found: {:?}",
            folds
        );
    }

    #[test]
    fn multiline_params_gives_both_param_and_body_folds() {
        let src = concat!(
            "@QUICKFUNCS(\n",    // lsp 0
            "  ~f<int>(\n",      // lsp 1  ← ~ here = func_lsp
            "    x<int>,\n",     // lsp 2
            "    y<int>\n",      // lsp 3
            "  ) {\n",           // lsp 4  ← { here = open_lsp
            "    return x\n",    // lsp 5
            "  }\n",             // lsp 6  ← } here = close_lsp
            ")\n"                // lsp 7
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        // Param fold: ~ (lsp 1) → line before { (lsp 3)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "param fold (1→3) missing: {:?}",
            folds
        );
        // Body fold: { (lsp 4) → line before } (lsp 5)
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line == 5),
            "body fold (4→5) missing: {:?}",
            folds
        );
    }

    #[test]
    fn else_does_not_break_body_fold() {
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
            "body fold must cover past else: {:?}",
            folds
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
    fn object_literals_fold_with_closing_brace_visible() {
        let src = concat!(
            "@DATA(\n",
            "  a = {\n    x = 1\n  }\n",  // lsp 1-3
            "  b = {\n    y = 2\n  }\n",  // lsp 4-6
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // `}` at lsp 3 → end = 2 (visible `}` below)
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
            "single-line table must not fold: {:?}",
            folds
        );
    }

    #[test]
    fn multiline_table_folds() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",         // lsp 1  ← : here
            "    host = \"x\"\n",  // lsp 2
            "    port = 80\n",     // lsp 3
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 2),
            "table fold (start=1) missing: {:?}",
            folds
        );
    }

    #[test]
    fn table_fold_does_not_include_trailing_blank_line() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",         // lsp 1
            "    host = \"x\"\n",  // lsp 2
            "    port = 80\n",     // lsp 3
            "\n",                  // lsp 4 — blank, no tokens
            ")\n"                  // lsp 5
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        let f = folds.iter().find(|f| f.start_line == 1)
            .expect("table fold missing");
        assert_eq!(f.end_line, 3, "fold must end at last token (lsp 3), not blank: {:?}", folds);
    }

    #[test]
    fn blank_between_tables_not_in_first_fold() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",            // lsp 1
            "    host = \"x\"\n",     // lsp 2
            "    port = 80\n",        // lsp 3
            "\n",                     // lsp 4 — blank
            "  cache:\n",             // lsp 5
            "    port = 6379\n",      // lsp 6
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        let first = folds.iter().find(|f| f.start_line == 1)
            .expect("first table fold missing");
        assert_eq!(first.end_line, 3,
            "first fold must end at lsp 3 (not blank lsp 4): {:?}", folds);

        assert!(
            folds.iter().any(|f| f.start_line == 5 && f.end_line >= 6),
            "second table fold missing: {:?}",
            folds
        );
    }

    #[test]
    fn two_tables_fold_independently() {
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
            "first table fold (1→3) missing: {:?}",
            folds
        );
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line >= 5),
            "second table fold (start=4) missing: {:?}",
            folds
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
            "single-line group array must not fold: {:?}",
            folds
        );
    }

    #[test]
    fn multiline_group_array_folds() {
        let src = concat!(
            "@DATA(\n",
            "  tags::\n",     // lsp 1  ← :: here
            "    \"alpha\"\n", // lsp 2
            "    \"beta\"\n",  // lsp 3
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 2),
            "group array fold (start=1) missing: {:?}",
            folds
        );
    }

    #[test]
    fn table_and_group_in_same_data_section() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",         // lsp 1
            "    host = \"x\"\n",  // lsp 2
            "    port = 80\n",     // lsp 3
            "  tags::\n",          // lsp 4
            "    \"a\"\n",         // lsp 5
            "    \"b\"\n",         // lsp 6
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "table fold (1→3) missing: {:?}",
            folds
        );
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line >= 5),
            "group array fold (start=4) missing: {:?}",
            folds
        );
    }
        }
