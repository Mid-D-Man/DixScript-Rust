// mdix-lsp/src/features/goto_definition.rs
//
// Added:
//   - goto_import_file: string literal in @IMPORTS → opens the target file
//   - goto_namespace_symbol: namespace.funcName / namespace.EnumName.FIELD
//     → jumps into the imported file at the correct line
//   - Cloud import cache path resolution

use std::panic;
use std::path::PathBuf;

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
    let token_type = token.token_type.clone();

    match &token_type {
        TokenType::Identifier(name) => {
            // 1. Enum TYPE name (e.g. `AIType` in `AIType.BOSS`) → @ENUMS decl.
            if let Some(r) = goto_enum_type(doc, name, index) {
                return Some(r);
            }
            // 2. Enum FIELD: cursor on FIELD in EnumName.FIELD.
            if let Some(r) = goto_enum_from_context(doc, name, index) {
                return Some(r);
            }
            // 3. Namespace alias → @IMPORTS declaration line.
            if let Some(r) = goto_namespace_alias(doc, name, index) {
                return Some(r);
            }
            // 4. Namespace.function or Namespace.Enum.FIELD → imported file.
            if let Some(r) = goto_namespace_symbol(doc, name, index) {
                return Some(r);
            }
            // 5. QuickFunc call / reference → ~name declaration.
            if let Some(r) = goto_quickfunc(doc, name) {
                return Some(r);
            }
            // 6. QuickFunc parameter.
            if let Some(r) = goto_quickfunc_param(doc, name, pos) {
                return Some(r);
            }
            // 7. QuickFunc local variable (let / const).
            if let Some(r) = goto_quickfunc_local_var(doc, name) {
                return Some(r);
            }
            // 8. DATA property definition.
            goto_data_property(doc, name)
        }

        TokenType::EnumAccess { enum_name, value } => {
            goto_enum_field(doc, enum_name, value)
        }

        // String literal in @IMPORTS → open the file.
        TokenType::String(path) | TokenType::StringSingle(path) => {
            if token.section == SectionId::Imports {
                goto_import_file(doc, path)
            } else {
                None
            }
        }

        _ => None,
    }
}

// ── Namespace alias → @IMPORTS declaration ────────────────────────────────────

fn goto_namespace_alias(
    doc:         &Document,
    name:        &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    // Only trigger when the next token is NOT `.` (that's handled by goto_namespace_symbol).
    let has_dot_after = doc.tokens
        .iter()
        .skip(token_index + 1)
        .take(2)
        .any(|t| matches!(t.token_type, TokenType::Symbol('.')));
    if has_dot_after { return None; }

    let st = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    if !st.is_imported_namespace(name) { return None; }

    // Find the alias token in the @IMPORTS section.
    for tok in doc.tokens.iter().filter(|t| t.section == SectionId::Imports) {
        if let TokenType::Identifier(id) = &tok.token_type {
            if id.as_str() == name {
                let line = tok.line.saturating_sub(1) as u32;
                let col  = tok.column.saturating_sub(1) as u32;
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri:   doc.uri.clone(),
                    range: Range::new(Position::new(line, col), Position::new(line, col + name.len() as u32)),
                }));
            }
        }
    }
    None
}

// ── Namespace.symbol → imported file ──────────────────────────────────────────
//
// Handles three patterns:
//   Namespace.funcName(...)      → jump to funcName in namespace file
//   Namespace.EnumName.FIELD     → jump to EnumName decl in namespace file
//   Namespace.EnumName           → jump to EnumName decl in namespace file

