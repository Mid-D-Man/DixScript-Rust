// mdix-lsp/src/features/document_highlights.rs
//! Document highlight provider.
//!
//! When the cursor rests on an identifier, all matching occurrences in the
//! document are highlighted. Writes (declarations) use WRITE kind; reads use READ.
//!
//! Scoping rules:
//!   - QuickFunc parameters → only within @QUICKFUNCS tokens
//!   - QuickFunc names      → declaration + all call sites (any section)
//!   - Enum type names      → declaration + all EnumAccess tokens with that name
//!   - Enum field access    → all EnumAccess tokens for that exact enum.field
//!   - Import aliases       → declaration + all identifier uses in any section
//!   - Data variables       → all matching identifiers in @DATA

use std::panic;

use tower_lsp::lsp_types::{DocumentHighlight, DocumentHighlightKind, Position, Range};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;

use crate::document::Document;
use crate::features::hover::token_and_index_at;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<Vec<DocumentHighlight>> {
    let result =
        panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc, pos)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("document_highlights panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>, pos: Position) -> Option<Vec<DocumentHighlight>> {
    let doc = doc?;
    let (token, _index) = token_and_index_at(&doc.tokens, pos)?;

    let highlights = match &token.token_type {
        // Exact enum.field access — highlight all identical enum accesses.
        TokenType::EnumAccess { enum_name, value } => {
            let en = enum_name.clone();
            let v  = value.clone();
            highlights_for_enum_access(&doc.tokens, &en, &v)
        }

        // Plain identifier — resolve what kind of symbol it is then highlight all.
        TokenType::Identifier(name) => {
            let name = name.clone();
            highlights_for_identifier(doc, &name, token.section)
        }

        _ => return None,
    };

    if highlights.is_empty() { None } else { Some(highlights) }
}

// ── EnumAccess highlights ─────────────────────────────────────────────────────

fn highlights_for_enum_access(
    tokens:    &[Token],
    enum_name: &str,
    value:     &str,
) -> Vec<DocumentHighlight> {
    tokens
        .iter()
        .filter_map(|t| {
            if let TokenType::EnumAccess { enum_name: en, value: v } = &t.token_type {
                if en.as_str() == enum_name && v.as_str() == value {
                    let len = en.len() + 1 + v.len();
                    return Some(make_highlight(t, len, DocumentHighlightKind::READ));
                }
            }
            None
        })
        .collect()
}

// ── Identifier highlights ─────────────────────────────────────────────────────

fn highlights_for_identifier(
    doc:    &Document,
    name:   &str,
    origin: SectionId,
) -> Vec<DocumentHighlight> {
    let ctx = IdentifierContext::resolve(doc, name, origin);
    let mut out = Vec::new();

    for (idx, token) in doc.tokens.iter().enumerate() {
        match &token.token_type {
            TokenType::Identifier(tok_name) if tok_name.as_str() == name => {
                // Parameters are scoped to @QUICKFUNCS only.
                if ctx.is_param && token.section != SectionId::QuickFuncs {
                    continue;
                }

                let kind = declaration_or_read(token, idx, &ctx, &doc.tokens);
                out.push(make_highlight(token, name.len(), kind));
            }

            // When the cursor is on an enum TYPE name, also highlight all EnumAccess
            // tokens whose enum_name matches — so "AIType" lights up "AIType.BOSS" etc.
            TokenType::EnumAccess { enum_name, .. } if ctx.is_enum && enum_name.as_str() == name => {
                // Highlight just the enum_name portion of the access token.
                out.push(make_highlight(token, name.len(), DocumentHighlightKind::READ));
            }

            _ => {}
        }
    }

    out
}

// ── Symbol context ────────────────────────────────────────────────────────────

struct IdentifierContext {
    is_quickfunc: bool,
    is_enum:      bool,
    is_import:    bool,
    is_param:     bool,
    // param belonging to which function (for tighter scoping if needed later)
    #[allow(dead_code)]
    param_func:   Option<String>,
}

impl IdentifierContext {
    fn resolve(doc: &Document, name: &str, origin: SectionId) -> Self {
        let ast = doc.ast.as_ref();

        let is_quickfunc = ast
            .and_then(|a| a.quick_functions.as_ref())
            .map(|qf| qf.functions.iter().any(|f| f.name == name))
            .unwrap_or(false);

        let is_enum = ast
            .and_then(|a| a.enums.as_ref())
            .map(|e| e.enums.iter().any(|d| d.name == name))
            .unwrap_or(false);

        let is_import = ast
            .and_then(|a| a.imports.as_ref())
            .map(|i| i.imports.iter().any(|imp| imp.alias == name))
            .unwrap_or(false);

        let mut is_param   = false;
        let mut param_func = None;

        if origin == SectionId::QuickFuncs {
            if let Some(qf) = ast.and_then(|a| a.quick_functions.as_ref()) {
                for func in &qf.functions {
                    if func.parameters.iter().any(|p| p.name == name) {
                        is_param   = true;
                        param_func = Some(func.name.clone());
                        break;
                    }
                }
            }
        }

        IdentifierContext { is_quickfunc, is_enum, is_import, is_param, param_func }
    }
}

/// Decide whether a token occurrence is a WRITE (declaration) or READ (use).
fn declaration_or_read(
    token:  &Token,
    index:  usize,
    ctx:    &IdentifierContext,
    tokens: &[Token],
) -> DocumentHighlightKind {
    // QuickFunc declaration: `~` immediately precedes the identifier.
    if ctx.is_quickfunc && token.section == SectionId::QuickFuncs {
        if index > 0 {
            if let TokenType::Symbol('~') = &tokens[index - 1].token_type {
                return DocumentHighlightKind::WRITE;
            }
        }
    }

    // Enum type declaration: identifier is followed by `{` in @ENUMS.
    if ctx.is_enum && token.section == SectionId::Enums {
        if let Some(next) = tokens.get(index + 1) {
            if matches!(next.token_type, TokenType::Symbol('{')) {
                return DocumentHighlightKind::WRITE;
            }
        }
    }

    // Import alias declaration: identifier followed by `from` or `from_cloud`.
    if ctx.is_import && token.section == SectionId::Imports {
        if let Some(next) = tokens.get(index + 1) {
            if matches!(&next.token_type,
                TokenType::Keyword(k) if *k == "from" || *k == "from_cloud")
            {
                return DocumentHighlightKind::WRITE;
            }
        }
    }

    // Parameter declaration: identifier followed by optional `<type>` or `,` / `)` in
    // @QUICKFUNCS and the cursor is at the function parameter position.
    if ctx.is_param && token.section == SectionId::QuickFuncs {
        // A parameter declaration sits right after `(` or `,` inside the param list.
        if index > 0 {
            let prev = &tokens[index - 1].token_type;
            if matches!(prev, TokenType::Symbol('(') | TokenType::Symbol(',')) {
                return DocumentHighlightKind::WRITE;
            }
        }
    }

    DocumentHighlightKind::READ
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn make_highlight(token: &Token, len: usize, kind: DocumentHighlightKind) -> DocumentHighlight {
    let line = token.line.saturating_sub(1) as u32;
    let col  = token.column.saturating_sub(1) as u32;
    DocumentHighlight {
        range: Range::new(
            Position::new(line, col),
            Position::new(line, col + len as u32),
        ),
        kind: Some(kind),
    }
               }
