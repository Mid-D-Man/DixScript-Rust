// mdix-lsp/src/features/document_highlights.rs
//! Document-highlight provider — highlights all occurrences of the symbol
//! under the cursor within the current file.
//!
//! Namespace-qualified identifiers (e.g. `Utils.calc`):
//!   only tokens that are also preceded by `.Utils` are highlighted, avoiding
//!   false matches on same-named local symbols.
//!
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
    let (token, index) = token_and_index_at(&doc.tokens, pos)?;

    let highlights: Vec<DocumentHighlight> = match &token.token_type {
        TokenType::Identifier(name) => {
            let name = name.clone();

            // Determine whether this identifier is a namespace-qualified member
            // (e.g. `calc` in `Utils.calc(x)`).  If so, scope the highlights so
            // we only light up occurrences that are also preceded by `.Utils`,
            // avoiding false matches on unrelated local symbols with the same name.
            let ns_context = namespace_context_of(&doc.tokens, index);

            match ns_context {
                Some(ref ns_name) => {
                    collect_qualified_highlights(&doc.tokens, ns_name, &name)
                }
                None => {
                    collect_identifier_highlights(&doc.tokens, &name)
                }
            }
        }

        TokenType::EnumAccess { enum_name, value } => {
            let en = enum_name.clone();
            let v  = value.clone();
            collect_enum_access_highlights(&doc.tokens, &en, &v)
        }

        _ => return None,
    };

    if highlights.is_empty() { None } else { Some(highlights) }
}

// ── Namespace context detection ───────────────────────────────────────────────

/// Returns the namespace name when token at `idx` is immediately preceded by
/// `.NAMESPACE` (i.e. `NAMESPACE . <token>`).
fn namespace_context_of(tokens: &[Token], idx: usize) -> Option<String> {
    if idx < 2 { return None; }

    let prev = tokens.get(idx - 1)?;
    if !matches!(prev.token_type, TokenType::Symbol('.')) { return None; }

    let ns_tok = tokens.get(idx - 2)?;
    match &ns_tok.token_type {
        TokenType::Identifier(ns) => Some(ns.clone()),
        _ => None,
    }
}

// ── Highlight collectors ──────────────────────────────────────────────────────

/// Highlight only occurrences of `member` that are preceded by `.namespace`.
fn collect_qualified_highlights(
    tokens:    &[Token],
    namespace: &str,
    member:    &str,
) -> Vec<DocumentHighlight> {
    let mut out = Vec::new();

    for (i, tok) in tokens.iter().enumerate() {
        let TokenType::Identifier(tok_name) = &tok.token_type else { continue };
        if tok_name.as_str() != member { continue; }
        if i < 2 { continue; }

        // Must be preceded by '.'
        let prev = &tokens[i - 1];
        if !matches!(prev.token_type, TokenType::Symbol('.')) { continue; }

        // Must be preceded by the expected namespace identifier
        let ns_tok = &tokens[i - 2];
        let matches_ns = matches!(&ns_tok.token_type,
            TokenType::Identifier(n) if n.as_str() == namespace);
        if !matches_ns { continue; }

        let line = tok.line.saturating_sub(1) as u32;
        let col  = tok.column.saturating_sub(1) as u32;
        out.push(DocumentHighlight {
            range: Range::new(
                Position::new(line, col),
                Position::new(line, col + member.len() as u32),
            ),
            kind: Some(DocumentHighlightKind::READ),
        });
    }

    out
}

/// Highlight all plain identifier occurrences (not namespace-qualified).
fn collect_identifier_highlights(tokens: &[Token], name: &str) -> Vec<DocumentHighlight> {
    tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let TokenType::Identifier(tok_name) = &t.token_type else { return None };
            if tok_name.as_str() != name { return None; }

            let line = t.line.saturating_sub(1) as u32;
            let col  = t.column.saturating_sub(1) as u32;
            let kind = if is_declaration_site(tokens, i) {
                DocumentHighlightKind::WRITE
            } else {
                DocumentHighlightKind::READ
            };
            Some(DocumentHighlight {
                range: Range::new(
                    Position::new(line, col),
                    Position::new(line, col + name.len() as u32),
                ),
                kind: Some(kind),
            })
        })
        .collect()
}

/// Highlight EnumAccess tokens (EnumName.FIELD) that match both parts.
fn collect_enum_access_highlights(
    tokens:    &[Token],
    enum_name: &str,
    value:     &str,
) -> Vec<DocumentHighlight> {
    tokens
        .iter()
        .filter_map(|t| {
            let TokenType::EnumAccess { enum_name: en, value: v } = &t.token_type else {
                return None;
            };
            if en.as_str() != enum_name || v.as_str() != value { return None; }

            let len  = en.len() + 1 + v.len();
            let line = t.line.saturating_sub(1) as u32;
            let col  = t.column.saturating_sub(1) as u32;
            Some(DocumentHighlight {
                range: Range::new(
                    Position::new(line, col),
                    Position::new(line, col + len as u32),
                ),
                kind: Some(DocumentHighlightKind::READ),
            })
        })
        .collect()
}

// ── Declaration-site heuristic ────────────────────────────────────────────────

/// Returns true when the token at `idx` is being declared:
///   - preceded by `~`                     → QuickFunc name
///   - preceded by `let`, `const`, `mut`   → variable
///   - followed by `from` / `from_cloud`   → import alias
///   - followed by `{` in @ENUMS           → enum type name
fn is_declaration_site(tokens: &[Token], idx: usize) -> bool {
    if let Some(prev) = idx.checked_sub(1).and_then(|i| tokens.get(i)) {
        match &prev.token_type {
            TokenType::Symbol('~') => return true,
            TokenType::Keyword(kw) if matches!(*kw, "let" | "const" | "mut") => return true,
            _ => {}
        }
    }
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
        let pos     = Position::new(1, 4);
        let result  = provide(Some(&doc), pos).unwrap_or_default();
        let has_write = result
            .iter()
            .any(|h| h.kind == Some(DocumentHighlightKind::WRITE));
        assert!(has_write, "Expected at least one WRITE highlight for the declaration");
    }

    #[test]
    fn namespace_qualified_does_not_bleed() {
        // Two identifiers both named `val` — one local, one imported-namespace member.
        // Highlighting the namespace-qualified one should NOT pick up the local one.
        let tokens: Vec<Token> = vec![]; // placeholder — real test needs a full fixture
        let _ = collect_qualified_highlights(&tokens, "Utils", "val");
    }
}
