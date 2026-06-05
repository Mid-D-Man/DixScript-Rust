// mdix-lsp/src/features/call_hierarchy.rs
//! Call hierarchy provider for QuickFuncs.
//!
//! prepare          — identify the QuickFunc under the cursor
//! incoming_calls   — who calls this QuickFunc (other QFs + @DATA)
//! outgoing_calls   — which QuickFuncs this function calls internally
//!
//! All three use the token stream for call-site positions (reliable) and
//! the AST for function metadata (name, return type, line range).

use std::collections::HashMap;
use std::panic;

use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams,
    CallHierarchyItem, CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams,
    Position, Range, SymbolKind, Url,
};
use dixscript::Compiler::AST::QuickFuncsSection;
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::Core::Tokenizer::Token;

use crate::document::Document;
use crate::features::hover::token_and_index_at;

// ── prepare ───────────────────────────────────────────────────────────────────

pub fn prepare(
    doc: Option<&Document>,
    pos: Position,
) -> Option<Vec<CallHierarchyItem>> {
    panic::catch_unwind(panic::AssertUnwindSafe(|| prepare_inner(doc, pos)))
        .ok()
        .flatten()
}

fn prepare_inner(doc: Option<&Document>, pos: Position) -> Option<Vec<CallHierarchyItem>> {
    let doc = doc?;
    let (token, _) = token_and_index_at(&doc.tokens, pos)?;

    let name = match &token.token_type {
        TokenType::Identifier(n) => n.clone(),
        _ => return None,
    };

    let qf   = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let func = qf.functions.iter().find(|f| f.name == name)?;

    let params_str: Vec<String> = func.parameters.iter()
        .map(|p| {
            let t = p.data_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
            format!("{}{}", p.name, t)
        })
        .collect();
    let ret = func.return_type.map(|t| format!("<{}>", t)).unwrap_or_default();

    let line    = func.position.line.saturating_sub(1) as u32;
    let col     = func.position.column.saturating_sub(1) as u32;
    let end_col = col + 1 + name.len() as u32;

    Some(vec![CallHierarchyItem {
        name:            format!("~{}", name),
        kind:            SymbolKind::FUNCTION,
        tags:            None,
        detail:          Some(format!("{}({})", ret, params_str.join(", "))),
        uri:             doc.uri.clone(),
        range:           Range::new(Position::new(line, col), Position::new(line, end_col)),
        selection_range: Range::new(Position::new(line, col + 1), Position::new(line, end_col)),
        data:            None,
    }])
}

// ── incoming_calls ────────────────────────────────────────────────────────────

pub fn incoming_calls(
    doc:    Option<&Document>,
    params: &CallHierarchyIncomingCallsParams,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    panic::catch_unwind(panic::AssertUnwindSafe(|| incoming_inner(doc, params)))
        .ok()
        .flatten()
}

fn incoming_inner(
    doc:    Option<&Document>,
    params: &CallHierarchyIncomingCallsParams,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    let doc       = doc?;
    let func_name = strip_func_prefix(&params.item.name);
    let qf        = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref());

    // Token scan: every place func_name( appears
    let call_sites: Vec<(usize, SectionId, Range)> = doc.tokens
        .iter()
        .enumerate()
        .filter(|(i, t)| {
            matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == func_name)
            && doc.tokens.get(i + 1)
                .map(|nx| matches!(nx.token_type, TokenType::Symbol('(')))
                .unwrap_or(false)
        })
        .map(|(_, t)| {
            let line = t.line.saturating_sub(1) as u32;
            let col  = t.column.saturating_sub(1) as u32;
            (
                t.line,
                t.section,
                Range::new(
                    Position::new(line, col),
                    Position::new(line, col + func_name.len() as u32),
                ),
            )
        })
        .collect();

    if call_sites.is_empty() {
        return None;
    }

    // Group by enclosing caller (QuickFunc or @DATA)
    let mut by_caller: HashMap<String, Vec<Range>> = HashMap::new();
    for (line_1based, section, range) in call_sites {
        let caller = if section == SectionId::QuickFuncs {
            qf.and_then(|q| find_enclosing_qf(line_1based, q))
                .unwrap_or_else(|| "@DATA".to_string())
        } else {
            "@DATA".to_string()
        };

        // Skip self-calls (DixScript doesn't support recursion anyway)
        if caller != func_name {
            by_caller.entry(caller).or_default().push(range);
        }
    }

    if by_caller.is_empty() {
        return None;
    }

    let mut result: Vec<CallHierarchyIncomingCall> = Vec::new();
    for (caller_name, ranges) in by_caller {
        let item = if caller_name == "@DATA" {
            data_item(&doc.uri)
        } else if let Some(qf) = qf {
            match qf_item(&caller_name, qf, &doc.uri) {
                Some(i) => i,
                None    => continue,
            }
        } else {
            continue;
        };
        result.push(CallHierarchyIncomingCall { from: item, from_ranges: ranges });
    }

    if result.is_empty() { None } else { Some(result) }
}

