// mdix-lsp/src/features/goto_definition.rs
//! Go-to-definition provider.
//!
//! Handles:
//!   - QuickFunc call sites → declaration
//!   - Enum access (position-aware: field vs type name)
//!   - Object property access inside QuickFuncs
//!   - Imported namespace members (functions, enums)
//!   - Import aliases → @IMPORTS entry
//!   - Import path strings → the imported file on disk
//!   - DATA variable references
//!   - QuickFunc parameters
//!   - QuickFunc local variable declarations
//!   - ConfigAccess tokens → @CONFIG entry
//!   - InterpolatedString → identifier under cursor inside {expr}
//!   - `~` symbol → the QuickFunc declaration that follows
//!   - Section keywords → section start

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
            let name = name.clone();

            // Priority 1: imported namespace member
            if let Some(r) = find_imported_namespace_member(doc, &name, index) {
                return Some(r);
            }

            // Priority 2: object property via dot access (let obj = {x=1}; obj.x)
            if token.section == SectionId::QuickFuncs && index >= 2 {
                let prev      = doc.tokens.get(index - 1);
                let prev_prev = doc.tokens.get(index - 2);
                if let (Some(dot), Some(obj)) = (prev, prev_prev) {
                    if matches!(dot.token_type, TokenType::Symbol('.')) {
                        if let TokenType::Identifier(obj_name) = &obj.token_type {
                            if let Some(loc) = find_qf_object_property_def(doc, obj_name, &name) {
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
                if let Some(loc) = find_quickfunc_def(doc, &name) {
                    return Some(GotoDefinitionResponse::Scalar(loc));
                }
            }

            // Priority 4: Enum type name → @ENUMS
            if let Some(loc) = find_enum_def(doc, &name) {
                return Some(GotoDefinitionResponse::Scalar(loc));
            }

            // Priority 5: Import alias → @IMPORTS
            if let Some(loc) = find_import_def(doc, &name) {
                return Some(GotoDefinitionResponse::Scalar(loc));
            }

            // Priority 6: QuickFunc local variable declaration
            if token.section == SectionId::QuickFuncs {
                if let Some(loc) = find_quickfunc_local_var_def(doc, &name) {
                    return Some(GotoDefinitionResponse::Scalar(loc));
                }
            }

            // Priority 7: DATA variable
            if let Some(loc) = find_data_var_def(doc, &name, token.section) {
                return Some(GotoDefinitionResponse::Scalar(loc));
            }

            // Priority 8: QuickFunc parameter
            if token.section == SectionId::QuickFuncs {
                if let Some(loc) = find_param_def(doc, &name) {
                    return Some(GotoDefinitionResponse::Scalar(loc));
                }
            }

            None
        }


        // ── Interpolated string — navigate to identifier inside {expr} ────────
        TokenType::InterpolatedString(template) => {
            find_interpolated_string_def(token, template, pos, doc)
        }

        // ── ~ symbol — navigate to the QuickFunc that follows ─────────────────
        // (User is at the declaration prefix; nothing to jump TO, but we try
        // to jump to the identifier itself for consistency.)
        TokenType::Symbol('~') => {
            doc.tokens.get(index + 1).and_then(|next| {
                if let TokenType::Identifier(name) = &next.token_type {
                    find_quickfunc_def(doc, name).map(GotoDefinitionResponse::Scalar)
                } else {
                    None
                }
            })
        }

        // ── Import path string — navigate to the file on disk ────────────────
        TokenType::String(path) if token.section == SectionId::Imports => {
            navigate_to_import_path(path, doc)
        }
        TokenType::StringSingle(path) if token.section == SectionId::Imports => {
            navigate_to_import_path(path, doc)
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

// ── Interpolated string goto ──────────────────────────────────────────────────
//
// Source: $"Hello {name} and {x + y}"
//           ^token.column (1-based, points to $)
//
// template = "Hello {name} and {x + y}"  (content without $" wrapper)
//
// cursor_col_0 (0-based LSP) → template_offset = cursor_col_0 - (token.column-1) - 2

fn find_interpolated_string_def(
    token:    &Token,
    template: &str,
    pos:      Position,
    doc:      &Document,
) -> Option<GotoDefinitionResponse> {
    let token_start_col_0 = token.column.saturating_sub(1);
    let cursor_col_0      = pos.character as usize;

    // Guard: cursor must be past the $" prefix
    if cursor_col_0 < token_start_col_0.saturating_add(2) {
        return None;
    }

    let template_offset = cursor_col_0 - token_start_col_0 - 2;

    let ident = extract_ident_at_template_offset(template, template_offset)?;

    // Try to navigate to the identifier's definition.
    // Order: param → local var → QF call → enum → import → data var
    find_param_def(doc, &ident)
        .or_else(|| find_quickfunc_local_var_def(doc, &ident))
        .or_else(|| find_quickfunc_def(doc, &ident))
        .or_else(|| find_enum_def(doc, &ident))
        .or_else(|| find_import_def(doc, &ident))
        .or_else(|| find_data_var_def_unconstrained(doc, &ident))
        .map(GotoDefinitionResponse::Scalar)
}

/// Find `{...}` block at `offset` in `template`, then extract the identifier
/// under the cursor within that block.
fn extract_ident_at_template_offset(template: &str, offset: usize) -> Option<String> {
    let chars: Vec<char> = template.chars().collect();
    if offset >= chars.len() { return None; }

    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '{' {
            let block_start = i;
            let mut depth   = 1usize;
            let mut j       = i + 1;

            while j < chars.len() && depth > 0 {
                match chars[j] {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _   => {}
                }
                j += 1;
            }
            // Block spans [block_start, j)  where j is the char AFTER '}'

            if offset >= block_start && offset < j {
                // Cursor is inside this {…} block
                let content_start = block_start + 1;
                let content_end   = j.saturating_sub(1); // position of '}'

                // FIX (Group I): cursor sitting exactly on '{' (offset == block_start)
                // or exactly on '}' (offset == content_end) is not inside an
                // identifier and must return None. Without this guard, offset ==
                // block_start makes expr_offset = offset.saturating_sub(content_start)
                // clamp to 0 instead of going negative, which incorrectly resolves
                // to the first identifier in the block (e.g. cursor on '{' in
                // "Hello {name}!" wrongly returned Some("name") instead of None).
                if offset <= block_start || offset >= content_end {
                    return None;
                }

                if content_start >= content_end { return None; }

                let expr: String = chars[content_start..content_end].iter().collect();
                let expr_offset  = offset.saturating_sub(content_start);
                return find_ident_at_str_offset(&expr, expr_offset);
            }

            i = j;
        } else {
            i += 1;
        }
    }
    None
}

/// Find the identifier token that covers `offset` (char index) in `s`.
/// Returns `None` for non-identifier chars (digits, operators, spaces, etc.)
/// and also rejects strings that start with a digit (numeric literals).
fn find_ident_at_str_offset(s: &str, offset: usize) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    if offset >= chars.len() { return None; }

    let ch = chars[offset];
    if !ch.is_alphanumeric() && ch != '_' { return None; }

    // Walk back to identifier start
    let mut start = offset;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }

    // Walk forward to identifier end
    let mut end = offset;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    let ident: String = chars[start..end].iter().collect();

    // Reject pure-digit strings (numeric literals) and keywords unlikely to be vars
    if ident.is_empty() || chars[start].is_ascii_digit() {
        return None;
    }
    // Reject DixScript keyword literals
    if matches!(ident.as_str(), "true" | "false" | "null" | "and" | "or" | "not") {
        return None;
    }

    Some(ident)
}

// ── Import path navigation ────────────────────────────────────────────────────

fn navigate_to_import_path(path: &str, doc: &Document) -> Option<GotoDefinitionResponse> {
    // Resolve relative to the current file's directory
    let current_path = doc.uri.to_file_path().ok()?;
    let base_dir     = current_path.parent()?;

    let target = if path.starts_with('/') || path.starts_with("http") {
        // Absolute or URL — absolute paths only (URLs not navigable on disk)
        if path.starts_with("http") { return None; }
        std::path::PathBuf::from(path)
    } else {
        base_dir.join(path)
    };

    if target.exists() {
        let uri = Url::from_file_path(&target).ok()?;
        Some(GotoDefinitionResponse::Scalar(make_location(&uri, 0, 0, 0, 0)))
    } else {
        tracing::debug!("navigate_to_import_path: file not found: {}", target.display());
        None
    }
}

// ── @CONFIG entry navigation ──────────────────────────────────────────────────

fn find_config_entry_def(doc: &Document, key: &str) -> Option<GotoDefinitionResponse> {
    let config = doc.ast.as_ref()?.config.as_ref()?;
    let entry  = config.entries.iter().find(|e| e.key == key)?;

    if !entry.position.is_valid() { return None; }

    let line = entry.position.line.saturating_sub(1) as u32;
    let col  = entry.position.column.saturating_sub(1) as u32;

    // Try to refine position from the token stream
    let refined = doc.tokens.iter()
        .filter(|t| t.section == SectionId::Config && t.line == entry.position.line)
        .find(|t| matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == key))
        .map(|t| (t.line.saturating_sub(1) as u32, t.column.saturating_sub(1) as u32));

    let (line, col) = refined.unwrap_or((line, col));
    Some(GotoDefinitionResponse::Scalar(make_location(
        &doc.uri, line, col, line, col + key.len() as u32,
    )))
}

