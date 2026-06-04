// mdix-lsp/src/features/code_actions.rs
//! Code action / quick-fix provider.
//!
//! ## Security quick-fix
//! A "Insert @SECURITY section" code action fires when SEC001 is present.
//!
//! ## Date / Timestamp picker actions
//! When the cursor is on a Date or Timestamp token, increment/decrement
//! actions are offered as a lightweight substitute for a native date picker
//! (LSP has no date-picker protocol equivalent to documentColor).

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

            // Missing @SECURITY — main quick-fix
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

    // ── 2. Proactive: DEncryptor present but no @SECURITY ────────────────────
    if !added_security_insert {
        if let Some(info) = encryptor_without_security(doc) {
            if let Some(action) = fix_insert_security(doc, &info.algorithm) {
                actions.push(CodeActionOrCommand::CodeAction(action));
                if info.algorithm == "xor" {
                    if let Some(xor_fix) = fix_replace_xor_in_dlm(doc) {
                        actions.push(CodeActionOrCommand::CodeAction(xor_fix));
                    }
                }
            }
        }
    }

    // ── 3. Date / Timestamp picker actions ────────────────────────────────────
    actions.extend(date_time_actions(doc, range));

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

fn encryptor_without_security(doc: &Document) -> Option<EncryptorInfo> {
    let ast = doc.ast.as_ref()?;
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
            let sec_algo = match algorithm {
                "aes128-gcm"        => "aes128-gcm",
                "chacha20-poly1305" => "chacha20-poly1305",
                _                   => "aes256-gcm",
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
        "aes256-gcm"        => "AES-256-GCM",
        "aes128-gcm"        => "AES-128-GCM",
        "chacha20-poly1305" => "ChaCha20-Poly1305",
        "xor"               => "XOR (weak)",
        _                   => algorithm,
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

// ── Date / Timestamp picker actions ──────────────────────────────────────────
//
// LSP has no native "date picker" protocol (unlike documentColor which gives
// an inline color wheel). We provide increment/decrement code actions as the
// closest practical equivalent — clicking the lightbulb on a date shows these.

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
    doc:    &Document,
    line1:  usize,
    col1:   usize,
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
    // Parse date portion and keep the time suffix intact
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

/// Parse a `YYYY-MM-DD` string into (year, month, day).
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

/// Split a timestamp into its date portion and everything after (T…).
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

/// Apply day / month / year deltas with correct calendar overflow/underflow.
fn apply_date_delta(y: i32, m: u32, d: u32, dd: i32, dm: i32, dy: i32) -> String {
    // Apply year delta
    let mut year  = y + dy;
    // Apply month delta with year carry
    let mut month = m as i32 + dm;
    while month < 1  { month += 12; year -= 1; }
    while month > 12 { month -= 12; year += 1; }
    let month = month as u32;
    // Clamp day to the month's actual length
    let max_day = days_in_month(year, month);
    let mut day = d.min(max_day);
    // Apply day delta
    let mut day_i = day as i32 + dd;
    loop {
        if day_i < 1 {
            // Underflow: go to previous month
            let prev_month = if month == 1 { 12 } else { month - 1 };
            let prev_year  = if month == 1 { year - 1 } else { year };
            day_i += days_in_month(prev_year, prev_month) as i32;
            // We just need the final day, not recursing months here
            // For simplicity handle only one-step underflow (±1 day)
            if day_i < 1 { day_i = 1; } // clamp to sane value
            break;
        } else if day_i > max_day as i32 {
            day_i -= max_day as i32;
            // Advance month
            month = if month == 12 { 1 } else { month + 1 };
            if month == 1 { year += 1; }
            // For simplicity break after one advance
            if day_i > days_in_month(year, month) as i32 {
                day_i = days_in_month(year, month) as i32;
            }
            break;
        } else {
            break;
        }
    }
    day = day_i.max(1) as u32;
    format_date(year, month, day)
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
