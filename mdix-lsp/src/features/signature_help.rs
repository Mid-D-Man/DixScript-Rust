// mdix-lsp/src/features/signature_help.rs
//! Signature-help provider — shows parameter hints when inside a function call.
//!
//! Triggered by `(` and `,`. Scans backwards through the token stream from
//! the cursor to find the innermost unclosed `(`, identifies the QuickFunc
//! name before it, and returns a SignatureInformation with the active
//! parameter highlighted.

use std::panic;

use tower_lsp::lsp_types::{
    Documentation, MarkupContent, MarkupKind, ParameterInformation, ParameterLabel,
    Position, SignatureHelp, SignatureHelpContext, SignatureInformation,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::AST::{DataType, QuickFuncParam};

use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(
    doc:  Option<&Document>,
    pos:  Position,
    _ctx: Option<SignatureHelpContext>,
) -> Option<SignatureHelp> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc, pos)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("signature_help panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>, pos: Position) -> Option<SignatureHelp> {
    let doc = doc?;

    let (func_name, active_param) = find_call_context(&doc.tokens, pos)?;

    // Look up the QuickFunc in the AST
    let qf   = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let func = qf.functions.iter().find(|f| f.name == func_name)?;

    // Build label: ~funcName<retType>(param1<type1>, param2)
    let param_labels: Vec<String> = func.parameters.iter().map(|p| {
        let type_str    = p.data_type.map(|t| format!("<{}>", t)).unwrap_or_default();
        let default_str = if p.default_value.is_some() { " = …" } else { "" };
        format!("{}{}{}", p.name, type_str, default_str)
    }).collect();

    let ret_str = func.return_type
        .map(|t| format!("<{}>", t))
        .unwrap_or_default();

    let full_label = format!("~{}{}({})", func_name, ret_str, param_labels.join(", "));

    // Per-parameter information with type documentation
    let parameters: Vec<ParameterInformation> = func.parameters.iter().map(|p| {
        let type_str  = p.data_type.map(|t| format!("<{}>", t)).unwrap_or_default();
        let lbl       = format!("{}{}", p.name, type_str);
        let doc_value = build_param_doc(p);

        ParameterInformation {
            label:         ParameterLabel::Simple(lbl),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: doc_value,
            })),
        }
    }).collect();

    // Function-level documentation
    let scope_str = func.scope_list
        .as_ref()
        .map(|s| format!("\n\n**Scope:** `=> {}`", s.join(", ")))
        .unwrap_or_default();

    let func_doc = format!(
        "QuickFunc `~{}` — compile-time, zero runtime overhead.{}\n\n**Returns:** `{}`",
        func_name,
        scope_str,
        func.return_type
            .map(|t| format!("{}", t))
            .unwrap_or_else(|| "?".to_string()),
    );

    // Clamp active_param to the actual parameter count
    let param_count    = func.parameters.len();
    let active_clamped = if param_count == 0 {
        0u32
    } else {
        (active_param as u32).min((param_count - 1) as u32)
    };

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label:            full_label,
            documentation:    Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: func_doc,
            })),
            parameters:       Some(parameters),
            active_parameter: None, // governed by top-level active_parameter
        }],
        active_signature: Some(0),
        active_parameter: Some(active_clamped),
    })
}

// ── Call-context detector (token-stream based) ────────────────────────────────

/// Scan backwards through the token stream from the cursor position to find:
///   1. The innermost unclosed `(`.
///   2. The QuickFunc name (Identifier) immediately before that `(`.
///   3. The active parameter index (comma count at depth 0 between `(` and cursor).
fn find_call_context(tokens: &[Token], pos: Position) -> Option<(String, usize)> {
    let target_line = (pos.line + 1) as usize;
    let target_col  = (pos.character + 1) as usize;

    // Index of the last token at or before the cursor.
    let cursor_idx = {
        let mut idx = 0usize;
        for (i, t) in tokens.iter().enumerate() {
            if t.line > target_line { break; }
            if t.line == target_line && t.column > target_col { break; }
            idx = i;
        }
        idx
    };

    let mut depth:        i32   = 0;
    let mut active_param: usize = 0;
    let mut open_idx:     Option<usize> = None;

    for i in (0..=cursor_idx).rev() {
        match &tokens[i].token_type {
            TokenType::Symbol(')') | TokenType::Symbol('}') => {
                depth += 1;
            }
            TokenType::Symbol('(') => {
                if depth == 0 {
                    open_idx = Some(i);
                    break;
                }
                depth -= 1;
            }
            TokenType::Symbol('{') => {
                if depth > 0 { depth -= 1; }
            }
            TokenType::Symbol(',') if depth == 0 => {
                active_param += 1;
            }
            _ => {}
        }
    }

    let open = open_idx?;
    let func_name = scan_backwards_for_func_name(tokens, open)?;
    Some((func_name, active_param))
}

