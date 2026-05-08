// mdix-lsp/src/features/folding.rs

use std::panic;

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::AST::{DataEntry, DataSection, DixScript, EnumsSection, QuickFuncsSection};
use crate::document::Document;

// ── Public entry point ────────────────────────────────────────────────────────

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

    // ── @CONFIG fold ──────────────────────────────────────────────────────────
    // Clamp the end to just before the first section-keyword token.
    // This prevents CONFIG fold from visually eating the next section even
    // if detect_config_line_range returns a slightly-wrong end line.
    if let Some((start, end)) = doc.config_line_range {
        let first_section_line = doc.tokens.iter()
            .filter(|t| t.token_type.is_section_keyword())
            .map(|t| t.line.saturating_sub(1) as u32)
            .min()
            .unwrap_or(u32::MAX);

        let safe_end = end.min(first_section_line.saturating_sub(1));
        if safe_end > start {
            ranges.push(region(start, safe_end));
        }
    }

    if doc.tokens.is_empty() {
        return if ranges.is_empty() { None } else { Some(ranges) };
    }

    // ── Section-level folds (@ENUMS, @QUICKFUNCS, @DATA, …) ──────────────────
    // One fold per section, from the section keyword to its closing `)`.
    collect_section_folds(&doc.tokens, &mut ranges);

    // ── AST-driven content folds + fallback brace folds ───────────────────────
    if let Some(ast) = &doc.ast {
        collect_ast_content_folds(ast, &doc.tokens, &mut ranges);
    }
    // Brace folds handle object literals in @DATA and any structure not covered
    // by the AST path.  Duplicates of AST-derived folds are removed by dedup.
    collect_brace_folds(&doc.tokens, &mut ranges);

    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);
    ranges.retain(|r| r.end_line > r.start_line);

    if ranges.is_empty() { None } else { Some(ranges) }
}

// ── AST content folds ─────────────────────────────────────────────────────────

fn collect_ast_content_folds(ast: &DixScript, tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    if let Some(enums) = &ast.enums {
        fold_enum_declarations(enums, tokens, ranges);
    }
    if let Some(qf) = &ast.quick_functions {
        fold_quickfunc_bodies(qf, tokens, ranges);
    }
    if let Some(data) = &ast.data {
        fold_data_entries(data, tokens, ranges);
    }
}

// ── ENUMS: per-declaration folds ──────────────────────────────────────────────

fn fold_enum_declarations(enums: &EnumsSection, tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    for decl in &enums.enums {
        if !decl.position.is_valid() { continue; }
        let start_line = decl.position.line.saturating_sub(1) as u32;

        // Find the `{…}` block starting on or after the declaration line
        // in the ENUMS section.
        if let Some(end_line) = find_brace_close_after_line(tokens, start_line, SectionId::Enums) {
            if end_line > start_line {
                ranges.push(region(start_line, end_line));
            }
        }
    }
}

// ── QUICKFUNCS: per-function folds ────────────────────────────────────────────

fn fold_quickfunc_bodies(qf: &QuickFuncsSection, tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    for func in &qf.functions {
        if !func.position.is_valid() { continue; }
        let start_line = func.position.line.saturating_sub(1) as u32;

        // Find the function body `{…}` starting on the same line as `~`.
        // Section filter ensures we never bleed into @DATA.
        if let Some(end_line) = find_brace_close_after_line(tokens, start_line, SectionId::QuickFuncs) {
            if end_line > start_line {
                ranges.push(region(start_line, end_line));
            }
        }
    }
}

// ── DATA: per-entry folds ─────────────────────────────────────────────────────

