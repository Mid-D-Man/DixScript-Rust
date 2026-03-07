//! Go-to-definition provider.
//!
//! Handles three cases:
//!   1. Cursor on a QuickFunc call → jumps to its ~name declaration
//!   2. Cursor on an EnumAccess token → jumps to the enum field declaration
//!   3. Cursor on an import path string → opens the imported .mdix file

use tower_lsp::lsp_types::{
    GotoDefinitionResponse, Location, Position, Range, Url,
};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use crate::document::Document;
use crate::features::hover::token_at;

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<GotoDefinitionResponse> {
    let doc   = doc?;
    let token = token_at(&doc.tokens, pos)?;

    match &token.token_type {
        TokenType::Identifier(name) => goto_quickfunc(doc, name),
        TokenType::EnumAccess { enum_name, value } => goto_enum_field(doc, enum_name, value),
        TokenType::String(path) | TokenType::StringSingle(path) => goto_import(doc, path),
        _ => None,
    }
}

// ── QuickFunc definition ───────────────────────────────────────────────────────

fn goto_quickfunc(doc: &Document, name: &str) -> Option<GotoDefinitionResponse> {
    let ast = doc.ast.as_ref()?;
    let qf  = ast.quick_functions.as_ref()?;

    for func in &qf.functions {
        if func.name == name {
            let line = func.position.line.saturating_sub(1) as u32;
            let col  = func.position.column.saturating_sub(1) as u32;
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri:   doc.uri.clone(),
                range: Range::new(
                    Position::new(line, col),
                    Position::new(line, col + name.len() as u32 + 1), // +1 for ~
                ),
            }));
        }
    }

    None
}

// ── Enum field definition ──────────────────────────────────────────────────────

fn goto_enum_field(
    doc: &Document,
    enum_name: &str,
    field_name: &str,
) -> Option<GotoDefinitionResponse> {
    let ast   = doc.ast.as_ref()?;
    let enums = ast.enums.as_ref()?;

    for decl in &enums.enums {
        if decl.name != enum_name {
            continue;
        }
        for field in &decl.fields {
            if field.name != field_name {
                continue;
            }
            let line = field.position.line.saturating_sub(1) as u32;
            let col  = field.position.column.saturating_sub(1) as u32;
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri:   doc.uri.clone(),
                range: Range::new(
                    Position::new(line, col),
                    Position::new(line, col + field_name.len() as u32),
                ),
            }));
        }
    }

    None
}

// ── Import path definition ─────────────────────────────────────────────────────

fn goto_import(doc: &Document, path: &str) -> Option<GotoDefinitionResponse> {
    // Resolve the import path relative to the current document's directory.
    let base = doc.uri.to_file_path().ok()?;
    let dir  = base.parent()?;
    let target = dir.join(path);

    if !target.exists() {
        return None;
    }

    let target_uri = Url::from_file_path(target).ok()?;
    Some(GotoDefinitionResponse::Scalar(Location {
        uri:   target_uri,
        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
    }))
  }
