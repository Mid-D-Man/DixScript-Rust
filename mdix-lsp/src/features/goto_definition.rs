// mdix-lsp/src/features/goto_definition.rs
//! Go-to-definition provider.
//!
//! Approach B: @CONFIG tokens are real tokens with SectionId::Config and
//! accurate positions. No config_line_range needed.

use std::panic;

use tower_lsp::lsp_types::{
    GotoDefinitionResponse, Location, Position, Range, Url,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::AST::DixScript;

use crate::document::Document;
use crate::features::hover::token_and_index_at;

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<GotoDefinitionResponse> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc, pos)));
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

    // ── @CONFIG lines ─────────────────────────────────────────────────────────
    // In Approach B, config tokens are real tokens. If the cursor is on a
    // SectionId::Config token we still don't jump anywhere useful — @CONFIG
    // is defined in-file and the key position IS the definition. Just return
    // None so the editor shows "no definition" rather than jumping to itself.
    if doc.pos_in_config(pos) {
        return None;
    }

    let (token, index) = token_and_index_at(&doc.tokens, pos)?;
    definition_for(token, index, doc)
}

fn definition_for(
    token: &Token,
    index: usize,
    doc:   &Document,
) -> Option<GotoDefinitionResponse> {
    match &token.token_type {

        // ── QuickFunc call site → declaration ────────────────────────────────
        TokenType::Identifier(name) => {
            // Check if this is a call site (followed by `(`).
            let is_call = doc.tokens.get(index + 1)
                .map(|t| matches!(t.token_type, TokenType::Symbol('(')))
                .unwrap_or(false);

            if is_call {
                if let Some(loc) = find_quickfunc_def(doc, name) {
                    return Some(GotoDefinitionResponse::Scalar(loc));
                }
            }

            // Enum type name → @ENUMS declaration
            if let Some(loc) = find_enum_def(doc, name) {
                return Some(GotoDefinitionResponse::Scalar(loc));
            }

            // Namespace alias → @IMPORTS declaration
            if let Some(loc) = find_import_def(doc, name) {
                return Some(GotoDefinitionResponse::Scalar(loc));
            }

            // Variable in @DATA → its definition line
            if let Some(loc) = find_data_var_def(doc, name, token.section) {
                return Some(GotoDefinitionResponse::Scalar(loc));
            }

            // QuickFunc parameter → declaration in the enclosing function
            if token.section == SectionId::QuickFuncs {
                if let Some(loc) = find_param_def(doc, name) {
                    return Some(GotoDefinitionResponse::Scalar(loc));
                }
            }

            None
        }

        // ── Enum access (EnumName.FIELD) → enum declaration ──────────────────
        TokenType::EnumAccess { enum_name, .. } => {
            find_enum_def(doc, enum_name)
                .map(GotoDefinitionResponse::Scalar)
        }

        // ── Section keywords → start of that section ─────────────────────────
        TokenType::SectionConfig     => section_loc(&doc.tokens, &doc.uri, SectionId::Config),
        TokenType::SectionImports    => section_loc(&doc.tokens, &doc.uri, SectionId::Imports),
        TokenType::SectionDLM        => section_loc(&doc.tokens, &doc.uri, SectionId::Dlm),
        TokenType::SectionEnums      => section_loc(&doc.tokens, &doc.uri, SectionId::Enums),
        TokenType::SectionQuickFuncs => section_loc(&doc.tokens, &doc.uri, SectionId::QuickFuncs),
        TokenType::SectionData       => section_loc(&doc.tokens, &doc.uri, SectionId::Data),
        TokenType::SectionSecurity   => section_loc(&doc.tokens, &doc.uri, SectionId::Security),

        _ => None,
    }
}

// ── QuickFunc declaration lookup ──────────────────────────────────────────────

fn find_quickfunc_def(doc: &Document, name: &str) -> Option<Location> {
    let qf   = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let func = qf.functions.iter().find(|f| f.name == name)?;

    if !func.position.is_valid() { return None; }

    let line = func.position.line.saturating_sub(1) as u32;
    let col  = func.position.column.saturating_sub(1) as u32;

    // Refine: find the identifier token for the function name itself
    // (the token after `~`) so we jump to the name, not to `~`.
    let refined = find_func_name_token(&doc.tokens, name, func.position.line);
    let (line, col) = refined.unwrap_or((line, col));

    Some(make_location(&doc.uri, line, col, line, col + name.len() as u32))
}

/// Find the Identifier token for `name` on or near `def_line` in @QUICKFUNCS.
fn find_func_name_token(
    tokens:   &[Token],
    name:     &str,
    def_line: usize,
) -> Option<(u32, u32)> {
    tokens.iter()
        .filter(|t| {
            t.section == SectionId::QuickFuncs
                && t.line >= def_line
                && t.line <= def_line + 2
        })
        .find(|t| matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == name))
        .map(|t| {
            (
                t.line.saturating_sub(1) as u32,
                t.column.saturating_sub(1) as u32,
            )
        })
}

// ── Enum definition lookup ────────────────────────────────────────────────────

fn find_enum_def(doc: &Document, enum_name: &str) -> Option<Location> {
    let enums = doc.ast.as_ref()?.enums.as_ref()?;
    let decl  = enums.enums.iter().find(|e| e.name == enum_name)?;

    if !decl.position.is_valid() { return None; }

    let line = decl.position.line.saturating_sub(1) as u32;
    let col  = decl.position.column.saturating_sub(1) as u32;

    // Refine to the Identifier token for the enum name.
    let refined = doc.tokens.iter()
        .filter(|t| {
            t.section == SectionId::Enums
                && t.line >= decl.position.line
                && t.line <= decl.position.line + 1
        })
        .find(|t| matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == enum_name))
        .map(|t| (
            t.line.saturating_sub(1) as u32,
            t.column.saturating_sub(1) as u32,
        ));

    let (line, col) = refined.unwrap_or((line, col));

    Some(make_location(
        &doc.uri, line, col, line, col + enum_name.len() as u32,
    ))
}

