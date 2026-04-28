// mdix-lsp/src/features/folding.rs
//
// Folding strategy:
//   Section folds  — use token.section tags: keyword line → max line of
//                    all tokens tagged with that section.  This is immune
//                    to nested `(` inside data values or function calls
//                    corrupting the paren-counting approach.
//   Brace folds    — stack-based { / } matching, but only emit a range
//                    when the opening and closing braces share the same
//                    section (prevents cross-section folds).
//   Data AST folds — TableProperty and GroupArray from the AST give
//                    reliable ranges for nested data structures.
//   QuickFunc folds — one fold per function body from the AST.

use std::panic;
use std::collections::HashMap;

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
    if doc.tokens.is_empty() { return None; }

    let mut ranges: Vec<FoldingRange> = Vec::new();

    // Section-level folds (the most important).
    collect_section_folds(&doc.tokens, &mut ranges);

    // Per-function folds inside @QUICKFUNCS.
    if let Some(ast) = &doc.ast {
        if let Some(qf) = &ast.quick_functions {
            collect_quickfunc_folds(&doc.tokens, qf, &mut ranges);
        }
        // Object / table / array folds from @DATA.
        if let Some(data) = &ast.data {
            collect_data_entry_folds(data, &mut ranges);
        }
    }

    // Brace folds for enum bodies, inline objects, etc.
    collect_brace_folds(&doc.tokens, &mut ranges);

    // Deduplicate and sort.
    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);
    // Remove zero-length folds.
    ranges.retain(|r| r.end_line > r.start_line);

    if ranges.is_empty() { None } else { Some(ranges) }
}

// ── Section folds ─────────────────────────────────────────────────────────────
//
// For each section keyword token (e.g. SectionEnums at line 5):
//   start_line = that token's line (0-based)
//   end_line   = max line of ALL tokens whose .section == that SectionId
//
// This works because the lexer stamps every token with the section it was
// scanned in, including the closing `)`.  No paren-counting needed.
// @CONFIG is excluded — it is stripped before tokenisation.

fn section_id_of_keyword(tt: &TokenType) -> SectionId {
    match tt {
        TokenType::SectionImports    => SectionId::Imports,
        TokenType::SectionDLM        => SectionId::Dlm,
        TokenType::SectionEnums      => SectionId::Enums,
        TokenType::SectionQuickFuncs => SectionId::QuickFuncs,
        TokenType::SectionData       => SectionId::Data,
        TokenType::SectionSecurity   => SectionId::Security,
        _ => SectionId::None,  // SectionConfig is stripped before tokenisation
    }
}

fn collect_section_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    // Pass 1: collect the maximum (last) line for each section id.
    let mut section_max: HashMap<SectionId, u32> = HashMap::new();
    for tok in tokens {
        if tok.section == SectionId::None { continue; }
        let line = tok.line.saturating_sub(1) as u32;
        let entry = section_max.entry(tok.section).or_insert(0);
        if line > *entry { *entry = line; }
    }

    // Pass 2: find each section keyword → use its line as start.
    for tok in tokens {
        if !tok.token_type.is_section_keyword() { continue; }
        let sid = section_id_of_keyword(&tok.token_type);
        if sid == SectionId::None { continue; }
        let start_line = tok.line.saturating_sub(1) as u32;
        if let Some(&end_line) = section_max.get(&sid) {
            if end_line > start_line {
                ranges.push(region(start_line, end_line));
            }
        }
    }
}

// ── QuickFunc body folds ──────────────────────────────────────────────────────
//
// One fold per function: from the `~name` declaration line to either
// the line before the next function, or the last token in the section.

fn collect_quickfunc_folds(
    tokens: &[Token],
    qf:     &dixscript::Compiler::AST::QuickFuncsSection,
    ranges: &mut Vec<FoldingRange>,
) {
    if qf.functions.is_empty() { return; }

    // Max line of the entire @QUICKFUNCS section (for last function's end).
    let qf_max_line = tokens.iter()
        .filter(|t| t.section == SectionId::QuickFuncs)
        .map(|t| t.line.saturating_sub(1) as u32)
        .max()
        .unwrap_or(0);

    for (i, func) in qf.functions.iter().enumerate() {
        let start_line = func.position.line.saturating_sub(1) as u32;

        let end_line = if i + 1 < qf.functions.len() {
            // End just before the next function's declaration line.
            qf.functions[i + 1].position.line.saturating_sub(2) as u32
        } else {
            // Last function ends at the section's last token.
            qf_max_line
        };

        if end_line > start_line {
            ranges.push(region(start_line, end_line));
        }
    }
}

