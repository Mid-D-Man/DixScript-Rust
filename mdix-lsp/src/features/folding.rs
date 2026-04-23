// mdix-lsp/src/features/folding.rs
//! Code folding provider.
//!
//! Folds:
//!   - Every top-level section: @CONFIG(...), @DATA(...), @ENUMS(...), etc.
//!   - Every `{ ... }` block spanning multiple lines:
//!       enum type bodies, QuickFunc bodies, object literals, @SECURITY blocks
//!   - Multi-line table properties:  `server: host = "x"\n  port = 8080`
//!   - Multi-line group arrays:      `tags::\n  "a", "b", "c"`

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::AST::{DataEntry, DataSection};
use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    let doc = doc?;
    if doc.tokens.is_empty() { return None; }

    let mut ranges: Vec<FoldingRange> = Vec::new();

    // Pass 1: section-level folds ( @DATA(...) etc. ) via paren matching
    collect_section_folds(&doc.tokens, &mut ranges);

    // Pass 2: brace-block folds { ... } — enum bodies, func bodies, objects
    collect_brace_folds(&doc.tokens, &mut ranges);

    // Pass 3: DATA-entry folds for multi-line table props and group arrays
    if let Some(ast) = &doc.ast {
        if let Some(data) = &ast.data {
            collect_data_entry_folds(data, &mut ranges);
        }
    }

    // Stable sort then deduplicate by (start, end) pair
    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);

    if ranges.is_empty() { None } else { Some(ranges) }
}

// ── Pass 1: section paren folds ───────────────────────────────────────────────

/// For every section keyword token, find its matching `)` and emit one fold
/// that covers the full section block.
fn collect_section_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];

        if !tok.token_type.is_section_keyword() {
            i += 1;
            continue;
        }

        let start_line = tok.line.saturating_sub(1) as u32;

        // The opening `(` should be the very next non-trivial token
        let open_idx = tokens[i..]
            .iter()
            .enumerate()
            .skip(1)
            .take(5)
            .find(|(_, t)| matches!(t.token_type, TokenType::Symbol('(')))
            .map(|(offset, _)| i + offset);

        if let Some(oi) = open_idx {
            if let Some(end_line) = find_matching_paren(tokens, oi) {
                if end_line > start_line {
                    ranges.push(region(start_line, end_line));
                }
                i = oi + 1;
                continue;
            }
        }

        i += 1;
    }
}

/// Walk tokens from `open_idx` (a `(` token), returning the 0-based line of
/// its matching `)`.  Handles nested parens correctly.
fn find_matching_paren(tokens: &[Token], open_idx: usize) -> Option<u32> {
    let mut depth = 0i32;
    for tok in tokens[open_idx..].iter() {
        match &tok.token_type {
            TokenType::Symbol('(') => depth += 1,
            TokenType::Symbol(')') => {
                depth -= 1;
                if depth == 0 {
                    return Some(tok.line.saturating_sub(1) as u32);
                }
            }
            TokenType::EndOfFile => break,
            _ => {}
        }
    }
    None
}

// ── Pass 2: brace block folds ─────────────────────────────────────────────────

/// Emit a fold for every `{ ... }` block that spans more than one source line.
///
/// Covers:
///   - Enum type bodies:     `AIType { PASSIVE = 0, … }`
///   - QuickFunc bodies:     `~weapon(…) { return { … } }`
///   - Nested object values: `enemy = { name = "orc", health = 100 }`
///   - @SECURITY blocks:     `encryption -> { mode = "keyfile", … }`
fn collect_brace_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    // Stack holds the 0-based start line of each `{` we've seen
    let mut stack: Vec<u32> = Vec::new();

    for tok in tokens {
        match &tok.token_type {
            TokenType::Symbol('{') => {
                stack.push(tok.line.saturating_sub(1) as u32);
            }
            TokenType::Symbol('}') => {
                if let Some(start_line) = stack.pop() {
                    let end_line = tok.line.saturating_sub(1) as u32;
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

// ── Pass 3: DATA entry folds ──────────────────────────────────────────────────

/// Emit folds for multi-line table properties and group arrays in @DATA.
///
/// Table property example (3 lines → fold):
///   `player.config: speed = 5,`
///   `               jump = 3,`
///   `               ai_type = AIType.AGGRESSIVE`
///
/// Group array example (4 lines → fold):
///   `enemies::`
///   `  createEnemy("Goblin", 50, 10),`
///   `  createEnemy("Orc", 100, 20),`
///   `  createEnemy("Boss", 500, 80)`
fn collect_data_entry_folds(data: &DataSection, ranges: &mut Vec<FoldingRange>) {
    for entry in &data.entries {
        match entry {
            DataEntry::TableProperty { position, properties, .. } => {
                let start_line = position.line.saturating_sub(1) as u32;
                if let Some(last) = properties.last() {
                    let end_line = last.position.line.saturating_sub(1) as u32;
                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }
            DataEntry::GroupArray { position, items, .. } => {
                let start_line = position.line.saturating_sub(1) as u32;
                if let Some(last) = items.last() {
                    let end_line = last.position().line.saturating_sub(1) as u32;
                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }
            _ => {}
        }
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn region(start_line: u32, end_line: u32) -> FoldingRange {
    FoldingRange {
        start_line,
        end_line,
        kind: Some(FoldingRangeKind::Region),
        // Remaining fields (start_character, end_character, collapsed_text)
        // fall back to their defaults (None / None / None).
        ..Default::default()
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
    fn folding_none_doc() {
        assert!(provide(None).is_none());
    }

    #[test]
    fn section_fold_data() {
        let src = "@DATA(\n  x = 1\n  y = 2\n)";
        let doc = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 0 && f.end_line == 3),
            "expected @DATA fold from line 0 to 3, got: {:?}", folds
        );
    }

    #[test]
    fn brace_fold_enum_body() {
        let src = "@ENUMS(\n  AIType {\n    PASSIVE = 0,\n    BOSS = 1\n  }\n)";
        let doc = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // Should have at least the @ENUMS section fold and the AIType body fold
        assert!(folds.len() >= 2, "expected at least 2 folds, got {:?}", folds);
    }

    #[test]
    fn no_single_line_folds() {
        let src = "@DATA(\n  x = 1\n)";
        let doc = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(
            folds.iter().all(|f| f.end_line > f.start_line),
            "single-line folds must be excluded: {:?}", folds
        );
    }

    #[test]
    fn group_array_multiline_fold() {
        let src = "@DATA(\n  tags::\n    \"alpha\",\n    \"beta\",\n    \"v1\"\n)";
        let doc = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // The group array should produce a fold
        assert!(
            folds.iter().any(|f| f.start_line == 1),
            "expected group array fold starting at line 1, got: {:?}", folds
        );
    }
               }
