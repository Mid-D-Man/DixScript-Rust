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
        tracing::debug!("folding: no tokens and no config range — skipping");
        return None;
    }

    let mut ranges: Vec<FoldingRange> = Vec::new();

    // ── @CONFIG fold ──────────────────────────────────────────────────────────
    // Clamp to just before the first real section token so that even if
    // detect_config_line_range overshoots, the CONFIG fold never eats @ENUMS.
    if let Some((start, raw_end)) = doc.config_line_range {
        let first_section = doc.tokens.iter()
            .filter(|t| t.token_type.is_section_keyword())
            .map(|t| t.line.saturating_sub(1) as u32)
            .min()
            .unwrap_or(u32::MAX);

        let safe_end = raw_end.min(first_section.saturating_sub(1));

        tracing::debug!(
            "folding: CONFIG raw=({}–{}) first_section_line={} clamped_end={}",
            start, raw_end, first_section, safe_end
        );

        if safe_end > start {
            ranges.push(region(start, safe_end));
            tracing::debug!("folding: CONFIG fold pushed ({}–{})", start, safe_end);
        } else {
            tracing::debug!("folding: CONFIG fold skipped (start >= safe_end)");
        }
    }

    if doc.tokens.is_empty() {
        tracing::debug!("folding: no tokens — returning CONFIG fold only");
        return if ranges.is_empty() { None } else { Some(ranges) };
    }

    // ── Section-level folds ───────────────────────────────────────────────────
    let section_count_before = ranges.len();
    collect_section_folds(&doc.tokens, &mut ranges);
    tracing::debug!(
        "folding: section folds added = {}",
        ranges.len() - section_count_before
    );

    // ── AST-driven content folds ──────────────────────────────────────────────
    if let Some(ast) = &doc.ast {
        let before = ranges.len();
        collect_ast_content_folds(ast, &doc.tokens, &mut ranges);
        tracing::debug!(
            "folding: AST content folds added = {}",
            ranges.len() - before
        );
    } else {
        tracing::debug!("folding: no AST available — skipping content folds");
    }

    // ── Brace folds ───────────────────────────────────────────────────────────
    let before_brace = ranges.len();
    collect_brace_folds(&doc.tokens, &mut ranges);
    tracing::debug!(
        "folding: brace folds added = {}",
        ranges.len() - before_brace
    );

    let total_before_dedup = ranges.len();
    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);
    ranges.retain(|r| r.end_line > r.start_line);
    let total_final = ranges.len();

    tracing::debug!(
        "folding: total before_dedup={} final={} (removed {})",
        total_before_dedup,
        total_final,
        total_before_dedup - total_final
    );

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

// ── ENUMS ─────────────────────────────────────────────────────────────────────

fn fold_enum_declarations(enums: &EnumsSection, tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    for decl in &enums.enums {
        if !decl.position.is_valid() {
            tracing::debug!("folding: enum '{}' — invalid position, skipping", decl.name);
            continue;
        }
        let start_line = decl.position.line.saturating_sub(1) as u32;

        match find_brace_close_after_line(tokens, start_line, SectionId::Enums) {
            Some(end_line) if end_line > start_line => {
                tracing::debug!(
                    "folding: enum '{}' fold ({}–{})",
                    decl.name, start_line, end_line
                );
                ranges.push(region(start_line, end_line));
            }
            Some(end_line) => {
                tracing::debug!(
                    "folding: enum '{}' single-line at {} — no fold",
                    decl.name, end_line
                );
            }
            None => {
                tracing::debug!(
                    "folding: enum '{}' — no closing brace found from line {}",
                    decl.name, start_line
                );
            }
        }
    }
}

// ── QUICKFUNCS ────────────────────────────────────────────────────────────────

fn fold_quickfunc_bodies(qf: &QuickFuncsSection, tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    for func in &qf.functions {
        if !func.position.is_valid() {
            tracing::debug!("folding: func '{}' — invalid position, skipping", func.name);
            continue;
        }
        let start_line = func.position.line.saturating_sub(1) as u32;

        match find_brace_close_after_line(tokens, start_line, SectionId::QuickFuncs) {
            Some(end_line) if end_line > start_line => {
                tracing::debug!(
                    "folding: func '~{}' fold ({}–{})",
                    func.name, start_line, end_line
                );
                ranges.push(region(start_line, end_line));
            }
            Some(_) => {
                tracing::debug!(
                    "folding: func '~{}' at line {} is single-line — no fold",
                    func.name, start_line
                );
            }
            None => {
                tracing::debug!(
                    "folding: func '~{}' — no closing brace found from line {}",
                    func.name, start_line
                );
            }
        }
    }
}

