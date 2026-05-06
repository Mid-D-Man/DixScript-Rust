// mdix-lsp/src/features/folding.rs

use std::panic;

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::AST::{DataEntry, DataSection};
use crate::document::Document;

pub fn provide(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)));
    result.unwrap_or_else(|payload| {
        let msg = payload.downcast_ref::<String>().cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown panic".to_string());
        tracing::error!("folding panicked: {}", msg);
        None
    })
}

fn provide_inner(doc: Option<&Document>) -> Option<Vec<FoldingRange>> {
    let doc = doc?;

    if doc.tokens.is_empty() && doc.config_line_range.is_none() {
        return None;
    }

    let mut ranges: Vec<FoldingRange> = Vec::new();

    // @CONFIG fold — source-text derived; no tokens exist for @CONFIG lines.
    if let Some((start, end)) = doc.config_line_range {
        if end > start {
            ranges.push(region(start, end));
        }
    }

    if !doc.tokens.is_empty() {
        collect_section_folds(&doc.tokens, &mut ranges);
        collect_quickfunc_folds(&doc.tokens, &mut ranges);
        collect_brace_folds(&doc.tokens, &mut ranges);

        if let Some(ast) = &doc.ast {
            if let Some(data) = &ast.data {
                // Pass tokens so table property folds can scan for actual end lines
                // of multi-line object values, not just declaration lines.
                collect_data_entry_folds(data, &doc.tokens, &mut ranges);
            }
        }
    }

    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);
    ranges.retain(|r| r.end_line > r.start_line);

    if ranges.is_empty() { None } else { Some(ranges) }
}

// ── Shared low-level helpers ──────────────────────────────────────────────────

/// Returns the index of the first section-keyword token found after `from_idx`.
/// Used to restrict searches to a single section and prevent runaway scans.
fn find_section_scan_end(tokens: &[Token], from_idx: usize) -> usize {
    tokens.iter()
        .enumerate()
        .skip(from_idx + 1)
        .find(|(_, t)| t.token_type.is_section_keyword())
        .map(|(i, _)| i)
        .unwrap_or(tokens.len())
}

