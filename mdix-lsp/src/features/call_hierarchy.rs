// mdix-lsp/src/features/call_hierarchy.rs
//! Call hierarchy provider for QuickFuncs.
//!
//! prepare        — identify the QuickFunc under cursor (works on definition
//!                  `~funcName`, call sites `funcName(...)`, AND the `~` tilde)
//! incoming_calls — who calls this QuickFunc (other QFs + @DATA)
//! outgoing_calls — which QuickFuncs this function internally calls
//!
//! All three use the token stream for call-site positions (reliable) and
//! the AST for function metadata.

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

    // Find the QuickFunc name at the cursor.
    //
    // We accept:
    //   1. Cursor on an Identifier token whose name matches a known QF
    //   2. Cursor on the `~` Symbol — look at the following Identifier
    //   3. Cursor on a QuickFunc CALL SITE (Identifier followed by `(`)
    //      — even if located in @DATA or another @QUICKFUNCS body

    let (token, index) = token_and_index_at(&doc.tokens, pos)?;

    let func_name: String = match &token.token_type {
        // Case 1 & 3: plain identifier
        TokenType::Identifier(n) => n.clone(),

        // Case 2: tilde prefix — grab the next identifier
        TokenType::Symbol('~') => {
            doc.tokens.get(index + 1).and_then(|next| {
                if let TokenType::Identifier(n) = &next.token_type {
                    Some(n.clone())
                } else {
                    None
                }
            })?
        }

        _ => return None,
    };

    let qf   = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let func = qf.functions.iter().find(|f| f.name == func_name)?;

    Some(vec![build_qf_item(&func_name, func, &doc.uri)])
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
    let func_name = strip_tilde_and_suffix(&params.item.name);
    let qf        = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref());

    // Scan every token for `func_name (` or `func_name <type> (`
    // Strategy: find Identifier(func_name) where, skipping forward past
    // any optional `<...>` annotation, we eventually hit Symbol('(').
    let call_sites: Vec<(usize, SectionId, Range)> = doc.tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            if !matches!(&t.token_type, TokenType::Identifier(n) if n.as_str() == func_name) {
                return None;
            }
            // Check if this is a call site: scan forward up to 8 tokens for '('
            // (handles `funcName<type>(` and `funcName(`)
            let is_call = is_call_site_forward(&doc.tokens, i);
            if !is_call { return None; }

            let line = t.line.saturating_sub(1) as u32;
            let col  = t.column.saturating_sub(1) as u32;
            Some((
                t.line,
                t.section,
                Range::new(
                    Position::new(line, col),
                    Position::new(line, col + func_name.len() as u32),
                ),
            ))
        })
        .collect();

    if call_sites.is_empty() { return None; }

    // Group by caller name
    let mut by_caller: HashMap<String, Vec<Range>> = HashMap::new();

    for (line_1based, section, range) in call_sites {
        let caller = caller_name(line_1based, section, func_name, qf);
        if caller != func_name {
            by_caller.entry(caller).or_default().push(range);
        }
    }

    if by_caller.is_empty() { return None; }

    let mut result: Vec<CallHierarchyIncomingCall> = Vec::new();

    for (caller_name, ranges) in by_caller {
        let item = if caller_name == "@DATA" {
            data_item(&doc.uri)
        } else if let Some(q) = qf {
            match qf_item_by_name(&caller_name, q, &doc.uri) {
                Some(i) => i,
                None    => continue,
            }
        } else {
            continue
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
    let func_name = strip_tilde_and_suffix(&params.item.name);
    let qf        = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let func      = qf.functions.iter().find(|f| f.name == func_name)?;

    // All known QuickFunc names (local calls only — not builtins)
    let known: std::collections::HashSet<&str> =
        qf.functions.iter().map(|f| f.name.as_str()).collect();

    // Body line range of this function
    let start_line = func.position.line;
    let end_line   = qf.functions.iter()
        .filter(|f| f.position.is_valid() && f.position.line > start_line)
        .map(|f| f.position.line)
        .min()
        .unwrap_or(usize::MAX);

    // Collect outgoing call sites within the function body
    let mut calls: HashMap<String, Vec<Range>> = HashMap::new();

    for (i, t) in doc.tokens.iter().enumerate() {
        if t.line < start_line { continue; }
        if t.line >= end_line  { break; }
        if t.section != SectionId::QuickFuncs { continue; }

        if let TokenType::Identifier(callee) = &t.token_type {
            let is_call = is_call_site_forward(&doc.tokens, i);
            if is_call
                && callee.as_str() != func_name       // not self
                && known.contains(callee.as_str())    // local QF only
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

    if calls.is_empty() { return None; }

    let result: Vec<CallHierarchyOutgoingCall> = calls
        .into_iter()
        .filter_map(|(callee_name, from_ranges)| {
            let item = qf_item_by_name(&callee_name, qf, &doc.uri)?;
            Some(CallHierarchyOutgoingCall { to: item, from_ranges })
        })
        .collect();

    if result.is_empty() { None } else { Some(result) }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Check whether token at `idx` is a function call:
/// scans forward (up to 8 tokens) skipping `<type>` annotations and scope
/// declarations until we hit `(`.  Returns false if we hit something else first.
fn is_call_site_forward(tokens: &[Token], idx: usize) -> bool {
    let mut j = idx + 1;
    let mut angle_depth = 0i32;

    while j < tokens.len() && (j - idx) <= 8 {
        match &tokens[j].token_type {
            // Type annotation open/close
            TokenType::Symbol('<')         => { angle_depth += 1; j += 1; }
            TokenType::Symbol('>')         => { angle_depth -= 1; j += 1; }
            TokenType::BitwiseOp(op) if *op == ">>" => { angle_depth -= 2; j += 1; }

            // Inside annotation — skip identifiers, data types, commas
            TokenType::DataType(_) | TokenType::Identifier(_) | TokenType::Symbol(',')
                if angle_depth > 0 => { j += 1; }

            // Scope declaration ` => global`
            TokenType::Arrow if angle_depth == 0 => {
                // Skip the scope identifier(s) until we reach `(`
                j += 1;
                while j < tokens.len()
                    && !matches!(&tokens[j].token_type, TokenType::Symbol('('))
                {
                    j += 1;
                }
                if j < tokens.len() && matches!(&tokens[j].token_type, TokenType::Symbol('(')) {
                    return true;
                }
                return false;
            }

            // Found the open paren — this is a call site
            TokenType::Symbol('(') if angle_depth == 0 => return true,

            // Anything else at depth 0 means this isn't a call
            _ if angle_depth == 0 => return false,

            _ => { j += 1; }
        }
    }
    false
}

/// Determine the "caller" name for a call site at `line_1based` in `section`.
///
/// - In @QUICKFUNCS: the enclosing function (the one that starts at or before
///   this line, and is NOT the function being called — that would be a
///   definition hit, which the caller already filters with `caller != func_name`)
/// - In @DATA or anywhere else: "@DATA"
fn caller_name(
    line_1based: usize,
    section:     SectionId,
    _func_name:  &str,
    qf:          Option<&QuickFuncsSection>,
) -> String {
    if section == SectionId::QuickFuncs {
        if let Some(q) = qf {
            if let Some(name) = find_enclosing_qf(line_1based, q) {
                return name;
            }
        }
    }
    "@DATA".to_string()
}

/// The QuickFunc whose definition line is the largest value ≤ `target_line`.
fn find_enclosing_qf(target_line: usize, qf: &QuickFuncsSection) -> Option<String> {
    qf.functions.iter()
        .filter(|f| f.position.is_valid() && f.position.line <= target_line)
        .max_by_key(|f| f.position.line)
        .map(|f| f.name.clone())
}

/// Strip the `~` prefix and any `<retType>(params)` suffix from an item name.
/// Example: `~createEnemy<object>(name, hp)` → `createEnemy`
fn strip_tilde_and_suffix(display: &str) -> &str {
    let s = display.trim_start_matches('~');
    // Take up to first '<' or '(' whichever comes first
    let end = s.find(|c| c == '<' || c == '(').unwrap_or(s.len());
    s[..end].trim()
}

// ── Item constructors ─────────────────────────────────────────────────────────

/// Build a `CallHierarchyItem` for a known QuickFunction.
fn build_qf_item(
    name: &str,
    func: &dixscript::Compiler::AST::QuickFunction,
    uri:  &Url,
) -> CallHierarchyItem {
    let line    = func.position.line.saturating_sub(1) as u32;
    let col     = func.position.column.saturating_sub(1) as u32;
    let end_col = col + 1 + name.len() as u32; // col+1 skips the '~'

    let ret = func.return_type.map(|t| format!("<{}>", t)).unwrap_or_default();
    let params: Vec<String> = func.parameters.iter()
        .map(|p| {
            let t = p.data_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
            format!("{}{}", p.name, t)
        })
        .collect();

    CallHierarchyItem {
        name:            format!("~{}", name),
        kind:            SymbolKind::FUNCTION,
        tags:            None,
        detail:          Some(format!("~{}{}({})", name, ret, params.join(", "))),
        uri:             uri.clone(),
        range:           Range::new(Position::new(line, col), Position::new(line, end_col)),
        selection_range: Range::new(Position::new(line, col + 1), Position::new(line, end_col)),
        data:            None,
    }
}

/// Look up a QuickFunc by name and build its `CallHierarchyItem`.
fn qf_item_by_name(
    name: &str,
    qf:   &QuickFuncsSection,
    uri:  &Url,
) -> Option<CallHierarchyItem> {
    let func = qf.functions.iter().find(|f| f.name == name)?;
    Some(build_qf_item(name, func, uri))
}

fn data_item(uri: &Url) -> CallHierarchyItem {
    CallHierarchyItem {
        name:            "@DATA".to_string(),
        kind:            SymbolKind::MODULE,
        tags:            None,
        detail:          Some("Data section call site".to_string()),
        uri:             uri.clone(),
        range:           Range::new(Position::new(0, 0), Position::new(0, 5)),
        selection_range: Range::new(Position::new(0, 0), Position::new(0, 5)),
        data:            None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_tilde_and_suffix_variants() {
        assert_eq!(strip_tilde_and_suffix("~myFunc"),                  "myFunc");
        assert_eq!(strip_tilde_and_suffix("~myFunc<object>(a, b)"),    "myFunc");
        assert_eq!(strip_tilde_and_suffix("~myFunc(a, b)"),            "myFunc");
        assert_eq!(strip_tilde_and_suffix("myFunc"),                   "myFunc");
        assert_eq!(strip_tilde_and_suffix("~build<int>(x<int>)"),      "build");
    }

    #[test]
    fn is_call_site_forward_with_type_annotation() {
        // Simulate token sequence: Identifier("f") Symbol('<') DataType("object") Symbol('>') Symbol('(')
        use dixscript::Compiler::Core::Tokenizer::token::SectionId;
        let make = |tt: TokenType| Token::new(tt, 1, 1, SectionId::None);

        let tokens = vec![
            make(TokenType::Identifier("f".to_string())),    // 0
            make(TokenType::Symbol('<')),                      // 1
            make(TokenType::DataType("object")),               // 2
            make(TokenType::Symbol('>')),                      // 3
            make(TokenType::Symbol('(')),                      // 4
        ];
        assert!(is_call_site_forward(&tokens, 0));
    }

    #[test]
    fn is_call_site_forward_direct() {
        use dixscript::Compiler::Core::Tokenizer::token::SectionId;
        let make = |tt: TokenType| Token::new(tt, 1, 1, SectionId::None);

        let tokens = vec![
            make(TokenType::Identifier("f".to_string())),
            make(TokenType::Symbol('(')),
        ];
        assert!(is_call_site_forward(&tokens, 0));
    }

    #[test]
    fn is_call_site_forward_not_a_call() {
        use dixscript::Compiler::Core::Tokenizer::token::SectionId;
        let make = |tt: TokenType| Token::new(tt, 1, 1, SectionId::None);

        let tokens = vec![
            make(TokenType::Identifier("x".to_string())),
            make(TokenType::Symbol('=')),
            make(TokenType::Integer(5)),
        ];
        assert!(!is_call_site_forward(&tokens, 0));
    }
    }
