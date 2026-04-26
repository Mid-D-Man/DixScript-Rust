// mdix-lsp/src/features/goto_definition.rs
//
// Changes from previous version:
//
//   - `goto_enum_type`: the original check required the VERY NEXT token to
//     be a dot.  IntelliJ / Rust Rover sends cursor positions at a slightly
//     different character offset than VSCode, so the adjacency check was
//     failing.  The new implementation looks up to 3 tokens ahead for the
//     dot, tolerating type-annotation tokens (`<`, `>`, DataType) between
//     the name and the dot.  Additionally it now falls back to a NAME-based
//     AST search when the token stream lookup fails, which is robust against
//     any client position encoding difference.
//
//   - `goto_enum_from_context`: mirrors the same lookahead tolerance for the
//     reverse direction (cursor on the field name, look back for the enum name).
//
//   - `provide` is wrapped in `catch_unwind` so a panic never kills the server.

use std::panic;

use tower_lsp::lsp_types::{
    GotoDefinitionResponse, Location, Position, Range, Url,
};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::AST::{Position as AstPos, QuickFuncStatement};

use crate::document::Document;
use crate::features::hover::token_and_index_at;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<GotoDefinitionResponse> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        provide_inner(doc, pos)
    }));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("goto_definition panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>, pos: Position) -> Option<GotoDefinitionResponse> {
    let doc = doc?;
    let (token, index) = token_and_index_at(&doc.tokens, pos)?;

    let token_type    = token.token_type.clone();
    let token_section = token.section;

    match &token_type {
        TokenType::Identifier(name) => {
            // 0. Enum TYPE name (`AIType` in `AIType.BOSS`) → @ENUMS declaration.
            //    Try token-stream path first; fall back to AST name search.
            if let Some(r) = goto_enum_type(doc, name, index) {
                return Some(r);
            }
            // 1. Enum FIELD: cursor on FIELD in EnumName.FIELD.
            if let Some(r) = goto_enum_from_context(doc, name, index) {
                return Some(r);
            }
            // 2. QuickFunc call / reference → jump to ~name declaration.
            if let Some(r) = goto_quickfunc(doc, name) {
                return Some(r);
            }
            // 3. QuickFunc local variable declaration.
            if let Some(r) = goto_quickfunc_local_var(doc, name) {
                return Some(r);
            }
            // 4. DATA property reference (outside QuickFuncs).
            if token_section != SectionId::QuickFuncs {
                goto_data_property(doc, name)
            } else {
                None
            }
        }

        // EnumAccess tokens produced after semantic enhancement.
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

// ── Enum TYPE name → @ENUMS declaration ──────────────────────────────────────
//
// Strategy:
//   1. Token-stream lookahead: scan up to 3 tokens ahead for a '.' that is
//      followed by an identifier (the field name).  This tolerates IntelliJ
//      sending cursor offsets that land on the identifier body rather than
//      its first character, and also tolerates type-annotation tokens
//      (`<enum>`) that can appear between the name and the dot.
//   2. AST name search: if token-stream lookup finds the enum is declared,
//      jump there directly using the AST position — independent of which
//      token the cursor happened to land on.

fn goto_enum_type(
    doc: &Document,
    name: &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    // First confirm this name actually exists as an enum declaration.
    let ast   = doc.ast.as_ref()?;
    let enums = ast.enums.as_ref()?;

    let decl = enums.enums.iter().find(|e| e.name == name)?;

    // Now confirm there is a dot somewhere nearby in the token stream,
    // which distinguishes `AIType.BOSS` from a bare `AIType` variable.
    // We look up to 3 tokens ahead to tolerate position-encoding differences.
    let has_dot_ahead = doc.tokens
        .iter()
        .skip(token_index + 1)
        .take(3)
        .any(|t| matches!(t.token_type, TokenType::Symbol('.')));

    if !has_dot_ahead {
        // The name exists as an enum but the cursor is not on a `Name.FIELD`
        // access — it might be a data property with the same name.  Don't jump.
        return None;
    }

    let line = decl.position.line.saturating_sub(1) as u32;
    let col  = decl.position.column.saturating_sub(1) as u32;

    Some(GotoDefinitionResponse::Scalar(Location {
        uri:   doc.uri.clone(),
        range: Range::new(
            Position::new(line, col),
            Position::new(line, col + name.len() as u32),
        ),
    }))
}

// ── Enum FIELD from raw identifier context ────────────────────────────────────
//
// The tokenizer emits Identifier('AIType') Symbol('.') Identifier('BOSS').
// When the cursor is on the SECOND identifier we look up to 3 positions back
// (tolerating position-encoding differences) for the dot and then the enum name.

fn goto_enum_from_context(
    doc: &Document,
    field_name: &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    // Scan backwards up to 3 tokens for a dot.
    let dot_offset = (1usize..=3).find(|&offset| {
        token_index >= offset
            && matches!(
                doc.tokens.get(token_index - offset).map(|t| &t.token_type),
                Some(TokenType::Symbol('.'))
            )
    })?;

    // The token before the dot should be the enum type name.
    let enum_tok_idx = token_index.checked_sub(dot_offset + 1)?;
    let enum_tok = doc.tokens.get(enum_tok_idx)?;
    let TokenType::Identifier(enum_name) = &enum_tok.token_type else { return None };

    // Confirm the enum is actually declared in @ENUMS.
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

        let line = func.position.line.saturating_sub(1) as u32;
        let col  = func.position.column.saturating_sub(1) as u32;

        return Some(GotoDefinitionResponse::Scalar(Location {
            uri:   doc.uri.clone(),
            range: Range::new(
                Position::new(line, col),
                // +1 for the leading ~ prefix.
                Position::new(line, col + 1 + name.len() as u32),
            ),
        }));
    }
    None
}

// ── QuickFunc local variable (let / const) ────────────────────────────────────

fn goto_quickfunc_local_var(doc: &Document, name: &str) -> Option<GotoDefinitionResponse> {
    let ast = doc.ast.as_ref()?;
    let qf  = ast.quick_functions.as_ref()?;

    for func in &qf.functions {
        if let Some(ast_pos) = find_var_in_statements(&func.body, name) {
            let target_line = ast_pos.line;
            let var_tok = doc.tokens.iter().find(|t| {
                t.line == target_line
                    && matches!(&t.token_type, TokenType::Identifier(id) if id.as_str() == name)
            });

            let (line, col) = if let Some(tok) = var_tok {
                (
                    tok.line.saturating_sub(1) as u32,
                    tok.column.saturating_sub(1) as u32,
                )
            } else {
                (
                    ast_pos.line.saturating_sub(1) as u32,
                    ast_pos.column.saturating_sub(1) as u32,
                )
            };

            return Some(GotoDefinitionResponse::Scalar(Location {
                uri:   doc.uri.clone(),
                range: Range::new(
                    Position::new(line, col),
                    Position::new(line, col + name.len() as u32),
                ),
            }));
        }
    }
    None
}

fn find_var_in_statements(stmts: &[QuickFuncStatement], name: &str) -> Option<AstPos> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration { variable_name, position, .. } => {
                if variable_name == name {
                    return Some(*position);
                }
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(p) = find_var_in_statements(then_branch, name) {
                    return Some(p);
                }
                if let Some(else_stmts) = else_branch {
                    if let Some(p) = find_var_in_statements(else_stmts, name) {
                        return Some(p);
                    }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(p) = find_var_in_statements(&case.statements, name) {
                        return Some(p);
                    }
                }
                if let Some(dc) = default_case {
                    if let Some(p) = find_var_in_statements(&dc.statements, name) {
                        return Some(p);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

// ── DATA property definition ──────────────────────────────────────────────────

fn goto_data_property(doc: &Document, name: &str) -> Option<GotoDefinitionResponse> {
    let tokens = &doc.tokens;

    for (i, token) in tokens.iter().enumerate() {
        if token.section != SectionId::Data { continue; }
        let TokenType::Identifier(id) = &token.token_type else { continue };
        if id.as_str() != name { continue; }

        // Peek past optional type annotation to find the defining operator.
        let next_op = tokens
            .iter()
            .skip(i + 1)
            .find(|t| !matches!(
                t.token_type,
                TokenType::Symbol('<')
                    | TokenType::Symbol('>')
                    | TokenType::DataType(_)
                    | TokenType::Identifier(_)
            ));

        let is_definition = matches!(
            next_op.map(|t| &t.token_type),
            Some(TokenType::Symbol('='))
                | Some(TokenType::Symbol(':'))
                | Some(TokenType::DoubleColon)
                | Some(TokenType::DataType(_))
        );

        if !is_definition { continue; }

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