/// Returns the 0-based line of the `)` that closes the first `(` found in
/// `tokens`, respecting balanced nesting.  Only counts `Symbol('(')` /
/// `Symbol(')')` — curly braces are ignored.
fn paren_close_line(tokens: &[Token]) -> Option<u32> {
    let mut depth      = 0i32;
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

/// Returns the 0-based line of the `}` that closes the first `{` found in
/// `tokens`, respecting balanced nesting.  Only counts curly braces.
fn find_matching_close_brace(tokens: &[Token]) -> Option<u32> {
    let mut depth      = 0i32;
    let mut found_open = false;

    for token in tokens {
        match &token.token_type {
            TokenType::Symbol('{') => {
                depth += 1;
                found_open = true;
            }
            TokenType::Symbol('}') if found_open => {
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

// ── Section folds (one fold per @SECTION block) ───────────────────────────────

fn collect_section_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let section_starts: Vec<(usize, u32)> = tokens.iter().enumerate()
        .filter(|(_, t)| {
            t.token_type.is_section_keyword()
                && section_id_of_keyword(&t.token_type) != SectionId::None
        })
        .map(|(i, t)| (i, t.line.saturating_sub(1) as u32))
        .collect();

    for (i, &(tok_idx, start_line)) in section_starts.iter().enumerate() {
        // Restrict the search to this section's own tokens.
        let scan_end = section_starts.get(i + 1).map(|(j, _)| *j).unwrap_or(tokens.len());
        let search   = &tokens[tok_idx..scan_end];

        if let Some(end_line) = paren_close_line(search) {
            if end_line > start_line {
                ranges.push(region(start_line, end_line));
            }
        }
    }
}

// ── QuickFunc per-function folds ──────────────────────────────────────────────
//
// Each function fold runs from its `~` line to the matching `}` of its body.
// This is the same range that collect_brace_folds produces for the same `{}`
// block, so duplicates are removed during the final dedup pass.
//
// Key fixes over the previous version:
//   1. `paren_close_line` is bounded to the QUICKFUNCS section only
//      (prevents the scan from drifting into @DATA and returning a wrong
//      qf_end_line when DATA contains function-call parens).
//   2. Per-function end line comes from find_matching_close_brace, not from
//      (next_tilde - 1), so the fold ends exactly at the closing `}` of the
//      body instead of one line before the next `~`.

fn collect_quickfunc_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let qf_section_idx = match tokens.iter()
        .position(|t| matches!(t.token_type, TokenType::SectionQuickFuncs))
    {
        Some(i) => i,
        None    => return,
    };

    // Hard bound: do not look past the next section keyword.
    let qf_scan_end = find_section_scan_end(tokens, qf_section_idx);

    // Section-level end line (for the last function's fallback).
    let qf_end_line = paren_close_line(&tokens[qf_section_idx..qf_scan_end])
        .unwrap_or_else(|| {
            tokens[..qf_scan_end].iter().rev()
                .find(|t| !matches!(t.token_type, TokenType::EndOfFile) && t.line > 0)
                .map(|t| t.line.saturating_sub(1) as u32)
                .unwrap_or(0)
        });

    // Collect (global_token_index, 0-based_line) for every `~` within the section.
    let func_tilde_positions: Vec<(usize, u32)> = tokens
        .iter()
        .enumerate()
        .skip(qf_section_idx)
        .take(qf_scan_end.saturating_sub(qf_section_idx))
        .filter(|(_, t)| matches!(t.token_type, TokenType::Symbol('~')))
        .map(|(i, t)| (i, t.line.saturating_sub(1) as u32))
        .collect();

    if func_tilde_positions.is_empty() { return; }

    for (func_idx, &(tilde_tok_idx, start_line)) in func_tilde_positions.iter().enumerate() {
        // Scan limit for this function: the next function's `~` token, or section end.
        let scan_limit = func_tilde_positions
            .get(func_idx + 1)
            .map(|(ti, _)| *ti)
            .unwrap_or(qf_scan_end);

        // End the fold exactly at the closing `}` of this function's body.
        let end_line = find_matching_close_brace(&tokens[tilde_tok_idx..scan_limit])
            .unwrap_or(qf_end_line);

        if end_line > start_line {
            ranges.push(region(start_line, end_line));
        }
    }
}

// ── Brace folds (objects, arrays, function bodies) ────────────────────────────

fn collect_brace_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let mut stack: Vec<u32> = Vec::new();

    for token in tokens {
        match &token.token_type {
            TokenType::Symbol('{') => {
                stack.push(token.line.saturating_sub(1) as u32);
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

// ── DATA entry folds ──────────────────────────────────────────────────────────

fn collect_data_entry_folds(
    data:   &DataSection,
    tokens: &[Token],
    ranges: &mut Vec<FoldingRange>,
) {
    // Build a sorted list of all valid entry start lines.
    // SimpleProperty and ObjectProperty entries are included so they act
    // as natural upper bounds for the table-property end-line scan.
    let mut all_entry_lines: Vec<u32> = data.entries.iter()
        .map(entry_start_line)
        .filter(|&l| l != u32::MAX)
        .collect();
    all_entry_lines.sort_unstable();
    all_entry_lines.dedup();

    for entry in &data.entries {
        match entry {
            DataEntry::TableProperty { position, properties, .. } => {
                if !position.is_valid() { continue; }
                let start_line = position.line.saturating_sub(1) as u32;

                if let Some(last_prop) = properties.iter().rev().find(|p| p.position.is_valid()) {
                    let last_decl = last_prop.position.line.saturating_sub(1) as u32;

                    // The scan stops one line before the next entry that begins
                    // strictly after `last_decl`.  u32::MAX when this is the
                    // last entry (scan stops at EOF / section keyword instead).
                    let limit = all_entry_lines.iter()
                        .find(|&&l| l > last_decl)
                        .map(|&l| l.saturating_sub(1))
                        .unwrap_or(u32::MAX);

                    // Extend end past the declaration line to capture the closing
                    // `}` / `]` of any multi-line object or array value.
                    let end_line = table_property_actual_end_line(tokens, last_decl, limit);

                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }

            DataEntry::GroupArray { position, items, .. } => {
                if !position.is_valid() || items.is_empty() { continue; }
                let start_line = position.line.saturating_sub(1) as u32;

                let last_item_line = items.iter().rev()
                    .map(|v| v.position())
                    .find(|p| p.is_valid())
                    .map(|p| p.line.saturating_sub(1) as u32);

                if let Some(end_line) = last_item_line {
                    if end_line > start_line {
                        ranges.push(region(start_line, end_line));
                    }
                }
            }

            // ObjectProperty / SimpleProperty folds come from collect_brace_folds.
            DataEntry::ObjectProperty { .. } | DataEntry::SimpleProperty { .. } => {}
        }
    }
}

/// Extract the 0-based start line from any DataEntry variant.
/// Returns `u32::MAX` when the position is invalid / unknown.
fn entry_start_line(entry: &DataEntry) -> u32 {
    let pos = match entry {
        DataEntry::SimpleProperty  { position, .. } => *position,
        DataEntry::TableProperty   { position, .. } => *position,
        DataEntry::GroupArray      { position, .. } => *position,
        DataEntry::ObjectProperty  { position, .. } => *position,
    };
    if pos.is_valid() { pos.line.saturating_sub(1) as u32 } else { u32::MAX }
}

/// Scan DATA-section tokens beginning at `from_line`, tracking `{`/`[` depth.
/// Returns the 0-based line of the last `}`/`]` that returns depth to zero.
/// Falls back to `from_line` when no multi-line value is found (scalar properties).
///
/// `limit` is an inclusive upper-line bound — the scan stops (at depth 0) when
/// a DATA token is seen on a line strictly greater than `limit`.
///
/// This fixes folds for TableProperty entries like:
///
/// ```text
/// game.settings:
///     player = {           ← start_line
///         start_health = 100,
///     }
///     difficulty = {       ← last_decl (from_line)
///         easy_multiplier = 0.5f,
///         hard_multiplier = 1.5f
///     }                    ← returned end_line ✓
/// ```
fn table_property_actual_end_line(tokens: &[Token], from_line: u32, limit: u32) -> u32 {
    let mut last_close_line = from_line;
    let mut depth           = 0i32;

    for token in tokens.iter() {
        let line = token.line.saturating_sub(1) as u32;

        if line < from_line                                 { continue; }
        // Stop on any section keyword (guard against scanning past DATA).
        if token.token_type.is_section_keyword()            { break;    }
        if matches!(token.token_type, TokenType::EndOfFile) { break;    }
        // Only follow DATA-section tokens.
        if token.section != SectionId::Data                 { break;    }
        // At depth 0, respect the upper limit.
        if depth == 0 && line > limit                       { break;    }

        match &token.token_type {
            TokenType::Symbol('{') | TokenType::Symbol('[') => {
                depth += 1;
            }
            TokenType::Symbol('}') | TokenType::Symbol(']') => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        last_close_line = line;
                    }
                }
            }
            _ => {}
        }
    }

    last_close_line
}

// ── Fold constructor ──────────────────────────────────────────────────────────

fn region(start_line: u32, end_line: u32) -> FoldingRange {
    FoldingRange {
        start_line,
        end_line,
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
        for fold in &folds {
            if fold.start_line == 0 {
                assert!(fold.end_line <= 2, "@ENUMS fold extends too far: {:?}", fold);
            }
        }
    }

    #[test]
    fn single_quickfunc_gets_fold() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~calc<int>(x) {\n",
            "    return x\n",
            "  }\n",
            ")"
        );
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(folds.len() >= 2, "got: {:?}", folds);
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 3),
            "function fold missing or wrong range: {:?}", folds
        );
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
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 4),
            "object brace fold missing: {:?}", folds
        );
    }

    #[test]
    fn quickfunc_fold_bounded_to_section() {
        // DATA section has function calls with parens — these must NOT affect
        // the QUICKFUNCS section fold or per-function fold end lines.
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~add<int>(a<int>, b<int>) {\n",
            "    return a + b\n",
            "  }\n",
            ")\n",
            "@DATA(\n",
            "  result = add(10, 20)\n",
            ")\n",
        );
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // The QUICKFUNCS section fold must end on or before line 4 (the `)` of QUICKFUNCS).
        let qf_section_fold = folds.iter().find(|f| f.start_line == 0);
        if let Some(f) = qf_section_fold {
            assert!(f.end_line <= 4,
                "QUICKFUNCS section fold overshot into DATA: {:?}", f);
        }
    }

    #[test]
    fn table_property_fold_covers_object_values() {
        let src = concat!(
            "@DATA(\n",
            "  game.settings:\n",
            "    player = {\n",
            "      hp = 100\n",
            "    },\n",
            "    difficulty = {\n",
            "      mult = 1.5f\n",
            "    }\n",
            ")\n",
        );
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // game.settings fold should extend to at least line 7 (closing `}` of difficulty).
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 7),
            "table property fold did not cover object value end: {:?}", folds
        );
    }
}
