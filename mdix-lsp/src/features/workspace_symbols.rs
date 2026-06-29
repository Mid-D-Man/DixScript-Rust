
//! Workspace symbol provider — powers Cmd+T / Ctrl+T symbol search.
//!
//! Searches all indexed documents for symbols matching `query` using
//! a case-insensitive substring match. Returns up to 256 results,
//! sorted with exact prefix matches first.

use std::panic;

use dashmap::DashMap;
use tower_lsp::lsp_types::{Location, Position, Range, SymbolInformation, SymbolKind, Url};
use dixscript::Compiler::AST::{DataEntry, DixScript, Position as AstPosition};

use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(
    documents: &DashMap<Url, Document>,
    query:     &str,
) -> Option<Vec<SymbolInformation>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        provide_inner(documents, query)
    }));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("workspace_symbols panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(
    documents:   &DashMap<Url, Document>,
    query:       &str,
) -> Option<Vec<SymbolInformation>> {
    let query_lower = query.to_lowercase();
    let mut symbols: Vec<SymbolInformation> = Vec::new();

    for entry in documents.iter() {
        let uri = entry.key().clone();
        let doc = entry.value();
        if let Some(ast) = &doc.ast {
            collect_symbols(ast, &uri, &query_lower, &mut symbols);
        }
    }

    if symbols.is_empty() {
        return None;
    }

    // Sort: exact prefix matches first, then alphabetical
    symbols.sort_by(|a, b| {
        let a_prefix = a.name.to_lowercase().starts_with(&query_lower);
        let b_prefix = b.name.to_lowercase().starts_with(&query_lower);
        match (a_prefix, b_prefix) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _             => a.name.cmp(&b.name),
        }
    });

    symbols.truncate(256);
    Some(symbols)
}

// ── Per-document collection ───────────────────────────────────────────────────

fn collect_symbols(
    ast:         &DixScript,
    uri:         &Url,
    query_lower: &str,
    out:         &mut Vec<SymbolInformation>,
) {
    let file_name = file_name_of(uri);

    // ── @QUICKFUNCS ───────────────────────────────────────────────────────────
    if let Some(qf) = &ast.quick_functions {
        for func in &qf.functions {
            if !matches_query(&func.name, query_lower) { continue; }

            let params: Vec<String> = func.parameters.iter()
                .map(|p| {
                    let t = p.data_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
                    format!("{}{}", p.name, t)
                })
                .collect();
            let ret  = func.return_type.map(|t| format!("<{}>", t)).unwrap_or_default();
            let name = format!("~{}{}({})", func.name, ret, params.join(", "));

            let (line, col) = ast_pos(func.position);
            let end_col     = col + 1 + func.name.len() as u32; // skip ~ char
            out.push(make_sym(name, SymbolKind::FUNCTION, uri, line, col, line, end_col, &file_name));
        }
    }

    // ── @ENUMS ────────────────────────────────────────────────────────────────
    if let Some(enums) = &ast.enums {
        for decl in &enums.enums {
            if matches_query(&decl.name, query_lower) {
                let (line, col) = ast_pos(decl.position);
                out.push(make_sym(
                    decl.name.clone(), SymbolKind::ENUM, uri,
                    line, col, line, col + decl.name.len() as u32,
                    &file_name,
                ));
            }

            // Enum fields: match on "EnumName.FIELD" or just "FIELD"
            for field in &decl.fields {
                let full = format!("{}.{}", decl.name, field.name);
                if matches_query(&full, query_lower) || matches_query(&field.name, query_lower) {
                    let (line, col) = ast_pos(field.position);
                    let detail = field.value.map(|v| format!(" = {}", v)).unwrap_or_default();
                    out.push(make_sym(
                        format!("{}{}", full, detail), SymbolKind::ENUM_MEMBER, uri,
                        line, col, line, col + field.name.len() as u32,
                        &file_name,
                    ));
                }
            }
        }
    }

    // ── @DATA ─────────────────────────────────────────────────────────────────
    if let Some(data) = &ast.data {
        for entry in &data.entries {
            match entry {
                DataEntry::SimpleProperty { name, position, .. } => {
                    if matches_query(name, query_lower) {
                        let (line, col) = ast_pos(*position);
                        out.push(make_sym(
                            name.clone(), SymbolKind::VARIABLE, uri,
                            line, col, line, col + name.len() as u32,
                            &file_name,
                        ));
                    }
                }
                DataEntry::TableProperty { path, position, .. } => {
                    let label = path.segments.join(".");
                    if matches_query(&label, query_lower) {
                        let (line, col) = ast_pos(*position);
                        out.push(make_sym(
                            format!("{}:", label), SymbolKind::OBJECT, uri,
                            line, col, line, col + label.len() as u32,
                            &file_name,
                        ));
                    }
                }
                DataEntry::GroupArray { path, items, position } => {
                    let label = path.segments.join(".");
                    if matches_query(&label, query_lower) {
                        let (line, col) = ast_pos(*position);
                        out.push(make_sym(
                            format!("{}[{}]", label, items.len()), SymbolKind::ARRAY, uri,
                            line, col, line, col + label.len() as u32,
                            &file_name,
                        ));
                    }
                }
                DataEntry::ObjectProperty { name, position, .. } => {
                    if matches_query(name, query_lower) {
                        let (line, col) = ast_pos(*position);
                        out.push(make_sym(
                            format!("{} {{…}}", name), SymbolKind::OBJECT, uri,
                            line, col, line, col + name.len() as u32,
                            &file_name,
                        ));
                    }
                }
            }
        }
    }

    // ── @IMPORTS ──────────────────────────────────────────────────────────────
    if let Some(imports) = &ast.imports {
        for import in &imports.imports {
            if !matches_query(&import.alias, query_lower) { continue; }
            let (line, col) = ast_pos(import.position);
            let kind        = if import.is_cloud_import { "cloud" } else { "local" };
            out.push(make_sym(
                format!("{} ({} import)", import.alias, kind), SymbolKind::NAMESPACE, uri,
                line, col, line, col + import.alias.len() as u32,
                &file_name,
            ));
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

#[inline]
fn matches_query(name: &str, query: &str) -> bool {
    query.is_empty() || name.to_lowercase().contains(query)
}

#[inline]
fn ast_pos(pos: AstPosition) -> (u32, u32) {
    if pos.is_valid() {
        (pos.line.saturating_sub(1) as u32, pos.column.saturating_sub(1) as u32)
    } else {
        (0, 0)
    }
}

fn file_name_of(uri: &Url) -> String {
    uri.path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or("?")
        .to_string()
}

#[allow(deprecated)]
fn make_sym(
    name:       String,
    kind:       SymbolKind,
    uri:        &Url,
    start_line: u32,
    start_col:  u32,
    end_line:   u32,
    end_col:    u32,
    container:  &str,
) -> SymbolInformation {
    SymbolInformation {
        name,
        kind,
        tags:           None,
        deprecated:     None,
        container_name: Some(container.to_string()),
        location: Location {
            uri:   uri.clone(),
            range: Range::new(
                Position::new(start_line, start_col),
                Position::new(end_line, end_col),
            ),
        },
    }
  }
