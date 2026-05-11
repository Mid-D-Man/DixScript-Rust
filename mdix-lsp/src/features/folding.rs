// mdix-lsp/src/features/folding.rs
//! Folding provider.
//!
//! ## Fold regions produced
//!
//! 1. **Enum declaration bodies** — `{ … }` depth-tracked within @ENUMS
//! 2. **QuickFunc bodies** — body fold from `{` to before `}`; when params
//!    span multiple lines an additional param fold is emitted from `~` to
//!    the line before `{`.
//! 3. **Inline object literals** — `{ … }` depth-tracked within @DATA / @SECURITY.
//! 4. **Table properties / group arrays** — token-based: each `:` or `::`
//!    in @DATA opens a fold that runs to just before the next `:` / `::`,
//!    or to just before the section's closing `)`.
//!
//! Section-level folds (@DATA, @ENUMS, …) are intentionally omitted.

use std::panic;

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};

use dixscript::Compiler::AST::{
    AstVisitorBase, DataSection, EnumDeclaration,
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

    if doc.tokens.is_empty() {
        return None;
    }

    // AST-driven folds: enum bodies, quickfunc bodies, object literals
    if let Some(ast) = &doc.ast {
        let mut visitor = FoldingVisitor {
            tokens: &doc.tokens,
            ranges: &mut ranges,
        };
        visitor.visit(ast);
    }

    // Token-based folds for table properties and group arrays
    collect_data_table_group_folds(&doc.tokens, &mut ranges);

    // Finalise
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

// ── Table property / group array folds (token-based) ─────────────────────────
//
// Object property folds work by tracking { / } tokens.
// Table and group entries have no braces — instead they are delimited by:
//   Symbol(':')   → table property  (server.config: host = "x", port = 8080)
//   DoubleColon   → group array     (tags:: "alpha", "beta")
//
// Strategy: collect all such tokens in the DATA section in source order.
// Each one starts a fold that ends just before the next delimiter line,
// or just before the section's closing `)`.

fn collect_data_table_group_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let section_end = section_last_lsp(tokens, SectionId::Data);

    // Collect the LSP line of every : or :: token inside @DATA, in order.
    let mut starts: Vec<u32> = tokens
        .iter()
        .filter(|t| t.section == SectionId::Data)
        .filter(|t| matches!(&t.token_type, TokenType::Symbol(':') | TokenType::DoubleColon))
        .map(tok_lsp_line)
        .collect();

    // Remove consecutive duplicates (both : and :: on the same line is unusual
    // but dedup keeps the first occurrence, which is fine).
    starts.dedup();

    for (i, &start) in starts.iter().enumerate() {
        let end = if i + 1 < starts.len() {
            // End just before the next entry's delimiter line.
            starts[i + 1].saturating_sub(1)
        } else {
            // Last entry: end just before the section's closing `)`.
            section_end
                .map(|l| l.saturating_sub(1))
                .unwrap_or(start)
        };

        if end > start {
            ranges.push(make_fold(start, end));
        }
    }
}

// ── AST Visitor ───────────────────────────────────────────────────────────────

struct FoldingVisitor<'a> {
    tokens: &'a [Token],
    ranges: &'a mut Vec<FoldingRange>,
}

impl<'a> FoldingVisitor<'a> {
    // ── QuickFunc folds ───────────────────────────────────────────────────────

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
}

impl<'a> AstVisitorBase for FoldingVisitor<'a> {
    type Result = ();
    fn default_result(&self) -> () {}

    // ── Enum declaration bodies ───────────────────────────────────────────────
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

    // ── QuickFunc bodies ──────────────────────────────────────────────────────
    fn visit_quick_function(&mut self, func: &QuickFunction) -> () {
        self.add_quickfunc_folds(func);
    }

    // ── DATA: inline object literal folds only ────────────────────────────────
    // Table/group entry folds are handled separately by
    // collect_data_table_group_folds (token-based, not AST-based).
    fn visit_data_section(&mut self, section: &DataSection) -> () {
        if !section.position.is_valid() { return; }

        let data_start_lsp = ast_lsp_line(section.position.line);
        let data_end_lsp   = section_last_lsp(self.tokens, SectionId::Data);

        // Brace folds for every `{ … }` object literal within @DATA.
        collect_brace_folds_in_range(
            self.tokens,
            data_start_lsp + 1,
            data_end_lsp,
            self.ranges,
        );
    }

    // ── SECURITY: inline object folds ────────────────────────────────────────
    fn visit_security_section(&mut self, section: &SecuritySection) -> () {
        if !section.position.is_valid() { return; }
        let sec_start = ast_lsp_line(section.position.line);
        let sec_end   = section_last_lsp(self.tokens, SectionId::Security);
        collect_brace_folds_in_range(self.tokens, sec_start + 1, sec_end, self.ranges);
    }
}

// ── Brace-close finder ────────────────────────────────────────────────────────

