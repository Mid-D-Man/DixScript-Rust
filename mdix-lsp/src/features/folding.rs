// mdix-lsp/src/features/folding.rs
//!
//! Fold regions:
//! 1. Enum bodies         — { } pairs inside @ENUMS
//! 2. QuickFunc bodies    — { } of each function body only
//! 3. DATA object literals — { } pairs inside @DATA
//! 4. Table / group arrays — delimiter to last content token (comments excluded)
//! 5. SECURITY objects    — { } pairs inside @SECURITY
//!
//! ## Brace fold behaviour
//! fold(open_lsp, close_lsp): start = `{` line, end = `}` line.
//! Editor hides lines open_lsp+1 through close_lsp. Shows `{...}` collapsed.
//! Matches Rust/Java behaviour — the whole block including `}` is hidden.
//!
//! ## QuickFunc fold
//! On encountering `~`, scan forward until first `{`, depth-track to matching `}`.
//! fold(`{` line, `}` line). One fold per function, params irrelevant.

use std::panic;
use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};
use dixscript::Compiler::AST::{
    AstVisitorBase, DataEntry, DataSection, EnumsSection,
    QuickFuncsSection, SecuritySection,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;

pub fn provide(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc))).unwrap_or_else(|payload| {
        let msg = payload.downcast_ref::<String>().cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown panic".to_string());
        tracing::error!("folding panicked: {}", msg);
        None
    })
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
        a.start_line.cmp(&b.start_line).then_with(|| b.end_line.cmp(&a.end_line))
    });
    ranges.dedup_by_key(|r| (r.start_line, r.end_line));
    ranges.retain(|r| r.end_line > r.start_line);

    tracing::debug!("folding: {} ranges produced", ranges.len());
    if ranges.is_empty() { None } else { Some(ranges) }
}

// ── AST Visitor ───────────────────────────────────────────────────────────────

struct FoldingVisitor<'a> {
    tokens: &'a [Token],
    ranges: &'a mut Vec<FoldingRange>,
}

impl<'a> AstVisitorBase for FoldingVisitor<'a> {
    type Result = ();
    fn default_result(&self) -> () {}

    fn visit_enums_section(&mut self, section: &EnumsSection) -> () {
        let from_lsp = self.tokens.iter()
            .find(|t| matches!(t.token_type, TokenType::SectionEnums))
            .map(tok_lsp_line)
            .or_else(|| {
                if section.position.is_valid() {
                    Some(section.position.line.saturating_sub(1) as u32)
                } else {
                    None
                }
            });

        let from_lsp = match from_lsp { Some(l) => l, None => return };
        let end_lsp = section_last_token_lsp(self.tokens, SectionId::Enums);
        collect_brace_folds(self.tokens, from_lsp + 1, end_lsp, self.ranges);
    }

    fn visit_quickfuncs_section(&mut self, _section: &QuickFuncsSection) -> () {
        collect_quickfunc_folds(self.tokens, self.ranges);
    }

    fn visit_data_section(&mut self, section: &DataSection) -> () {
        if !section.position.is_valid() { return; }

        let data_start_lsp = self.tokens.iter()
            .find(|t| matches!(t.token_type, TokenType::SectionData))
            .map(tok_lsp_line)
            .unwrap_or_else(|| section.position.line.saturating_sub(1) as u32);

        let data_end_lsp = section_last_token_lsp(self.tokens, SectionId::Data);

        // Object literal { } folds
        collect_brace_folds(self.tokens, data_start_lsp + 1, data_end_lsp, self.ranges);

        // Table / group array folds
        collect_data_entry_folds(self.tokens, section, self.ranges);
    }

    fn visit_security_section(&mut self, section: &SecuritySection) -> () {
        if !section.position.is_valid() { return; }

        let sec_start = self.tokens.iter()
            .find(|t| matches!(t.token_type, TokenType::SectionSecurity))
            .map(tok_lsp_line)
            .unwrap_or_else(|| section.position.line.saturating_sub(1) as u32);

        let sec_end = section_last_token_lsp(self.tokens, SectionId::Security);
        collect_brace_folds(self.tokens, sec_start + 1, sec_end, self.ranges);
    }
}

// ── QuickFunc body fold ───────────────────────────────────────────────────────
//
// Walk tokens. When `~` is encountered inside @QUICKFUNCS:
//   1. Scan forward to the first `{` — ignore name, type annotation, scope, params entirely.
//   2. Depth-track `{`/`}` from that opening brace to find its matching close.
//   3. Emit fold(`{` line, `}` line).
//   4. Advance past the closing `}` to the next function.
//
// This produces exactly ONE fold per function covering the body block `{...}`,
// regardless of whether params are on one line or spread across many.

fn collect_quickfunc_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];

        if tok.section == SectionId::QuickFuncs
            && matches!(tok.token_type, TokenType::Symbol('~'))
        {
            // Step 1: find first `{` after `~`
            let mut open_idx: Option<usize> = None;
            for j in (i + 1)..tokens.len() {
                match &tokens[j].token_type {
                    TokenType::Symbol('{') => { open_idx = Some(j); break; }
                    TokenType::EndOfFile   => break,
                    _ => {}
                }
            }

            if let Some(oi) = open_idx {
                let open_lsp = tok_lsp_line(&tokens[oi]);

                // Step 2: depth-track to matching `}`
                let mut depth: i32 = 0;
                let mut close_idx: Option<usize> = None;
                for j in oi..tokens.len() {
                    match &tokens[j].token_type {
                        TokenType::Symbol('{') => depth += 1,
                        TokenType::Symbol('}') => {
                            depth -= 1;
                            if depth == 0 {
                                close_idx = Some(j);
                                break;
                            }
                        }
                        TokenType::EndOfFile => break,
                        _ => {}
                    }
                }

                if let Some(ci) = close_idx {
                    let close_lsp = tok_lsp_line(&tokens[ci]);
                    if close_lsp > open_lsp {
                        ranges.push(make_fold(open_lsp, close_lsp));
                    }
                    // Step 4: skip past the closing `}` to avoid re-processing
                    i = ci + 1;
                    continue;
                }
            }
        }

        i += 1;
    }
}

// ── Brace { } fold collector ──────────────────────────────────────────────────
//
// Scans tokens in the 1-based line range [from_lsp+1, to_lsp+1] and emits a
// fold for every balanced { } pair.
//
// fold(open_lsp, close_lsp): start=`{` line, end=`}` line.
// When collapsed the editor hides lines from open_lsp+1 through close_lsp,
// producing the `{...}` visual — identical to Rust/Java folding behaviour.

fn collect_brace_folds(
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
                    // Only fold if { and } are on different lines
                    if close_lsp > open_lsp {
                        ranges.push(make_fold(open_lsp, close_lsp));
                    }
                }
            }
            _ => {}
        }
    }
}

// ── Table / group array folds ─────────────────────────────────────────────────
//
// For each TableProperty / GroupArray entry:
//   start = LSP line of the `:` or `::` token
//   end   = LSP line of the last non-comment DATA token before the next entry
//
// Comments between entries are explicitly excluded so they don't get consumed
// into the preceding entry's fold range.

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

        // Find the `:` or `::` delimiter at or within 1 line of the entry position
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

        // Last DATA token below delimiter, before next entry, excluding comments.
        // Comments sitting between two entries are NOT consumed into this fold.
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

// ── Helpers ───────────────────────────────────────────────────────────────────

#[inline]
fn tok_lsp_line(tok: &Token) -> u32 {
    tok.line.saturating_sub(1) as u32
}

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
