// mdix-lsp/src/features/code_actions.rs
//! Code action / quick-fix provider.
//!
//! FIXES:
//!   1. Security missing squiggly is now at the @DLM line (not 0:0)
//!   2. DAuditor has its own validation — if auditor present with no DAuditor
//!      subtype that is fine; it warns on unrecognised subtype.
//!   3. Light bulb to insert @SECURITY fires reliably via both diagnostics
//!      AND a proactive check when no diagnostics are present.

use std::panic;
use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionResponse,
    Diagnostic, Position, Range, TextEdit, Url, WorkspaceEdit,
};
use dixscript::Compiler::AST::data_types::{DLMModuleType, DLMModuleSubtype};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use crate::document::Document;

pub fn provide(
    doc: Option<&Document>,
    diagnostics: &[Diagnostic],
) -> Option<CodeActionResponse> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc, diagnostics)));
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
    doc: Option<&Document>,
    diagnostics: &[Diagnostic],
) -> Option<CodeActionResponse> {
    let doc = doc?;
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    let mut added_security_insert = false;

    // ── Actions derived from diagnostics ──────────────────────────────────────
    for diag in diagnostics {
        let source = diag.source.as_deref().unwrap_or("");
        let msg    = diag.message.to_lowercase();

        if source.contains("semantic") || source.contains("parser") {
            // Missing @SECURITY section (squiggly is now on @DLM line)
            if (msg.contains("security") && msg.contains("missing"))
                || msg.contains("@security section is required")
                || msg.contains("encryptor requires")
            {
                if !added_security_insert {
                    if let Some(action) = fix_insert_security(doc, infer_algorithm_from_doc(doc)) {
                        actions.push(CodeActionOrCommand::CodeAction(action));
                        added_security_insert = true;
                    }
                }
            }

            // Weak XOR encryption
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

    // ── Proactive: DEncryptor present, no @SECURITY ───────────────────────────
    if !added_security_insert && has_encryptor_no_security(doc) {
        if let Some(action) = fix_insert_security(doc, infer_algorithm_from_doc(doc)) {
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
    }

    if actions.is_empty() { None } else { Some(actions) }
}

// ── Security insertion ────────────────────────────────────────────────────────

fn infer_algorithm_from_doc(doc: &Document) -> &'static str {
    let ast = match &doc.ast { Some(a) => a, None => return "aes256-gcm" };
    let dlm = match &ast.dlm { Some(d) => d, None => return "aes256-gcm" };
    for m in &dlm.modules {
        if matches!(m.module_type, DLMModuleType::DEncryptor) {
            return match m.subtype {
                Some(DLMModuleSubtype::Aes128)   => "aes128-gcm",
                Some(DLMModuleSubtype::Chacha20)  => "chacha20-poly1305",
                Some(DLMModuleSubtype::Xor)       => "xor",
                _                                 => "aes256-gcm",
            };
        }
    }
    "aes256-gcm"
}

fn fix_insert_security(doc: &Document, algorithm: &str) -> Option<CodeAction> {
    let line_count = doc.source.lines().count() as u32;

    // Choose KDF block only when password mode is sensible
    let security_block = if algorithm == "xor" {
        concat!(
            "\n\n@SECURITY(\n",
            "  encryption -> {\n",
            "    mode      = \"keyfile\",\n",
            "    algorithm = \"xor\"\n",
            "  }\n",
            ")"
        ).to_string()
    } else {
        format!(
            "\n\n@SECURITY(\n  encryption -> {{\n    mode      = \"${{1|password,keyfile|}}\",\n    algorithm = \"{}\"\n  }}\n)",
            algorithm
        )
    };

    let edit = TextEdit {
        range:    Range::new(
            Position::new(line_count, 0),
            Position::new(line_count, 0),
        ),
        new_text: security_block,
    };

    Some(make_action(
        &format!("Insert @SECURITY section ({})", algorithm),
        CodeActionKind::QUICKFIX,
        doc.uri.clone(),
        vec![edit],
        true,
    ))
}

// ── XOR fix ───────────────────────────────────────────────────────────────────

fn fix_replace_xor_in_dlm(doc: &Document) -> Option<CodeAction> {
    // Find the `xor` identifier token inside @DLM
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
                    "Replace weak 'xor' with 'aes256'",
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

// ── Proactive check ───────────────────────────────────────────────────────────

fn has_encryptor_no_security(doc: &Document) -> bool {
    let ast = match &doc.ast { Some(a) => a, None => return false };
    let has_enc = ast.dlm.as_ref()
        .map(|d| d.modules.iter().any(|m| matches!(m.module_type, DLMModuleType::DEncryptor)))
        .unwrap_or(false);
    has_enc && ast.security.is_none()
}

// ── Constructors ──────────────────────────────────────────────────────────────

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