// ── Import alias lookup ───────────────────────────────────────────────────────

fn find_import_def(doc: &Document, alias: &str) -> Option<Location> {
    // Check symbol table first.
    let st = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    if !st.is_imported_namespace(alias) { return None; }

    let imports = doc.ast.as_ref()?.imports.as_ref()?;
    let import  = imports.imports.iter().find(|i| i.alias == alias)?;

    if !import.position.is_valid() { return None; }

    let line = import.position.line.saturating_sub(1) as u32;
    let col  = import.position.column.saturating_sub(1) as u32;

    // Refine to the alias Identifier token.
    let refined = doc.tokens.iter()
        .filter(|t| {
            t.section == SectionId::Imports
                && t.line >= import.position.line
                && t.line <= import.position.line + 1
        })
        .find(|t| matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == alias))
        .map(|t| (
            t.line.saturating_sub(1) as u32,
            t.column.saturating_sub(1) as u32,
        ));

    let (line, col) = refined.unwrap_or((line, col));

    Some(make_location(
        &doc.uri, line, col, line, col + alias.len() as u32,
    ))
}

// ── DATA variable definition lookup ──────────────────────────────────────────

fn find_data_var_def(doc: &Document, name: &str, section: SectionId) -> Option<Location> {
    // Only useful when called from a non-DATA section referencing a DATA key.
    // In DATA itself, the cursor IS on the definition.
    if section == SectionId::Data { return None; }

    let data = doc.ast.as_ref()?.data.as_ref()?;

    use dixscript::Compiler::AST::DataEntry;
    for entry in &data.entries {
        match entry {
            DataEntry::SimpleProperty { name: n, position, .. } if n == name => {
                if !position.is_valid() { return None; }
                let line = position.line.saturating_sub(1) as u32;
                let col  = position.column.saturating_sub(1) as u32;
                return Some(make_location(
                    &doc.uri, line, col, line, col + name.len() as u32,
                ));
            }
            DataEntry::TableProperty { path, position, .. }
                if path.segments.first().map(|s| s.as_str()) == Some(name) =>
            {
                if !position.is_valid() { return None; }
                let line = position.line.saturating_sub(1) as u32;
                let col  = position.column.saturating_sub(1) as u32;
                return Some(make_location(
                    &doc.uri, line, col, line, col + name.len() as u32,
                ));
            }
            DataEntry::GroupArray { path, position, .. }
                if path.segments.first().map(|s| s.as_str()) == Some(name) =>
            {
                if !position.is_valid() { return None; }
                let line = position.line.saturating_sub(1) as u32;
                let col  = position.column.saturating_sub(1) as u32;
                return Some(make_location(
                    &doc.uri, line, col, line, col + name.len() as u32,
                ));
            }
            _ => {}
        }
    }
    None
}

// ── QuickFunc parameter definition lookup ────────────────────────────────────

fn find_param_def(doc: &Document, name: &str) -> Option<Location> {
    let qf = doc.ast.as_ref()?.quick_functions.as_ref()?;

    for func in &qf.functions {
        for param in &func.parameters {
            if param.name != name { continue; }
            if !param.position.is_valid() { return None; }

            let line = param.position.line.saturating_sub(1) as u32;
            let col  = param.position.column.saturating_sub(1) as u32;

            // Refine to the actual parameter Identifier token.
            let refined = doc.tokens.iter()
                .filter(|t| {
                    t.section == SectionId::QuickFuncs
                        && t.line >= param.position.line
                        && t.line <= param.position.line + 1
                })
                .find(|t| matches!(&t.token_type,
                    TokenType::Identifier(n) if n.as_str() == name))
                .map(|t| (
                    t.line.saturating_sub(1) as u32,
                    t.column.saturating_sub(1) as u32,
                ));

            let (line, col) = refined.unwrap_or((line, col));

            return Some(make_location(
                &doc.uri, line, col, line, col + name.len() as u32,
            ));
        }
    }
    None
}

// ── Section keyword → section start ──────────────────────────────────────────

fn section_loc(
    tokens: &[Token],
    uri:    &Url,
    id:     SectionId,
) -> Option<GotoDefinitionResponse> {
    let tok = tokens.iter().find(|t| t.section == id)?;
    let line = tok.line.saturating_sub(1) as u32;
    let col  = tok.column.saturating_sub(1) as u32;
    Some(GotoDefinitionResponse::Scalar(
        make_location(uri, line, col, line, col + section_keyword_len(id)),
    ))
}

fn section_keyword_len(id: SectionId) -> u32 {
    match id {
        SectionId::Config     => 7,   // @CONFIG
        SectionId::Imports    => 8,   // @IMPORTS
        SectionId::Dlm        => 4,   // @DLM
        SectionId::Enums      => 6,   // @ENUMS
        SectionId::QuickFuncs => 11,  // @QUICKFUNCS
        SectionId::Data       => 5,   // @DATA
        SectionId::Security   => 9,   // @SECURITY
        SectionId::None       => 1,
    }
}

// ── Location constructor ──────────────────────────────────────────────────────

fn make_location(
    uri:        &Url,
    start_line: u32,
    start_col:  u32,
    end_line:   u32,
    end_col:    u32,
) -> Location {
    Location {
        uri:   uri.clone(),
        range: Range::new(
            Position::new(start_line, start_col),
            Position::new(end_line,   end_col),
        ),
    }
}