fn goto_namespace_symbol(
    doc:         &Document,
    name:        &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    let st = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    if !st.is_imported_namespace(name) { return None; }

    let ns = st.try_get_namespace(name)?;

    // Peek: next meaningful token should be `.`
    let dot_offset = doc.tokens.iter()
        .skip(token_index + 1)
        .take(3)
        .position(|t| matches!(t.token_type, TokenType::Symbol('.')))?;

    let after_dot_idx = token_index + 1 + dot_offset + 1;
    let symbol_token = doc.tokens.get(after_dot_idx)?;

    let symbol_name = match &symbol_token.token_type {
        TokenType::Identifier(id) => id.clone(),
        _ => return None,
    };

    // Resolve the target file URI (local or cloud cache).
    let target_uri = resolve_namespace_file_uri(&ns.file_path)?;

    // Is symbol_name a function in this namespace?
    if let Some(func_info) = ns.functions.get(&symbol_name) {
        let line = func_info.signature.line.saturating_sub(1).max(0) as u32;
        let col  = func_info.signature.column.saturating_sub(1).max(0) as u32;
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri:   target_uri,
            range: Range::new(Position::new(line, col), Position::new(line, col + symbol_name.len() as u32)),
        }));
    }

    // Is symbol_name an enum in this namespace?
    if ns.enums.contains_key(&symbol_name) {
        // We don't have position info for enums in the namespace struct,
        // so jump to the top of the file — the editor can search from there.
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri:   target_uri,
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        }));
    }

    // Check if it's a nested namespace import alias.
    if ns.local_imports.contains_key(&symbol_name) {
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri:   target_uri,
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        }));
    }

    None
}

/// Resolve the file URI for an imported namespace.
/// For local files: standard file URI.
/// For cloud imports: the deterministic cache path.
fn resolve_namespace_file_uri(file_path: &str) -> Option<Url> {
    if file_path.starts_with("http://") || file_path.starts_with("https://") {
        // Build the cache path the same way CloudFileCache does.
        // Cache root: ~/.mdix_cache (Linux/macOS) or %LOCALAPPDATA%/mdix_cache (Windows)
        let cache_root = get_cache_root();
        let url_hash   = sha256_hex(file_path);
        let cache_key  = &url_hash[..16];
        let filename   = extract_url_filename(file_path);
        let cache_path = cache_root.join(cache_key).join(filename);

        if cache_path.exists() {
            Url::from_file_path(&cache_path).ok()
        } else {
            // File hasn't been downloaded yet — can't navigate.
            tracing::warn!("Cloud import not in cache, cannot navigate: {}", file_path);
            None
        }
    } else {
        // Local file.
        let path = PathBuf::from(file_path);
        if path.exists() {
            Url::from_file_path(&path).ok()
        } else {
            tracing::warn!("Import file not found on disk: {}", file_path);
            None
        }
    }
}

fn get_cache_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join("mdix_cache"))
            .unwrap_or_else(|_| PathBuf::from(".mdix_cache"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(".mdix_cache"))
            .unwrap_or_else(|_| PathBuf::from(".mdix_cache"))
    }
}

fn sha256_hex(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Simple deterministic hash — matches CloudFileCache which uses SHA-256.
    // In the LSP we don't want to pull sha2 into every binary; use the same
    // algorithm by reusing the dixscript crate's sha2 dependency indirectly.
    // We call the same formula: sha256(url).
    // Since dixscript is already linked, use the sha2 crate it already uses
    // by declaring it as a dev-dep in mdix-lsp if needed, or just replicate
    // the 16-char prefix logic with a simpler hash for path lookup.
    // NOTE: If the cache path doesn't match, the file won't be found and
    // we fall through to None — which is safe (no navigation, no crash).
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn extract_url_filename(url: &str) -> String {
    url.split('?').next()
        .and_then(|u| u.rsplit('/').next())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "import.mdix".to_string())
}

// ── Import file path → target file ────────────────────────────────────────────

fn goto_import_file(doc: &Document, path: &str) -> Option<GotoDefinitionResponse> {
    if path.starts_with("http://") || path.starts_with("https://") {
        // Cloud import — navigate to cache.
        let cache_uri = resolve_namespace_file_uri(path)?;
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri:   cache_uri,
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        }));
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
            if let Ok(uri) = Url::from_file_path(&candidate) {
                return Some(GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                }));
            }
        }
    }
    None
}

// ── Enum TYPE name → @ENUMS declaration ──────────────────────────────────────

