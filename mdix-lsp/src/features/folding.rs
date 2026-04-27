// mdix-lsp/src/features/folding.rs
//! Code folding provider.
//! Wrapped in catch_unwind to prevent deep AST panics from killing the server.

use std::panic;

use tower_lsp::lsp_types::{FoldingRange, FoldingRangeKind};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::AST::{DataEntry, DataSection};
use crate::document::Document;

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

    collect_section_folds(&doc.tokens, &mut ranges);
    collect_brace_folds(&doc.tokens, &mut ranges);

    if let Some(ast) = &doc.ast {
        if let Some(data) = &ast.data {
            collect_data_entry_folds(data, &mut ranges);
        }
    }

    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);

    if ranges.is_empty() { None } else { Some(ranges) }
}

fn collect_section_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        if !tok.token_type.is_section_keyword() { i += 1; continue; }

        let start_line = tok.line.saturating_sub(1) as u32;

        let open_idx = tokens[i..]
            .iter().enumerate().skip(1).take(5)
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

fn collect_brace_folds(tokens: &[Token], ranges: &mut Vec<FoldingRange>) {
    let mut stack: Vec<u32> = Vec::new();
    for tok in tokens {
        match &tok.token_type {
            TokenType::Symbol('{') => stack.push(tok.line.saturating_sub(1) as u32),
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

fn region(start_line: u32, end_line: u32) -> FoldingRange {
    FoldingRange {
        start_line,
        end_line,
        kind: Some(FoldingRangeKind::Region),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::run_pipeline;
    use crate::document::Document;
    use tower_lsp::lsp_types::Url;

    fn test_doc(source: &str) -> Document {
        let mut doc = Document::new(Url::parse("file:///test.mdix").unwrap(), source.to_string(), 0);
        run_pipeline(&mut doc);
        doc
    }

    #[test] fn folding_none_doc() { assert!(provide(None).is_none()); }

    #[test]
    fn section_fold_data() {
        let doc = test_doc("@DATA(\n  x = 1\n  y = 2\n)");
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(folds.iter().any(|f| f.start_line == 0 && f.end_line == 3),
            "expected @DATA fold: {:?}", folds);
    }

    #[test]
    fn no_single_line_folds() {
        let doc = test_doc("@DATA(\n  x = 1\n)");
        let folds = provide(Some(&doc)).unwrap_or_default();
        assert!(folds.iter().all(|f| f.end_line > f.start_line));
    }
}
