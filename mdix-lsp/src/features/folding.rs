// mdix-lsp/src/features/folding.rs
//!
//! Fold regions:
//! 1. @CONFIG section         — paren-depth tracking on Config tokens
//! 2. Enum bodies             — collect_brace_folds_in_range on @ENUMS tokens
//! 3. QuickFunc bodies        — ~ → params → { → }
//! 4. QuickFunc params        — when ~ line != { line
//! 5. DATA object literals    — collect_brace_folds_in_range
//! 6. Table / group arrays    — last non-comment DATA token before next entry
//! 7. SECURITY objects        — collect_brace_folds_in_range
//!
//! Approach B: @CONFIG tokens are real tokens with SectionId::Config and
//! accurate positions. No source-scanning or stored line ranges needed.

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

    // ── @CONFIG fold (Approach B) ─────────────────────────────────────────────
    // @CONFIG tokens are now real tokens. Find the section keyword and track
    // paren depth to find the matching close paren.
    collect_config_fold(&doc.tokens, &mut ranges);

    // ── Content folds via AST visitor ─────────────────────────────────────────
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

// ── @CONFIG fold ──────────────────────────────────────────────────────────────
//
// Finds the SectionConfig token and the matching closing ')' by paren-depth
// tracking. Emits one fold: @CONFIG( line → line before ).
//
// The fold end is the line BEFORE the ')' so the closing paren stays visible.

fn collect_config_fold(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    // Find @CONFIG keyword token.
    let config_tok = match tokens
        .iter()
        .find(|t| matches!(t.token_type, TokenType::SectionConfig))
    {
        Some(t) => t,
        None    => return,
    };

    let open_lsp = tok_lsp_line(config_tok);

    // Find the opening '(' after @CONFIG.
    let open_paren_idx = match tokens
        .iter()
        .enumerate()
        .find(|(_, t)| {
            t.section == SectionId::Config
                && matches!(t.token_type, TokenType::Symbol('('))
        }) {
        Some((i, _)) => i,
        None => return,
    };

    // Track paren depth to find the matching ')'.
    let mut depth: i32 = 0;
    let mut close_lsp: Option<u32> = None;

    for tok in tokens.iter().skip(open_paren_idx) {
        match &tok.token_type {
            TokenType::Symbol('(') if tok.section == SectionId::Config => {
                depth += 1;
            }
            TokenType::Symbol(')') if tok.section == SectionId::Config => {
                depth -= 1;
                if depth == 0 {
                    close_lsp = Some(tok_lsp_line(tok));
                    break;
                }
            }
            TokenType::EndOfFile => break,
            _ => {}
        }
    }

    let close_lsp = match close_lsp {
        Some(l) => l,
        // Fallback: use last Config token if we never found the close paren.
        None => tokens
            .iter()
            .rev()
            .find(|t| {
                t.section == SectionId::Config
                    && !matches!(t.token_type, TokenType::EndOfFile)
            })
            .map(tok_lsp_line)
            .unwrap_or(open_lsp),
    };

    // Fold from the @CONFIG( line to the line before the closing ).
    let fold_end = close_lsp.saturating_sub(1);
    if fold_end > open_lsp {
        ranges.push(make_fold(open_lsp, fold_end));
    }
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
    fn visit_enums_section(&mut self, section: &EnumsSection) -> () {
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
        // from_lsp + 1: skip the @ENUMS( line itself.
        collect_brace_folds_in_range(self.tokens, from_lsp + 1, end_lsp, self.ranges);
    }

    // ── @QUICKFUNCS ───────────────────────────────────────────────────────────
    fn visit_quickfuncs_section(&mut self, _section: &QuickFuncsSection) -> () {
        collect_quickfunc_folds(self.tokens, self.ranges);
    }

    // ── @DATA ─────────────────────────────────────────────────────────────────
    fn visit_data_section(&mut self, section: &DataSection) -> () {
        if !section.position.is_valid() { return; }

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
//   1. Find the first `(` after `~`    → param list open
//   2. Depth-track ( / ) to close      → param list close index
//   3. Find first `{` after close      → body open (no section filter —
//      the `{` in `) {` can have a different section tag due to lexer ordering)
//   4. Depth-track { / } to close      → body close
//
// Emits:
//   • Body fold:  { line → line before }   (always, when body spans > 1 line)
//   • Param fold: ~ line → line before {   (when ~ and { are on different lines)

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
        // ── Step 1: first `(` after `~` ──────────────────────────────────────
        let paren_open = tokens
            .iter()
            .enumerate()
            .skip(tilde_idx + 1)
            .find(|(_, t)| matches!(t.token_type, TokenType::Symbol('(')));

        let (open_idx, _) = match paren_open {
            Some(r) => r,
            None => continue,
        };

        // ── Step 2: depth-track ( / ) ─────────────────────────────────────────
        let paren_close_idx = {
            let mut depth = 0i32;
            let mut found = None;
            for (idx, tok) in tokens.iter().enumerate().skip(open_idx) {
                match &tok.token_type {
                    TokenType::Symbol('(') => depth += 1,
                    TokenType::Symbol(')') => {
                        depth -= 1;
                        if depth == 0 { found = Some(idx); break; }
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

        // ── Step 3: first `{` after param close ───────────────────────────────
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

        // ── Step 4: depth-track { / } ─────────────────────────────────────────
        let close_lsp = {
            let mut depth = 0i32;
            let mut found = None;
            for tok in tokens.iter().skip(body_open_idx) {
                match &tok.token_type {
                    TokenType::Symbol('{') => depth += 1,
                    TokenType::Symbol('}') => {
                        depth -= 1;
                        if depth == 0 { found = Some(tok_lsp_line(tok)); break; }
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

        // ── Body fold ─────────────────────────────────────────────────────────
        let body_end = close_lsp.saturating_sub(1);
        if body_end > open_lsp {
            ranges.push(make_fold(open_lsp, body_end));
        }

        // ── Param fold (only when multiline) ──────────────────────────────────
        if tilde_lsp < open_lsp {
            let param_end = open_lsp.saturating_sub(1);
            if param_end > tilde_lsp {
                ranges.push(make_fold(tilde_lsp, param_end));
            }
        }
    }
}

// ── Table property / group array folds ───────────────────────────────────────

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

        let bound_1based: Option<usize> = entries
            .get(i + 1)
            .map(|e| e.position())
            .filter(|p| p.is_valid() && p.line > 0)
            .map(|p| p.line);

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