// ── Brace folds ───────────────────────────────────────────────────────────────
//
// Stack-based { / } matching.  A fold is only emitted when the opening `{`
// and closing `}` share the same section — this prevents a closing `}` in
// @DATA from being matched against an opening `{` in @QUICKFUNCS.

fn collect_brace_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    // Stack entries: (open_line_0based, open_section).
    let mut stack: Vec<(u32, SectionId)> = Vec::new();

    for tok in tokens {
        match &tok.token_type {
            TokenType::Symbol('{') => {
                let line = tok.line.saturating_sub(1) as u32;
                stack.push((line, tok.section));
            }
            TokenType::Symbol('}') => {
                if let Some((start_line, open_section)) = stack.pop() {
                    let end_line = tok.line.saturating_sub(1) as u32;
                    // Only emit if the brace pair is within the same section
                    // and spans more than one line.
                    if end_line > start_line && open_section == tok.section {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }
            TokenType::EndOfFile => break,
            _ => {}
        }
    }
}

// ── Data entry folds (from AST) ───────────────────────────────────────────────
//
// TableProperty and GroupArray entries can span many lines.  The AST gives
// us reliable start/end positions without any token-scanning heuristics.

fn collect_data_entry_folds(data: &DataSection, ranges: &mut Vec<FoldingRange>) {
    for entry in &data.entries {
        match entry {
            DataEntry::TableProperty { position, properties, .. } => {
                if let Some(last) = properties.last() {
                    let start_line = position.line.saturating_sub(1) as u32;
                    let end_line   = last.position.line.saturating_sub(1) as u32;
                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }
            DataEntry::GroupArray { position, items, .. } => {
                if let Some(last) = items.last() {
                    let start_line = position.line.saturating_sub(1) as u32;
                    let last_pos   = last.position();
                    let end_line   = last_pos.line.saturating_sub(1) as u32;
                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }
            DataEntry::ObjectProperty { position, object, .. } => {
                // object is a boxed Value; use its position if it's an object literal.
                let end_pos = object.position();
                if end_pos.is_valid() {
                    let start_line = position.line.saturating_sub(1) as u32;
                    let end_line   = end_pos.line.saturating_sub(1) as u32;
                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }
            DataEntry::SimpleProperty { .. } => {}
        }
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn region(start_line: u32, end_line: u32) -> FoldingRange {
    FoldingRange {
        start_line,
        end_line,
        kind: Some(FoldingRangeKind::Region),
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
            source.to_string(),
            0,
        );
        run_pipeline(&mut doc);
        doc
    }

    #[test]
    fn no_crash_on_none() {
        assert!(provide(None).is_none());
    }

    #[test]
    fn section_folds_are_generated() {
        let doc = test_doc(
            "@ENUMS(\n  AIType { PASSIVE = 0, AGGRESSIVE = 1 }\n)\n\
             @DATA(\n  x = 1\n  y = 2\n)",
        );
        let folds = provide(Some(&doc)).unwrap_or_default();
        // @ENUMS should produce a fold starting on line 0.
        assert!(
            folds.iter().any(|f| f.start_line == 0),
            "expected @ENUMS fold: {:?}", folds
        );
        // @DATA should produce a fold starting on line 3.
        assert!(
            folds.iter().any(|f| f.start_line == 3),
            "expected @DATA fold: {:?}", folds
        );
    }

    #[test]
    fn enum_fold_does_not_extend_past_section() {
        let doc = test_doc(
            "@ENUMS(\n  AIType { PASSIVE = 0, AGGRESSIVE = 1 }\n)\n\
             @DATA(\n  score = 100\n)",
        );
        let folds = provide(Some(&doc)).unwrap_or_default();
        // @ENUMS fold must end before @DATA starts (line 3).
        for fold in &folds {
            if fold.start_line == 0 {
                assert!(
                    fold.end_line < 3,
                    "@ENUMS fold incorrectly extends into @DATA: {:?}",
                    fold
                );
            }
        }
    }

    #[test]
    fn no_zero_length_folds() {
        let doc = test_doc("@DATA(\n  x = 1\n)");
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(folds.iter().all(|f| f.end_line > f.start_line));
    }
}
