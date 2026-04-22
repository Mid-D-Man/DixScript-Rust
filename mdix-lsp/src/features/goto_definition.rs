// mdix-lsp/src/features/goto_definition.rs
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
    let doc = doc?;
    let (token, index) = token_and_index_at(&doc.tokens, pos)?;

    let token_type    = token.token_type.clone();
    let token_section = token.section;

    match &token_type {
        TokenType::Identifier(name) => {
            // 0. Enum TYPE name (`AIType` in `AIType.BOSS`) → jump to @ENUMS declaration
            if let Some(r) = goto_enum_type(doc, name, index) {
                return Some(r);
            }
            // 1. Enum FIELD access: cursor on FIELD in EnumName.FIELD
            if let Some(r) = goto_enum_from_context(doc, name, index) {
                return Some(r);
            }
            // 2. QuickFunc call/reference → jump to ~name declaration
            if let Some(r) = goto_quickfunc(doc, name) {
                return Some(r);
            }
            // 3. QuickFunc local variable (let / const declaration inside body)
            if let Some(r) = goto_quickfunc_local_var(doc, name) {
                return Some(r);
            }
            // 4. DATA property reference – only outside QuickFuncs bodies
            if token_section != SectionId::QuickFuncs {
                goto_data_property(doc, name)
            } else {
                None
            }
        }

        // EnumAccess tokens produced after semantic enhancement
        TokenType::EnumAccess { enum_name, value } => {
            goto_enum_field(doc, enum_name, value)
        }

        // Import path string → open the target file
        TokenType::String(path) | TokenType::StringSingle(path) => {
            goto_import(doc, path)
        }

        _ => None,
    }
}

// ── Enum TYPE name → @ENUMS declaration ──────────────────────────────────────
//
// Fires when the cursor is on `AIType` in `AIType.BOSS` (the name is followed
// immediately by a dot).  We jump to the declaration block in @ENUMS.

fn goto_enum_type(
    doc: &Document,
    name: &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    // The token immediately after must be '.'
    let next = doc.tokens.get(token_index + 1)?;
    if !matches!(next.token_type, TokenType::Symbol('.')) {
        return None;
    }

    let ast   = doc.ast.as_ref()?;
    let enums = ast.enums.as_ref()?;

    for decl in &enums.enums {
        if decl.name != name { continue; }
        let line = decl.position.line.saturating_sub(1) as u32;
        let col  = decl.position.column.saturating_sub(1) as u32;
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

// ── Enum FIELD from raw identifier context ────────────────────────────────────
//
// The tokenizer emits  Identifier('AIType') Symbol('.') Identifier('BOSS').
// When the cursor is on the second identifier we look two positions back.

fn goto_enum_from_context(
    doc: &Document,
    field_name: &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    if token_index < 2 { return None; }

    let dot_tok  = doc.tokens.get(token_index - 1)?;
    if !matches!(dot_tok.token_type, TokenType::Symbol('.')) { return None; }

    let enum_tok = doc.tokens.get(token_index - 2)?;
    let TokenType::Identifier(enum_name) = &enum_tok.token_type else { return None };

    // Confirm it is actually declared in @ENUMS.
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
                // +1 for the leading ~ prefix
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
            // The AST position is the `let`/`const` keyword.  Find the
            // Identifier token for the variable name on the same source line
            // so the highlight lands on the name itself.
            let target_line = ast_pos.line; // 1-based
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

/// Recursively search QuickFunc statements for a `let`/`const` declaration
/// whose `variable_name` matches `name`.  Returns the statement position.
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
//
// Previous version skipped tokens on the same line as the cursor, which meant
// clicking ON a definition never navigated anywhere.  The skip is removed:
// `is_definition` already filters out non-defining occurrences.

fn goto_data_property(doc: &Document, name: &str) -> Option<GotoDefinitionResponse> {
    let tokens = &doc.tokens;

    for (i, token) in tokens.iter().enumerate() {
        if token.section != SectionId::Data { continue; }
        let TokenType::Identifier(id) = &token.token_type else { continue };
        if id.as_str() != name { continue; }

        // Peek past an optional type annotation to find the defining operator.
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
