// mdix-lsp/src/features/goto_definition.rs
//
// Changes from previous version:
//   - `goto_quickfunc_param` added: jumps to the parameter declaration when
//     clicking on a parameter name inside a QuickFunc body.  Called before
//     `goto_quickfunc_local_var` so params take priority over locals.
//   - Enum type lookahead tolerance kept from previous version.
//   - All panic already wrapped in catch_unwind.

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
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc, pos)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
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
            // 0. Enum TYPE name (e.g. `AIType` in `AIType.BOSS`) → @ENUMS decl.
            if let Some(r) = goto_enum_type(doc, name, index) {
                return Some(r);
            }
            // 1. Enum FIELD: cursor on FIELD in EnumName.FIELD.
            if let Some(r) = goto_enum_from_context(doc, name, index) {
                return Some(r);
            }
            // 2. QuickFunc call / reference → ~name declaration.
            if let Some(r) = goto_quickfunc(doc, name) {
                return Some(r);
            }
            // 3. QuickFunc parameter — must come before local var search.
            if token_section == SectionId::QuickFuncs {
                if let Some(r) = goto_quickfunc_param(doc, name, pos) {
                    return Some(r);
                }
            }
            // 4. QuickFunc local variable declaration.
            if let Some(r) = goto_quickfunc_local_var(doc, name) {
                return Some(r);
            }
            // 5. DATA property reference (outside QuickFuncs).
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
// Scans up to 3 tokens ahead for a dot to confirm this is `Name.FIELD` access.
// Tolerates IntelliJ cursor-offset differences.

fn goto_enum_type(
    doc: &Document,
    name: &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    let ast   = doc.ast.as_ref()?;
    let enums = ast.enums.as_ref()?;

    // Confirm the name is actually an enum declaration.
    let decl = enums.enums.iter().find(|e| e.name == name)?;

    // Confirm a dot appears within 3 tokens ahead.
    let has_dot_ahead = doc.tokens
        .iter()
        .skip(token_index + 1)
        .take(3)
        .any(|t| matches!(t.token_type, TokenType::Symbol('.')));

    if !has_dot_ahead { return None; }

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
// Cursor on the field name in `EnumName.FIELD` — scan up to 3 tokens back for
// the dot and then the enum name.

fn goto_enum_from_context(
    doc: &Document,
    field_name: &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    let dot_offset = (1usize..=3).find(|&offset| {
        token_index >= offset
            && matches!(
                doc.tokens.get(token_index - offset).map(|t| &t.token_type),
                Some(TokenType::Symbol('.'))
            )
    })?;

    let enum_tok_idx = token_index.checked_sub(dot_offset + 1)?;
    let enum_tok     = doc.tokens.get(enum_tok_idx)?;
    let TokenType::Identifier(enum_name) = &enum_tok.token_type else { return None };

    let ast   = doc.ast.as_ref()?;
    let enums = ast.enums.as_ref()?;
    if !enums.enums.iter().any(|e| e.name == *enum_name) { return None; }

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
                Position::new(line, col + 1 + name.len() as u32), // +1 for ~
            ),
        }));
    }
    None
}

// ── QuickFunc parameter declaration ──────────────────────────────────────────
//
// Searches parameter lists of all QuickFuncs.  When multiple functions share
// a parameter name, we use the cursor line to find the enclosing function.

fn goto_quickfunc_param(
    doc:  &Document,
    name: &str,
    pos:  Position,
) -> Option<GotoDefinitionResponse> {
    let ast = doc.ast.as_ref()?;
    let qf  = ast.quick_functions.as_ref()?;

    // Convert cursor to 1-based line for comparison with AST positions.
    let cursor_line_1based = pos.line as usize + 1;

    // Strategy: find the function whose definition line is closest to (and
    // before) the cursor.  This scopes the search to the enclosing function.
    let enclosing_func = qf.functions.iter()
        .filter(|f| f.position.line <= cursor_line_1based)
        .max_by_key(|f| f.position.line);

    // Search enclosing function first; fall back to all functions.
    let search_order: Vec<&dixscript::Compiler::AST::QuickFunction> = {
        let mut v = Vec::new();
        if let Some(f) = enclosing_func { v.push(f); }
        for f in &qf.functions {
            if !v.iter().any(|x| x.name == f.name) { v.push(f); }
        }
        v
    };

    for func in search_order {
        for param in &func.parameters {
            if param.name != name { continue; }

            // AST positions are 1-based → convert to 0-based for LSP.
            let line = param.position.line.saturating_sub(1) as u32;
            let col  = param.position.column.saturating_sub(1) as u32;

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
                (tok.line.saturating_sub(1) as u32, tok.column.saturating_sub(1) as u32)
            } else {
                (ast_pos.line.saturating_sub(1) as u32, ast_pos.column.saturating_sub(1) as u32)
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
                if variable_name == name { return Some(*position); }
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(p) = find_var_in_statements(then_branch, name) { return Some(p); }
                if let Some(else_stmts) = else_branch {
                    if let Some(p) = find_var_in_statements(else_stmts, name) { return Some(p); }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(p) = find_var_in_statements(&case.statements, name) { return Some(p); }
                }
                if let Some(dc) = default_case {
                    if let Some(p) = find_var_in_statements(&dc.statements, name) { return Some(p); }
                }
            }
            _ => {}
        }
    }
    None
}

// ── DATA property definition ──────────────────────────────────────────────────

fn goto_data_property(doc: &Document, name: &str) -> Option<GotoDefinitionResponse> {
    for (i, token) in doc.tokens.iter().enumerate() {
        if token.section != SectionId::Data { continue; }
        let TokenType::Identifier(id) = &token.token_type else { continue };
        if id.as_str() != name { continue; }

        let next_op = doc.tokens.iter().skip(i + 1)
            .find(|t| !matches!(t.token_type,
                TokenType::Symbol('<') | TokenType::Symbol('>')
                | TokenType::DataType(_) | TokenType::Identifier(_)));

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
    doc: &Document, enum_name: &str, field_name: &str,
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
    if path.starts_with("http://") || path.starts_with("https://") { return None; }

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
