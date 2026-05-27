// mdix-lsp/src/features/goto_definition.rs
//! Go-to-definition provider.
//!
//! Approach B: @CONFIG tokens are real tokens with SectionId::Config and
//! accurate positions. No config_line_range needed.
//!
//! Fixes:
//!   - EnumAccess: position-aware — cursor on field name navigates to that
//!     specific field declaration, cursor on enum name navigates to enum type.
//!   - Identifier after '.': QuickFunc local object property navigation.
//!     `let someone = { sss = 5 }` then `someone.sss` → navigates to `sss`.
//!   - Imported-symbol navigation (2025): unchanged.

use std::panic;
use std::path::Path;

use tower_lsp::lsp_types::{
    GotoDefinitionResponse, Location, Position, Range, Url,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::AST::{
    DixScript, Expression, ObjectProperty, QuickFuncStatement, Value,
};

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
    definition_for(token, index, pos, doc)
}

fn definition_for(
    token: &Token,
    index: usize,
    pos:   Position,
    doc:   &Document,
) -> Option<GotoDefinitionResponse> {
    match &token.token_type {

        // ── Identifier ────────────────────────────────────────────────────────
        TokenType::Identifier(name) => {
            // Priority 1: member of an imported namespace (e.g. Utils.calc or Utils.Status.ACTIVE)
            if let Some(response) = find_imported_namespace_member(doc, name, index) {
                return Some(response);
            }

            // Priority 2: object property in QuickFunc — dot access on a local variable
            // e.g. `let someone = { sss = 5 }` then `sugar = someone.sss`
            // clicking `sss` navigates to the `sss` key in the object literal
            if token.section == SectionId::QuickFuncs && index >= 2 {
                let prev      = doc.tokens.get(index - 1);
                let prev_prev = doc.tokens.get(index - 2);
                if let (Some(dot_tok), Some(obj_tok)) = (prev, prev_prev) {
                    if matches!(dot_tok.token_type, TokenType::Symbol('.')) {
                        if let TokenType::Identifier(obj_name) = &obj_tok.token_type {
                            if let Some(loc) = find_qf_object_property_def(doc, obj_name, name) {
                                return Some(GotoDefinitionResponse::Scalar(loc));
                            }
                        }
                    }
                }
            }

            // Priority 3: QuickFunc call site → declaration
            let is_call = doc.tokens.get(index + 1)
                .map(|t| matches!(t.token_type, TokenType::Symbol('(')))
                .unwrap_or(false);

            if is_call {
                if let Some(loc) = find_quickfunc_def(doc, name) {
                    return Some(GotoDefinitionResponse::Scalar(loc));
                }
            }

            // Priority 4: Enum type name → @ENUMS declaration
            if let Some(loc) = find_enum_def(doc, name) {
                return Some(GotoDefinitionResponse::Scalar(loc));
            }

            // Priority 5: Namespace alias → @IMPORTS declaration in current file
            if let Some(loc) = find_import_def(doc, name) {
                return Some(GotoDefinitionResponse::Scalar(loc));
            }

            // Priority 6: Variable in @DATA → its definition line
            if let Some(loc) = find_data_var_def(doc, name, token.section) {
                return Some(GotoDefinitionResponse::Scalar(loc));
            }

            // Priority 7: QuickFunc parameter → declaration in the enclosing function
            if token.section == SectionId::QuickFuncs {
                if let Some(loc) = find_param_def(doc, name) {
                    return Some(GotoDefinitionResponse::Scalar(loc));
                }
            }

            None
        }

        // ── Enum access (EnumName.FIELD) ──────────────────────────────────────
        //
        // Position-aware: if cursor is after the dot (on the field name),
        // navigate to the specific field declaration. If before/on the dot
        // (on the enum type name), navigate to the enum type declaration.
        TokenType::EnumAccess { enum_name, value } => {
            // token.column is 1-based; pos.character is 0-based.
            let token_start_0 = token.column.saturating_sub(1);
            let dot_offset    = enum_name.len();          // index of '.' relative to token start
            let cursor_0      = pos.character as usize;

            // Cursor is on the value (field) part if it falls after the dot
            let on_value_part = cursor_0 > token_start_0 + dot_offset;

            if on_value_part {
                // Navigate to the specific field, fall back to enum type
                find_enum_field_def(doc, enum_name, value)
                    .or_else(|| find_enum_def(doc, enum_name))
                    .map(GotoDefinitionResponse::Scalar)
            } else {
                // Navigate to the enum type declaration
                find_enum_def(doc, enum_name)
                    .map(GotoDefinitionResponse::Scalar)
            }
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
                if let Some(func_info) = ns.functions.get(member_name) {
                    return navigate_to_imported_func(
                        &ns.file_path, member_name, func_info.ast.position,
                    );
                }
                if ns.enums.contains_key(member_name) {
                    return navigate_to_imported_file_start(&ns.file_path);
                }
                if let Some(local_ns) = ns.local_imports.get(member_name) {
                    return navigate_to_imported_file_start(&local_ns.file_path);
                }
                return None;
            }

            // ── Pattern B: ns.EnumType.FIELD ─────────────────────────────────
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

fn navigate_to_imported_func(
    file_path: &str,
    func_name: &str,
    ast_pos:   dixscript::Compiler::AST::Position,
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

fn navigate_to_imported_file_start(file_path: &str) -> Option<GotoDefinitionResponse> {
    let uri = file_uri_from_path(file_path)?;
    Some(GotoDefinitionResponse::Scalar(make_location(&uri, 0, 0, 0, 0)))
}

fn file_uri_from_path(path: &str) -> Option<Url> {
    if path.starts_with("http://") || path.starts_with("https://") {
        return None;
    }
    Url::from_file_path(Path::new(path)).ok()
}

// ── QuickFunc object property navigation ──────────────────────────────────────
//
// Finds the definition of `prop_name` inside an object literal that is
// assigned to `obj_name` within any QuickFunc in the document.
//
// Example: `let someone = { sss = 5 }` then `someone.sss`
//   → searches QuickFuncs token stream for `someone = { ... sss = ... }`
//   → navigates to the `sss` key inside the object literal.
//
// Uses the token stream directly for robustness (AST positions on
// ObjectProperty may be UNKNOWN if the parser doesn't set them).

fn find_qf_object_property_def(
    doc:       &Document,
    obj_name:  &str,
    prop_name: &str,
) -> Option<Location> {
    let tokens = &doc.tokens;
    let n      = tokens.len();

    for i in 0..n {
        let t = &tokens[i];

        // Only look in @QUICKFUNCS section
        if t.section != SectionId::QuickFuncs { continue; }

        // Must be `obj_name` as an identifier
        if !matches!(&t.token_type, TokenType::Identifier(id) if id.as_str() == obj_name) {
            continue;
        }

        // Must NOT be preceded by `.` (would be a property access, not a definition)
        if i > 0 && matches!(&tokens[i - 1].token_type, TokenType::Symbol('.')) {
            continue;
        }

        // Look ahead for `=` within the next 8 tokens
        // (skipping possible type annotation: `<object>`)
        let eq_rel = tokens[i + 1..].iter().take(8).position(|t| {
            matches!(&t.token_type, TokenType::Symbol('='))
        });
        let eq_idx = match eq_rel {
            Some(j) => i + 1 + j,
            None    => continue,
        };

        // After `=`, look for `{` within the next 4 tokens
        let brace_rel = tokens[eq_idx + 1..].iter().take(4).position(|t| {
            t.section == SectionId::QuickFuncs
                && matches!(&t.token_type, TokenType::Symbol('{'))
        });
        let brace_idx = match brace_rel {
            Some(j) => eq_idx + 1 + j,
            None    => continue,
        };

        // Scan inside the braces (depth-tracked) for `prop_name =`
        let mut depth = 0i32;
        for j in brace_idx..n {
            if tokens[j].section != SectionId::QuickFuncs { continue; }

            match &tokens[j].token_type {
                TokenType::Symbol('{') => depth += 1,

                TokenType::Symbol('}') => {
                    depth -= 1;
                    if depth <= 0 { break; } // left the object literal
                }

                // At depth 1 we are at the direct properties of the object
                TokenType::Identifier(id) if id.as_str() == prop_name && depth == 1 => {
                    // Confirm it is a key (next non-whitespace token is `=`)
                    let next_is_eq = tokens
                        .get(j + 1)
                        .map(|t| matches!(&t.token_type, TokenType::Symbol('=')))
                        .unwrap_or(false);

                    if next_is_eq {
                        let line = tokens[j].line.saturating_sub(1) as u32;
                        let col  = tokens[j].column.saturating_sub(1) as u32;
                        return Some(make_location(
                            &doc.uri,
                            line, col,
                            line, col + prop_name.len() as u32,
                        ));
                    }
                }

                _ => {}
            }
        }
    }

    None
}

// ── Enum field definition lookup ──────────────────────────────────────────────
//
// Navigate to the specific field declaration inside an @ENUMS block.
// Uses the AST position when valid, falls back to the token stream.

fn find_enum_field_def(doc: &Document, enum_name: &str, field_name: &str) -> Option<Location> {
    let enums = doc.ast.as_ref()?.enums.as_ref()?;
    let decl  = enums.enums.iter().find(|e| e.name == enum_name)?;
    let field = decl.fields.iter().find(|f| f.name == field_name)?;

    // Try AST position first (set by parser when tokens are available)
    if field.position.is_valid() {
        let l = field.position.line.saturating_sub(1) as u32;
        let c = field.position.column.saturating_sub(1) as u32;

        // Refine with token stream to get the exact token start
        let refined = doc.tokens
            .iter()
            .filter(|t| {
                t.section == SectionId::Enums
                    && t.line == field.position.line
            })
            .find(|t| matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == field_name))
            .map(|t| (t.line.saturating_sub(1) as u32, t.column.saturating_sub(1) as u32));

        let (line, col) = refined.unwrap_or((l, c));
        return Some(make_location(
            &doc.uri, line, col, line, col + field_name.len() as u32,
        ));
    }

    // Fallback: search the token stream in @ENUMS section.
    // Find `field_name` as an identifier that is NOT preceded by `.`
    // (to avoid matching usages like `EnumName.FIELD` elsewhere).
    for (i, t) in doc.tokens.iter().enumerate() {
        if t.section != SectionId::Enums { continue; }
        if !matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == field_name) {
            continue;
        }
        // In @ENUMS, a field declaration is NOT preceded by `.`
        if i > 0 && matches!(&doc.tokens[i - 1].token_type, TokenType::Symbol('.')) {
            continue;
        }
        let line = t.line.saturating_sub(1) as u32;
        let col  = t.column.saturating_sub(1) as u32;
        return Some(make_location(
            &doc.uri, line, col, line, col + field_name.len() as u32,
        ));
    }

    None
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

// ── Enum type declaration lookup ──────────────────────────────────────────────

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
