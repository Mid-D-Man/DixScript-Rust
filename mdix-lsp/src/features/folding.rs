// mdix-lsp/src/features/folding.rs
//!
//! Fold regions:
//! 1. Section folds       — paren-depth from @ keyword to closing )
//! 2. Enum bodies         — collect_brace_folds_in_range on @ENUMS tokens
//! 3. QuickFunc bodies    — token-based: ~ → ( → ) → { → }
//! 4. QuickFunc params    — emitted when ~ line != { line
//! 5. DATA object literals — collect_brace_folds_in_range
//! 6. Table / group arrays — last non-comment DATA token before next entry
//! 7. SECURITY objects    — collect_brace_folds_in_range

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

    // 1. Section-level folds (paren-depth, independent per section)
    collect_section_folds(&doc.tokens, &mut ranges);

    // 2-7. Content folds via AST visitor
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

// ── Section folding ───────────────────────────────────────────────────────────
//
// For each section keyword token (@DATA, @ENUMS, etc.):
//   1. Find its opening `(` (the very next `(` after the keyword)
//   2. Depth-track `(` / `)` to find the section's outermost closing `)`
//   3. Fold from keyword line to closing `)` line
//
// Paren depth correctly handles nested parens inside sections:
//   - @QUICKFUNCS: param lists ~f(x) open/close within depth > 1
//   - @DATA: QuickFunc calls func(args) open/close within depth > 1
//   - All nested parens balance before the section's own `)` brings depth to 0
//
// @CONFIG has no tokens (stripped before tokenisation) — silently skipped.

fn collect_section_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    // Find all section keyword tokens in document order
    let section_keywords: Vec<(usize, u32)> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.token_type.is_section_keyword())
        .map(|(i, t)| (i, tok_lsp_line(t)))
        .collect();

    for (kw_idx, kw_lsp) in section_keywords {
        // The opening `(` should be the very next token after the keyword
        let open_paren = tokens
            .iter()
            .enumerate()
            .skip(kw_idx + 1)
            .find(|(_, t)| matches!(t.token_type, TokenType::Symbol('(')));

        let (open_idx, _) = match open_paren {
            Some(r) => r,
            None => continue,
        };

        // Depth-track ( / ) — find the outermost closing )
        let close_lsp = paren_close_lsp(tokens, open_idx);

        if let Some(close) = close_lsp {
            if close > kw_lsp {
                ranges.push(make_fold(kw_lsp, close));
            }
        }
    }
}

