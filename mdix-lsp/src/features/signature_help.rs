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
use dixscript::Compiler::AST::{DataType, ElemType, QuickFuncParam};

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

    // Look up the QuickFunc in the AST.
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

    let full_label = format!(
        "~{}{}({})",
        func_name, ret_str, param_labels.join(", ")
    );

    // Per-parameter information with type documentation.
    let parameters: Vec<ParameterInformation> = func.parameters.iter().map(|p| {
        let type_str  = p.data_type.map(|t| format!("<{}>", t)).unwrap_or_default();
        let lbl       = format!("{}{}", p.name, type_str);
        let doc_value = build_param_doc(p);

        ParameterInformation {
            label: ParameterLabel::Simple(lbl),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: doc_value,
            })),
        }
    }).collect();

    // Function-level documentation.
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

    // Clamp active_param to the actual parameter count.
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

// ── Call-context detector ─────────────────────────────────────────────────────

/// Scan backwards through the token stream from the cursor to find:
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

    let open      = open_idx?;
    let func_name = scan_backwards_for_func_name(tokens, open)?;
    Some((func_name, active_param))
}

/// Walk backwards from `open_paren_idx` to extract the function name,
/// skipping any `<type>` annotation between the name and `(`.
///
/// Handles nested typed-collection annotations like `<array<int>>` where
/// the lexer may emit `>>` as a single `BitwiseOp` token.
///
/// `BitwiseOp` holds `&'static str`; the match arm receives `&&'static str`,
/// so we dereference with `*op` for a plain `&'static str == &str` comparison
/// (stable, no nightly features needed).
fn scan_backwards_for_func_name(tokens: &[Token], open_paren_idx: usize) -> Option<String> {
    let mut i           = open_paren_idx.checked_sub(1)?;
    let mut angle_depth = 0i32;

    loop {
        match &tokens[i].token_type {
            // Plain closing angle `>`
            TokenType::Symbol('>') => {
                angle_depth += 1;
            }

            // `>>` emitted as a single BitwiseOp token — counts as two closing angles.
            // BitwiseOp holds &&'static str; *op gives &'static str for comparison.
            TokenType::BitwiseOp(op) if *op == ">>" => {
                angle_depth += 2;
            }

            // Opening angle `<`
            TokenType::Symbol('<') => {
                angle_depth -= 1;
                if angle_depth < 0 { angle_depth = 0; }
            }

            // Keywords / DataType tokens / commas inside `<…>` annotations — skip
            TokenType::Keyword(_)
            | TokenType::Symbol(',')
                if angle_depth > 0 =>
            {
                // inside annotation, keep scanning
            }

            // Identifier outside any annotation — this is the function name
            TokenType::Identifier(name) if angle_depth == 0 => {
                return Some(name.clone());
            }

            // Anything else outside angle brackets means we have left the call site
            _ if angle_depth == 0 => return None,

            _ => {}
        }

        if i == 0 { break; }
        i -= 1;
    }
    None
}

// ── Parameter documentation ───────────────────────────────────────────────────

fn build_param_doc(param: &QuickFuncParam) -> String {
    let type_detail = describe_data_type(param.data_type);

    let default_note = if param.default_value.is_some() {
        "\n\n*Has a default value — this parameter is optional.*"
    } else {
        ""
    };

    format!("**`{}`** — {}{}", param.name, type_detail, default_note)
}

/// Human-readable description for any `Option<DataType>`, including the
/// typed-collection variants `TypedArray` and `TypedTuple`.
fn describe_data_type(dt: Option<DataType>) -> String {
    match dt {
        None                              => "any type (no annotation)".to_string(),
        Some(DataType::Int)               => "32-bit signed integer".to_string(),
        Some(DataType::Long)              => "64-bit signed integer".to_string(),
        Some(DataType::Float)             => "32-bit float (requires `f` suffix on literals)".to_string(),
        Some(DataType::Double)            => "64-bit double (IEEE 754 f64)".to_string(),
        Some(DataType::String)            => "UTF-8 string".to_string(),
        Some(DataType::Bool)              => "boolean — `true` or `false`".to_string(),
        Some(DataType::Array)             => "ordered collection (untyped)".to_string(),
        Some(DataType::Tuple)             => "mixed-type collection (max 6 elements, untyped)".to_string(),
        Some(DataType::Object)            => "key-value map `{ key = value }`".to_string(),
        Some(DataType::Hex)               => "hex color or integer literal".to_string(),
        Some(DataType::Blob)              => "base64-encoded binary `b:(...)`".to_string(),
        Some(DataType::Regex)             => "compiled regular expression `r:(...)`".to_string(),
        Some(DataType::Date)              => "ISO 8601 date `YYYY-MM-DD`".to_string(),
        Some(DataType::Timestamp)         => "ISO 8601 timestamp".to_string(),
        Some(DataType::Enum)              => "enum value from @ENUMS".to_string(),
        Some(DataType::Any)               => "any type".to_string(),
        Some(DataType::Function)          => "callable".to_string(),
        Some(DataType::Range)             => "numeric range".to_string(),

        // ── Typed collections ────────────────────────────────────────────────
        Some(DataType::TypedArray(elem))  => {
            format!(
                "typed array — every element must be `<{}>` (annotation: `<array<{}>>`)",
                elem, elem
            )
        }
        Some(DataType::TypedTuple(slots)) => {
            let types: Vec<String> = slots
                .iter()
                .filter_map(|&s| s)
                .map(|e| format!("`{}`", e))
                .collect();
            if types.is_empty() {
                "typed tuple (max 6 elements)".to_string()
            } else {
                let inner: String = slots
                    .iter()
                    .filter_map(|&s| s)
                    .map(|e| format!("{}", e))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "typed tuple — element types: {} (annotation: `<tuple<{}>>`)",
                    types.join(", "), inner
                )
            }
        }
    }
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

    #[test]
    fn typed_array_param_description() {
        use dixscript::Compiler::AST::{DataType, ElemType, Position as AstPos};
        use dixscript::Compiler::AST::QuickFuncParam;
        let param = QuickFuncParam {
            name:          "items".to_string(),
            data_type:     Some(DataType::TypedArray(ElemType::Int)),
            default_value: None,
            position:      AstPos::UNKNOWN,
        };
        let doc = build_param_doc(&param);
        assert!(doc.contains("array"), "should mention array: {}", doc);
        assert!(doc.contains("int"),   "should mention int: {}", doc);
    }

    #[test]
    fn typed_tuple_param_description() {
        use dixscript::Compiler::AST::{DataType, ElemType, Position as AstPos};
        use dixscript::Compiler::AST::QuickFuncParam;
        let mut slots = [None; 6];
        slots[0] = Some(ElemType::Int);
        slots[1] = Some(ElemType::String);
        let param = QuickFuncParam {
            name:          "pair".to_string(),
            data_type:     Some(DataType::TypedTuple(slots)),
            default_value: None,
            position:      AstPos::UNKNOWN,
        };
        let doc = build_param_doc(&param);
        assert!(doc.contains("tuple"),  "should mention tuple: {}", doc);
        assert!(doc.contains("int"),    "should mention int: {}", doc);
        assert!(doc.contains("string"), "should mention string: {}", doc);
    }
}
