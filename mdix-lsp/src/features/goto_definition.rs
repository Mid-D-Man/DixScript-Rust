use tower_lsp::lsp_types::{
    GotoDefinitionResponse, Location, Position, Range, Url,
};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;
use crate::features::hover::token_and_index_at;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<GotoDefinitionResponse> {
    let doc = doc?;
    let (token, index) = token_and_index_at(&doc.tokens, pos)?;

    // Clone to release the borrow on doc.tokens before we do further lookups.
    let token_type = token.token_type.clone();

    match &token_type {
        TokenType::Identifier(name) => {
            // 1. Part of EnumName.FieldName? → jump to the field declaration.
            if let Some(r) = goto_enum_from_context(doc, name, index) {
                return Some(r);
            }
            // 2. QuickFunc call site → jump to ~name declaration.
            if let Some(r) = goto_quickfunc(doc, name) {
                return Some(r);
            }
            // 3. DATA property reference → jump to its definition.
            goto_data_property(doc, name, pos.line as usize + 1)
        }

        // EnumAccess tokens are produced by the semantic enhancer path;
        // handle them if they happen to appear in doc.tokens.
        TokenType::EnumAccess { enum_name, value } => {
            goto_enum_field(doc, enum_name, value)
        }

        // Import path string → open the target file.
        TokenType::String(path) | TokenType::StringSingle(path) => {
            goto_import(doc, path)
        }

        _ => None,
    }
}

// ── Enum access from raw identifier context ───────────────────────────────────
//
// The basic tokeniser emits  Identifier('AIType') Symbol('.') Identifier('BOSS').
// When the cursor is on the second identifier we look two positions back to
// find the enum name, then jump to its field declaration.

fn goto_enum_from_context(
    doc: &Document,
    field_name: &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    if token_index < 2 { return None; }

    let dot_tok = doc.tokens.get(token_index - 1)?;
    if !matches!(dot_tok.token_type, TokenType::Symbol('.')) { return None; }

    let enum_tok = doc.tokens.get(token_index - 2)?;
    let TokenType::Identifier(enum_name) = &enum_tok.token_type else { return None };

    // Verify the name is actually declared in @ENUMS.
    let ast   = doc.ast.as_ref()?;
    let enums = ast.enums.as_ref()?;
    if !enums.enums.iter().any(|e| e.name == *enum_name) {
        return None;
    }

    goto_enum_field(doc, enum_name, field_name)
}

// ── QuickFunc declaration ─────────────────────────────────────────────────────

fn goto_quickfunc(doc: &Document, name: &str) -> Option<GotoDefinitionResponse> {
    let ast = doc.ast.as_ref()?;
    let qf  = ast.quick_functions.as_ref()?;

    for func in &qf.functions {
        if func.name != name { continue; }

        // func.position points to the '~' prefix (or just before the name).
        // With Option B the line numbers match the original source exactly.
        let line = func.position.line.saturating_sub(1) as u32;
        let col  = func.position.column.saturating_sub(1) as u32;

        return Some(GotoDefinitionResponse::Scalar(Location {
            uri:   doc.uri.clone(),
            range: Range::new(
                Position::new(line, col),
                // +1 for the leading ~
                Position::new(line, col + 1 + name.len() as u32),
            ),
        }));
    }
    None
}

// ── DATA property definition ──────────────────────────────────────────────────
//
// Scan the DATA-section tokens for an identifier followed by `=`, `:`, or `::`
// that is NOT on the caller's line (so we don't "jump to self").

fn goto_data_property(
    doc: &Document,
    name: &str,
    caller_line: usize, // 1-based, matches token.line
) -> Option<GotoDefinitionResponse> {
    let tokens = &doc.tokens;

    for (i, token) in tokens.iter().enumerate() {
        if token.section != SectionId::Data     { continue; }
        let TokenType::Identifier(id) = &token.token_type else { continue };
        if id.as_str() != name                  { continue; }
        if token.line == caller_line             { continue; } // skip self-reference

        // Peek past optional type annotation to find the defining operator.
        let next_op = tokens
            .iter()
            .skip(i + 1)
            .find(|t| !matches!(
                t.token_type,
                TokenType::Symbol('<')
                | TokenType::Symbol('>')
                | TokenType::DataType(_)
                | TokenType::Identifier(_) // type annotation identifier
            ));

        let is_definition = matches!(
            next_op.map(|t| &t.token_type),
            Some(TokenType::Symbol('='))
            | Some(TokenType::Symbol(':'))
            | Some(TokenType::DoubleColon)
            | Some(TokenType::DataType(_))
        );

        if !is_definition { continue; }

        // With Option B, token.line already reflects the original source line.
        let line = token.line.saturating_sub(1) as u32;
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

// ── Enum field declaration ────────────────────────────────────────────────────

fn goto_enum_field(
    doc: &Document,
    enum_name: &str,
    field_name: &str,
) -> Option<GotoDefinitionResponse> {
    let ast   = doc.ast.as_ref()?;
    let enums = ast.enums.as_ref()?;

    for decl in &enums.enums {
        if !decl.name.eq_ignore_ascii_case(enum_name) { continue; }
        for field in &decl.fields {
            if !field.name.eq_ignore_ascii_case(field_name) { continue; }

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

// ── Import path → target file ─────────────────────────────────────────────────

fn goto_import(doc: &Document, path: &str) -> Option<GotoDefinitionResponse> {
    // Cloud imports have no local file to jump to.
    if path.starts_with("http://") || path.starts_with("https://") {
        return None;
    }

    let base = doc.uri.to_file_path().ok()?;
    let dir  = base.parent()?;

    let candidates = if path.ends_with(".mdix") {
        vec![dir.join(path)]
    } else {
        vec![dir.join(path), dir.join(format!("{}.mdix", path))]
    };

    for candidate in candidates {
        if candidate.exists() {
            if let Ok(uri) = Url::from_file_path(candidate) {
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                }));
            }
        }
    }
    None
}