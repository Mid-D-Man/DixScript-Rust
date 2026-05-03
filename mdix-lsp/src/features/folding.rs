// mdix-lsp/src/features/folding.rs
//!
//! ## Root causes of the previous bugs:
//!
//! 1. Section folds used `section_max` (max line of tokens stamped with a
//!    SectionId). If the lexer doesn't stamp tokens in @DATA/@QUICKFUNCS
//!    correctly (or stamps the closing `)` with SectionId::None), the max
//!    is wrong or missing → only @ENUMS worked, others didn't.
//!    FIX: paren-counting from the section keyword token — no SectionId stamps needed.
//!
//! 2. QF function folds used `func.position` from the AST. If the parser
//!    sets all positions to line 0 or the same line, folds overlap and only
//!    some show. FIX: scan for `FunctionPrefix` (`~`) tokens directly.
//!
//! 3. Brace folds checked `open_section == close_section` strictly —
//!    if one brace had SectionId::None, the fold was silently dropped.
//!    FIX: emit brace folds regardless of section, guard only by multi-line.
//!
//! 4. DATA entry folds didn't guard `position.is_valid()`, so a zero-position
//!    entry would produce a fold starting at line 0 that swallowed everything.

use std::panic;

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::AST::{DataEntry, DataSection};
use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("folding panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    let doc = doc?;

    // Need at least some tokens OR a config range.
    if doc.tokens.is_empty() && doc.config_line_range.is_none() {
        return None;
    }

    let mut ranges: Vec<FoldingRange> = Vec::new();

    // ── @CONFIG fold (from stored range — no tokens exist for it) ─────────
    if let Some((start, end)) = doc.config_line_range {
        if end > start {
            ranges.push(region(start, end));
        }
    }

    if !doc.tokens.is_empty() {
        // ── Section folds (paren-counting, no SectionId stamps needed) ────
        collect_section_folds(&doc.tokens, &mut ranges);

        // ── QuickFunc body folds (FunctionPrefix tokens) ─────────────────
        collect_quickfunc_folds(&doc.tokens, &mut ranges);

        // ── Brace folds {…} ───────────────────────────────────────────────
        collect_brace_folds(&doc.tokens, &mut ranges);

        // ── DATA entry folds (from AST, guarded) ─────────────────────────
        if let Some(ast) = &doc.ast {
            if let Some(data) = &ast.data {
                collect_data_entry_folds(data, &mut ranges);
            }
        }
    }

    // Sort, deduplicate, remove zero-length.
    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);
    ranges.retain(|r| r.end_line > r.start_line);

    if ranges.is_empty() { None } else { Some(ranges) }
}

// ── Section folds via paren-counting ─────────────────────────────────────────
//
// Find each section keyword token. From that position, count `(` and `)`
// (respecting nesting) to find the matching close paren. That line is the
// fold end. No SectionId stamps required.

fn collect_section_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    // Collect (token_index, lsp_start_line) for each section keyword.
    let section_starts: Vec<(usize, u32)> = tokens.iter().enumerate()
        .filter(|(_, t)| {
            t.token_type.is_section_keyword()
                && section_id_of_keyword(&t.token_type) != SectionId::None
        })
        .map(|(i, t)| (i, t.line.saturating_sub(1) as u32))
        .collect();

    for (i, &(tok_idx, start_line)) in section_starts.iter().enumerate() {
        // Only scan up to the next section keyword so we can't overshoot.
        let scan_end = section_starts.get(i + 1).map(|(j, _)| *j).unwrap_or(tokens.len());
        let search = &tokens[tok_idx..scan_end];

        if let Some(end_line) = paren_close_line(search) {
            if end_line > start_line {
                ranges.push(region(start_line, end_line));
            }
        }
    }
}