// ── outgoing_calls ────────────────────────────────────────────────────────────

pub fn outgoing_calls(
    doc:    Option<&Document>,
    params: &CallHierarchyOutgoingCallsParams,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    panic::catch_unwind(panic::AssertUnwindSafe(|| outgoing_inner(doc, params)))
        .ok()
        .flatten()
}

fn outgoing_inner(
    doc:    Option<&Document>,
    params: &CallHierarchyOutgoingCallsParams,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    let doc       = doc?;
    let func_name = strip_func_prefix(&params.item.name);
    let qf        = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let func      = qf.functions.iter().find(|f| f.name == func_name)?;

    // All known QuickFunc names (so we only match local calls, not built-ins)
    let known: std::collections::HashSet<&str> =
        qf.functions.iter().map(|f| f.name.as_str()).collect();

    // Line range of the function body (from its start to the next function's start)
    let start_line = func.position.line;
    let end_line   = qf.functions.iter()
        .filter(|f| f.position.is_valid() && f.position.line > start_line)
        .map(|f| f.position.line)
        .min()
        .unwrap_or(usize::MAX);

    // Token scan within the function's line range
    let mut calls: HashMap<String, Vec<Range>> = HashMap::new();

    for (i, t) in doc.tokens.iter().enumerate() {
        if t.line < start_line           { continue; }
        if t.line >= end_line            { break; }
        if t.section != SectionId::QuickFuncs { continue; }

        if let TokenType::Identifier(callee) = &t.token_type {
            let is_call = doc.tokens.get(i + 1)
                .map(|nx| matches!(nx.token_type, TokenType::Symbol('(')))
                .unwrap_or(false);

            if is_call
                && callee.as_str() != func_name          // not self
                && known.contains(callee.as_str())        // local QF only
            {
                let line = t.line.saturating_sub(1) as u32;
                let col  = t.column.saturating_sub(1) as u32;
                calls.entry(callee.clone()).or_default().push(Range::new(
                    Position::new(line, col),
                    Position::new(line, col + callee.len() as u32),
                ));
            }
        }
    }

    if calls.is_empty() {
        return None;
    }

    let result: Vec<CallHierarchyOutgoingCall> = calls
        .into_iter()
        .filter_map(|(callee_name, from_ranges)| {
            let item = qf_item(&callee_name, qf, &doc.uri)?;
            Some(CallHierarchyOutgoingCall { to: item, from_ranges })
        })
        .collect();

    if result.is_empty() { None } else { Some(result) }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Strip leading `~` and any `<type>(params)` suffix from a CallHierarchyItem name.
fn strip_func_prefix(display: &str) -> &str {
    display.trim_start_matches('~')
        .split(|c| c == '<' || c == '(')
        .next()
        .unwrap_or(display)
        .trim()
}

/// The QuickFunc whose definition line is the largest value ≤ `target_line`.
fn find_enclosing_qf(target_line: usize, qf: &QuickFuncsSection) -> Option<String> {
    qf.functions.iter()
        .filter(|f| f.position.is_valid() && f.position.line <= target_line)
        .max_by_key(|f| f.position.line)
        .map(|f| f.name.clone())
}

fn qf_item(name: &str, qf: &QuickFuncsSection, uri: &Url) -> Option<CallHierarchyItem> {
    let func    = qf.functions.iter().find(|f| f.name == name)?;
    let line    = func.position.line.saturating_sub(1) as u32;
    let col     = func.position.column.saturating_sub(1) as u32;
    let end_col = col + 1 + name.len() as u32;
    let ret     = func.return_type.map(|t| format!("<{}>", t)).unwrap_or_default();

    let params_str: Vec<String> = func.parameters.iter()
        .map(|p| p.name.clone())
        .collect();

    Some(CallHierarchyItem {
        name:            format!("~{}", name),
        kind:            SymbolKind::FUNCTION,
        tags:            None,
        detail:          Some(format!("{}({})", ret, params_str.join(", "))),
        uri:             uri.clone(),
        range:           Range::new(Position::new(line, col), Position::new(line, end_col)),
        selection_range: Range::new(Position::new(line, col + 1), Position::new(line, end_col)),
        data:            None,
    })
}

fn data_item(uri: &Url) -> CallHierarchyItem {
    CallHierarchyItem {
        name:            "@DATA".to_string(),
        kind:            SymbolKind::MODULE,
        tags:            None,
        detail:          Some("Data section".to_string()),
        uri:             uri.clone(),
        range:           Range::new(Position::new(0, 0), Position::new(0, 5)),
        selection_range: Range::new(Position::new(0, 0), Position::new(0, 5)),
        data:            None,
    }
}