/// Starting at token index `open_idx` (which must be a `(`), depth-track
/// `(` / `)` pairs and return the 0-based LSP line of the outermost closing `)`.
fn paren_close_lsp(tokens: &[Token], open_idx: usize) -> Option<u32> {
    let mut depth = 0i32;
    let mut found = None;
    for tok in tokens.iter().skip(open_idx) {
        match &tok.token_type {
            TokenType::Symbol('(') => depth += 1,
            TokenType::Symbol(')') => {
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
}

// ── AST Visitor ───────────────────────────────────────────────────────────────

struct FoldingVisitor<'a> {
    tokens: &'a [Token],
    ranges: &'a mut Vec<FoldingRange>,
}

impl<'a> AstVisitorBase for FoldingVisitor<'a> {
    type Result = ();
    fn default_result(&self) -> () {}

    // ── @ENUMS ────────────────────────────────────────────────────────────────
    //
    // Same brace-fold method as DATA object literals.
    // Uses the actual SectionEnums TOKEN line (not AST position) for reliability.
    // This correctly folds enums regardless of whether the opening `{` is on
    // the same line as the enum name or on the next line.
    fn visit_enums_section(&mut self, section: &EnumsSection) -> () {
        // Use the token's line rather than AST position — more reliable
        let from_lsp = self
            .tokens
            .iter()
            .find(|t| matches!(t.token_type, TokenType::SectionEnums))
            .map(tok_lsp_line)
            .or_else(|| {
                if section.position.is_valid() {
                    Some(section.position.line.saturating_sub(1) as u32)
                } else {
                    None
                }
            });

        let from_lsp = match from_lsp {
            Some(l) => l,
            None => return,
        };

        let end_lsp = section_last_token_lsp(self.tokens, SectionId::Enums);
        // from_lsp + 1: skip the @ENUMS( line itself
        collect_brace_folds_in_range(self.tokens, from_lsp + 1, end_lsp, self.ranges);
    }

    // ── @QUICKFUNCS ───────────────────────────────────────────────────────────
    fn visit_quickfuncs_section(&mut self, _section: &QuickFuncsSection) -> () {
        collect_quickfunc_folds(self.tokens, self.ranges);
    }

    // ── @DATA ─────────────────────────────────────────────────────────────────
    fn visit_data_section(&mut self, section: &DataSection) -> () {
        if !section.position.is_valid() { return; }

        // Use the SectionData token for a reliable start line
        let data_start_lsp = self
            .tokens
            .iter()
            .find(|t| matches!(t.token_type, TokenType::SectionData))
            .map(tok_lsp_line)
            .unwrap_or_else(|| section.position.line.saturating_sub(1) as u32);

        let data_end_lsp = section_last_token_lsp(self.tokens, SectionId::Data);

        // Object literal { } folds
        collect_brace_folds_in_range(
            self.tokens,
            data_start_lsp + 1,
            data_end_lsp,
            self.ranges,
        );

        // Table property and group array folds
        collect_data_entry_folds(self.tokens, section, self.ranges);
    }

    // ── @SECURITY ─────────────────────────────────────────────────────────────
    fn visit_security_section(&mut self, section: &SecuritySection) -> () {
        if !section.position.is_valid() { return; }

        let sec_start = self
            .tokens
            .iter()
            .find(|t| matches!(t.token_type, TokenType::SectionSecurity))
            .map(tok_lsp_line)
            .unwrap_or_else(|| section.position.line.saturating_sub(1) as u32);

        let sec_end = section_last_token_lsp(self.tokens, SectionId::Security);
        collect_brace_folds_in_range(self.tokens, sec_start + 1, sec_end, self.ranges);
    }
}

// ── QuickFunc fold collection ─────────────────────────────────────────────────
//
// For each `~` token in @QUICKFUNCS:
//   1. Find the first `(` after `~`     → param list open
//   2. Depth-track `(` / `)` to close  → param list close index
//   3. Find first `{` after close       → body open (NO section filter — this
//      is the critical fix; the section tag on `) {` brace can sometimes differ)
//   4. Depth-track `{` / `}` to close  → body close
//
// Emits:
//   • Body fold:  { line → line before }   (always, if body spans > 1 line)
//   • Param fold: ~ line → line before {   (only when ~ and { are on different lines)

fn collect_quickfunc_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
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
        // ── Step 1: first `(` after `~` = param list open ────────────────────
        let paren_open = tokens
            .iter()
            .enumerate()
            .skip(tilde_idx + 1)
            .find(|(_, t)| matches!(t.token_type, TokenType::Symbol('(')));

        let (open_idx, _) = match paren_open {
            Some(r) => r,
            None => continue,
        };

        // ── Step 2: depth-track ( / ) to find param list close ────────────────
        let paren_close_idx = {
            let mut depth = 0i32;
            let mut found = None;
            for (idx, tok) in tokens.iter().enumerate().skip(open_idx) {
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

        // ── Step 3: first `{` after param close = body open ──────────────────
        // IMPORTANT: no section filter here. The `{` in `) {` can occasionally
        // receive a section tag that doesn't match due to lexer ordering. Just
        // take the very next `{` after the `)` — that IS the body open.
        let body_open = tokens
            .iter()
            .enumerate()
            .skip(paren_close_idx + 1)
            .find(|(_, t)| matches!(t.token_type, TokenType::Symbol('{')));

        let (body_open_idx, body_open_tok) = match body_open {
            Some(r) => r,
            None => continue,
        };
        let open_lsp = tok_lsp_line(body_open_tok);

        // ── Step 4: depth-track { / } from body open to find body close ──────
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
// For each TableProperty / GroupArray AST entry:
//   start  = LSP line of the `:` or `::` delimiter token
//   end    = LSP line of the last DATA token that is:
//              - strictly below the delimiter line
//              - strictly before the next entry's start line (or section close)
//              - NOT a Comment token (comments above next entry are excluded)
//
// Blank lines have no tokens so they are automatically excluded from end_lsp.
// Single-line entries produce no tokens below delimiter → no fold.

fn collect_data_entry_folds(
    tokens:  &[Token],
    section: &DataSection,
    ranges:  &mut Vec<FoldingRange>,
) {
    let entries   = &section.entries;
    let sec_close = section_last_token_lsp(tokens, SectionId::Data);

    for (i, entry) in entries.iter().enumerate() {
        let entry_pos = match entry {
            DataEntry::TableProperty { position, .. }
            | DataEntry::GroupArray   { position, .. } => *position,
            _ => continue,
        };

        if !entry_pos.is_valid() || entry_pos.line == 0 { continue; }

        let entry_line_1based = entry_pos.line;

        // Find the : or :: delimiter at or within 1 line of the entry position
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

        // Upper bound: next entry's 1-based AST line (exclusive)
        let bound_1based: Option<usize> = entries
            .get(i + 1)
            .map(|e| e.position())
            .filter(|p| p.is_valid() && p.line > 0)
            .map(|p| p.line);

        // Last DATA token below delimiter, before bound, excluding comments.
        // Comments directly above the next entry are NOT part of this fold.
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
// Scans tokens in the given 0-based LSP line range and emits a fold for every
// balanced { } pair. Used for enum bodies, DATA object literals, SECURITY objects.
// End = close_lsp - 1 so `}` stays visible below the fold.
// Single-line pairs (open and close on same line) produce no fold.

fn collect_brace_folds_in_range(
    tokens:   &[Token],
    from_lsp: u32,
    to_lsp:   Option<u32>,
    ranges:   &mut Vec<FoldingRange>,
) {
    // Convert 0-based LSP lines to 1-based for comparison with token lines
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
                    // Only fold if { and } are on different lines
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

/// 0-based LSP line of the last non-EOF token with the given SectionId.
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

    // ── Section folds ─────────────────────────────────────────────────────────

    #[test]
    fn multiline_data_section_gets_section_fold() {
        let src = "@DATA(\n  x = 1\n  y = 2\n)\n";
        //  lsp:  0          1       2     3
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // @DATA( at lsp 0, ) at lsp 3 → fold(0, 3)
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line == 3),
            "@DATA section fold (0→3) missing: {:?}", folds
        );
    }

    #[test]
    fn multiline_enums_section_gets_section_fold() {
        let src = "@ENUMS(\n  T { A = 0 }\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line >= 2),
            "@ENUMS section fold missing: {:?}", folds
        );
    }

    #[test]
    fn multiline_quickfuncs_section_gets_section_fold() {
        let src = "@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line >= 4),
            "@QUICKFUNCS section fold missing: {:?}", folds
        );
    }

    #[test]
    fn two_sections_fold_independently_no_bleeding() {
        let src = "@ENUMS(\n  T { A = 0 }\n)\n@DATA(\n  x = 1\n)\n";
        //  lsp:  0              1            2   3          4     5
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        // @ENUMS fold must not extend into @DATA territory
        let enums_fold = folds.iter().find(|f| f.start_line == 0);
        assert!(enums_fold.is_some(), "@ENUMS section fold missing: {:?}", folds);
        assert!(
            enums_fold.unwrap().end_line <= 2,
            "@ENUMS fold bled into @DATA: {:?}", enums_fold
        );

        // @DATA fold starts at lsp 3
        assert!(
            folds.iter().any(|f| f.start_line == 3),
            "@DATA section fold missing: {:?}", folds
        );
    }

    #[test]
    fn single_line_section_does_not_fold() {
        let src = "@DLM(DCompressor.gzip)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // ( and ) on same line → close_lsp == kw_lsp → no fold
        assert!(
            !folds.iter().any(|f| f.start_line == 0 && f.end_line == 0),
            "single-line section must not produce zero-span fold: {:?}", folds
        );
    }

    // ── Enum body folds ───────────────────────────────────────────────────────

    #[test]
    fn single_line_enum_body_does_not_fold() {
        let src = "@ENUMS(\n  T { A = 0, B = 1 }\n)\n";
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // { and } on lsp 1 → no body fold for the enum members
        assert!(
            !folds.iter().any(|f| f.start_line == 1 && f.end_line == 1),
            "single-line enum must not produce body fold: {:?}", folds
        );
    }

    #[test]
    fn multiline_enum_body_folds() {
        // { on same line as name, members on next lines
        let src = "@ENUMS(\n  AIType {\n    PASSIVE = 0,\n    BOSS = 1\n  }\n)\n";
        //  lsp:  0              1              2              3            4   5
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // { at lsp 1, } at lsp 4 → fold(1, 3)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "enum body fold (1→3) missing: {:?}", folds
        );
    }

    #[test]
    fn enum_with_first_entry_on_brace_line_folds() {
        // { AND first member on the same line, remaining members below
        let src = "@ENUMS(\n  ServerType { DEV = 1,\n    STAGING = 2,\n    PROD = 3\n  }\n)\n";
        //  lsp:  0                    1                  2               3             4   5
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // { at lsp 1, } at lsp 4 → fold(1, 3)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "enum fold with first-entry-on-brace-line (1→3) missing: {:?}", folds
        );
    }

    #[test]
    fn sibling_enums_fold_independently() {
        let src = "@ENUMS(\n  A {\n    X = 0\n  }\n  B {\n    Y = 0\n  }\n)\n";
        //  lsp:  0        1       2     3   4       5     6   7
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1), "enum A fold missing: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "enum B fold missing: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line < 4, "enum A fold ate enum B: {:?}", f);
        }
    }

    // ── QuickFunc folds ───────────────────────────────────────────────────────

    #[test]
    fn single_line_params_gives_body_fold_only() {
        let src = "@QUICKFUNCS(\n  ~f<int>(x) {\n    return x\n  }\n)\n";
        //  lsp:  0             1              2         3   4
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // { at lsp 1, } at lsp 3 → body fold (1, 2)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 2),
            "body fold (1→2) missing: {:?}", folds
        );
        // ~ and { on same line → no param fold (would be zero-span)
        assert!(
            !folds.iter().any(|f| f.start_line == 1 && f.end_line == 1),
            "zero-span param fold must not appear: {:?}", folds
        );
    }

    #[test]
    fn multiline_params_gives_both_param_and_body_folds() {
        let src = concat!(
            "@QUICKFUNCS(\n",    // lsp 0
            "  ~f<int>(\n",      // lsp 1  ← ~ here (tilde_lsp = 1)
            "    x<int>,\n",     // lsp 2
            "    y<int>\n",      // lsp 3
            "  ) {\n",           // lsp 4  ← { here (open_lsp = 4)
            "    return x\n",    // lsp 5
            "  }\n",             // lsp 6  ← } here (close_lsp = 6)
            ")\n"                // lsp 7
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        // Param fold: ~ (lsp 1) → line before { (lsp 3 = 4-1)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 3),
            "param fold (1→3) missing: {:?}", folds
        );
        // Body fold: { (lsp 4) → line before } (lsp 5 = 6-1)
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line == 5),
            "body fold (4→5) missing: {:?}", folds
        );
    }

    #[test]
    fn scoped_multiline_params_gives_both_folds() {
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
            "scoped param fold (1→3) missing: {:?}", folds
        );
        assert!(
            folds.iter().any(|f| f.start_line == 4 && f.end_line == 5),
            "scoped body fold (4→5) missing: {:?}", folds
        );
    }

    #[test]
    fn many_params_multiline_gives_both_folds() {
        // Mirrors the real pattern from all_datatypes_test.mdix
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~testAll<string> => global(\n", // lsp 1
            "    a<int>,\n",                    // lsp 2
            "    b<float>,\n",                  // lsp 3
            "    c<string>,\n",                 // lsp 4
            "    d<bool>\n",                    // lsp 5
            "  ) {\n",                          // lsp 6  ← {
            "    return \"ok\"\n",              // lsp 7
            "  }\n",                            // lsp 8  ← }
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 5),
            "param fold (1→5) missing: {:?}", folds
        );
        assert!(
            folds.iter().any(|f| f.start_line == 6 && f.end_line == 7),
            "body fold (6→7) missing: {:?}", folds
        );
    }

    #[test]
    fn else_branch_does_not_break_body_fold() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~check<int>(x) {\n",  // lsp 1  ← { (body open)
            "    if: x > 0 {\n",     // lsp 2
            "      return 1\n",      // lsp 3
            "    } else {\n",        // lsp 4  ← } (depth 1→0→1), { (depth 1→2)
            "      return 0\n",      // lsp 5
            "    }\n",               // lsp 6  ← } (depth 2→1)
            "  }\n",                 // lsp 7  ← } (depth 1→0) = body close
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 6),
            "body fold must span past else: {:?}", folds
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
        assert!(folds.iter().any(|f| f.start_line == 1), "~a body fold missing: {:?}", folds);
        assert!(folds.iter().any(|f| f.start_line == 4), "~b body fold missing: {:?}", folds);
        for f in folds.iter().filter(|f| f.start_line == 1) {
            assert!(f.end_line < 4, "~a fold ate ~b: {:?}", f);
        }
    }

    // ── DATA object literals ──────────────────────────────────────────────────

    #[test]
    fn data_object_with_first_member_on_brace_line_folds() {
        // { AND first member on same line — must still fold
        let src = concat!(
            "@DATA(\n",
            "  obj = { name = \"x\",\n",  // lsp 1  ← { here
            "    value = 42\n",            // lsp 2
            "  }\n",                       // lsp 3  ← } here
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        // { at lsp 1, } at lsp 3 → fold(1, 2)
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line == 2),
            "object fold (1→2) missing when first member on brace line: {:?}", folds
        );
    }

    #[test]
    fn sibling_data_objects_fold_independently() {
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

    // ── DATA table / group folds ──────────────────────────────────────────────

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
    fn multiline_table_folds_without_trailing_blank() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",         // lsp 1  ← : here
            "    host = \"x\"\n",  // lsp 2
            "    port = 80\n",     // lsp 3
            "\n",                  // lsp 4  blank — no tokens
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        let f = folds.iter().find(|f| f.start_line == 1)
            .expect("table fold missing");
        assert_eq!(f.end_line, 3,
            "fold must end at last token (lsp3), not blank (lsp4): {:?}", folds);
    }

    #[test]
    fn multiline_group_array_folds() {
        let src = concat!(
            "@DATA(\n",
            "  tags::\n",      // lsp 1  ← :: here
            "    \"alpha\"\n", // lsp 2
            "    \"beta\"\n",  // lsp 3
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 2),
            "group array fold (start=1) missing: {:?}", folds
        );
    }

    #[test]
    fn comment_above_next_table_not_included_in_fold() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",          // lsp 1
            "    host = \"x\"\n",   // lsp 2
            "    port = 80\n",      // lsp 3
            "\n",                   // lsp 4  blank
            "  // cache section\n", // lsp 5  comment — excluded
            "  cache:\n",           // lsp 6
            "    port = 6379\n",    // lsp 7
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        let first = folds.iter().find(|f| f.start_line == 1)
            .expect("first table fold missing");
        assert_eq!(first.end_line, 3,
            "first fold must end at lsp3 (not comment lsp5): {:?}", folds);

        assert!(
            folds.iter().any(|f| f.start_line == 6 && f.end_line >= 7),
            "second table fold (start=6) missing: {:?}", folds
        );
    }

    #[test]
    fn blank_between_tables_excluded_from_first_fold() {
        let src = concat!(
            "@DATA(\n",
            "  server:\n",            // lsp 1
            "    host = \"x\"\n",     // lsp 2
            "    port = 80\n",        // lsp 3
            "\n",                     // lsp 4  blank
            "  cache:\n",             // lsp 5
            "    port = 6379\n",      // lsp 6
            ")\n"
        );
        let d = make_doc(src);
        let folds = provide(Some(&d)).unwrap_or_default();

        let first = folds.iter().find(|f| f.start_line == 1)
            .expect("first table fold missing");
        assert_eq!(first.end_line, 3,
            "first fold must end at lsp3, not blank lsp4: {:?}", folds);

        assert!(
            folds.iter().any(|f| f.start_line == 5 && f.end_line >= 6),
            "second table fold (start=5) missing: {:?}", folds
        );
    }
}