// ── QuickFunc local variable declaration ─────────────────────────────────────

/// Find the first `let`/`const`/`let mut` declaration of `name` inside any
/// QuickFunc body (including nested if/switch branches).
pub fn find_quickfunc_local_var_def(doc: &Document, name: &str) -> Option<Location> {
    let qf = doc.ast.as_ref()?.quick_functions.as_ref()?;
    for func in &qf.functions {
        if let Some(loc) = find_var_decl_in_stmts(&func.body, name, &doc.uri) {
            return Some(loc);
        }
    }
    None
}

fn find_var_decl_in_stmts(
    stmts: &[QuickFuncStatement],
    name:  &str,
    uri:   &Url,
) -> Option<Location> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration { variable_name, position, .. }
                if *variable_name == name =>
            {
                if !position.is_valid() { continue; }
                let line = position.line.saturating_sub(1) as u32;
                let col  = position.column.saturating_sub(1) as u32;
                return Some(make_location(uri, line, col, line, col + name.len() as u32));
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(loc) = find_var_decl_in_stmts(then_branch, name, uri) { return Some(loc); }
                if let Some(eb) = else_branch {
                    if let Some(loc) = find_var_decl_in_stmts(eb, name, uri) { return Some(loc); }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(loc) = find_var_decl_in_stmts(&case.statements, name, uri) { return Some(loc); }
                }
                if let Some(dc) = default_case {
                    if let Some(loc) = find_var_decl_in_stmts(&dc.statements, name, uri) { return Some(loc); }
                }
            }
            _ => {}
        }
    }
    None
}

