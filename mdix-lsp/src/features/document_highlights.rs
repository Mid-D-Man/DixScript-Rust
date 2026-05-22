// mdix-lsp/src/features/document_highlight.rs
//! Document-highlight provider — highlights all occurrences of the symbol
//! under the cursor within the current file.
//!
//! - Identifiers: all tokens with the same name
//! - EnumAccess: all usages of the same enum name
//! Write kind = declaration site; Read kind = usage site.

use std::panic;

use tower_lsp::lsp_types::{DocumentHighlight, DocumentHighlightKind, Position, Range};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};

use crate::document::Document;
use crate::features::hover::token_and_index_at;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<Vec<DocumentHighlight>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc, pos)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("document_highlight panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>, pos: Position) -> Option<Vec<DocumentHighlight>> {
    let doc = doc?;
    let (token, _index) = token_and_index_at(&doc.tokens, pos)?;

    let target_name: String = match &token.token_type {
        TokenType::Identifier(n)            => n.clone(),
        TokenType::EnumAccess { enum_name, .. } => enum_name.clone(),
        _ => return None,
    };

    if target_name.is_empty() {
        return None;
    }

    let highlights: Vec<DocumentHighlight> = doc
        .tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let matches = match &t.token_type {
                TokenType::Identifier(n) => n.as_str() == target_name.as_str(),
                TokenType::EnumAccess { enum_name, .. } => {
                    enum_name.as_str() == target_name.as_str()
                }
                _ => false,
            };

            if !matches {
                return None;
            }

            let line = t.line.saturating_sub(1) as u32;
            let col  = t.column.saturating_sub(1) as u32;
            let len  = target_name.len() as u32;

            let kind = if is_declaration_site(&doc.tokens, i) {
                DocumentHighlightKind::WRITE
            } else {
                DocumentHighlightKind::READ
            };

            Some(DocumentHighlight {
                range: Range::new(
                    Position::new(line, col),
                    Position::new(line, col + len),
                ),
                kind: Some(kind),
            })
        })
        .collect();

    if highlights.is_empty() { None } else { Some(highlights) }
}

// ── Declaration-site heuristic ────────────────────────────────────────────────

/// Returns true when the token at `idx` is an identifier being declared:
///   - preceded by `~`                     → QuickFunc name
///   - preceded by `let`, `const`, `mut`   → variable
///   - followed by `from` / `from_cloud`   → import alias
///   - followed by `{` in @ENUMS           → enum type name
fn is_declaration_site(tokens: &[Token], idx: usize) -> bool {
    // Check previous token
    if let Some(prev) = idx.checked_sub(1).and_then(|i| tokens.get(i)) {
        match &prev.token_type {
            TokenType::Symbol('~') => return true,
            TokenType::Keyword(kw) if matches!(*kw, "let" | "const" | "mut") => return true,
            _ => {}
        }
    }

    // Check next token
    if let Some(next) = tokens.get(idx + 1) {
        match &next.token_type {
            TokenType::Keyword(kw) if matches!(*kw, "from" | "from_cloud") => return true,
            TokenType::Symbol('{') => return true,
            _ => {}
        }
    }

    false
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::run_pipeline;
    use crate::document::Document;
    use tower_lsp::lsp_types::{Position, Url};

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
    fn highlights_all_occurrences() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~calc<int>(x<int>) { return x }\n",
            ")\n",
            "@DATA(\n",
            "  result = calc(5)\n",
            ")"
        );
        let doc    = test_doc(src);
        let pos    = Position::new(1, 4);
        let result = provide(Some(&doc), pos);
        assert!(result.is_some());
        assert!(result.unwrap().len() >= 2);
    }

    #[test]
    fn no_highlights_for_non_identifier() {
        let src = "@DATA(\n  x = 42\n)";
        let doc  = test_doc(src);
        assert!(provide(Some(&doc), Position::new(1, 6)).is_none());
    }

    #[test]
    fn declaration_site_is_write() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~build<object>(name) { return { n = name } }\n",
            ")\n",
            "@DATA(\n",
            "  x = build(\"hi\")\n",
            ")"
        );
        let doc     = test_doc(src);
        let pos     = Position::new(1, 4); // `build` declaration
        let result  = provide(Some(&doc), pos).unwrap_or_default();
        let has_write = result
            .iter()
            .any(|h| h.kind == Some(DocumentHighlightKind::WRITE));
        assert!(has_write, "Expected at least one WRITE highlight for the declaration");
    }
}
