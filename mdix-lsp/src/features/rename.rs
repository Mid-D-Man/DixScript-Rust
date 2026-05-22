// mdix-lsp/src/features/rename.rs
//! Rename provider.
//!
//! Implements prepareRename (validates the symbol under the cursor can be renamed)
//! and rename (collects all TextEdits to rename every occurrence).
//!
//! Scoping mirrors document_highlights: parameters are scoped to @QUICKFUNCS,
//! everything else is document-wide.

use std::collections::HashMap;
use std::panic;

use tower_lsp::lsp_types::{
    Position, PrepareRenameResponse, Range, TextEdit, Url, WorkspaceEdit,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;

use crate::document::Document;
use crate::features::hover::token_and_index_at;

// ── prepareRename ─────────────────────────────────────────────────────────────

/// Validate that the symbol under the cursor can be renamed and return its range.
/// Returning `None` tells the editor to show "nothing to rename" without an error.
pub fn prepare(doc: Option<&Document>, pos: Position) -> Option<PrepareRenameResponse> {
    panic::catch_unwind(panic::AssertUnwindSafe(|| prepare_inner(doc, pos)))
        .ok()
        .flatten()
}

fn prepare_inner(doc: Option<&Document>, pos: Position) -> Option<PrepareRenameResponse> {
    let doc = doc?;
    let (token, _) = token_and_index_at(&doc.tokens, pos)?;

    // Only plain identifiers are renameable.
    let name = match &token.token_type {
        TokenType::Identifier(n) => n.clone(),
        _ => return None,
    };

    // Section keywords and built-in static objects are not renameable.
    if is_builtin_or_section(&name) {
        return None;
    }

    let line = token.line.saturating_sub(1) as u32;
    let col  = token.column.saturating_sub(1) as u32;

    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: Range::new(
            Position::new(line, col),
            Position::new(line, col + name.len() as u32),
        ),
        placeholder: name,
    })
}

// ── rename ────────────────────────────────────────────────────────────────────

pub fn provide(
    doc:      Option<&Document>,
    uri:      &Url,
    pos:      Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        provide_inner(doc, uri, pos, new_name)
    }));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("rename panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(
    doc:      Option<&Document>,
    uri:      &Url,
    pos:      Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    let doc = doc?;

    if !is_valid_identifier(new_name) {
        tracing::debug!("rename rejected: '{}' is not a valid identifier", new_name);
        return None;
    }

    let (token, _) = token_and_index_at(&doc.tokens, pos)?;

    let name = match &token.token_type {
        TokenType::Identifier(n) => n.clone(),
        _ => return None,
    };

    if is_builtin_or_section(&name) {
        return None;
    }

    let is_param = is_parameter(doc, &name, token.section);
    let edits    = collect_edits(&doc.tokens, &name, is_param, new_name);

    if edits.is_empty() {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

// ── Edit collection ───────────────────────────────────────────────────────────

fn collect_edits(
    tokens:   &[Token],
    name:     &str,
    is_param: bool,
    new_name: &str,
) -> Vec<TextEdit> {
    tokens
        .iter()
        .filter_map(|t| {
            if let TokenType::Identifier(tok_name) = &t.token_type {
                if tok_name.as_str() != name {
                    return None;
                }
                // Parameters only renamed within @QUICKFUNCS.
                if is_param && t.section != SectionId::QuickFuncs {
                    return None;
                }
                let line = t.line.saturating_sub(1) as u32;
                let col  = t.column.saturating_sub(1) as u32;
                Some(TextEdit {
                    range: Range::new(
                        Position::new(line, col),
                        Position::new(line, col + name.len() as u32),
                    ),
                    new_text: new_name.to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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

/// Identifiers that must not be renamed (built-in static objects, DLM modules).
fn is_builtin_or_section(name: &str) -> bool {
    matches!(
        name,
        "Math"       | "DateTime" | "Array"     | "Random"
        | "Guid"     | "IpAddress"| "Enum"      | "Dix"
        | "DCompressor" | "DEncryptor" | "DAuditor"
    )
}

/// A valid DixScript identifier: starts with letter/underscore, rest alphanumeric/underscore.
pub fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
               }