// ── Unconstrained data variable lookup (used by interpolated string handler) ──

/// Like `find_data_var_def` but does not restrict by section.
fn find_data_var_def_unconstrained(doc: &Document, name: &str) -> Option<Location> {
    find_data_var_def(doc, name, SectionId::QuickFuncs) // non-Data section → no restriction
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
            // Pattern A: ns.Member
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

            // Pattern B: ns.EnumType.FIELD  (3-part)
            if token_index >= 4 {
                let prev2 = doc.tokens.get(token_index - 3)?;
                if matches!(prev2.token_type, TokenType::Symbol('.')) {
                    let ns2 = doc.tokens.get(token_index - 4)?;
                    if let TokenType::Identifier(actual_ns) = &ns2.token_type {
                        if let Some(ns) = st.try_get_namespace(actual_ns.as_str()) {
                            if let Some(fields) = ns.enums.get(potential_ns.as_str()) {
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
        (ast_pos.line.saturating_sub(1) as u32, ast_pos.column.saturating_sub(1) as u32)
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

// ── QuickFunc object property navigation ─────────────────────────────────────

fn find_qf_object_property_def(
    doc:       &Document,
    obj_name:  &str,
    prop_name: &str,
) -> Option<Location> {
    let tokens = &doc.tokens;
    let n      = tokens.len();

    for i in 0..n {
        let t = &tokens[i];
        if t.section != SectionId::QuickFuncs { continue; }
        if !matches!(&t.token_type, TokenType::Identifier(id) if id.as_str() == obj_name) {
            continue;
        }
        // Must NOT be preceded by '.' (would be a usage, not definition)
        if i > 0 && matches!(&tokens[i - 1].token_type, TokenType::Symbol('.')) {
            continue;
        }

        // Look ahead for '=' (within 8 tokens, skipping type annotations)
        let eq_rel = tokens[i + 1..].iter().take(8).position(|t| {
            matches!(&t.token_type, TokenType::Symbol('='))
        });
        let eq_idx = match eq_rel { Some(j) => i + 1 + j, None => continue };

        // After '=', look for '{' within 4 tokens
        let brace_rel = tokens[eq_idx + 1..].iter().take(4).position(|t| {
            t.section == SectionId::QuickFuncs
                && matches!(&t.token_type, TokenType::Symbol('{'))
        });
        let brace_idx = match brace_rel { Some(j) => eq_idx + 1 + j, None => continue };

        // Scan inside braces for prop_name =
        let mut depth = 0i32;
        for j in brace_idx..n {
            if tokens[j].section != SectionId::QuickFuncs { continue; }
            match &tokens[j].token_type {
                TokenType::Symbol('{') => depth += 1,
                TokenType::Symbol('}') => {
                    depth -= 1;
                    if depth <= 0 { break; }
                }
                TokenType::Identifier(id) if id.as_str() == prop_name && depth == 1 => {
                    let next_is_eq = tokens.get(j + 1)
                        .map(|t| matches!(&t.token_type, TokenType::Symbol('=')))
                        .unwrap_or(false);
                    if next_is_eq {
                        let line = tokens[j].line.saturating_sub(1) as u32;
                        let col  = tokens[j].column.saturating_sub(1) as u32;
                        return Some(make_location(
                            &doc.uri, line, col, line, col + prop_name.len() as u32,
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

// ── Enum field definition ─────────────────────────────────────────────────────

fn find_enum_field_def(doc: &Document, enum_name: &str, field_name: &str) -> Option<Location> {
    let enums = doc.ast.as_ref()?.enums.as_ref()?;
    let decl  = enums.enums.iter().find(|e| e.name == enum_name)?;
    let field = decl.fields.iter().find(|f| f.name == field_name)?;

    if field.position.is_valid() {
        let l = field.position.line.saturating_sub(1) as u32;
        let c = field.position.column.saturating_sub(1) as u32;
        let refined = doc.tokens.iter()
            .filter(|t| t.section == SectionId::Enums && t.line == field.position.line)
            .find(|t| matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == field_name))
            .map(|t| (t.line.saturating_sub(1) as u32, t.column.saturating_sub(1) as u32));
        let (line, col) = refined.unwrap_or((l, c));
        return Some(make_location(&doc.uri, line, col, line, col + field_name.len() as u32));
    }

    // Fallback: token stream scan in @ENUMS
    for (i, t) in doc.tokens.iter().enumerate() {
        if t.section != SectionId::Enums { continue; }
        if !matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == field_name) {
            continue;
        }
        // Field declaration is NOT preceded by '.'
        if i > 0 && matches!(&doc.tokens[i - 1].token_type, TokenType::Symbol('.')) {
            continue;
        }
        let line = t.line.saturating_sub(1) as u32;
        let col  = t.column.saturating_sub(1) as u32;
        return Some(make_location(&doc.uri, line, col, line, col + field_name.len() as u32));
    }

    None
}

// ── QuickFunc declaration ─────────────────────────────────────────────────────

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
        .map(|t| (t.line.saturating_sub(1) as u32, t.column.saturating_sub(1) as u32))
}

// ── Enum type declaration ─────────────────────────────────────────────────────

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
        .map(|t| (t.line.saturating_sub(1) as u32, t.column.saturating_sub(1) as u32));

    let (line, col) = refined.unwrap_or((line, col));
    Some(make_location(&doc.uri, line, col, line, col + enum_name.len() as u32))
}

// ── Import alias ──────────────────────────────────────────────────────────────

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
        .map(|t| (t.line.saturating_sub(1) as u32, t.column.saturating_sub(1) as u32));

    let (line, col) = refined.unwrap_or((line, col));
    Some(make_location(&doc.uri, line, col, line, col + alias.len() as u32))
}

// ── DATA variable ─────────────────────────────────────────────────────────────

fn find_data_var_def(doc: &Document, name: &str, section: SectionId) -> Option<Location> {
    // Don't navigate when cursor is already in @DATA (you're at the definition)
    if section == SectionId::Data { return None; }

    let data = doc.ast.as_ref()?.data.as_ref()?;

    use dixscript::Compiler::AST::DataEntry;
    for entry in &data.entries {
        match entry {
            DataEntry::SimpleProperty { name: n, position, .. } if *n == name => {
                if !position.is_valid() { return None; }
                let line = position.line.saturating_sub(1) as u32;
                let col  = position.column.saturating_sub(1) as u32;
                return Some(make_location(&doc.uri, line, col, line, col + name.len() as u32));
            }
            DataEntry::TableProperty { path, position, .. }
                if path.segments.first().map(|s| s.as_str()) == Some(name) =>
            {
                if !position.is_valid() { return None; }
                let line = position.line.saturating_sub(1) as u32;
                let col  = position.column.saturating_sub(1) as u32;
                return Some(make_location(&doc.uri, line, col, line, col + name.len() as u32));
            }
            DataEntry::GroupArray { path, position, .. }
                if path.segments.first().map(|s| s.as_str()) == Some(name) =>
            {
                if !position.is_valid() { return None; }
                let line = position.line.saturating_sub(1) as u32;
                let col  = position.column.saturating_sub(1) as u32;
                return Some(make_location(&doc.uri, line, col, line, col + name.len() as u32));
            }
            DataEntry::ObjectProperty { name: n, position, .. } if *n == name => {
                if !position.is_valid() { return None; }
                let line = position.line.saturating_sub(1) as u32;
                let col  = position.column.saturating_sub(1) as u32;
                return Some(make_location(&doc.uri, line, col, line, col + name.len() as u32));
            }
            _ => {}
        }
    }
    None
}

// ── QuickFunc parameter ───────────────────────────────────────────────────────

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
                .find(|t| matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == name))
                .map(|t| (t.line.saturating_sub(1) as u32, t.column.saturating_sub(1) as u32));

            let (line, col) = refined.unwrap_or((line, col));
            return Some(make_location(&doc.uri, line, col, line, col + name.len() as u32));
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ident_simple() {
        // {name} at offset 1 (pointing to 'n' in 'name', after '{')
        // template = "Hello {name}!"
        // block_start=6, block_end=12, content="name", expr_offset=0
        assert_eq!(
            extract_ident_at_template_offset("Hello {name}!", 7),
            Some("name".to_string())
        );
        assert_eq!(
            extract_ident_at_template_offset("Hello {name}!", 9),
            Some("name".to_string())
        );
    }

    #[test]
    fn extract_ident_arithmetic() {
        // {x + y} — cursor on 'x' or 'y'
        // template = "{x + y}"
        // block [0,7), content = "x + y"
        // offset 0 → '{' → None (outside ident)... wait offset 0 IS '{', we'd look at block
        // Actually offset 1 → 'x' → block [0,7), expr_offset = 1-1 = 0 → 'x' in "x + y"
        assert_eq!(
            extract_ident_at_template_offset("{x + y}", 1),
            Some("x".to_string())
        );
        assert_eq!(
            extract_ident_at_template_offset("{x + y}", 5),
            Some("y".to_string())
        );
    }

    #[test]
    fn extract_ident_on_brace_returns_none() {
        // Cursor directly on '{' or '}' — not an identifier
        // '{' is at index 6 in "Hello {name}!"
        assert_eq!(extract_ident_at_template_offset("Hello {name}!", 6), None);
    }

    #[test]
    fn extract_ident_on_closing_brace_returns_none() {
        // '}' is at index 11 in "Hello {name}!"
        assert_eq!(extract_ident_at_template_offset("Hello {name}!", 11), None);
    }

    #[test]
    fn extract_ident_outside_braces_returns_none() {
        // Cursor on literal text outside {}
        assert_eq!(extract_ident_at_template_offset("Hello {name}!", 2), None);
    }

    #[test]
    fn extract_ident_property_access() {
        // {obj.field} — cursor on 'obj' or 'field'
        assert_eq!(
            extract_ident_at_template_offset("{obj.field}", 1),
            Some("obj".to_string())
        );
        assert_eq!(
            extract_ident_at_template_offset("{obj.field}", 5),
            Some("field".to_string())
        );
    }

    #[test]
    fn extract_ident_rejects_numeric() {
        // {x + 42} — cursor on '4'
        assert_eq!(extract_ident_at_template_offset("{x + 42}", 5), None);
    }

    #[test]
    fn extract_ident_empty_block_returns_none() {
        // {} — no content at all
        assert_eq!(extract_ident_at_template_offset("a {} b", 2), None);
        assert_eq!(extract_ident_at_template_offset("a {} b", 3), None);
    }

    #[test]
    fn find_ident_at_str_offset_basic() {
        assert_eq!(find_ident_at_str_offset("myVar", 0), Some("myVar".to_string()));
        assert_eq!(find_ident_at_str_offset("myVar", 4), Some("myVar".to_string()));
        assert_eq!(find_ident_at_str_offset("x + y", 0), Some("x".to_string()));
        assert_eq!(find_ident_at_str_offset("x + y", 4), Some("y".to_string()));
        assert_eq!(find_ident_at_str_offset("x + y", 2), None); // space
    }

    #[test]
    fn find_ident_at_str_offset_rejects_keywords() {
        assert_eq!(find_ident_at_str_offset("true", 0), None);
        assert_eq!(find_ident_at_str_offset("false", 0), None);
        assert_eq!(find_ident_at_str_offset("null", 0), None);
    }
                                }