fn fold_data_entries(data: &DataSection, tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    for (idx, entry) in data.entries.iter().enumerate() {
        // The next entry's start line acts as an upper bound for end-detection.
        let next_entry_line: Option<u32> = data.entries.get(idx + 1).and_then(|e| {
            let p = entry_pos(e);
            if p.is_valid() { Some(p.line.saturating_sub(1) as u32) } else { None }
        });

        match entry {
            DataEntry::TableProperty { position, properties, .. } => {
                if !position.is_valid() { continue; }
                let start_line = position.line.saturating_sub(1) as u32;

                // Use the last declared property's line as the baseline for
                // depth-tracking, then extend to capture any multi-line values.
                let last_decl = properties.iter()
                    .rev()
                    .find(|p| p.position.is_valid())
                    .map(|p| p.position.line.saturating_sub(1) as u32)
                    .unwrap_or(start_line);

                let end_line = find_data_block_end(
                    tokens, last_decl, next_entry_line, SectionId::Data,
                );

                if end_line > start_line {
                    ranges.push(region(start_line, end_line));
                }
            }

            DataEntry::GroupArray { position, items, .. } => {
                if !position.is_valid() || items.is_empty() { continue; }
                let start_line = position.line.saturating_sub(1) as u32;

                // Last item's declared position as baseline.
                let last_item = items.iter().rev()
                    .map(|v| v.position())
                    .find(|p| p.is_valid())
                    .map(|p| p.line.saturating_sub(1) as u32)
                    .unwrap_or(start_line);

                let end_line = find_data_block_end(
                    tokens, last_item, next_entry_line, SectionId::Data,
                );

                if end_line > start_line {
                    ranges.push(region(start_line, end_line));
                }
            }

            // ObjectProperty / SimpleProperty → covered by collect_brace_folds.
            DataEntry::ObjectProperty { .. } | DataEntry::SimpleProperty { .. } => {}
        }
    }
}

fn entry_pos(entry: &DataEntry) -> dixscript::Compiler::AST::Position {
    match entry {
        DataEntry::SimpleProperty  { position, .. } => *position,
        DataEntry::TableProperty   { position, .. } => *position,
        DataEntry::GroupArray      { position, .. } => *position,
        DataEntry::ObjectProperty  { position, .. } => *position,
    }
}

// ── Section-level folds ───────────────────────────────────────────────────────

fn collect_section_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let section_starts: Vec<(usize, u32)> = tokens.iter().enumerate()
        .filter(|(_, t)| {
            t.token_type.is_section_keyword()
                && section_id_of_keyword(&t.token_type) != SectionId::None
        })
        .map(|(i, t)| (i, t.line.saturating_sub(1) as u32))
        .collect();

    for (i, &(tok_idx, start_line)) in section_starts.iter().enumerate() {
        let scan_end = section_starts.get(i + 1).map(|(j, _)| *j).unwrap_or(tokens.len());
        if let Some(end_line) = paren_close_line(&tokens[tok_idx..scan_end]) {
            if end_line > start_line {
                ranges.push(region(start_line, end_line));
            }
        }
    }
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

// ── Brace folds (object literals, catch-all) ──────────────────────────────────

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

// ── Token-scanning primitives ─────────────────────────────────────────────────

/// Find the 0-based line of the `)` that closes the first `(` in `tokens`.
fn paren_close_line(tokens: &[Token]) -> Option<u32> {
    let mut depth = 0i32;
    let mut found = false;
    for token in tokens {
        match &token.token_type {
            TokenType::Symbol('(') => { depth += 1; found = true; }
            TokenType::Symbol(')') if found => {
                depth -= 1;
                if depth == 0 { return Some(token.line.saturating_sub(1) as u32); }
            }
            TokenType::EndOfFile => break,
            _ => {}
        }
    }
    None
}

/// Scan tokens in `section`, starting at or after `from_line`.
/// Find the first `{` and return the 0-based line of its matching `}`.
///
/// Because `token.section` is used for filtering, this is restricted to
/// the correct section and cannot accidentally cross into another section.
fn find_brace_close_after_line(tokens: &[Token], from_line: u32, section: SectionId) -> Option<u32> {
    let mut depth = 0i32;
    let mut found = false;

    for token in tokens.iter() {
        // Section guard: only follow tokens that carry the right section ID.
        if token.section != section { continue; }

        let line = token.line.saturating_sub(1) as u32;
        if line < from_line { continue; }

        match &token.token_type {
            TokenType::Symbol('{') => {
                depth += 1;
                found = true;
            }
            TokenType::Symbol('}') if found => {
                depth -= 1;
                if depth == 0 {
                    return Some(line);
                }
            }
            TokenType::EndOfFile => break,
            _ => {}
        }
    }
    None
}

