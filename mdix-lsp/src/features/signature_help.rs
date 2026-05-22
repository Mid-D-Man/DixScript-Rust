// mdix-lsp/src/features/signature_help.rs
//! Signature help provider.
//!
//! Shows the parameter list of a QuickFunc when the user types `(` after its name
//! or `,` between arguments. Also covers built-in static methods.

use std::panic;

use tower_lsp::lsp_types::{
    ParameterInformation, ParameterLabel, Position, SignatureHelp,
    SignatureHelpContext, SignatureInformation,
};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Builtins::Resolver::static_object_registry;

use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(
    doc: Option<&Document>,
    pos: Position,
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

    // Walk backwards through source to find the active call.
    let (func_name, active_param) = find_active_call(&doc.source, pos)?;

    // Try QuickFuncs first.
    if let Some(sig) = quickfunc_signature(doc, &func_name, active_param) {
        return Some(sig);
    }

    // Try built-in static methods: look for "Object.method" pattern.
    if let Some((obj, method)) = func_name.split_once('.') {
        static_object_registry::initialize_static_registry();
        if let Some(sig) = static_method_signature(obj, method, active_param) {
            return Some(sig);
        }
    }

    None
}

// ── QuickFunc signature ───────────────────────────────────────────────────────

fn quickfunc_signature(
    doc:          &Document,
    name:         &str,
    active_param: u32,
) -> Option<SignatureHelp> {
    let qf    = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let func  = qf.functions.iter().find(|f| f.name == name)?;

    let params: Vec<ParameterInformation> = func
        .parameters
        .iter()
        .map(|p| {
            let label = if let Some(dt) = p.data_type {
                format!("{}<{}>", p.name, dt)
            } else {
                p.name.clone()
            };
            let label_with_default = if p.default_value.is_some() {
                format!("{} = …", label)
            } else {
                label
            };
            ParameterInformation {
                label:         ParameterLabel::Simple(label_with_default),
                documentation: None,
            }
        })
        .collect();

    let ret = func
        .return_type
        .map(|t| format!("<{}>", t))
        .unwrap_or_else(|| "<?>".to_string());

    let param_labels: Vec<String> = func
        .parameters
        .iter()
        .map(|p| {
            if let Some(dt) = p.data_type {
                format!("{}<{}>", p.name, dt)
            } else {
                p.name.clone()
            }
        })
        .collect();

    let label = format!("~{}{}({})", name, ret, param_labels.join(", "));

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: None,
            parameters: Some(params),
            active_parameter: Some(active_param),
        }],
        active_signature: Some(0),
        active_parameter: Some(active_param),
    })
}

// ── Static method signature ───────────────────────────────────────────────────

fn static_method_signature(
    object:       &str,
    method:       &str,
    active_param: u32,
) -> Option<SignatureHelp> {
    let info = static_object_registry::get_method_info(object, method)?;

    let param_count = info.parameter_count.max(0) as usize;
    let params: Vec<ParameterInformation> = (1..=param_count)
        .map(|i| ParameterInformation {
            label:         ParameterLabel::Simple(format!("arg{}", i)),
            documentation: None,
        })
        .collect();

    let param_str: Vec<String> = (1..=param_count)
        .map(|i| format!("arg{}", i))
        .collect();

    let label = format!(
        "{}.{}({}) → <{}>",
        object,
        method,
        param_str.join(", "),
        info.return_type.get_type_name(),
    );

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: Some(tower_lsp::lsp_types::Documentation::String(
                info.description.to_string(),
            )),
            parameters: Some(params),
            active_parameter: Some(active_param),
        }],
        active_signature: Some(0),
        active_parameter: Some(active_param),
    })
}

// ── Source-text scan ──────────────────────────────────────────────────────────

/// Walk backwards through the source line to find which function call the cursor
/// is inside and which parameter position (0-based) it is at.
fn find_active_call(source: &str, pos: Position) -> Option<(String, u32)> {
    let line_text = source.lines().nth(pos.line as usize)?;
    let up_to: &str = &line_text[..(pos.character as usize).min(line_text.len())];

    // Count commas at depth 1 (skipping nested parens) to get active param index.
    let mut depth: i32       = 0;
    let mut active_param: u32 = 0;
    let mut paren_pos: Option<usize> = None;

    for (i, ch) in up_to.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    paren_pos = Some(i);
                    break;
                }
                depth -= 1;
            }
            ',' if depth == 0 => active_param += 1,
            _ => {}
        }
    }

    let paren_pos = paren_pos?;
    let before_paren = up_to[..paren_pos].trim_end();

    // Extract function name (possibly "Object.method" or just "name").
    let func_name = before_paren
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.')
        .last()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())?;

    Some((func_name, active_param))
      }
