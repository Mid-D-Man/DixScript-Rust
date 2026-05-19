// mdix-lsp/src/features/document_symbols.rs
//! Document symbol provider — powers the outline panel / breadcrumb bar.
//!
//! Approach B: @CONFIG section position is read from the SectionConfig token
//! in doc.tokens. No config_line_range field needed.

use std::panic;
use tower_lsp::lsp_types::{
    DocumentSymbolResponse, Location, Position, Range, SymbolInformation, SymbolKind, Url,
};
use dixscript::Compiler::AST::{DataEntry, DixScript};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;

pub fn provide(doc: Option<&Document>) -> Option<DocumentSymbolResponse> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("document_symbols panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>) -> Option<DocumentSymbolResponse> {
    let doc = doc?;
    let ast = doc.ast.as_ref()?;
    let uri = &doc.uri;

    let mut symbols: Vec<SymbolInformation> = Vec::new();

    // ── @CONFIG ───────────────────────────────────────────────────────────────
    // Read position from the real SectionConfig token — no stored line range.
    if let Some(config_range) = config_section_range(&doc.tokens) {
        symbols.push(make_symbol(
            "@CONFIG".to_string(),
            SymbolKind::MODULE,
            uri,
            config_range.0, 0,
            config_range.1, 0,
        ));
        // Individual config keys — positions from ConfigSection AST entries
        // which were populated by process_config_tokens with real token positions.
        if let Some(ref config) = ast.config {
            for entry in &config.entries {
                if entry.position.is_valid() {
                    let line = entry.position.line.saturating_sub(1) as u32;
                    let col  = entry.position.column.saturating_sub(1) as u32;
                    symbols.push(make_symbol(
                        entry.key.clone(),
                        SymbolKind::PROPERTY,
                        uri,
                        line, col, line, col + entry.key.len() as u32,
                    ));
                }
            }
        }
    }

    // ── @IMPORTS ──────────────────────────────────────────────────────────────
    if let Some(ref imports) = ast.imports {
        if imports.position.is_valid() {
            let line = imports.position.line.saturating_sub(1) as u32;
            symbols.push(make_symbol(
                "@IMPORTS".to_string(), SymbolKind::MODULE, uri,
                line, 0, line, 8,
            ));
        }
        for import in &imports.imports {
            if import.position.is_valid() {
                let line = import.position.line.saturating_sub(1) as u32;
                let col  = import.position.column.saturating_sub(1) as u32;
                symbols.push(make_symbol(
                    format!("{} (import)", import.alias),
                    SymbolKind::NAMESPACE,
                    uri, line, col, line, col + import.alias.len() as u32,
                ));
            }
        }
    }

    // ── @DLM ──────────────────────────────────────────────────────────────────
    if let Some(ref dlm) = ast.dlm {
        if dlm.position.is_valid() {
            let line = dlm.position.line.saturating_sub(1) as u32;
            symbols.push(make_symbol(
                "@DLM".to_string(), SymbolKind::MODULE, uri,
                line, 0, line, 4,
            ));
        }
    }

    // ── @ENUMS ────────────────────────────────────────────────────────────────
    if let Some(ref enums) = ast.enums {
        if enums.position.is_valid() {
            let line = enums.position.line.saturating_sub(1) as u32;
            symbols.push(make_symbol(
                "@ENUMS".to_string(), SymbolKind::MODULE, uri,
                line, 0, line, 6,
            ));
        }
        for decl in &enums.enums {
            if decl.position.is_valid() {
                let line = decl.position.line.saturating_sub(1) as u32;
                let col  = decl.position.column.saturating_sub(1) as u32;
                symbols.push(make_symbol(
                    decl.name.clone(),
                    SymbolKind::ENUM,
                    uri, line, col, line, col + decl.name.len() as u32,
                ));
                for field in &decl.fields {
                    if field.position.is_valid() {
                        let fline = field.position.line.saturating_sub(1) as u32;
                        let fcol  = field.position.column.saturating_sub(1) as u32;
                        let label = field.value
                            .map(|v| format!("{} = {}", field.name, v))
                            .unwrap_or_else(|| field.name.clone());
                        symbols.push(make_symbol(
                            label,
                            SymbolKind::ENUM_MEMBER,
                            uri, fline, fcol, fline, fcol + field.name.len() as u32,
                        ));
                    }
                }
            }
        }
    }

    // ── @QUICKFUNCS ───────────────────────────────────────────────────────────
    if let Some(ref qf) = ast.quick_functions {
        if qf.position.is_valid() {
            let line = qf.position.line.saturating_sub(1) as u32;
            symbols.push(make_symbol(
                "@QUICKFUNCS".to_string(), SymbolKind::MODULE, uri,
                line, 0, line, 11,
            ));
        }
        for func in &qf.functions {
            if func.position.is_valid() {
                let line = func.position.line.saturating_sub(1) as u32;
                let col  = func.position.column.saturating_sub(1) as u32;
                let ret  = func.return_type.map(|t| format!("<{}>", t)).unwrap_or_default();
                let params: Vec<String> = func.parameters.iter()
                    .map(|p| {
                        let t = p.data_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
                        format!("{}{}", p.name, t)
                    })
                    .collect();
                let detail = format!("~{}{}({})", func.name, ret, params.join(", "));
                symbols.push(make_symbol(
                    detail,
                    SymbolKind::FUNCTION,
                    uri, line, col,
                    line, col + 1 + func.name.len() as u32,
                ));
            }
        }
    }

    // ── @DATA ─────────────────────────────────────────────────────────────────
    if let Some(ref data) = ast.data {
        if data.position.is_valid() {
            let line = data.position.line.saturating_sub(1) as u32;
            symbols.push(make_symbol(
                "@DATA".to_string(), SymbolKind::MODULE, uri,
                line, 0, line, 5,
            ));
        }
        for entry in &data.entries {
            match entry {
                DataEntry::SimpleProperty { name, position, .. } => {
                    if position.is_valid() {
                        let line = position.line.saturating_sub(1) as u32;
                        let col  = position.column.saturating_sub(1) as u32;
                        symbols.push(make_symbol(
                            name.clone(),
                            SymbolKind::VARIABLE,
                            uri, line, col, line, col + name.len() as u32,
                        ));
                    }
                }
                DataEntry::TableProperty { path, position, .. } => {
                    if position.is_valid() {
                        let line  = position.line.saturating_sub(1) as u32;
                        let col   = position.column.saturating_sub(1) as u32;
                        let label = path.segments.join(".");
                        symbols.push(make_symbol(
                            format!("{}: (table)", label),
                            SymbolKind::OBJECT,
                            uri, line, col, line, col + label.len() as u32,
                        ));
                    }
                }
                DataEntry::GroupArray { path, position, items, .. } => {
                    if position.is_valid() {
                        let line  = position.line.saturating_sub(1) as u32;
                        let col   = position.column.saturating_sub(1) as u32;
                        let label = path.segments.join(".");
                        symbols.push(make_symbol(
                            format!("{} (array[{}])", label, items.len()),
                            SymbolKind::ARRAY,
                            uri, line, col, line, col + label.len() as u32,
                        ));
                    }
                }
                DataEntry::ObjectProperty { name, position, .. } => {
                    if position.is_valid() {
                        let line = position.line.saturating_sub(1) as u32;
                        let col  = position.column.saturating_sub(1) as u32;
                        symbols.push(make_symbol(
                            format!("{} (object)", name),
                            SymbolKind::OBJECT,
                            uri, line, col, line, col + name.len() as u32,
                        ));
                    }
                }
            }
        }
    }

    // ── @SECURITY ─────────────────────────────────────────────────────────────
    if let Some(ref sec) = ast.security {
        if sec.position.is_valid() {
            let line = sec.position.line.saturating_sub(1) as u32;
            symbols.push(make_symbol(
                "@SECURITY".to_string(), SymbolKind::MODULE, uri,
                line, 0, line, 9,
            ));
        }
    }

    if symbols.is_empty() {
        None
    } else {
        #[allow(deprecated)]
        Some(DocumentSymbolResponse::Flat(symbols))
    }
}