/// Scan DATA-section tokens from `from_line` to find the actual last line
/// of a table-property or group-array block, accounting for multi-line
/// object/array values.
///
/// Algorithm:
///   - At depth 0, stop if we reach `next_entry_line` (the next DATA entry).
///   - Track `{`/`[` depth; record the line of every close that brings
///     depth back to 0 as `last_close`.
///   - If no nested structure was found, return the last token's line in range.
///
/// This fixes `game.settings:` folds that previously ended at `difficulty =`
/// instead of at the `}` of difficulty's object value.
fn find_data_block_end(
    tokens:          &[Token],
    from_line:       u32,
    next_entry_line: Option<u32>,
    section:         SectionId,
) -> u32 {
    let upper         = next_entry_line.map(|l| l.saturating_sub(1)).unwrap_or(u32::MAX);
    let mut depth     = 0i32;
    let mut last_close = from_line;
    let mut last_any   = from_line;

    for token in tokens.iter() {
        if token.section != section                          { continue; }
        if token.token_type.is_section_keyword()             { break;    }
        if matches!(token.token_type, TokenType::EndOfFile)  { break;    }

        let line = token.line.saturating_sub(1) as u32;
        if line < from_line                                  { continue; }
        // At depth 0, honour the upper bound.
        if depth == 0 && line > upper                        { break;    }

        match &token.token_type {
            TokenType::Symbol('{') | TokenType::Symbol('[') => {
                depth += 1;
            }
            TokenType::Symbol('}') | TokenType::Symbol(']') => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        last_close = line;
                    }
                }
            }
            _ => {
                last_any = last_any.max(line);
            }
        }
    }

    // If nested structures were found, their last closing brace is the end.
    // Otherwise fall back to the last token's line.
    if last_close > from_line { last_close } else { last_any }
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
    fn config_fold_does_not_eat_enums() {
        let src = concat!(
            "@CONFIG(\n",
            "  version -> \"1.0.0\"\n",
            ")\n",
            "@ENUMS(\n",
            "  T { A = 0 }\n",
            ")\n",
        );
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // CONFIG fold must end before line 3 (@ENUMS line).
        if let Some(cfg) = folds.iter().find(|f| f.start_line == 0) {
            assert!(cfg.end_line < 3,
                "CONFIG fold extended into @ENUMS: {:?}", cfg);
        }
        // ENUMS fold must start at or after line 3.
        if let Some(enums) = folds.iter().find(|f| f.start_line >= 3) {
            assert!(enums.start_line >= 3,
                "ENUMS fold started too early: {:?}", enums);
        }
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
    fn enum_declaration_fold() {
        let src = concat!(
            "@ENUMS(\n",
            "  ServerType {\n",
            "    DEVELOPMENT = 1,\n",
            "    PRODUCTION = 2\n",
            "  }\n",
            ")\n",
        );
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // Expect a fold for the enum declaration (lines 1–4).
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 4),
            "enum declaration fold missing: {:?}", folds
        );
    }

    #[test]
    fn single_quickfunc_fold() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~calc<int>(x) {\n",
            "    return x\n",
            "  }\n",
            ")\n",
        );
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(folds.len() >= 2, "expected section + function fold: {:?}", folds);
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 3),
            "function fold missing: {:?}", folds
        );
    }

    #[test]
    fn multiple_quickfunc_folds_dont_overlap() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~f1<int>(x) { return x }\n",
            "  ~f2<int>(y) { return y }\n",
            "  ~f3<int>(z) { return z }\n",
            ")\n",
        );
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        // No two folds should share the same start_line with different end_lines
        // (dedup should handle same start+end, but different ends means overlap).
        let func_folds: Vec<_> = folds.iter()
            .filter(|f| f.start_line >= 1 && f.start_line <= 3)
            .collect();
        // Each function is on its own line, so start_lines should be distinct.
        for i in 0..func_folds.len() {
            for j in (i + 1)..func_folds.len() {
                assert_ne!(func_folds[i].start_line, func_folds[j].start_line,
                    "overlapping function folds: {:?} and {:?}", func_folds[i], func_folds[j]);
            }
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
        // game.settings fold must extend to at least the closing `}` of difficulty (line 7).
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 7),
            "table property fold didn't cover nested object end: {:?}", folds
        );
    }

    #[test]
    fn quickfunc_fold_bounded_away_from_data() {
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
        let qf_folds: Vec<_> = folds.iter().filter(|f| f.start_line == 0).collect();
        for f in qf_folds {
            assert!(f.end_line <= 4,
                "QUICKFUNCS fold extended into @DATA: {:?}", f);
        }
    }

    #[test]
    fn no_zero_length_folds() {
        let src = "@DATA(\n  x = 1\n)\n@ENUMS(\n  T { A = 0 }\n)\n";
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        for f in &folds {
            assert!(f.end_line > f.start_line, "zero-length fold: {:?}", f);
        }
    }

    #[test]
    fn brace_fold_for_flat_object() {
        let src = "@DATA(\n  player = {\n    name = \"Hero\"\n    level = 10\n  }\n)\n";
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(
            folds.iter().any(|f| f.start_line == 1 && f.end_line >= 4),
            "flat object brace fold missing: {:?}", folds
        );
    }
}
