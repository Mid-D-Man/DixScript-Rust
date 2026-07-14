//! Find-all-references provider.
//!
//! Returns every location in the document where the symbol under the cursor
//! is referenced. Mirrors document_highlights but returns Location objects.

use std::panic;

use tower_lsp::lsp_types::{Location, Position, Range, Url};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;

use crate::document::Document;
use crate::features::hover::token_and_index_at;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(
    doc:                   Option<&Document>,
    uri:                   &Url,
    pos:                   Position,
    include_declaration:   bool,
) -> Option<Vec<Location>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        provide_inner(doc, uri, pos, include_declaration)
    }));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("references panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(
    doc:                 Option<&Document>,
    uri:                 &Url,
    pos:                 Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    let doc = doc?;
    let (token, _) = token_and_index_at(&doc.tokens, pos)?;

    let locations: Vec<Location> = match &token.token_type {
        TokenType::Identifier(name) => {
            let name     = name.clone();
            let is_param = is_parameter(doc, &name, token.section);
            collect_identifier_locations(&doc.tokens, &name, is_param, uri)
        }



        _ => return None,
    };

    if locations.is_empty() {
        return None;
    }

    // Optionally exclude the declaration itself.
    // (LSP clients pass `includeDeclaration`; we honour it if false.)
    if include_declaration {
        Some(locations)
    } else {
        // A heuristic: declaration is the first occurrence (lowest line number).
        let mut locs = locations;
        if locs.len() > 1 {
            locs.sort_by_key(|l| (l.range.start.line, l.range.start.character));
            locs.remove(0);
        }
        Some(locs)
    }
}

// ── Collectors ────────────────────────────────────────────────────────────────

fn collect_identifier_locations(
    tokens:   &[Token],
    name:     &str,
    is_param: bool,
    uri:      &Url,
) -> Vec<Location> {
    tokens
        .iter()
        .filter_map(|t| {
            if let TokenType::Identifier(tok_name) = &t.token_type {
                if tok_name.as_str() == name {
                    if is_param && t.section != SectionId::QuickFuncs {
                        return None;
                    }
                    return Some(make_location(uri, t, name.len()));
                }
            }
            None
        })
        .collect()
}


// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_location(uri: &Url, token: &Token, len: usize) -> Location {
    let line = token.line.saturating_sub(1) as u32;
    let col  = token.column.saturating_sub(1) as u32;
    Location {
        uri: uri.clone(),
        range: Range::new(
            Position::new(line, col),
            Position::new(line, col + len as u32),
        ),
    }
}

fn is_parameter(doc: &Document, name: &str, origin: SectionId) -> bool {
    if origin != SectionId::QuickFuncs {
        return false;
    }
    doc.ast
        .as_ref()
        .and_then(|a| a.quick_functions.as_ref())
        .map(|qf| {
            qf.functions
                .iter()
                .any(|f| f.parameters.iter().any(|p| p.name == name))
        })
        .unwrap_or(false)
      }