// ── @CONFIG section line range from token stream ──────────────────────────────
//
// Returns (start_lsp_line, end_lsp_line) where both are 0-based LSP lines.
// start = line of @CONFIG keyword token
// end   = line of the last token with SectionId::Config
//
// Falls back to None if the file has no @CONFIG section.

fn config_section_range(
    tokens: &[dixscript::Compiler::Core::Tokenizer::Token],
) -> Option<(u32, u32)> {
    // Find the SectionConfig token for the start line.
    let start_tok = tokens
        .iter()
        .find(|t| matches!(t.token_type, TokenType::SectionConfig))?;

    let start_lsp = start_tok.line.saturating_sub(1) as u32;

    // End = last token (excluding EOF) that carries SectionId::Config.
    let end_lsp = tokens
        .iter()
        .rev()
        .find(|t| {
            t.section == SectionId::Config
                && !matches!(t.token_type, TokenType::EndOfFile)
        })
        .map(|t| t.line.saturating_sub(1) as u32)
        .unwrap_or(start_lsp);

    Some((start_lsp, end_lsp))
}

// ── Symbol constructor ────────────────────────────────────────────────────────

#[allow(deprecated)]
fn make_symbol(
    name:       String,
    kind:       SymbolKind,
    uri:        &Url,
    start_line: u32,
    start_col:  u32,
    end_line:   u32,
    end_col:    u32,
) -> SymbolInformation {
    SymbolInformation {
        name,
        kind,
        location: Location {
            uri:   uri.clone(),
            range: Range::new(
                Position::new(start_line, start_col),
                Position::new(end_line,   end_col),
            ),
        },
        tags:           None,
        deprecated:     None,
        container_name: None,
    }
}
