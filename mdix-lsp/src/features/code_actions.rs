//! Code action / quick-fix provider.
//!
//! Current quick fixes:
//!   1. Missing @SECURITY when DEncryptor is present in @DLM
//!      → Insert a default @SECURITY block at the end of the document.
//!   2. Weak encryption algorithm (xor) in @DLM
//!      → Replace with aes256.
//!   3. Unknown enum value
//!      → Suggest known values from the same enum as alternatives.

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionResponse,
    Diagnostic, Position, Range, TextEdit, Url, WorkspaceEdit,
};
use std::collections::HashMap;
use dixscript::Compiler::AST::data_types::DLMModuleType;
use dixscript::Compiler::Core::Tokenizer::TokenType;
use crate::document::Document;

pub fn provide(
    doc: Option<&Document>,
    diagnostics: &[Diagnostic],
) -> Option<CodeActionResponse> {
    let doc = doc?;
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();

    for diag in diagnostics {
        // Route each diagnostic to a fix generator by its source tag.
        let source = diag.source.as_deref().unwrap_or("");

        if source.contains("semantic") || source.contains("parser") {
            let msg = diag.message.to_lowercase();

            if msg.contains("security") && msg.contains("missing") {
                if let Some(action) = fix_missing_security(doc) {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }

            if msg.contains("xor") || msg.contains("weak") {
                if let Some(action) = fix_weak_encryption(doc, diag) {
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }

            if msg.contains("unknown enum") || msg.contains("undefined") {
                actions.extend(fix_unknown_enum(doc, diag));
            }
        }
    }

    // Always offer "Insert missing @SECURITY" if DEncryptor is present
    // but @SECURITY is absent, regardless of whether a diagnostic fired.
    if has_encryptor_no_security(doc) {
        if let Some(action) = fix_missing_security(doc) {
            // Avoid duplicates if we already added it from a diagnostic.
            let already = actions.iter().any(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => ca.title == action.title,
                _ => false,
            });
            if !already {
                actions.push(CodeActionOrCommand::CodeAction(action));
            }
        }
    }

    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

// ── Fix: insert default @SECURITY ─────────────────────────────────────────────

fn fix_missing_security(doc: &Document) -> Option<CodeAction> {
    let line_count = doc.source.lines().count() as u32;

    let snippet = concat!(
        "\n\n@SECURITY(\n",
        "  encryption -> {\n",
        "    mode = \"password\",\n",
        "    algorithm = \"aes256-gcm\"\n",
        "  }\n",
        ")"
    );

    let edit = text_edit(
        Range::new(
            Position::new(line_count, 0),
            Position::new(line_count, 0),
        ),
        snippet.to_string(),
    );

    Some(make_action(
        "Insert default @SECURITY section",
        CodeActionKind::QUICKFIX,
        doc.uri.clone(),
        vec![edit],
    ))
}

// ── Fix: replace weak encryption algorithm ────────────────────────────────────

fn fix_weak_encryption(doc: &Document, diag: &Diagnostic) -> Option<CodeAction> {
    // Find the exact "xor" token location from the token stream.
    for token in &doc.tokens {
        if let TokenType::Identifier(id) = &token.token_type {
            if id.eq_ignore_ascii_case("xor") {
                let line = token.line.saturating_sub(1) as u32;
                let col  = token.column.saturating_sub(1) as u32;

                let edit = text_edit(
                    Range::new(
                        Position::new(line, col),
                        Position::new(line, col + 3), // "xor" is 3 chars
                    ),
                    "aes256".to_string(),
                );

                return Some(make_action(
                    "Replace weak 'xor' with 'aes256'",
                    CodeActionKind::QUICKFIX,
                    doc.uri.clone(),
                    vec![edit],
                ));
            }
        }
    }
    None
}

// ── Fix: suggest known enum values ────────────────────────────────────────────

fn fix_unknown_enum(doc: &Document, diag: &Diagnostic) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    // Try to extract the enum name from the diagnostic message.
    // Expected pattern: "unknown value 'X' for enum 'Y'" or similar.
    let msg = &diag.message;
    let enum_name = extract_quoted_word(msg, 1); // second quoted word is typically the enum name

    let enum_name = match enum_name {
        Some(n) => n,
        None    => return actions,
    };

    let ast = match &doc.ast {
        Some(a) => a,
        None    => return actions,
    };

    let enums = match &ast.enums {
        Some(e) => e,
        None    => return actions,
    };

    for decl in &enums.enums {
        if !decl.name.eq_ignore_ascii_case(&enum_name) {
            continue;
        }
        for field in &decl.fields {
            let replacement = format!("{}.{}", decl.name, field.name);
            let edit = text_edit(diag.range, replacement.clone());
            actions.push(CodeActionOrCommand::CodeAction(make_action(
                &format!("Replace with {}", replacement),
                CodeActionKind::QUICKFIX,
                doc.uri.clone(),
                vec![edit],
            )));
        }
    }

    actions
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn has_encryptor_no_security(doc: &Document) -> bool {
    let ast = match &doc.ast {
        Some(a) => a,
        None    => return false,
    };

    let has_encryptor = ast
        .dlm
        .as_ref()
        .map(|dlm| {
            dlm.modules
                .iter()
                .any(|m| matches!(m.module_type, DLMModuleType::DEncryptor))
        })
        .unwrap_or(false);

    has_encryptor && ast.security.is_none()
}

fn make_action(
    title: &str,
    kind: CodeActionKind,
    uri: Url,
    edits: Vec<TextEdit>,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(uri, edits);

    CodeAction {
        title:                title.to_string(),
        kind:                 Some(kind),
        diagnostics:          None,
        edit:                 Some(WorkspaceEdit {
            changes:              Some(changes),
            document_changes:     None,
            change_annotations:   None,
        }),
        command:              None,
        is_preferred:         Some(true),
        disabled:             None,
        data:                 None,
    }
}

fn text_edit(range: Range, new_text: String) -> TextEdit {
    TextEdit { range, new_text }
}

/// Extracts the nth quoted word from a string.
/// e.g. `extract_quoted_word("unknown 'FOO' for enum 'MyEnum'", 1)` → Some("MyEnum")
fn extract_quoted_word(s: &str, n: usize) -> Option<String> {
    let mut count = 0;
    let mut start = None;

    for (i, c) in s.char_indices() {
        if c == '\'' {
            if start.is_none() {
                start = Some(i + 1);
            } else {
                let word = &s[start.unwrap()..i];
                if count == n {
                    return Some(word.to_string());
                }
                count += 1;
                start = None;
            }
        }
    }
    None
  }