/// Scan `tokens` for the first `(`, then find the matching `)`.
/// Returns the 0-based LSP line of the closing `)`.
fn paren_close_line(tokens: &[Token]) -> Option<u32> {
    let mut depth = 0i32;
    let mut found_open = false;

    for token in tokens {
        match &token.token_type {
            TokenType::Symbol('(') => {
                depth += 1;
                found_open = true;
            }
            TokenType::Symbol(')') if found_open => {
                depth -= 1;
                if depth == 0 {
                    return Some(token.line.saturating_sub(1) as u32);
                }
            }
            TokenType::EndOfFile => break,
            _ => {}
        }
    }
    None
}

fn section_id_of_keyword(tt: &TokenType) -> SectionId {
    match tt {
        TokenType::SectionImports    => SectionId::Imports,
        TokenType::SectionDLM        => SectionId::Dlm,
        TokenType::SectionEnums      => SectionId::Enums,
        TokenType::SectionQuickFuncs => SectionId::QuickFuncs,
        TokenType::SectionData       => SectionId::Data,
        TokenType::SectionSecurity   => SectionId::Security,
        _                            => SectionId::None,
    }
}

// ── QuickFunc body folds ──────────────────────────────────────────────────────
//
// Find all `~` (FunctionPrefix) tokens regardless of SectionId stamp.
// Each function's fold: from its `~` line to just before the next `~`.
// The last function folds to the end of the `@QUICKFUNCS(...)` block.
//
// This avoids relying on AST position accuracy entirely.

fn collect_quickfunc_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    // Find where the @QUICKFUNCS section token is.
    let qf_section_tok = tokens.iter()
        .find(|t| matches!(t.token_type, TokenType::SectionQuickFuncs));
    let qf_section_idx = match tokens.iter()
        .position(|t| matches!(t.token_type, TokenType::SectionQuickFuncs))
    {
        Some(i) => i,
        None    => return,
    };

    // Find the end of the @QUICKFUNCS block using paren-counting.
    let qf_end_line = paren_close_line(&tokens[qf_section_idx..])
        .unwrap_or_else(|| {
            tokens.last().map(|t| t.line.saturating_sub(1) as u32).unwrap_or(0)
        });

    // Collect all FunctionPrefix (`~`) lines within the QF block.
    // We accept tokens with SectionId::QuickFuncs OR SectionId::None
    // (some lexers may not stamp inner tokens).
    let func_lines: Vec<u32> = tokens.iter()
        .skip(qf_section_idx)
        .filter(|t| {
            // Stop past the end of the QF section.
            (t.line.saturating_sub(1) as u32) <= qf_end_line
        })
        .filter(|t| matches!(t.token_type, TokenType::FunctionPrefix))
        .map(|t| t.line.saturating_sub(1) as u32)
        .collect();

    if func_lines.is_empty() { return; }

    // Single function: just the section fold covers it.
    if func_lines.len() == 1 { return; }

    for (i, &start) in func_lines.iter().enumerate() {
        let end = if i + 1 < func_lines.len() {
            // End one line before the next function's `~`.
            func_lines[i + 1].saturating_sub(1)
        } else {
            // Last function: end at the section close paren line.
            qf_end_line
        };

        if end > start {
            ranges.push(region(start, end));
        }
    }
}

// ── Brace folds ───────────────────────────────────────────────────────────────
//
// Simple { / } matching. Emit whenever the braces span more than one line.
// No section filtering — nested objects in @DATA and enum bodies all fold.

fn collect_brace_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    // Stack of opening brace line numbers.
    let mut stack: Vec<u32> = Vec::new();

    for token in tokens {
        match &token.token_type {
            TokenType::Symbol('{') => {
                let line = token.line.saturating_sub(1) as u32;
                stack.push(line);
            }
            TokenType::Symbol('}') => {
                if let Some(start_line) = stack.pop() {
                    let end_line = token.line.saturating_sub(1) as u32;
                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }
            TokenType::EndOfFile => break,
            _ => {}
        }
    }
}

