//! Code action / quick-fix provider.
//!
//! ## Actions provided
//!
//! ### Diagnostics-driven
//! - SEC001: "Insert @SECURITY section" when DEncryptor present without @SECURITY
//! - Weak XOR: "Replace 'xor' with 'aes256' in @DLM"
//! - Unknown enum: offer valid enum field replacements
//! - Missing let/let mut: insert declaration on the error line
//! - Missing closing brace: insert `}` after the error line
//!
//! ### Context-driven (position-based)
//! - ⬇ Spread inline object / table-property / group-array / array to multiple lines
//! - ⟳ Reformat file — normalise spacing, indentation, operators (always available)
//!
//! ### Proactive (cursor-position-independent)
//! - DEncryptor present but no @SECURITY → insert full @SECURITY section
//! - @SECURITY present but missing encryption entry → complete the block
//!
//! ### Date / Timestamp
//! - Increment / decrement day, month, year via lightbulb on date/timestamp tokens

use std::panic;
use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionResponse,
    Diagnostic, Position, Range, TextEdit, Url, WorkspaceEdit,
};
use dixscript::Compiler::AST::data_types::{DLMModuleType, DLMModuleSubtype};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use dixscript::Compiler::Core::Tokenizer::token::SectionId;

use crate::document::Document;
use crate::features::formatting::format_source;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(
    doc:         Option<&Document>,
    range:       Range,
    diagnostics: &[Diagnostic],
) -> Option<CodeActionResponse> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        provide_inner(doc, range, diagnostics)
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
    range:       Range,
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

            // Missing / required @SECURITY section
            if is_security_missing_msg(&msg) {
                if !added_security_insert {
                    let algorithm = infer_algorithm_from_doc(doc);
                    if let Some(action) = fix_insert_security(doc, algorithm.as_str()) {
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

    // ── 2. Proactive: DEncryptor present but no @SECURITY at all ─────────────
    if !added_security_insert {
        if let Some(info) = encryptor_without_security(doc) {
            if let Some(action) = fix_insert_security(doc, &info.algorithm) {
                actions.push(CodeActionOrCommand::CodeAction(action));
                added_security_insert = true;
                if info.algorithm == "xor" {
                    if let Some(xor_fix) = fix_replace_xor_in_dlm(doc) {
                        actions.push(CodeActionOrCommand::CodeAction(xor_fix));
                    }
                }
            }
        }
    }

    // ── 3. Proactive: @SECURITY present but missing encryption entry ──────────
    if !added_security_insert {
        actions.extend(provide_complete_security_actions(doc));
    }

    // ── 4. Spread inline properties to multiple lines ─────────────────────────
    actions.extend(provide_spread_actions(doc, range));

    // ── 5. Date / Timestamp picker actions ────────────────────────────────────
    actions.extend(date_time_actions(doc, range));

    // ── 6. let / let mut declaration fixes ───────────────────────────────────
    actions.extend(provide_let_declaration_actions(doc, range, diagnostics));

    // ── 7. Missing closing brace ──────────────────────────────────────────────
    actions.extend(provide_missing_brace_actions(doc, range, diagnostics));

    // ── 8. Reformat file (always offered if changes would be made) ────────────
    if let Some(action) = provide_reformat_action(doc) {
        actions.push(CodeActionOrCommand::CodeAction(action));
    }

    if actions.is_empty() { None } else { Some(actions) }
}

// ── Security-message detection ────────────────────────────────────────────────
//
// The SEC001 message from the analyser is:
//   "@SECURITY section required: DEncryptor.{algo} is present in @DLM but no
//    @SECURITY block was found."
// So we must match "required", not just "missing".

fn is_security_missing_msg(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    // Primary: matches SEC001 and the generic "security ... required/missing" family
    (lower.contains("security") && (lower.contains("missing") || lower.contains("required")))
        // DEncryptor explicitly mentioned alongside security
        || (lower.contains("dencryptor") && lower.contains("security"))
        // Explicit codes / keywords used elsewhere in the codebase
        || lower.contains("sec001")
        || lower.contains("encryptor requires")
}

// ── DLM introspection ─────────────────────────────────────────────────────────

struct EncryptorInfo {
    algorithm: String,
}

fn encryptor_without_security(doc: &Document) -> Option<EncryptorInfo> {
    let ast = doc.ast.as_ref()?;
    // Only trigger when @SECURITY is completely absent from the AST.
    // If @SECURITY exists but is incomplete, provide_complete_security_actions handles it.
    if ast.security.is_some() { return None; }
    let dlm = ast.dlm.as_ref()?;
    let enc = dlm.modules.iter().find(|m| matches!(m.module_type, DLMModuleType::DEncryptor))?;
    let algorithm = match enc.subtype {
        Some(DLMModuleSubtype::Aes128)   => "aes128-gcm",
        Some(DLMModuleSubtype::Aes256)   => "aes256-gcm",
        Some(DLMModuleSubtype::Chacha20) => "chacha20-poly1305",
        Some(DLMModuleSubtype::Xor)      => "xor",
        _                                => "aes256-gcm",
    };
    Some(EncryptorInfo { algorithm: algorithm.to_string() })
}

fn infer_algorithm_from_doc(doc: &Document) -> String {
    encryptor_without_security(doc)
        .map(|i| i.algorithm)
        .unwrap_or_else(|| "aes256-gcm".to_string())
}

// ── @SECURITY insertion ───────────────────────────────────────────────────────

fn fix_insert_security(doc: &Document, algorithm: &str) -> Option<CodeAction> {
    let line_count = doc.source.lines().count() as u32;
    let needs_leading_newline = !doc.source.ends_with('\n');
    let prefix = if needs_leading_newline { "\n" } else { "" };
    let security_block = build_security_block(prefix, algorithm);
    let insert_pos = Position::new(line_count, 0);
    let edit = TextEdit {
        range:    Range::new(insert_pos, insert_pos),
        new_text: security_block,
    };
    let title = format!("Insert @SECURITY section ({})", algorithm_display_name(algorithm));
    Some(make_action(&title, CodeActionKind::QUICKFIX, doc.uri.clone(), vec![edit], true))
}

/// Build a complete @SECURITY block for the given algorithm.
///
/// Always includes both an `encryption` block **and** a `keystore` block with
/// `auto_generate = true`.  Without `keystore → { auto_generate = true }` the
/// compiler expects an existing key file and will not produce a `.mdix.key`
/// output — which is the unintuitive behaviour this function is designed to
/// avoid.
fn build_security_block(prefix: &str, algorithm: &str) -> String {
    match algorithm {
        "xor" => format!(
            "{}\n\
             @SECURITY(\n\
             \x20 // ⚠️  XOR is obfuscation only — consider upgrading to aes256\n\
             \x20 encryption -> {{\n\
             \x20   mode      = \"keyfile\",\n\
             \x20   algorithm = \"xor\"\n\
             \x20 }}\n\
             \x20 keystore -> {{\n\
             \x20   auto_generate = true\n\
             \x20 }}\n\
             )\n",
            prefix
        ),
        _ => {
            let sec_algo = match algorithm {
                "aes128-gcm"        => "aes128-gcm",
                "chacha20-poly1305" => "chacha20-poly1305",
                _                   => "aes256-gcm",
            };
            format!(
                "{}\n\
                 @SECURITY(\n\
                 \x20 // mode: \"keyfile\"  → compiler auto-generates a .mdix.key file\n\
                 \x20 // mode: \"password\" → compiler prompts for a password at compile time\n\
                 \x20 encryption -> {{\n\
                 \x20   mode      = \"keyfile\",\n\
                 \x20   algorithm = \"{}\"\n\
                 \x20 }}\n\
                 \x20 keystore -> {{\n\
                 \x20   auto_generate = true\n\
                 \x20 }}\n\
                 )\n",
                prefix, sec_algo
            )
        }
    }
}

fn algorithm_display_name(algorithm: &str) -> &str {
    match algorithm {
        "aes256-gcm"        => "AES-256-GCM",
        "aes128-gcm"        => "AES-128-GCM",
        "chacha20-poly1305" => "ChaCha20-Poly1305",
        "xor"               => "XOR (weak — obfuscation only)",
        _                   => algorithm,
    }
}

// ── Complete an existing but incomplete @SECURITY section ─────────────────────
//
// Triggered proactively when @SECURITY exists in the AST but has no
// `encryption` entry.  This covers the case where the user has written an
// empty `@SECURITY()` block (so SEC001 is suppressed) but the encryption
// configuration is still missing.
//
// Also inserts `keystore → { auto_generate = true }` so that compilation
// immediately produces a `.mdix.key` file without any further edits.

fn provide_complete_security_actions(doc: &Document) -> Vec<CodeActionOrCommand> {
    let ast = match &doc.ast { Some(a) => a, None => return vec![] };

    let security = match &ast.security {
        Some(s) => s,
        None    => return vec![],
    };

    // Nothing to do if encryption entry is already present.
    let has_encryption = security.entries.iter()
        .any(|e| e.block_key.eq_ignore_ascii_case("encryption"));
    if has_encryption { return vec![]; }

    // Derive algorithm from @DLM if available.
    let algorithm = ast.dlm.as_ref()
        .and_then(|d| d.modules.iter().find(|m| matches!(m.module_type, DLMModuleType::DEncryptor)))
        .map(|m| match m.subtype {
            Some(DLMModuleSubtype::Aes128)   => "aes128-gcm",
            Some(DLMModuleSubtype::Aes256)   => "aes256-gcm",
            Some(DLMModuleSubtype::Chacha20) => "chacha20-poly1305",
            Some(DLMModuleSubtype::Xor)      => "xor",
            _                                => "aes256-gcm",
        })
        .unwrap_or("aes256-gcm");

    // Find the line of the @SECURITY keyword token.
    let security_kw_line = doc.tokens.iter()
        .find(|t| matches!(t.token_type, TokenType::SectionSecurity))
        .map(|t| t.line.saturating_sub(1) as u32)
        .unwrap_or(0);

    // Insert the encryption + keystore blocks immediately after the
    // @SECURITY( opening line.
    let insert_line = security_kw_line + 1;
    let insert_pos  = Position::new(insert_line, 0);

    // Build the entry text including both the encryption block and the
    // keystore block so a `.mdix.key` file is generated on compilation.
    let entry = if algorithm == "xor" {
        "  // ⚠️  XOR is obfuscation only — consider upgrading to aes256\n  \
         encryption -> {\n    \
         mode      = \"keyfile\",\n    \
         algorithm = \"xor\"\n  \
         }\n  \
         keystore -> {\n    \
         auto_generate = true\n  \
         }\n".to_string()
    } else {
        format!(
            "  // mode: \"keyfile\"  → compiler auto-generates a .mdix.key file\n  \
             // mode: \"password\" → compiler prompts for a password at compile time\n  \
             encryption -> {{\n    \
             mode      = \"keyfile\",\n    \
             algorithm = \"{}\"\n  \
             }}\n  \
             keystore -> {{\n    \
             auto_generate = true\n  \
             }}\n",
            algorithm
        )
    };

    vec![CodeActionOrCommand::CodeAction(make_action(
        &format!("Complete @SECURITY: add encryption + keystore blocks ({})", algorithm_display_name(algorithm)),
        CodeActionKind::QUICKFIX,
        doc.uri.clone(),
        vec![TextEdit {
            range:    Range::new(insert_pos, insert_pos),
            new_text: entry,
        }],
        true,
    ))]
}

// ── XOR → aes256 replacement ──────────────────────────────────────────────────

fn fix_replace_xor_in_dlm(doc: &Document) -> Option<CodeAction> {
    for token in &doc.tokens {
        if let TokenType::Identifier(id) = &token.token_type {
            if id.eq_ignore_ascii_case("xor") {
                let line = token.line.saturating_sub(1) as u32;
                let col  = token.column.saturating_sub(1) as u32;
                let edit = TextEdit {
                    range:    Range::new(Position::new(line, col), Position::new(line, col + 3)),
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
            let edit = TextEdit { range: diag.range, new_text: replacement.clone() };
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

// ── Missing let / let mut declaration ─────────────────────────────────────────
//
// Triggered when a diagnostic reports an undefined-reference / undeclared-variable
// error and the error line looks like a bare assignment  `x = expr`  without
// the required `let` keyword.

fn provide_let_declaration_actions(
    doc:         &Document,
    _range:      Range,
    diagnostics: &[Diagnostic],
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    for diag in diagnostics {
        let msg = diag.message.to_lowercase();

        // Detect "undefined reference" family of errors.
        let needs_let = msg.contains("not defined")
            || msg.contains("undefined")
            || msg.contains("undeclared")
            || (msg.contains("identifier") && msg.contains("scope"))
            || (msg.contains("variable") && msg.contains("declared"));

        if !needs_let { continue; }

        let line_idx  = diag.range.start.line as usize;
        let line_text = match doc.source.lines().nth(line_idx) {
            Some(l) => l,
            None    => continue,
        };
        let trimmed = line_text.trim();

        // Skip lines that already have a declaration keyword or are not assignments.
        if trimmed.starts_with("let ")
            || trimmed.starts_with("let mut ")
            || trimmed.starts_with("const ")
            || trimmed.starts_with("return ")
            || trimmed.starts_with("if ")
            || trimmed.starts_with("elif ")
            || trimmed.starts_with("//")
            || trimmed.starts_with("log")
        {
            continue;
        }

        // The line must contain " = " (a simple assignment, not a comparison).
        // Reject lines with `==`, `!=`, `<=`, `>=`.
        let has_assign = trimmed.contains(" = ")
            && !trimmed.contains("==")
            && !trimmed.contains("!=")
            && !trimmed.contains("<=")
            && !trimmed.contains(">=");
        if !has_assign { continue; }

        // Column of the first non-whitespace character.
        let indent_len = (line_text.len() - line_text.trim_start().len()) as u32;
        let insert_pos  = Position::new(line_idx as u32, indent_len);
        let insert_range = Range::new(insert_pos, insert_pos);

        // Offer `let` (immutable).
        actions.push(CodeActionOrCommand::CodeAction(make_action(
            "Add 'let' immutable declaration",
            CodeActionKind::QUICKFIX,
            doc.uri.clone(),
            vec![TextEdit { range: insert_range, new_text: "let ".to_string() }],
            true,
        )));

        // Offer `let mut` (mutable).
        actions.push(CodeActionOrCommand::CodeAction(make_action(
            "Add 'let mut' mutable declaration",
            CodeActionKind::QUICKFIX,
            doc.uri.clone(),
            vec![TextEdit { range: insert_range, new_text: "let mut ".to_string() }],
            false,
        )));
    }

    actions
}

// ── Missing closing brace ─────────────────────────────────────────────────────
//
// Triggered when a diagnostic indicates a missing `}`.  Inserts a closing brace
// on the line immediately after the reported error position.

fn provide_missing_brace_actions(
    doc:         &Document,
    _range:      Range,
    diagnostics: &[Diagnostic],
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    for diag in diagnostics {
        let msg = diag.message.to_lowercase();

        let needs_brace = msg.contains("expected '}'")
            || msg.contains("missing '}'")
            || msg.contains("expected closing brace")
            || msg.contains("unclosed block")
            || msg.contains("unclosed brace")
            || (msg.contains("unexpected") && msg.contains("end of file"))
            || (msg.contains("missing token") && msg.contains("}"));

        if !needs_brace { continue; }

        let line_idx  = diag.range.start.line as usize;
        let line_text = doc.source.lines().nth(line_idx).unwrap_or("");
        let indent    = get_indent(line_text);

        // Insert `}` on the line after the error.
        let after_line = (line_idx as u32).saturating_add(1);
        let insert_pos = Position::new(after_line, 0);

        actions.push(CodeActionOrCommand::CodeAction(make_action(
            "Insert missing closing brace '}'",
            CodeActionKind::QUICKFIX,
            doc.uri.clone(),
            vec![TextEdit {
                range:    Range::new(insert_pos, insert_pos),
                new_text: format!("{}}}\n", indent),
            }],
            true,
        )));

        break; // One brace fix per request is sufficient.
    }

    actions
}

// ── Spread helpers — shared utilities ────────────────────────────────────────

/// Return the leading whitespace of `line` as an owned string.
fn get_indent(line: &str) -> String {
    let stripped_len = line.trim_start().len();
    line[..line.len() - stripped_len].to_string()
}

/// Split `text` by `separator` while respecting:
///   - nested brackets `() [] {}`
///   - single- and double-quoted strings (with `\` escape sequences)
///
/// Returns `None` when fewer than 2 parts are produced (nothing to spread).
fn split_respecting_nesting(text: &str, separator: char) -> Option<Vec<String>> {
    let mut parts:   Vec<String> = Vec::new();
    let mut current: String      = String::new();
    let mut depth:   i32         = 0;
    let mut in_str:  bool        = false;
    let mut str_ch:  char        = '"';
    let mut escaped: bool        = false;

    for ch in text.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && in_str {
            escaped = true;
            current.push(ch);
            continue;
        }
        if in_str {
            current.push(ch);
            if ch == str_ch { in_str = false; }
            continue;
        }
        match ch {
            '"' | '\'' => { in_str = true; str_ch = ch; current.push(ch); }
            '(' | '[' | '{' => { depth += 1; current.push(ch); }
            ')' | ']' | '}' => { depth = (depth - 1).max(0); current.push(ch); }
            c if c == separator && depth == 0 => {
                let part = current.trim().to_string();
                if !part.is_empty() { parts.push(part); }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let last = current.trim().to_string();
    if !last.is_empty() { parts.push(last); }

    if parts.len() >= 2 { Some(parts) } else { None }
}

/// Find the byte index of a table-property colon (`path:`) in `text`.
fn find_table_colon(text: &str) -> Option<usize> {
    let mut in_str:  bool  = false;
    let mut str_ch:  char  = '"';
    let mut escaped: bool  = false;
    let chars: Vec<char>   = text.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if escaped            { escaped = false; continue; }
        if ch == '\\' && in_str { escaped = true; continue; }
        if in_str {
            if ch == str_ch { in_str = false; }
            continue;
        }
        match ch {
            '"' | '\'' => { in_str = true; str_ch = ch; }
            ':' => {
                if chars.get(i + 1) == Some(&':') { continue; }
                let prev_is_digit = i > 0 && chars[i - 1].is_ascii_digit();
                let next_is_digit = chars.get(i + 1).map(|c| c.is_ascii_digit()).unwrap_or(false);
                if prev_is_digit && next_is_digit { continue; }
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

/// Find the byte index of `{` that looks like an inline object literal.
fn find_object_open(text: &str) -> Option<usize> {
    for (i, ch) in text.char_indices() {
        if ch != '{' { continue; }
        let after = &text[i + 1..];
        if let Some(close_rel) = after.rfind('}') {
            let inside = &after[..close_rel];
            if inside.contains('=') {
                return Some(i);
            }
        }
    }
    None
}

/// Return the `SectionId` of the first token on `line_idx` (0-based LSP line).
fn line_section_id(doc: &Document, line_idx: usize) -> SectionId {
    let target_1based = line_idx + 1;
    doc.tokens
        .iter()
        .find(|t| t.line == target_1based && t.section != SectionId::None)
        .map(|t| t.section)
        .unwrap_or(SectionId::None)
}

// ── Spread: table property ────────────────────────────────────────────────────

fn try_spread_table_property(
    doc:       &Document,
    line_idx:  usize,
    line_text: &str,
) -> Option<CodeAction> {
    if line_section_id(doc, line_idx) != SectionId::Data { return None; }

    let trimmed   = line_text.trim();
    let colon_pos = find_table_colon(trimmed)?;

    let path_part  = trimmed[..colon_pos].trim();
    let props_part = trimmed[colon_pos + 1..].trim();

    if props_part.is_empty() { return None; }

    let props = split_respecting_nesting(props_part, ',')?;

    let indent       = get_indent(line_text);
    let inner_indent = format!("{}  ", indent);

    let mut lines = vec![format!("{}{}:", indent, path_part)];
    for (i, prop) in props.iter().enumerate() {
        let p = prop.trim();
        if i < props.len() - 1 {
            lines.push(format!("{}{},", inner_indent, p));
        } else {
            lines.push(format!("{}{}", inner_indent, p));
        }
    }

    let edit = TextEdit {
        range: Range::new(
            Position::new(line_idx as u32, 0),
            Position::new(line_idx as u32, line_text.len() as u32),
        ),
        new_text: lines.join("\n"),
    };

    Some(make_action(
        "⬇ Spread table properties to multiple lines",
        CodeActionKind::REFACTOR_REWRITE,
        doc.uri.clone(),
        vec![edit],
        false,
    ))
}

// ── Spread: group array ────────────────────────────────────────────────────────

fn try_spread_group_array(
    doc:       &Document,
    line_idx:  usize,
    line_text: &str,
) -> Option<CodeAction> {
    if line_section_id(doc, line_idx) != SectionId::Data { return None; }

    let trimmed = line_text.trim();
    if !trimmed.contains("::") { return None; }

    let dc_pos     = trimmed.find("::")?;
    let path_part  = trimmed[..dc_pos].trim();
    let items_part = trimmed[dc_pos + 2..].trim();

    if items_part.is_empty() { return None; }

    let items = split_respecting_nesting(items_part, ',')?;

    let indent       = get_indent(line_text);
    let inner_indent = format!("{}  ", indent);

    let mut lines = vec![format!("{}{}::", indent, path_part)];
    for (i, item) in items.iter().enumerate() {
        let it = item.trim();
        if i < items.len() - 1 {
            lines.push(format!("{}{},", inner_indent, it));
        } else {
            lines.push(format!("{}{}", inner_indent, it));
        }
    }

    let edit = TextEdit {
        range: Range::new(
            Position::new(line_idx as u32, 0),
            Position::new(line_idx as u32, line_text.len() as u32),
        ),
        new_text: lines.join("\n"),
    };

    Some(make_action(
        "⬇ Spread group array to multiple lines",
        CodeActionKind::REFACTOR_REWRITE,
        doc.uri.clone(),
        vec![edit],
        false,
    ))
}

// ── Spread: object literal ────────────────────────────────────────────────────

fn try_spread_object_literal(
    doc:       &Document,
    line_idx:  usize,
    line_text: &str,
) -> Option<CodeAction> {
    let trimmed  = line_text.trim();
    let open_pos = find_object_open(trimmed)?;
    let after_open = &trimmed[open_pos + 1..];
    let close_rel  = after_open.rfind('}')?;
    let close_pos  = open_pos + 1 + close_rel;

    if close_pos <= open_pos + 1 { return None; }

    let before = trimmed[..open_pos].trim_end();
    let inside = trimmed[open_pos + 1..close_pos].trim();
    let suffix = trimmed[close_pos + 1..].trim();

    if !inside.contains(',') { return None; }

    let props = split_respecting_nesting(inside, ',')?;

    let indent       = get_indent(line_text);
    let inner_indent = format!("{}  ", indent);

    let opening_line = if before.is_empty() {
        format!("{} {{", indent)
    } else {
        format!("{}{} {{", indent, before)
    };

    let mut lines = vec![opening_line];
    for (i, prop) in props.iter().enumerate() {
        let p = prop.trim();
        if i < props.len() - 1 {
            lines.push(format!("{}{},", inner_indent, p));
        } else {
            lines.push(format!("{}{}", inner_indent, p));
        }
    }

    let closing_line = if suffix.is_empty() {
        format!("{}}}", indent)
    } else {
        format!("{}}}{}", indent, suffix)
    };
    lines.push(closing_line);

    let edit = TextEdit {
        range: Range::new(
            Position::new(line_idx as u32, 0),
            Position::new(line_idx as u32, line_text.len() as u32),
        ),
        new_text: lines.join("\n"),
    };

    Some(make_action(
        "⬇ Spread object properties to multiple lines",
        CodeActionKind::REFACTOR_REWRITE,
        doc.uri.clone(),
        vec![edit],
        false,
    ))
}

// ── Spread: array literal ──────────────────────────────────────────────────────

fn try_spread_array_literal(
    doc:       &Document,
    line_idx:  usize,
    line_text: &str,
) -> Option<CodeAction> {
    let trimmed     = line_text.trim();
    let bracket_pos = trimmed.find('[')?;
    let after_open  = &trimmed[bracket_pos + 1..];
    let close_rel   = after_open.rfind(']')?;
    let close_pos   = bracket_pos + 1 + close_rel;

    if close_pos <= bracket_pos + 1 { return None; }

    let before = trimmed[..bracket_pos].trim_end();
    let inside = trimmed[bracket_pos + 1..close_pos].trim();
    let suffix = trimmed[close_pos + 1..].trim();

    if !inside.contains(',') { return None; }

    let items = split_respecting_nesting(inside, ',')?;

    let indent       = get_indent(line_text);
    let inner_indent = format!("{}  ", indent);

    let opening_line = if before.is_empty() {
        format!("{}[", indent)
    } else {
        format!("{}{}[", indent, before)
    };

    let mut lines = vec![opening_line];
    for (i, item) in items.iter().enumerate() {
        let it = item.trim();
        if i < items.len() - 1 {
            lines.push(format!("{}{},", inner_indent, it));
        } else {
            lines.push(format!("{}{}", inner_indent, it));
        }
    }

    let closing_line = if suffix.is_empty() {
        format!("{}]", indent)
    } else {
        format!("{}]{}", indent, suffix)
    };
    lines.push(closing_line);

    let edit = TextEdit {
        range: Range::new(
            Position::new(line_idx as u32, 0),
            Position::new(line_idx as u32, line_text.len() as u32),
        ),
        new_text: lines.join("\n"),
    };

    Some(make_action(
        "⬇ Spread array items to multiple lines",
        CodeActionKind::REFACTOR_REWRITE,
        doc.uri.clone(),
        vec![edit],
        false,
    ))
}

// ── Spread dispatcher ─────────────────────────────────────────────────────────

fn provide_spread_actions(doc: &Document, range: Range) -> Vec<CodeActionOrCommand> {
    let line_idx  = range.start.line as usize;
    let line_text = match doc.source.lines().nth(line_idx) {
        Some(l) => l,
        None    => return Vec::new(),
    };

    if line_text.trim().len() < 10 { return Vec::new(); }

    let mut actions = Vec::new();

    if let Some(a) = try_spread_table_property(doc, line_idx, line_text) {
        actions.push(CodeActionOrCommand::CodeAction(a));
    }
    if let Some(a) = try_spread_group_array(doc, line_idx, line_text) {
        actions.push(CodeActionOrCommand::CodeAction(a));
    }
    if let Some(a) = try_spread_object_literal(doc, line_idx, line_text) {
        actions.push(CodeActionOrCommand::CodeAction(a));
    }
    if let Some(a) = try_spread_array_literal(doc, line_idx, line_text) {
        actions.push(CodeActionOrCommand::CodeAction(a));
    }

    actions
}

// ── Reformat file ──────────────────────────────────────────────────────────────

fn provide_reformat_action(doc: &Document) -> Option<CodeAction> {
    let formatted = format_source(&doc.source, 2);
    if formatted == doc.source { return None; }

    let line_count = doc.source.lines().count() as u32;
    let last_col   = doc.source.lines().last().map(|l| l.len() as u32).unwrap_or(0);

    let edit = TextEdit {
        range:    Range::new(Position::new(0, 0), Position::new(line_count, last_col)),
        new_text: formatted,
    };

    Some(make_action(
        "⟳ Reformat file",
        CodeActionKind::SOURCE,
        doc.uri.clone(),
        vec![edit],
        false,
    ))
}

// ── Date / Timestamp picker actions ──────────────────────────────────────────

fn date_time_actions(doc: &Document, range: Range) -> Vec<CodeActionOrCommand> {
    let range_start_line = (range.start.line + 1) as usize;
    let range_end_line   = (range.end.line   + 1) as usize;
    let mut actions      = Vec::new();

    for token in &doc.tokens {
        if token.line < range_start_line || token.line > range_end_line { continue; }
        match &token.token_type {
            TokenType::Date(date_str) => {
                actions.extend(make_date_actions(doc, token.line, token.column, date_str));
            }
            TokenType::Timestamp(ts_str) => {
                actions.extend(make_timestamp_actions(doc, token.line, token.column, ts_str));
            }
            _ => {}
        }
    }
    actions
}

fn make_date_actions(
    doc:      &Document,
    line1:    usize,
    col1:     usize,
    date_str: &str,
) -> Vec<CodeActionOrCommand> {
    let (y, m, d) = match parse_date(date_str) { Some(v) => v, None => return vec![] };
    let line  = line1.saturating_sub(1) as u32;
    let col   = col1.saturating_sub(1) as u32;
    let tok_range = Range::new(
        Position::new(line, col),
        Position::new(line, col + date_str.len() as u32),
    );

    let ops: &[(&str, i32, i32, i32)] = &[
        ("📅 Next day",       1,  0,  0),
        ("📅 Previous day",  -1,  0,  0),
        ("📅 Next month",     0,  1,  0),
        ("📅 Previous month", 0, -1,  0),
        ("📅 Next year",      0,  0,  1),
        ("📅 Previous year",  0,  0, -1),
    ];

    ops.iter().map(|(title, dd, dm, dy)| {
        let new_date = apply_date_delta(y, m, d, *dd, *dm, *dy);
        let edit = TextEdit { range: tok_range, new_text: new_date };
        CodeActionOrCommand::CodeAction(make_action(
            title, CodeActionKind::REFACTOR, doc.uri.clone(), vec![edit], false,
        ))
    }).collect()
}

fn make_timestamp_actions(
    doc:    &Document,
    line1:  usize,
    col1:   usize,
    ts_str: &str,
) -> Vec<CodeActionOrCommand> {
    let (date_part, time_suffix) = split_timestamp(ts_str);
    let (y, m, d) = match parse_date(date_part) { Some(v) => v, None => return vec![] };

    let line  = line1.saturating_sub(1) as u32;
    let col   = col1.saturating_sub(1) as u32;
    let tok_range = Range::new(
        Position::new(line, col),
        Position::new(line, col + ts_str.len() as u32),
    );

    let ops: &[(&str, i32, i32, i32)] = &[
        ("🕐 Next day",       1,  0,  0),
        ("🕐 Previous day",  -1,  0,  0),
        ("🕐 Next month",     0,  1,  0),
        ("🕐 Previous month", 0, -1,  0),
        ("🕐 Next year",      0,  0,  1),
        ("🕐 Previous year",  0,  0, -1),
    ];

    ops.iter().map(|(title, dd, dm, dy)| {
        let new_date = apply_date_delta(y, m, d, *dd, *dm, *dy);
        let new_ts   = format!("{}{}", new_date, time_suffix);
        let edit = TextEdit { range: tok_range, new_text: new_ts };
        CodeActionOrCommand::CodeAction(make_action(
            title, CodeActionKind::REFACTOR, doc.uri.clone(), vec![edit], false,
        ))
    }).collect()
}

// ── Date arithmetic helpers ───────────────────────────────────────────────────

fn parse_date(s: &str) -> Option<(i32, u32, u32)> {
    let s = s.trim();
    if s.len() < 10 { return None; }
    let y: i32 = s[0..4].parse().ok()?;
    if s.as_bytes().get(4) != Some(&b'-') { return None; }
    let m: u32 = s[5..7].parse().ok()?;
    if s.as_bytes().get(7) != Some(&b'-') { return None; }
    let d: u32 = s[8..10].parse().ok()?;
    if m < 1 || m > 12 || d < 1 || d > 31 { return None; }
    Some((y, m, d))
}

fn split_timestamp(ts: &str) -> (&str, &str) {
    if let Some(t_pos) = ts.find('T').or_else(|| ts.find('t')) {
        (&ts[..t_pos], &ts[t_pos..])
    } else {
        (ts, "")
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11               => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 30,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn format_date(y: i32, m: u32, d: u32) -> String {
    format!("{:04}-{:02}-{:02}", y, m, d)
}

fn apply_date_delta(y: i32, m: u32, d: u32, dd: i32, dm: i32, dy: i32) -> String {
    let mut year  = y + dy;
    let mut month = m as i32 + dm;
    while month < 1  { month += 12; year -= 1; }
    while month > 12 { month -= 12; year += 1; }
    let month = month as u32;
    let max_day = days_in_month(year, month);
    let day = d.min(max_day);
    let mut day_i = day as i32 + dd;
    let final_month;
    let final_year;
    if day_i < 1 {
        let prev_month = if month == 1 { 12 } else { month - 1 };
        let prev_year  = if month == 1 { year - 1 } else { year };
        day_i += days_in_month(prev_year, prev_month) as i32;
        if day_i < 1 { day_i = 1; }
        final_month = prev_month;
        final_year  = prev_year;
    } else if day_i > max_day as i32 {
        day_i -= max_day as i32;
        let next_month = if month == 12 { 1 } else { month + 1 };
        let next_year  = if month == 12 { year + 1 } else { year };
        if day_i > days_in_month(next_year, next_month) as i32 {
            day_i = days_in_month(next_year, next_month) as i32;
        }
        final_month = next_month;
        final_year  = next_year;
    } else {
        final_month = month;
        final_year  = year;
    }
    format_date(final_year, final_month, day_i.max(1) as u32)
}

// ── Shared action constructor ─────────────────────────────────────────────────

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

// ── Text parsing utilities ────────────────────────────────────────────────────

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