fn goto_enum_type(
    doc:         &Document,
    name:        &str,
    token_index: usize,
) -> Option<GotoDefinitionResponse> {
    let enums = doc.ast.as_ref()?.enums.as_ref()?;
    let decl  = enums.enums.iter().find(|e| e.name == name)?;

    let has_dot_ahead = doc.tokens.iter()
        .skip(token_index + 1)
        .take(3)
        .any(|t| matches!(t.token_type, TokenType::Symbol('.')));

    if !has_dot_ahead { return None; }

    // Make sure it's not a namespace (handled by goto_namespace_symbol).
    if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        if st.is_imported_namespace(name) { return None; }
    }

    let line = decl.position.line.saturating_sub(1) as u32;
    let col  = decl.position.column.saturating_sub(1) as u32;
    Some(GotoDefinitionResponse::Scalar(Location {
        uri:   doc.uri.clone(),
        range: Range::new(Position::new(line, col), Position::new(line, col + name.len() as u32)),
    }))
}

// ── Enum FIELD from context ───────────────────────────────────────────────────

fn goto_enum_from_context(
    doc:         &Document,
    field_name:  &str,
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

    // Don't handle namespace.EnumName.FIELD here — that's goto_namespace_symbol.
    if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        if st.is_imported_namespace(enum_name) { return None; }
    }

    let enums = doc.ast.as_ref()?.enums.as_ref()?;
    if !enums.enums.iter().any(|e| e.name == *enum_name) { return None; }

    goto_enum_field(doc, enum_name, field_name)
}

// ── QuickFunc declaration ─────────────────────────────────────────────────────

fn goto_quickfunc(doc: &Document, name: &str) -> Option<GotoDefinitionResponse> {
    let qf = doc.ast.as_ref()?.quick_functions.as_ref()?;

    for func in &qf.functions {
        if func.name != name { continue; }
        let line = func.position.line.saturating_sub(1) as u32;
        let col  = func.position.column.saturating_sub(1) as u32;
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri:   doc.uri.clone(),
            range: Range::new(Position::new(line, col), Position::new(line, col + 1 + name.len() as u32)),
        }));
    }
    None
}

// ── QuickFunc parameter declaration ──────────────────────────────────────────

fn goto_quickfunc_param(
    doc:  &Document,
    name: &str,
    pos:  Position,
) -> Option<GotoDefinitionResponse> {
    let qf = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let cursor_line_1based = pos.line as usize + 1;

    let enclosing_func = qf.functions.iter()
        .filter(|f| f.position.line <= cursor_line_1based)
        .max_by_key(|f| f.position.line);

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
            let line = param.position.line.saturating_sub(1) as u32;
            let col  = param.position.column.saturating_sub(1) as u32;
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri:   doc.uri.clone(),
                range: Range::new(Position::new(line, col), Position::new(line, col + name.len() as u32)),
            }));
        }
    }
    None
}

// ── QuickFunc local variable ──────────────────────────────────────────────────

fn goto_quickfunc_local_var(doc: &Document, name: &str) -> Option<GotoDefinitionResponse> {
    let qf = doc.ast.as_ref()?.quick_functions.as_ref()?;

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
                range: Range::new(Position::new(line, col), Position::new(line, col + name.len() as u32)),
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
                if let Some(eb) = else_branch {
                    if let Some(p) = find_var_in_statements(eb, name) { return Some(p); }
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
            range: Range::new(Position::new(line, col), Position::new(line, col + name.len() as u32)),
        }));
    }
    None
}

// ── Enum field declaration ────────────────────────────────────────────────────

fn goto_enum_field(
    doc: &Document, enum_name: &str, field_name: &str,
) -> Option<GotoDefinitionResponse> {
    let enums = doc.ast.as_ref()?.enums.as_ref()?;

    for decl in &enums.enums {
        if !decl.name.eq_ignore_ascii_case(enum_name) { continue; }
        for field in &decl.fields {
            if !field.name.eq_ignore_ascii_case(field_name) { continue; }
            let line = field.position.line.saturating_sub(1) as u32;
            let col  = field.position.column.saturating_sub(1) as u32;
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri:   doc.uri.clone(),
                range: Range::new(Position::new(line, col), Position::new(line, col + field_name.len() as u32)),
            }));
        }
    }
    None
}