// ── DATA entry folds (from AST) ───────────────────────────────────────────────
//
// TableProperty and GroupArray entries can span many lines.
// Only emits when both the start and end positions are valid.

fn collect_data_entry_folds(data: &DataSection, ranges: &mut Vec<FoldingRange>) {
    for entry in &data.entries {
        match entry {
            DataEntry::TableProperty { position, properties, .. } => {
                if !position.is_valid() { continue; }
                // Find the last property with a valid position.
                let last_valid = properties.iter().rev()
                    .find(|p| p.position.is_valid());
                if let Some(last) = last_valid {
                    let start_line = position.line.saturating_sub(1) as u32;
                    let end_line   = last.position.line.saturating_sub(1) as u32;
                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }
            DataEntry::GroupArray { position, items, .. } => {
                if !position.is_valid() || items.is_empty() { continue; }
                // Use the last item's position if valid.
                let last_valid_line = items.iter().rev()
                    .map(|v| v.position())
                    .find(|p| p.is_valid())
                    .map(|p| p.line.saturating_sub(1) as u32);
                if let Some(end_line) = last_valid_line {
                    let start_line = position.line.saturating_sub(1) as u32;
                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }
            // ObjectProperty: the {…} block is handled by collect_brace_folds.
            // SimpleProperty: single-line, no fold.
            DataEntry::ObjectProperty { .. } | DataEntry::SimpleProperty { .. } => {}
        }
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn region(start_line: u32, end_line: u32) -> FoldingRange {
    FoldingRange {
        start_line,
        end_line,
        kind:             Some(FoldingRangeKind::Region),
        start_character:  None,
        end_character:    None,
        collapsed_text:   None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::run_pipeline;
    use crate::document::Document;
    use tower_lsp::lsp_types::Url;

    fn test_doc(source: &str) -> Document {
        let mut doc = Document::new(
            Url::parse("file:///test.mdix").unwrap(),
            source.to_string(), 0,
        );
        run_pipeline(&mut doc);
        doc
    }

    #[test]
    fn no_crash_on_none() {
        assert!(provide(None).is_none());
    }

    #[test]
    fn single_section_folds() {
        let src = "@DATA(\n  x = 1\n  y = 2\n  z = 3\n)";
        let doc  = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // Should have at least one fold for @DATA (lines 0–4).
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line >= 4),
            "@DATA fold missing: {:?}", folds
        );
    }

    #[test]
    fn enum_fold_does_not_extend_past_data() {
        let src = "@ENUMS(\n  T { A = 0, B = 1 }\n)\n@DATA(\n  x = 1\n)";
        let doc  = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // @ENUMS fold should end at line 2 (the `)` of @ENUMS).
        for fold in &folds {
            if fold.start_line == 0 {
                assert!(
                    fold.end_line <= 2,
                    "@ENUMS fold extends too far: {:?}", fold
                );
            }
        }
    }

    #[test]
    fn multiple_quickfunc_folds() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~f1<int>(x) { return x }\n",
            "  ~f2<int>(y) { return y }\n",
            "  ~f3<int>(z) { return z }\n",
            ")"
        );
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // With 3 functions, we expect at least the section fold + 2 function folds
        // (since single-func folds are skipped for the last one which uses the section fold).
        // At minimum, multiple folds should exist.
        assert!(folds.len() >= 2, "expected multiple folds: {:?}", folds);
    }

    #[test]
    fn no_zero_length_folds() {
        let src = "@DATA(\n  x = 1\n)\n@ENUMS(\n  T { A = 0 }\n)";
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        for f in &folds {
            assert!(f.end_line > f.start_line, "zero-length fold: {:?}", f);
        }
    }

    #[test]
    fn brace_fold_for_object_in_data() {
        let src = "@DATA(\n  player = {\n    name = \"Hero\"\n    level = 10\n  }\n)";
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // Should have a brace fold for the { } on lines 1-4.
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 4),
            "object brace fold missing: {:?}", folds
        );
    }
}