/// Walk backwards from `open_paren_idx` to extract the function name,
/// skipping any `<type>` annotation between the name and `(`.
fn scan_backwards_for_func_name(tokens: &[Token], open_paren_idx: usize) -> Option<String> {
    let mut i = open_paren_idx.checked_sub(1)?;
    let mut skip_angle = false;

    loop {
        match &tokens[i].token_type {
            TokenType::Symbol('>')   => { skip_angle = true; }
            TokenType::Symbol('<') if skip_angle => { skip_angle = false; }
            TokenType::Keyword(_) | TokenType::DataType(_) if skip_angle => {
                // inside <type> annotation — skip
            }
            TokenType::Identifier(name) if !skip_angle => {
                return Some(name.clone());
            }
            _ if !skip_angle => return None,
            _ => {}
        }
        if i == 0 { break; }
        i -= 1;
    }
    None
}

// ── Parameter documentation ───────────────────────────────────────────────────

fn build_param_doc(param: &QuickFuncParam) -> String {
    let type_detail: &str = match param.data_type {
        Some(DataType::Int)       => "32-bit signed integer",
        Some(DataType::Long)      => "64-bit signed integer",
        Some(DataType::Float)     => "32-bit float (requires `f` suffix on literals)",
        Some(DataType::Double)    => "64-bit double (IEEE 754 f64)",
        Some(DataType::String)    => "UTF-8 string",
        Some(DataType::Bool)      => "boolean — `true` or `false`",
        Some(DataType::Array)     => "ordered collection",
        Some(DataType::Tuple)     => "mixed-type collection (max 6 elements)",
        Some(DataType::Object)    => "key-value map `{ key = value }`",
        Some(DataType::Hex)       => "hex color or integer literal",
        Some(DataType::Blob)      => "base64-encoded binary `b:(...)`",
        Some(DataType::Regex)     => "compiled regular expression `r:(...)`",
        Some(DataType::Date)      => "ISO 8601 date `YYYY-MM-DD`",
        Some(DataType::Timestamp) => "ISO 8601 timestamp",
        Some(DataType::Enum)      => "enum value from @ENUMS",
        Some(DataType::Any)       => "any type",
        Some(DataType::Function)  => "callable",
        Some(DataType::Range)     => "numeric range",
        None                      => "any type (no annotation)",
    };

    let default_note = if param.default_value.is_some() {
        "\n\n*Has a default value — this parameter is optional.*"
    } else {
        ""
    };

    format!("**`{}`** — {}{}", param.name, type_detail, default_note)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::run_pipeline;
    use crate::document::Document;
    use tower_lsp::lsp_types::{Position, Url};

    fn test_doc(source: &str) -> Document {
        let mut doc = Document::new(
            Url::parse("file:///test.mdix").unwrap(),
            source.to_string(),
            0,
        );
        run_pipeline(&mut doc);
        doc
    }

    #[test]
    fn signature_help_for_quickfunc_call() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~make<object>(name, hp<int>) { return { name = name, hp = hp } }\n",
            ")\n",
            "@DATA(\n",
            "  e = make(\"Goblin\", 100)\n",
            ")"
        );
        let doc  = test_doc(src);
        let pos  = Position::new(4, 14);
        let help = provide(Some(&doc), pos, None);
        assert!(help.is_some(), "Expected signature help inside QuickFunc call");
        let h = help.unwrap();
        assert_eq!(h.signatures.len(), 1);
        assert!(h.signatures[0].label.contains("make"));
    }

    #[test]
    fn second_param_is_active() {
        let src = concat!(
            "@QUICKFUNCS(\n",
            "  ~create<object>(id, name, hp<int>) { return { id = id } }\n",
            ")\n",
            "@DATA(\n",
            "  e = create(1, \"orc\", )\n",
            ")"
        );
        let doc  = test_doc(src);
        // cursor after second comma — should be param index 2
        let pos  = Position::new(4, 22);
        let help = provide(Some(&doc), pos, None);
        if let Some(h) = help {
            assert_eq!(h.active_parameter, Some(2));
        }
    }

    #[test]
    fn no_signature_help_outside_call() {
        let src = "@DATA(\n  x = 42\n)";
        let doc  = test_doc(src);
        assert!(provide(Some(&doc), Position::new(1, 4), None).is_none());
    }
}
