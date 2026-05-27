// mdix-lsp/src/features/code_lens.rs
// mdix-lsp/src/features/code_lens.rs
//! CodeLens provider — the "play button" for DixScript.

use std::panic;

use tower_lsp::lsp_types::{CodeLens, Command, Position, Range};
use serde_json::Value as JsonValue;
use dixscript::Compiler::Core::Tokenizer::TokenType;

use crate::document::Document;

pub const CMD_VALIDATE:         &str = "mdix.validate";
pub const CMD_TO_JSON:          &str = "mdix.convertToJson";
pub const CMD_TO_TOML:          &str = "mdix.convertToToml";
pub const CMD_MINIFY:           &str = "mdix.minify";
pub const CMD_COMPILE:          &str = "mdix.compile";
pub const CMD_SHOW_AST:         &str = "mdix.showAst";
pub const CMD_CREATE_RESOLVED:  &str = "mdix.createResolved";

pub const ALL_COMMANDS: &[&str] = &[
    CMD_VALIDATE,
    CMD_TO_JSON,
    CMD_TO_TOML,
    CMD_MINIFY,
    CMD_COMPILE,
    CMD_SHOW_AST,
    CMD_CREATE_RESOLVED,
];

pub fn provide(doc: Option<&Document>) -> Option<Vec<CodeLens>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("code_lens panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>) -> Option<Vec<CodeLens>> {
    let doc     = doc?;
    let uri_str = doc.uri.to_string();
    let uri_arg = JsonValue::String(uri_str.clone());

    let mut lenses: Vec<CodeLens> = Vec::new();

    let file_range = Range::new(Position::new(0, 0), Position::new(0, 0));

    lenses.push(make_lens(file_range, "▶ Validate",   CMD_VALIDATE,        vec![uri_arg.clone()]));
    lenses.push(make_lens(file_range, "→ JSON",        CMD_TO_JSON,         vec![uri_arg.clone()]));
    lenses.push(make_lens(file_range, "→ TOML",        CMD_TO_TOML,         vec![uri_arg.clone()]));
    lenses.push(make_lens(file_range, "⊡ Minify",      CMD_MINIFY,          vec![uri_arg.clone()]));
    lenses.push(make_lens(file_range, "⊞ Resolve",     CMD_CREATE_RESOLVED, vec![uri_arg.clone()]));
    lenses.push(make_lens(file_range, "⚙ Compile",     CMD_COMPILE,         vec![uri_arg.clone()]));

    for token in &doc.tokens {
        let is_data = matches!(token.token_type, TokenType::SectionData);
        let is_qf   = matches!(token.token_type, TokenType::SectionQuickFuncs);

        if is_data || is_qf {
            let line = token.line.saturating_sub(1) as u32;
            let sec_range = Range::new(Position::new(line, 0), Position::new(line, 0));

            if is_data {
                lenses.push(make_lens(
                    sec_range,
                    "→ JSON (section)",
                    CMD_TO_JSON,
                    vec![uri_arg.clone()],
                ));
            }

            if is_qf {
                lenses.push(make_lens(
                    sec_range,
                    "▶ Show AST",
                    CMD_SHOW_AST,
                    vec![uri_arg.clone()],
                ));
            }
        }
    }

    if lenses.is_empty() { None } else { Some(lenses) }
}

fn make_lens(range: Range, title: &str, command: &str, args: Vec<JsonValue>) -> CodeLens {
    CodeLens {
        range,
        command: Some(Command {
            title:     title.to_string(),
            command:   command.to_string(),
            arguments: Some(args),
        }),
        data: None,
    }
    }
