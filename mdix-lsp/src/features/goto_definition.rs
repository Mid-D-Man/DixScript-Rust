// mdix-lsp/src/features/goto_definition.rs
//! Go-to-definition provider.
//!
//! Approach B: @CONFIG tokens are real tokens with SectionId::Config and
//! accurate positions. No config_line_range needed.
//!
//! Imported-symbol navigation (2025):
//!   When the cursor is on a member of an imported namespace (e.g. the
//!   `calc` in `Utils.calc(x)`), we look up the namespace in the symbol
//!   table, obtain the absolute file path stored there, build a file:// URI
//!   and navigate to the function's position in that file.
//!   For 3-part qualified access (ns.EnumName.FIELD) we navigate to the
//!   start of the imported file (enum positions are not stored individually).

use std::panic;
use std::path::Path;

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

    // @CONFIG lines — definition IS the current line; nothing to jump to.
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

        // ── Identifier ────────────────────────────────────────────────────────
        TokenType::Identifier(name) => {
            // Priority 1: member of an imported namespace (e.g. Utils.calc or Utils.Status.ACTIVE)
            if let Some(response) = find_imported_namespace_member(doc, name, index) {
                return Some(response);
            }

            // Priority 2: QuickFunc call site → declaration
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

            // Namespace alias → @IMPORTS declaration in current file
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

// ── Imported namespace member navigation ──────────────────────────────────────
//
// Handles two patterns:
//   A) ns.FunctionName(…)  or  ns.EnumType
//   B) ns.EnumType.FIELD
//
// For functions we navigate to the exact position stored in QuickFunctionInfo.
// For enum types / fields we navigate to the start of the imported file
// (individual enum positions are not stored in ImportedNamespace).

fn find_imported_namespace_member(
    doc:         &Document,
    member_name: &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    if token_index < 2 { return None; }

    let prev = doc.tokens.get(token_index - 1)?;
    if !matches!(prev.token_type, TokenType::Symbol('.')) { return None; }

    let ns_token = doc.tokens.get(token_index - 2)?;

    let st = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;

    match &ns_token.token_type {
        TokenType::Identifier(potential_ns) => {
            // ── Pattern A: ns.Member ──────────────────────────────────────────
            if let Some(ns) = st.try_get_namespace(potential_ns.as_str()) {
                // Function?
                if let Some(func_info) = ns.functions.get(member_name) {
                    return navigate_to_imported_func(
                        &ns.file_path, member_name, func_info.ast.position,
                    );
                }
                // Enum type?
                if ns.enums.contains_key(member_name) {
                    return navigate_to_imported_file_start(&ns.file_path);
                }
                // Local import re-exported through this namespace?
                if let Some(local_ns) = ns.local_imports.get(member_name) {
                    return navigate_to_imported_file_start(&local_ns.file_path);
                }
                return None;
            }

            // ── Pattern B: ns.EnumType.FIELD ─────────────────────────────────
            // Here ns_token contains EnumType; check two tokens further back.
            if token_index >= 4 {
                let prev2 = doc.tokens.get(token_index - 3)?;
                if matches!(prev2.token_type, TokenType::Symbol('.')) {
                    let ns2_token = doc.tokens.get(token_index - 4)?;
                    if let TokenType::Identifier(actual_ns) = &ns2_token.token_type {
                        if let Some(ns) = st.try_get_namespace(actual_ns.as_str()) {
                            let enum_name = potential_ns.as_str();
                            if let Some(fields) = ns.enums.get(enum_name) {
                                if fields.contains_key(member_name) {
                                    return navigate_to_imported_file_start(&ns.file_path);
                                }
                            }
                        }
                    }
                }
            }

            None
        }
        _ => None,
    }
}

/// Build a GotoDefinitionResponse pointing at a function inside an imported file.
fn navigate_to_imported_func(
    file_path:   &str,
    func_name:   &str,
    ast_pos:     dixscript::Compiler::AST::Position,
) -> Option<GotoDefinitionResponse> {
    let uri = file_uri_from_path(file_path)?;
    let (line, col) = if ast_pos.is_valid() {
        (
            ast_pos.line.saturating_sub(1) as u32,
            ast_pos.column.saturating_sub(1) as u32,
        )
    } else {
        (0, 0)
    };
    Some(GotoDefinitionResponse::Scalar(make_location(
        &uri, line, col, line, col + func_name.len() as u32,
    )))
}

/// Build a GotoDefinitionResponse that opens the imported file at its start.
fn navigate_to_imported_file_start(file_path: &str) -> Option<GotoDefinitionResponse> {
    let uri = file_uri_from_path(file_path)?;
    Some(GotoDefinitionResponse::Scalar(make_location(&uri, 0, 0, 0, 0)))
}

fn file_uri_from_path(path: &str) -> Option<Url> {
    // Cloud URLs are not navigable via goto-definition
    if path.starts_with("http://") || path.starts_with("https://") {
        return None;
    }
    Url::from_file_path(Path::new(path)).ok()
}

// ── QuickFunc declaration lookup ──────────────────────────────────────────────

fn find_quickfunc_def(doc: &Document, name: &str) -> Option<Location> {
    let qf   = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let func = qf.functions.iter().find(|f| f.name == name)?;

    if !func.position.is_valid() { return None; }

    let line = func.position.line.saturating_sub(1) as u32;
    let col  = func.position.column.saturating_sub(1) as u32;

    let refined = find_func_name_token(&doc.tokens, name, func.position.line);
    let (line, col) = refined.unwrap_or((line, col));

    Some(make_location(&doc.uri, line, col, line, col + name.len() as u32))
}

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
        .map(|t| (
            t.line.saturating_sub(1) as u32,
            t.column.saturating_sub(1) as u32,
        ))
}

// ── Enum definition lookup ────────────────────────────────────────────────────

fn find_enum_def(doc: &Document, enum_name: &str) -> Option<Location> {
    let enums = doc.ast.as_ref()?.enums.as_ref()?;
    let decl  = enums.enums.iter().find(|e| e.name == enum_name)?;

    if !decl.position.is_valid() { return None; }

    let line = decl.position.line.saturating_sub(1) as u32;
    let col  = decl.position.column.saturating_sub(1) as u32;

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
    let st = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    if !st.is_imported_namespace(alias) { return None; }

    let imports = doc.ast.as_ref()?.imports.as_ref()?;
    let import  = imports.imports.iter().find(|i| i.alias == alias)?;

    if !import.position.is_valid() { return None; }

    let line = import.position.line.saturating_sub(1) as u32;
    let col  = import.position.column.saturating_sub(1) as u32;

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
        SectionId::Config     => 7,
        SectionId::Imports    => 8,
        SectionId::Dlm        => 4,
        SectionId::Enums      => 6,
        SectionId::QuickFuncs => 11,
        SectionId::Data       => 5,
        SectionId::Security   => 9,
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
