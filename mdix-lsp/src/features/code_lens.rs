use std::panic;

use tower_lsp::lsp_types::{CodeLens, Command, Position, Range};
use serde_json::Value as JsonValue;
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};

use crate::document::Document;

// CMD_VALIDATE intentionally removed from ALL_COMMANDS and the lens list.
pub const CMD_TO_JSON:          &str = "mdix.convertToJson";
pub const CMD_TO_TOML:          &str = "mdix.convertToToml";
pub const CMD_MINIFY:           &str = "mdix.minify";
pub const CMD_COMPILE:          &str = "mdix.compile";
pub const CMD_SHOW_AST:         &str = "mdix.showAst";
pub const CMD_CREATE_RESOLVED:  &str = "mdix.createResolved";
// Unlike CMD_EDIT_DATETIME/CMD_PREVIEW_BLOB below, this one genuinely needs
// the server (runs test text through the real `regex` crate DixScript's
// `regex` type is backed by) rather than being intercepted client-side --
// belongs in ALL_COMMANDS/execute_command like the rest of this block, not
// with the client-only commands.
pub const CMD_TEST_REGEX:       &str = "mdix.testRegex";

pub const ALL_COMMANDS: &[&str] = &[
    CMD_TO_JSON,
    CMD_TO_TOML,
    CMD_MINIFY,
    CMD_COMPILE,
    CMD_SHOW_AST,
    CMD_CREATE_RESOLVED,
    CMD_TEST_REGEX,
];

// ── Client-only commands ─────────────────────────────────────────────────────
//
// These are handled entirely inside the VS Code extension (registered via
// `vscode.commands.registerCommand`, opening a Webview) and are deliberately
// NOT added to `ALL_COMMANDS` / `execute_command` — same convention already
// used by `dixscript.restartServer`. vscode-languageclient always checks for
// a locally-registered command with this ID before forwarding a CodeLens
// click to the server via `workspace/executeCommand`, so a local handler
// intercepts these before the server ever sees them.
pub const CMD_EDIT_DATETIME: &str = "mdix.editDateTime";
pub const CMD_PREVIEW_BLOB:  &str = "mdix.previewBlob";

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

    // Five top-level lenses (Validate removed, Compile replaces it)
    lenses.push(make_lens(file_range, "→ JSON",    CMD_TO_JSON,         vec![uri_arg.clone()]));
    lenses.push(make_lens(file_range, "→ TOML",    CMD_TO_TOML,         vec![uri_arg.clone()]));
    lenses.push(make_lens(file_range, "⊡ Minify",  CMD_MINIFY,          vec![uri_arg.clone()]));
    lenses.push(make_lens(file_range, "⊞ Resolve", CMD_CREATE_RESOLVED, vec![uri_arg.clone()]));
    lenses.push(make_lens(file_range, "⚙ Compile", CMD_COMPILE,         vec![uri_arg.clone()]));

    for (idx, token) in doc.tokens.iter().enumerate() {
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

        // ── Date / Timestamp literal → inline picker lens ────────────────────
        let dt_kind = match &token.token_type {
            TokenType::Date(_)      => Some("date"),
            TokenType::Timestamp(_) => Some("timestamp"),
            _                       => None,
        };
        if let Some(kind) = dt_kind {
            let raw = token.get_token_value();
            let line = token.line.saturating_sub(1) as u32;
            let col  = token.column.saturating_sub(1) as u32;
            let len  = raw.len() as u32;
            let range = Range::new(Position::new(line, col), Position::new(line, col + len));
            let range_json = serde_json::to_value(&range).unwrap_or(JsonValue::Null);

            lenses.push(make_lens(
                range,
                "📅 Edit",
                CMD_EDIT_DATETIME,
                vec![
                    uri_arg.clone(),
                    range_json,
                    JsonValue::String(raw),
                    JsonValue::String(kind.to_string()),
                ],
            ));
        }

        // ── Blob constructor → preview lens ───────────────────────────────────
        if matches!(token.token_type, TokenType::BlobConstructor(_)) {
            if let Some(content) = find_blob_content(&doc.tokens, idx) {
                let line = token.line.saturating_sub(1) as u32;
                let col  = token.column.saturating_sub(1) as u32;
                let point = Range::new(Position::new(line, col), Position::new(line, col));

                lenses.push(make_lens(
                    point,
                    "▶ Preview blob",
                    CMD_PREVIEW_BLOB,
                    vec![uri_arg.clone(), JsonValue::String(content.to_string())],
                ));
            }
        }
    }

    if lenses.is_empty() { None } else { Some(lenses) }
}

// A `b:(...)` constructs as: BlobConstructor  Symbol('(')  <content>  Symbol(')').
// The content is almost always a String/StringSingle literal (base64 text);
// `b:()` (empty blob) has no content token at all. Bounded lookahead (a
// handful of tokens) keeps this cheap and safe against malformed input.
fn find_blob_content(tokens: &[Token], blob_idx: usize) -> Option<String> {
    let open_idx = blob_idx + 1;
    match tokens.get(open_idx).map(|t| &t.token_type) {
        Some(TokenType::Symbol('(')) => {}
        _ => return None,
    }

    match tokens.get(open_idx + 1).map(|t| &t.token_type) {
        Some(TokenType::String(s)) | Some(TokenType::StringSingle(s)) => Some(s.clone()),
        Some(TokenType::Symbol(')')) => Some(String::new()), // b:() — empty blob
        _ => None,
    }
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
