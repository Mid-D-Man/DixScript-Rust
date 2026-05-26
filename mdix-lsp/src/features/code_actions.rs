// mdix-lsp/src/features/code_actions.rs
//! Code action / quick-fix provider.
//!
//! ## Security quick-fix
//! A "Insert @SECURITY section" code action fires in two ways:
//!   1. Diagnostic-based: when a SEC001 squiggly is clicked (lightbulb).
//!   2. Proactive: whenever the document has DEncryptor in @DLM but no
//!      @SECURITY section, even if diagnostics haven't propagated yet.
//!
//! The generated @SECURITY block is customised to the encryption algorithm
//! detected in @DLM (aes256-gcm, aes128-gcm, chacha20-poly1305, or xor).
//! It also contains a `key_file` or `password` mode choice snippet.

use std::panic;
use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionResponse,
    Diagnostic, Position, Range, TextEdit, Url, WorkspaceEdit,
};
use dixscript::Compiler::AST::data_types::{DLMModuleType, DLMModuleSubtype};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(
    doc:         Option<&Document>,
    diagnostics: &[Diagnostic],
) -> Option<CodeActionResponse> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        provide_inner(doc, diagnostics)
    }));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("code_actions panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(
    doc:         Option<&Document>,
    diagnostics: &[Diagnostic],
) -> Option<CodeActionResponse> {
    let doc = doc?;
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    let mut added_security_insert = false;

    // ── 1. Diagnostic-driven actions ──────────────────────────────────────────
    for diag in diagnostics {
        let source = diag.source.as_deref().unwrap_or("");
        let msg    = diag.message.to_lowercase();

        if source.contains("semantic") || source.contains("parser") || source.contains("dixscript") {

            // Missing @SECURITY — main quick-fix
            if is_security_missing_msg(&msg) {
                if !added_security_insert {
                    let algorithm = infer_algorithm_from_doc(doc);
                    if let Some(action) = fix_insert_security(doc, algorithm) {
                        actions.push(CodeActionOrCommand::CodeAction(action));
                        added_security_insert = true;
                    }
                }
                continue;
            }

            // Weak XOR encryption warning
            if msg.contains("xor") || msg.contains("weak") || msg.contains("obfuscation") {
                if let Some(action) = fix_replace_xor_in_dlm(doc) {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }

            // Unknown / undefined enum
            if msg.contains("unknown enum") || msg.contains("undefined enum")
                || msg.contains("undeclared enum")
            {
                actions.extend(fix_unknown_enum(doc, diag));
            }
        }
    }

    // ── 2. Proactive: DEncryptor present but no @SECURITY ────────────────────
    //
    // This fires even before the diagnostic squiggly appears (e.g. immediately
    // after the user adds DEncryptor to @DLM but the analysis hasn't finished).
    if !added_security_insert {
        if let Some(info) = encryptor_without_security(doc) {
            if let Some(action) = fix_insert_security(doc, &info.algorithm) {
                actions.push(CodeActionOrCommand::CodeAction(action));

                // Also offer a "Replace xor with aes256" fix when xor is the culprit
                if info.algorithm == "xor" {
                    if let Some(xor_fix) = fix_replace_xor_in_dlm(doc) {
                        actions.push(CodeActionOrCommand::CodeAction(xor_fix));
                    }
                }
            }
        }
    }

    if actions.is_empty() { None } else { Some(actions) }
}

// ── Security-message detection ────────────────────────────────────────────────

fn is_security_missing_msg(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    (lower.contains("security") && lower.contains("missing"))
        || lower.contains("@security section is required")
        || lower.contains("encryptor requires")
        || lower.contains("sec001")
}

// ── DLM introspection ─────────────────────────────────────────────────────────

struct EncryptorInfo {
    algorithm: String,
}

/// Returns Some if the document has DEncryptor but no @SECURITY, else None.
fn encryptor_without_security(doc: &Document) -> Option<EncryptorInfo> {
    let ast = doc.ast.as_ref()?;

    if ast.security.is_some() {
        return None; // @SECURITY already present — nothing to do
    }

    let dlm = ast.dlm.as_ref()?;
    let enc = dlm.modules.iter().find(|m| matches!(m.module_type, DLMModuleType::DEncryptor))?;

    let algorithm = match enc.subtype {
        Some(DLMModuleSubtype::Aes128)  => "aes128-gcm",
        Some(DLMModuleSubtype::Aes256)  => "aes256-gcm",
        Some(DLMModuleSubtype::Chacha20) => "chacha20-poly1305",
        Some(DLMModuleSubtype::Xor)      => "xor",
        _                                => "aes256-gcm",
    };

    Some(EncryptorInfo { algorithm: algorithm.to_string() })
}

/// Infer the algorithm string from the AST (used for diagnostic-driven path).
fn infer_algorithm_from_doc(doc: &Document) -> String {
    encryptor_without_security(doc)
        .map(|i| i.algorithm)
        .unwrap_or_else(|| "aes256-gcm".to_string())
}

// ── @SECURITY insertion ───────────────────────────────────────────────────────

/// Build a TextEdit that appends a complete @SECURITY block at the end of the file.
fn fix_insert_security(doc: &Document, algorithm: &str) -> Option<CodeAction> {
    let line_count = doc.source.lines().count() as u32;

    // Determine whether the file already has a trailing newline
    let needs_leading_newline = !doc.source.ends_with('\n');
    let prefix = if needs_leading_newline { "\n" } else { "" };

    let security_block = build_security_block(prefix, algorithm);

    let insert_pos = Position::new(line_count, 0);

    let edit = TextEdit {
        range:    Range::new(insert_pos, insert_pos),
        new_text: security_block,
    };

    let title = format!(
        "Insert @SECURITY section ({})",
        algorithm_display_name(algorithm)
    );

    Some(make_action(
        &title,
        CodeActionKind::QUICKFIX,
        doc.uri.clone(),
        vec![edit],
        true,
    ))
}

/// Build the @SECURITY block text for the given algorithm.
///
/// For real encryption algorithms (aes256-gcm, aes128-gcm, chacha20-poly1305)
/// we produce a block with both mode options and the correct algorithm name.
/// For xor (weak) we just produce keyfile mode with a comment.
fn build_security_block(prefix: &str, algorithm: &str) -> String {
    match algorithm {
        "xor" => format!(
            "{}\n\
             @SECURITY(\n\
             \x20 // XOR is obfuscation only — consider upgrading to aes256\n\
             \x20 encryption -> {{\n\
             \x20   mode      = \"keyfile\",\n\
             \x20   algorithm = \"xor\"\n\
             \x20 }}\n\
             )\n",
            prefix
        ),

        _ => {
            // Map DLM subtype name to @SECURITY algorithm string
            let sec_algo = match algorithm {
                "aes128-gcm"         => "aes128-gcm",
                "chacha20-poly1305"  => "chacha20-poly1305",
                _                    => "aes256-gcm",  // default / aes256-gcm
            };
            format!(
                "{}\n\
                 @SECURITY(\n\
                 \x20 encryption -> {{\n\
                 \x20   // mode: \"keyfile\" (auto-generates .mdix.key) or \"password\" (prompts at compile time)\n\
                 \x20   mode      = \"keyfile\",\n\
                 \x20   algorithm = \"{}\"\n\
                 \x20 }}\n\
                 )\n",
                prefix, sec_algo
            )
        }
    }
}

fn algorithm_display_name(algorithm: &str) -> &str {
    match algorithm {
        "aes256-gcm"         => "AES-256-GCM",
        "aes128-gcm"         => "AES-128-GCM",
        "chacha20-poly1305"  => "ChaCha20-Poly1305",
        "xor"                => "XOR (weak)",
        _                    => algorithm,
    }
}

// ── XOR → aes256 replacement ──────────────────────────────────────────────────

fn fix_replace_xor_in_dlm(doc: &Document) -> Option<CodeAction> {
    for token in &doc.tokens {
        if let TokenType::Identifier(id) = &token.token_type {
            if id.eq_ignore_ascii_case("xor") {
                let line = token.line.saturating_sub(1) as u32;
                let col  = token.column.saturating_sub(1) as u32;
                let edit = TextEdit {
                    range:    Range::new(
                        Position::new(line, col),
                        Position::new(line, col + 3),
                    ),
                    new_text: "aes256".to_string(),
                };
                return Some(make_action(
                    "Replace weak 'xor' with 'aes256' in @DLM",
                    CodeActionKind::QUICKFIX,
                    doc.uri.clone(),
                    vec![edit],
                    true,
                ));
            }
        }
    }
    None
}

// ── Unknown enum fix ──────────────────────────────────────────────────────────

fn fix_unknown_enum(doc: &Document, diag: &Diagnostic) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    let enum_name = match extract_quoted_word(&diag.message, 0)
        .or_else(|| extract_quoted_word(&diag.message, 1))
    {
        Some(n) => n,
        None    => return actions,
    };

    let ast   = match &doc.ast   { Some(a) => a, None => return actions };
    let enums = match &ast.enums { Some(e) => e, None => return actions };

    for decl in &enums.enums {
        if !decl.name.eq_ignore_ascii_case(&enum_name) { continue; }
        for field in &decl.fields {
            let replacement = format!("{}.{}", decl.name, field.name);
            let edit = TextEdit {
                range:    diag.range,
                new_text: replacement.clone(),
            };
            actions.push(CodeActionOrCommand::CodeAction(make_action(
                &format!("Replace with {}", replacement),
                CodeActionKind::QUICKFIX,
                doc.uri.clone(),
                vec![edit],
                false,
            )));
        }
    }
    actions
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_action(
    title:        &str,
    kind:         CodeActionKind,
    uri:          Url,
    edits:        Vec<TextEdit>,
    is_preferred: bool,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(uri, edits);
    CodeAction {
        title:        title.to_string(),
        kind:         Some(kind),
        diagnostics:  None,
        edit:         Some(WorkspaceEdit { changes: Some(changes), ..Default::default() }),
        command:      None,
        is_preferred: Some(is_preferred),
        disabled:     None,
        data:         None,
    }
}

fn extract_quoted_word(s: &str, n: usize) -> Option<String> {
    let mut count = 0;
    let mut start = None;
    for (i, c) in s.char_indices() {
        if c == '\'' {
            if start.is_none() {
                start = Some(i + 1);
            } else {
                let word = &s[start.unwrap()..i];
                if count == n { return Some(word.to_string()); }
                count += 1;
                start = None;
            }
        }
    }
    None
}
