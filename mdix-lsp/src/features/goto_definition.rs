use tower_lsp::lsp_types::{
    GotoDefinitionResponse, Location, Position, Range, Url,
};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;
use crate::features::hover::token_at;

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<GotoDefinitionResponse> {
    let doc   = doc?;
    let token = token_at(&doc.tokens, pos)?;

    match &token.token_type {
        TokenType::Identifier(name) => {
            // 1. QuickFunc call site → jump to ~name declaration
            if let Some(r) = goto_quickfunc(doc, name) {
                return Some(r);
            }
            // 2. DATA property usage → jump to its definition line
            goto_data_property(doc, name, pos.line as usize + 1)
        }
        TokenType::EnumAccess { enum_name, value } => {
            goto_enum_field(doc, enum_name, value)
        }
        TokenType::String(path) | TokenType::StringSingle(path) => {
            // Could be an import path
            goto_import(doc, path)
        }
        _ => None,
    }
}

// ── QuickFunc definition ───────────────────────────────────────────────────────

fn goto_quickfunc(doc: &Document, name: &str) -> Option<GotoDefinitionResponse> {
    let ast = doc.ast.as_ref()?;
    let qf  = ast.quick_functions.as_ref()?;

    for func in &qf.functions {
        if func.name == name {
            let line = (func.position.line.saturating_sub(1) as u32)
                .saturating_add(doc.config_line_offset as u32);
            let col  = func.position.column.saturating_sub(1) as u32;
            // +1 for the leading ~
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri:   doc.uri.clone(),
                range: Range::new(
                    Position::new(line, col),
                    Position::new(line, col + 1 + name.len() as u32),
                ),
            }));
        }
    }
    None
}

// ── DATA property definition ───────────────────────────────────────────────────
//
// Finds the first token in the DATA section that looks like a property
// *definition* (an Identifier followed by `=`, `:`, or `::`) rather than
// a property *use* inside a function argument.

fn goto_data_property(
    doc: &Document,
    name: &str,
    caller_line: usize,     // 1-based line of the token we jumped from
) -> Option<GotoDefinitionResponse> {
    let tokens = &doc.tokens;

    for (i, token) in tokens.iter().enumerate() {
        if token.section != SectionId::Data {
            continue;
        }
        let TokenType::Identifier(id) = &token.token_type else { continue };
        if id.to_string() != name.to_string() {
            continue;
        }

        // Skip if this is the caller itself
        if token.line == caller_line {
            continue;
        }

        // Peek at the next meaningful token to decide if this is a definition.
        // A definition is: Identifier followed by `=`, `:`, `::`, or `<`
        let next_meaningful = tokens
            .iter()
            .skip(i + 1)
            .find(|t| !matches!(t.token_type, TokenType::Symbol('<') | TokenType::DataType(_)));

        let is_def = match next_meaningful.map(|t| &t.token_type) {
            Some(TokenType::Symbol('='))  => true,
            Some(TokenType::Symbol(':'))  => true,
            Some(TokenType::DoubleColon)  => true,
            Some(TokenType::Symbol('<'))  => true,  // type-annotated: name<int> = ...
            Some(TokenType::DataType(_))  => true,
            _ => false,
        };

        if !is_def {
            continue;
        }

        let line = (token.line.saturating_sub(1) as u32)
            .saturating_add(doc.config_line_offset as u32);
        let col  = token.column.saturating_sub(1) as u32;

        return Some(GotoDefinitionResponse::Scalar(Location {
            uri:   doc.uri.clone(),
            range: Range::new(
                Position::new(line, col),
                Position::new(line, col + name.len() as u32),
            ),
        }));
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
        if !decl.name.eq_ignore_ascii_case(enum_name) {
            continue;
        }
        for field in &decl.fields {
            if !field.name.eq_ignore_ascii_case(field_name) {
                continue;
            }
            let line = (field.position.line.saturating_sub(1) as u32)
                .saturating_add(doc.config_line_offset as u32);
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
    // cloud imports have no local file to jump to
    if path.starts_with("http://") || path.starts_with("https://") {
        return None;
    }

    let base = doc.uri.to_file_path().ok()?;
    let dir  = base.parent()?;

    // Try the path as-is, then with .mdix extension if missing
    let candidates = if path.ends_with(".mdix") {
        vec![dir.join(path)]
    } else {
        vec![dir.join(path), dir.join(format!("{}.mdix", path))]
    };

    for candidate in candidates {
        if candidate.exists() {
            if let Ok(target_uri) = Url::from_file_path(candidate) {
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri:   target_uri,
                    range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                }));
            }
        }
    }

    None
}