// ── DATA ──────────────────────────────────────────────────────────────────────

fn fold_data_entries(data: &DataSection, tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    for (idx, entry) in data.entries.iter().enumerate() {
        let next_entry_line: Option<u32> = data.entries.get(idx + 1).and_then(|e| {
            let p = entry_pos(e);
            if p.is_valid() { Some(p.line.saturating_sub(1) as u32) } else { None }
        });

        match entry {
            DataEntry::TableProperty { position, properties, .. } => {
                if !position.is_valid() { continue; }
                let start_line = position.line.saturating_sub(1) as u32;

                let last_decl = properties.iter()
                    .rev()
                    .find(|p| p.position.is_valid())
                    .map(|p| p.position.line.saturating_sub(1) as u32)
                    .unwrap_or(start_line);

                let end_line = find_data_block_end(
                    tokens, last_decl, next_entry_line, SectionId::Data,
                );

                tracing::debug!(
                    "folding: TableProperty start={} last_decl={} next_entry={:?} end={}",
                    start_line, last_decl, next_entry_line, end_line
                );

                if end_line > start_line {
                    ranges.push(region(start_line, end_line));
                }
            }

            DataEntry::GroupArray { position, items, .. } => {
                if !position.is_valid() || items.is_empty() { continue; }
                let start_line = position.line.saturating_sub(1) as u32;

                let last_item = items.iter().rev()
                    .map(|v| v.position())
                    .find(|p| p.is_valid())
                    .map(|p| p.line.saturating_sub(1) as u32)
                    .unwrap_or(start_line);

                let end_line = find_data_block_end(
                    tokens, last_item, next_entry_line, SectionId::Data,
                );

                tracing::debug!(
                    "folding: GroupArray start={} last_item={} next_entry={:?} end={}",
                    start_line, last_item, next_entry_line, end_line
                );

                if end_line > start_line {
                    ranges.push(region(start_line, end_line));
                }
            }

            DataEntry::ObjectProperty { position, .. } | DataEntry::SimpleProperty { position, .. } => {
                tracing::debug!(
                    "folding: SimpleProperty/ObjectProperty at line {} — brace folds handle this",
                    position.line.saturating_sub(1)
                );
            }
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
    let section_starts: Vec<(usize, u32, SectionId)> = tokens.iter().enumerate()
        .filter(|(_, t)| {
            t.token_type.is_section_keyword()
                && section_id_of_keyword(&t.token_type) != SectionId::None
        })
        .map(|(i, t)| (i, t.line.saturating_sub(1) as u32, section_id_of_keyword(&t.token_type)))
        .collect();

    for (i, &(tok_idx, start_line, sid)) in section_starts.iter().enumerate() {
        let scan_end = section_starts.get(i + 1).map(|(j, _, _)| *j).unwrap_or(tokens.len());

        match paren_close_line(&tokens[tok_idx..scan_end]) {
            Some(end_line) if end_line > start_line => {
                tracing::debug!(
                    "folding: section {:?} fold ({}–{})",
                    sid, start_line, end_line
                );
                ranges.push(region(start_line, end_line));
            }
            Some(_) => {
                tracing::debug!("folding: section {:?} at line {} is single-line", sid, start_line);
            }
            None => {
                tracing::debug!(
                    "folding: section {:?} at line {} — no closing paren found",
                    sid, start_line
                );
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

// ── Brace folds ───────────────────────────────────────────────────────────────

fn collect_brace_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let mut stack: Vec<(u32, SectionId)> = Vec::new();
    let mut count = 0usize;

    for token in tokens {
        match &token.token_type {
            TokenType::Symbol('{') => {
                let line = token.line.saturating_sub(1) as u32;
                stack.push((line, token.section));
            }
            TokenType::Symbol('}') => {
                if let Some((start_line, sid)) = stack.pop() {
                    let end_line = token.line.saturating_sub(1) as u32;
                    if end_line > start_line {
                        tracing::debug!(
                            "folding: brace fold {:?} ({}–{})",
                            sid, start_line, end_line
                        );
                        ranges.push(region(start_line, end_line));
                        count += 1;
                    }
                }
            }
            TokenType::EndOfFile => break,
            _ => {}
        }
    }

    tracing::debug!("folding: brace_folds total={}", count);
}

// ── Token scanning primitives ─────────────────────────────────────────────────

fn paren_close_line(tokens: &[Token]) -> Option<u32> {
    let mut depth = 0i32;
    let mut found = false;
    for token in tokens {
        match &token.token_type {
            TokenType::Symbol('(') => { depth += 1; found = true; }
            TokenType::Symbol(')') if found => {
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

/// Find the 0-based line of the closing `}` matching the first `{` at or after
/// `from_line` in `section`.  Section filtering prevents cross-section bleed.
fn find_brace_close_after_line(tokens: &[Token], from_line: u32, section: SectionId) -> Option<u32> {
    let mut depth = 0i32;
    let mut found = false;

    for token in tokens.iter() {
        if token.section != section { continue; }
        let line = token.line.saturating_sub(1) as u32;
        if line < from_line { continue; }

        match &token.token_type {
            TokenType::Symbol('{') => { depth += 1; found = true; }
            TokenType::Symbol('}') if found => {
                depth -= 1;
                if depth == 0 {
                    tracing::debug!(
                        "folding: find_brace_close {:?} from_line={} → end={}",
                        section, from_line, line
                    );
                    return Some(line);
                }
            }
            TokenType::EndOfFile => break,
            _ => {}
        }
    }

    tracing::debug!(
        "folding: find_brace_close {:?} from_line={} → None",
        section, from_line
    );
    None
}

/// Find the actual last line of a DATA block (table property or group array)
/// by scanning tokens from `from_line`, tracking `{`/`[` depth.
///
/// Returns the line of the last `}`/`]` that brings depth to zero, or the
/// line of the last token in range if no nested structure exists.
fn find_data_block_end(
    tokens:          &[Token],
    from_line:       u32,
    next_entry_line: Option<u32>,
    section:         SectionId,
) -> u32 {
    let upper        = next_entry_line.map(|l| l.saturating_sub(1)).unwrap_or(u32::MAX);
    let mut depth    = 0i32;
    let mut last_close = from_line;
    let mut last_any   = from_line;

    for token in tokens.iter() {
        if token.section != section                          { continue; }
        if token.token_type.is_section_keyword()             { break;    }
        if matches!(token.token_type, TokenType::EndOfFile)  { break;    }

        let line = token.line.saturating_sub(1) as u32;
        if line < from_line                                  { continue; }
        if depth == 0 && line > upper                        { break;    }

        match &token.token_type {
            TokenType::Symbol('{') | TokenType::Symbol('[') => { depth += 1; }
            TokenType::Symbol('}') | TokenType::Symbol(']') => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        last_close = line;
                    }
                }
            }
            _ => { last_any = last_any.max(line); }
        }
    }

    let result = if last_close > from_line { last_close } else { last_any };
    tracing::debug!(
        "folding: find_data_block_end from={} upper={} last_close={} last_any={} → {}",
        from_line, upper, last_close, last_any, result
    );
    result
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
    fn config_fold_does_not_eat_enums() {
        let src = "@CONFIG(\n  version -> \"1.0.0\"\n)\n@ENUMS(\n  T { A = 0 }\n)\n";
        let doc = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        if let Some(cfg) = folds.iter().find(|f| f.start_line == 0) {
            assert!(cfg.end_line < 3, "CONFIG fold ate ENUMS: {:?}", cfg);
        }
    }

    #[test]
    fn single_section_fold() {
        let src = "@DATA(\n  x = 1\n  y = 2\n)\n";
        let doc  = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 0 && f.end_line >= 3));
    }

    #[test]
    fn enum_declaration_fold() {
        let src = "@ENUMS(\n  ServerType {\n    DEV = 1,\n    PROD = 2\n  }\n)\n";
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line >= 4),
            "enum decl fold missing: {:?}", folds);
    }

    #[test]
    fn quickfunc_fold() {
        let src = "@QUICKFUNCS(\n  ~calc<int>(x) {\n    return x\n  }\n)\n";
        let doc  = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line >= 3),
            "quickfunc fold missing: {:?}", folds);
    }

    #[test]
    fn quickfunc_bounded_to_section() {
        let src = "@QUICKFUNCS(\n  ~add<int>(a,b) { return a }\n)\n@DATA(\n  r = add(1,2)\n)\n";
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        for f in folds.iter().filter(|f| f.start_line <= 1) {
            assert!(f.end_line <= 2, "QF fold bled into DATA: {:?}", f);
        }
    }

    #[test]
    fn table_property_covers_nested_objects() {
        let src = concat!(
            "@DATA(\n",
            "  game.settings:\n",
            "    player = {\n",
            "      hp = 100\n",
            "    },\n",
            "    difficulty = {\n",
            "      mult = 1.5f\n",
            "    }\n",
            ")\n"
        );
        let doc   = test_doc(src);
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 1 && f.end_line >= 7),
            "table fold didn't cover nested objects: {:?}", folds);
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
}