fn find_brace_close(tokens: &[Token], from_lsp: u32) -> Option<u32> {
    let from_1based = from_lsp + 1;
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

#[inline]
fn tok_lsp_line(tok: &Token) -> u32 {
    tok.line.saturating_sub(1) as u32
}

#[inline]
fn ast_lsp_line(ast_line_1based: usize) -> u32 {
    ast_line_1based.saturating_sub(1) as u32
}

/// Return the 0-based LSP line of the last non-EOF token with the given SectionId.
fn section_last_lsp(tokens: &[Token], id: SectionId) -> Option<u32> {
    tokens
        .iter()
        .rev()
        .find(|t| t.section == id && !matches!(t.token_type, TokenType::EndOfFile))
        .map(tok_lsp_line)
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

    // ── Enum body folds ───────────────────────────────────────────────────────

    #[test]
    fn multiline_enum_body_folds_with_visible_closing_brace() {
        let src = "@ENUMS(\n  ServerType {\n    DEV = 1,\n    PROD = 2\n  }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Enum body: start=1, `}` at line 4 → end=3
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
        let src = "@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // `~f<int>(x) {` is line 1. `}` at line 3 → body end = 2.
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 2),
            "single-line param function body fold missing: {:?}",
            folds
        );
    }

    #[test]
    fn multiline_params_produce_two_folds() {
        let src = concat!(
            "@QUICKFUNCS(\n",         // 0
            "  ~f<int>(\n",           // 1  ← func_start (param fold start)
            "    x<int>,\n",          // 2
            "    y<int>\n",           // 3
            "  ) {\n",                // 4  ← brace_open (body fold start)
            "    return x\n",         // 5
            "  }\n",                  // 6  ← close
            ")\n"                     // 7
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Param fold: start=1, end=3
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "param fold (1→3) missing: {:?}",
            folds
        );
        // Body fold: start=4, end=5
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line == 5),
            "body fold (4→5) missing: {:?}",
            folds
        );
    }

    #[test]
    fn else_branch_does_not_truncate_function_fold() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~check<int>(x) {\n",    // 1
            "    if: x > 0 {\n",       // 2
            "      return 1\n",        // 3
            "    } else {\n",          // 4
            "      return 0\n",        // 5
            "    }\n",                 // 6
            "  }\n",                   // 7
            ")\n"
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
            "  server.config:\n",    // 1  ← : token here, fold starts
            "    host = \"x\"\n",    // 2
            "    port = 8080\n",     // 3
            ")\n"                    // 4
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 2),
            "table property fold (start=1) missing: {:?}",
            folds
        );
    }

    #[test]
    fn two_table_entries_fold_independently() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",      // 1  ← first :
            "    host = \"x\"\n", // 2
            "    port = 80\n",  // 3
            "  cache:\n",       // 4  ← second :
            "    host = \"r\"\n", // 5
            "    port = 6379\n",// 6
            ")\n"               // 7
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // First entry: start=1, end=3 (just before line 4)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "first table fold (1→3) missing: {:?}",
            folds
        );
        // Second entry: start=4, end=5 or 6
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line >= 5),
            "second table fold (start=4) missing: {:?}",
            folds
        );
    }

    #[test]
    fn single_line_table_property_produces_no_table_fold() {
        // : is on same line as all content; next delimiter (or section end)
        // is on the immediately following line → end == start, no fold.
        let src = "@DATA(\n  server: host = \"x\", port = 80\n)\n";
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // Any fold that starts on line 1 would be the table; it should not exist.
        assert!(
            !folds.iter().any(|f| f.start_line == 1 && f.end_line == 1),
            "single-line table should not produce an entry fold: {:?}",
            folds
        );
    }

    #[test]
    fn group_array_with_multiple_items_gets_a_fold() {
        let src = concat!(
            "@DATA(\n",
            "  tags::\n",          // 1  ← :: token here, fold starts
            "    \"alpha\"\n",     // 2
            "    \"beta\"\n",      // 3
            ")\n"                  // 4
        );
        let d = doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 2),
            "group array fold (start=1) missing: {:?}",
            folds
        );
    }

    #[test]
    fn table_and_group_in_same_section_fold_independently() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",         // 1  ← :
            "    host = \"x\"\n",  // 2
            "    port = 80\n",     // 3
            "  tags::\n",          // 4  ← ::
            "    \"a\"\n",         // 5
            "    \"b\"\n",         // 6
            ")\n"
        );
        let d = doc(src);
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

    // ── General invariants ────────────────────────────────────────────────────

    #[test]
    fn no_zero_span_folds() {
        let src = concat!(
            "@ENUMS(\n  T{A=0}\n)\n",
            "@DATA(\n",
            "  x=1\n",
            "  srv:\n    host=\"x\"\n",
            "  tags::\n    \"a\"\n    \"b\"\n",
            ")\n"
        );
        let d = doc(src);
        for f in provide(Some(&d)).unwrap_or_default() {
            assert!(f.end_line > f.start_line, "zero-span fold: {:?}", f);
        }
    }
    }
