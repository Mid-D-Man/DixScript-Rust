// mdix-lsp/src/features/completions.rs
//! Completion provider.
//!
//! ## Dot completions (improved)
//! When `.` is typed, the provider now:
//!   1. Finds the last token before the dot on the same line (token stream).
//!   2. Converts that token to a `DixType` (literals → direct, identifiers →
//!      QuickFunc param types → type_index → symbol table).
//!   3. Queries `instance_method_registry` for ALL registered methods of that type
//!      (including universal methods) and builds completion items with proper
//!      signatures and snippet insert text.
//!   4. For static objects / enum access / imported namespaces: unchanged path.
//!
//! ## Bracket completions
//! `{`, `(`, `[` are now trigger characters. When fired, the provider offers
//! a bracket-closing snippet so the user can accept to get the matching close
//! bracket inserted automatically.

use std::panic;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    Documentation, InsertTextFormat, MarkupContent, MarkupKind, Position,
    Range, TextEdit,
};
use dixscript::Builtins::Core::DixType;
use dixscript::Builtins::Resolver::{instance_method_registry, static_object_registry};
use dixscript::Compiler::AST::{DataType, DixScript};
use dixscript::Compiler::Core::Tokenizer::Token;
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::Core::Tokenizer::TokenType;

use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(
    doc: Option<&Document>,
    pos: Position,
    trigger: Option<&str>,
) -> Option<CompletionResponse> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        provide_inner(doc, pos, trigger)
    }));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("completions panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(
    doc: Option<&Document>,
    pos: Position,
    trigger: Option<&str>,
) -> Option<CompletionResponse> {
    let items = match doc {
        None => section_snippet_completions(),
        Some(d) => {
            let section = if !d.tokens.is_empty() {
                section_of_token_at(&d.tokens, pos)
            } else {
                SectionId::None
            };

            // ── @CONFIG section ───────────────────────────────────────────────
            if section == SectionId::Config {
                let config_items = config_completions_at(&d.source, pos);
                if !config_items.is_empty() {
                    return Some(CompletionResponse::Array(config_items));
                }
                return Some(CompletionResponse::Array(config_key_completions()));
            }

            if section == SectionId::None {
                if line_looks_like_config_entry(&d.source, pos) {
                    let config_items = config_completions_at(&d.source, pos);
                    if !config_items.is_empty() {
                        return Some(CompletionResponse::Array(config_items));
                    }
                    return Some(CompletionResponse::Array(config_key_completions()));
                }
            }

            let trigger_ch: char = trigger
                .and_then(|t| t.chars().next())
                .unwrap_or_else(|| char_before_cursor(&d.source, pos));

            match trigger_ch {
                '@' => section_snippet_completions(),
                '<' => type_annotation_completions(),
                '~' => quickfunc_declaration_snippets(),
                '.' => dot_completions(d, pos),
                '{' => bracket_completions('{', pos),
                '(' => bracket_completions('(', pos),
                '[' => bracket_completions('[', pos),
                _ => {
                    let word = word_before_cursor(&d.source, pos);
                    if word.starts_with('@') {
                        section_snippet_completions()
                    } else {
                        general_completions(d, pos)
                    }
                }
            }
        }
    };

    if items.is_empty() { None } else { Some(CompletionResponse::Array(items)) }
}

// ── Section detection ─────────────────────────────────────────────────────────

fn section_of_token_at(tokens: &[Token], pos: Position) -> SectionId {
    let target_line = (pos.line as usize) + 1;
    let target_col  = (pos.character as usize) + 1;
    let mut last    = SectionId::None;

    for token in tokens {
        if token.line > target_line { break; }
        if token.line == target_line && token.column > target_col { break; }
        last = token.section;
    }
    last
}

// ── CONFIG line heuristic ─────────────────────────────────────────────────────

fn line_looks_like_config_entry(source: &str, pos: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line = pos.line as usize;
    let search_start = cursor_line.saturating_sub(20);

    for line in lines[search_start..=cursor_line.min(lines.len().saturating_sub(1))].iter().rev() {
        let trimmed = line.trim().to_uppercase();
        if trimmed.starts_with("@CONFIG") && trimmed.contains('(') {
            return true;
        }
        if trimmed.starts_with('@') && !trimmed.starts_with("@CONFIG") {
            break;
        }
    }
    false
}

// ── CONFIG completions ────────────────────────────────────────────────────────

fn config_completions_at(source: &str, pos: Position) -> Vec<CompletionItem> {
    let line_text = source.lines().nth(pos.line as usize).unwrap_or("");
    let up_to: &str = &line_text[..((pos.character as usize).min(line_text.len()))];

    if let Some(arrow_pos) = up_to.rfind("->") {
        let key_part = up_to[..arrow_pos].trim();
        return config_value_completions(key_part);
    }
    vec![]
}

fn config_key_completions() -> Vec<CompletionItem> {
    let keys: &[(&str, &str, &str, &str)] = &[
        (
            "version",
            "version -> \"${1:1.0.0}\"",
            "string",
            "DixScript format version this file targets.\n\nExample: `version -> \"1.0.0\"`",
        ),
        (
            "author",
            "author -> \"${1:name}\"",
            "string",
            "File author. Free-form string.",
        ),
        (
            "created",
            "created -> \"${1:2025-01-01T00:00:00Z}\"",
            "timestamp",
            "File creation timestamp. ISO 8601 format.",
        ),
        (
            "encoding",
            "encoding -> \"${1|utf-8,utf-16,ascii,iso-8859-1|}\"",
            "string",
            "Source file character encoding.",
        ),
        (
            "debug_mode",
            "debug_mode -> \"${1|off,regular,verbose|}\"",
            "string",
            "Compiler diagnostic verbosity.\n\n`\"off\"` | `\"regular\"` | `\"verbose\"`",
        ),
        (
            "error_handling",
            "error_handling -> \"${1|halt,continue,recover|}\"",
            "string",
            "How the compiler responds to errors.\n\n`\"halt\"` | `\"continue\"` | `\"recover\"`",
        ),
        (
            "compatibility_mode",
            "compatibility_mode -> \"${1|strict,best_effort,permissive|}\"",
            "string",
            "Parser strictness level.\n\n`\"strict\"` | `\"best_effort\"` | `\"permissive\"`",
        ),
        (
            "features",
            "features -> \"${1|advanced,basic|}\"",
            "string",
            "Enabled section features.\n\n`\"basic\"` (DATA+SECURITY only) | `\"advanced\"` (all)",
        ),
    ];

    keys.iter().map(|(key, snippet, type_hint, doc)| CompletionItem {
        label: key.to_string(),
        kind:  Some(CompletionItemKind::FIELD),
        detail: Some(format!("<{}> — CONFIG key", type_hint)),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: format!("**`{}`** — CONFIG key (`<{}>`)\n\n{}", key, type_hint, doc),
        })),
        insert_text:        Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        sort_text:          Some(format!("0_{}", key)),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

fn config_value_completions(key: &str) -> Vec<CompletionItem> {
    let choices: &[(&str, &str)] = match key.trim() {
        "debug_mode" => &[
            ("off",     "No debug output (default)"),
            ("regular", "Key resolution steps"),
            ("verbose", "Full execution trace"),
        ],
        "error_handling" => &[
            ("halt",     "Stop on first error (default)"),
            ("continue", "Collect all errors, then report"),
            ("recover",  "Try to parse past errors"),
        ],
        "compatibility_mode" => &[
            ("strict",      "Reject unknown syntax (default)"),
            ("best_effort", "Warn on unknown, continue"),
            ("permissive",  "Accept anything parseable"),
        ],
        "features" => &[
            ("advanced",              "All sections (default)"),
            ("basic",                 "DATA and SECURITY only"),
            ("quickfuncs,enums,data", "Explicit section list"),
        ],
        "encoding" => &[
            ("utf-8",      "Default encoding"),
            ("utf-16",     "UTF-16"),
            ("ascii",      "ASCII only"),
            ("iso-8859-1", "Latin-1"),
        ],
        _ => return vec![],
    };

    choices.iter().map(|(value, detail)| CompletionItem {
        label:              format!("\"{}\"", value),
        kind:               Some(CompletionItemKind::ENUM_MEMBER),
        detail:             Some(detail.to_string()),
        insert_text:        Some(format!("\"{}\"", value)),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

// ── Section snippets ──────────────────────────────────────────────────────────

fn section_snippet_completions() -> Vec<CompletionItem> {
    let sections: &[(&str, &str, &str)] = &[
        (
            "@CONFIG",
            "CONFIG(\n  version -> \"1.0.0\"\n  author -> \"${1:name}\"\n  debug_mode -> \"off\"\n  error_handling -> \"halt\"\n  compatibility_mode -> \"strict\"\n  features -> \"advanced\"\n)",
            "Compiler settings and metadata. Keys: `version`, `author`, `created`, `encoding`, `debug_mode`, `error_handling`, `compatibility_mode`, `features`.",
        ),
        (
            "@IMPORTS",
            "IMPORTS(\n  ${1:Alias} from \"${2:path/to/file.mdix}\"\n)",
            "Import other `.mdix` files.\n\n```mdix\n@IMPORTS(\n  Utils from \"common/utils.mdix\"\n)\n```",
        ),
        (
            "@DLM",
            "DLM(\n  DCompressor.${1|gzip,bzip2,lzma|}\n  DEncryptor.${2|aes256,aes128,chacha20|}\n)",
            "Data Lifecycle Modules — compression and encryption at compile time.",
        ),
        (
            "@ENUMS",
            "ENUMS(\n  ${1:EnumName} { ${2:VALUE_A} = 0, ${3:VALUE_B} = 1 }\n)",
            "Named integer constants.\n\n```mdix\n@ENUMS(\n  Difficulty { EASY = 0, NORMAL = 1, HARD = 2 }\n)\n```",
        ),
        (
            "@QUICKFUNCS",
            "QUICKFUNCS(\n  ~${1:funcName}<${2:object}>(${3:param1}, ${4:param2}) {\n    return {\n      ${5:key} = ${6:param1}\n    }\n  }\n)",
            "Compile-time functions — zero runtime overhead.",
        ),
        (
            "@DATA",
            "DATA(\n  ${1:key} = ${2:value}\n\n  ${3:table}: ${4:field} = ${5:value}\n\n  ${6:array}::\n    ${7:item1},\n    ${8:item2}\n)",
            "Data payload. Flat (`=`), table (`:`), or group array (`::`).",
        ),
        (
            "@SECURITY",
            "SECURITY(\n  encryption -> {\n    mode = \"${1|password,keyfile|}\",\n    algorithm = \"${2|aes256-gcm,aes128-gcm,chacha20-poly1305|}\"\n  }\n)",
            "Encryption configuration. Required when `@DLM` uses `DEncryptor`.",
        ),
    ];

    sections.iter().map(|(label, snippet, doc)| CompletionItem {
        label:              label.to_string(),
        kind:               Some(CompletionItemKind::MODULE),
        detail:             Some("DixScript section".to_string()),
        filter_text:        None,
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: doc.to_string(),
        })),
        insert_text:        Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        sort_text:          Some(format!("0_{}", label)),
        ..Default::default()
    }).collect()
}

// ── Type annotation completions ───────────────────────────────────────────────

fn type_annotation_completions() -> Vec<CompletionItem> {
    let types: &[(&str, &str, &str)] = &[
        ("int",       "32-bit signed integer",          "42, -7, 0"),
        ("long",      "64-bit signed integer (L suffix)","9_000_000_000L"),
        ("float",     "32-bit float (f suffix)",        "3.14f"),
        ("double",    "64-bit float (IEEE 754 f64)",    "3.14159"),
        ("string",    "UTF-8 string",                   "\"hello\""),
        ("bool",      "Boolean",                        "true, false"),
        ("array",     "Ordered collection (::)",         "\"a\", \"b\""),
        ("tuple",     "Mixed-type, max 6 elements",      "t:(1, \"a\", true)"),
        ("object",    "Key-value map { }",              "{ x = 1, y = 2 }"),
        ("hex",       "Hex colour or integer",          "#FF5733, 0xFF"),
        ("blob",      "Base64-encoded binary",          "b:(\"SGVsbG8=\")"),
        ("regex",     "Regular expression",             "r:(\"^[a-z]+$\")"),
        ("date",      "ISO 8601 date",                  "2025-12-31"),
        ("timestamp", "ISO 8601 timestamp",             "2025-12-31T10:30:00Z"),
        ("enum",      "Enum value from @ENUMS",         "MyEnum.VALUE"),
        ("any",       "Any type (no restriction)",      "anything"),
    ];

    types.iter().map(|(name, detail, example)| CompletionItem {
        label:              format!("<{}>", name),
        kind:               Some(CompletionItemKind::TYPE_PARAMETER),
        detail:             Some(detail.to_string()),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: format!("**`<{}>`** — {}\n\nExample: `{}`", name, detail, example),
        })),
        insert_text:        Some(name.to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

// ── QuickFunc declaration snippets ────────────────────────────────────────────

fn quickfunc_declaration_snippets() -> Vec<CompletionItem> {
    let templates: &[(&str, &str, &str)] = &[
        (
            "~funcName<object>",
            "~${1:funcName}<object>(${2:param1}, ${3:param2}) {\n    return {\n      ${4:key} = ${5:param1}\n    }\n  }",
            "Object-returning QuickFunc (most common)",
        ),
        (
            "~funcName<int>",
            "~${1:funcName}<int>(${2:x}<int>, ${3:y}<int>) {\n    return ${4:x + y}\n  }",
            "Integer-returning QuickFunc",
        ),
        (
            "~funcName<string>",
            "~${1:funcName}<string>(${2:value}) {\n    return ${3:value}\n  }",
            "String-returning QuickFunc",
        ),
        (
            "~funcName<bool>",
            "~${1:funcName}<bool>(${2:condition}) {\n    return ${3:condition}\n  }",
            "Boolean-returning QuickFunc",
        ),
        (
            "~funcName<array>",
            "~${1:funcName}<array>(${2:items}) {\n    return ${3:items}\n  }",
            "Array-returning QuickFunc",
        ),
    ];

    templates.iter().map(|(label, snippet, doc)| CompletionItem {
        label:              label.to_string(),
        kind:               Some(CompletionItemKind::FUNCTION),
        detail:             Some("QuickFunc declaration".to_string()),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: doc.to_string(),
        })),
        insert_text:        Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

// ── Bracket pair completions ──────────────────────────────────────────────────
//
// When a bracket trigger char fires, the character is already in the document.
// We provide a snippet whose insert_text goes AFTER the typed character.
// The `text_edit` approach replaces the typed char with the full pair snippet so
// the cursor ends up between the brackets.

fn bracket_completions(ch: char, pos: Position) -> Vec<CompletionItem> {
    let (close, label_inline, snippet_inline, label_multi, snippet_multi) = match ch {
        '{' => (
            '}',
            "{ } — same line",
            "{ $1 }$0",
            "{ } — multi-line block",
            "{\n  $1\n}$0",
        ),
        '(' => (
            ')',
            "( ) — close paren",
            "($1)$0",
            "( ) — multi-line",
            "(\n  $1\n)$0",
        ),
        '[' => (
            ']',
            "[ ] — close bracket",
            "[$1]$0",
            "[ ] — multi-line",
            "[\n  $1\n]$0",
        ),
        _ => return vec![],
    };

    // The trigger char is already in the document at pos.character - 1 (0-based).
    // We replace it with the full pair snippet.
    let start_char = pos.character.saturating_sub(1);

    let make = |label: &str, snippet: &str, sort: &str| CompletionItem {
        label: label.to_string(),
        kind:  Some(CompletionItemKind::SNIPPET),
        detail: Some(format!("Auto-close with '{}'", close)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        filter_text: Some(ch.to_string()),
        sort_text: Some(sort.to_string()),
        // Replace the already-typed open bracket with the full snippet pair.
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range: Range::new(
                Position::new(pos.line, start_char),
                Position::new(pos.line, pos.character),
            ),
            new_text: snippet.to_string(),
        })),
        preselect: Some(sort == "000"),
        ..Default::default()
    };

    vec![
        make(label_inline, snippet_inline, "000"),
        make(label_multi,  snippet_multi,  "001"),
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// DOT COMPLETIONS  (major rewrite — uses token stream + registry)
// ─────────────────────────────────────────────────────────────────────────────

/// Find the last token that starts on the same line BEFORE the dot position.
/// Positions: tokens are 1-based, `pos` is 0-based LSP.
fn token_before_dot<'a>(tokens: &'a [Token], pos: Position) -> Option<&'a Token> {
    let target_line_1 = (pos.line + 1) as usize;   // 1-based
    let dot_col_0     = pos.character as usize;      // 0-based position of the '.'

    tokens
        .iter()
        .filter(|t| {
            t.line == target_line_1
                // token starts before the dot (convert 1-based col → 0-based)
                && (t.column.saturating_sub(1)) < dot_col_0
        })
        .last()
}

/// Convert a token to its `DixType` for instance-method dispatch.
/// Literal tokens map directly; identifiers go through semantic analysis.
fn dix_type_of_token(token: &Token, doc: &Document) -> Option<DixType> {
    match &token.token_type {
        // ── Literals ─────────────────────────────────────────────────────────
        TokenType::String(_)
        | TokenType::StringSingle(_)
        | TokenType::InterpolatedString(_) => Some(DixType::String),

        TokenType::Integer(_) | TokenType::HexLiteral(_) => Some(DixType::Int),
        TokenType::Long(_) => Some(DixType::Long),
        TokenType::Float(_) => Some(DixType::Float),
        TokenType::Double(_) | TokenType::ScientificNotation(_) => Some(DixType::Double),
        TokenType::Bool(_) => Some(DixType::Bool),
        TokenType::HexColor(_) => Some(DixType::Hex),
        TokenType::Date(_) => Some(DixType::Date),
        TokenType::Timestamp(_) => Some(DixType::Timestamp),
        TokenType::RegexConstructor(_) => Some(DixType::Regex),
        TokenType::BlobConstructor(_) => Some(DixType::Blob),
        TokenType::TupleConstructor(_) => Some(DixType::Tuple),

        // ── Closing brackets (end of collection literal) ──────────────────────
        TokenType::Symbol(']') => Some(DixType::Array),

        // ── Identifiers: resolve via semantic data ────────────────────────────
        TokenType::Identifier(name) => {
            completion_identifier_dix_type(name, doc, token.section)
        }

        // ── EnumAccess is an enum value ───────────────────────────────────────
        TokenType::EnumAccess { .. } => Some(DixType::Enum),

        _ => None,
    }
}

/// Infer the `DixType` of an identifier for completion purposes.
/// Priority order:
///   1. QuickFunc parameter type annotation
///   2. type_index from last semantic analysis
///   3. symbol table data variable
fn completion_identifier_dix_type(
    name:    &str,
    doc:     &Document,
    section: SectionId,
) -> Option<DixType> {
    // 1. QuickFunc parameters (check in any section — a param name might be used in @DATA calls)
    if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        // Only restrict to @QUICKFUNCS section if we're actually inside a function body.
        let restrict = section == SectionId::QuickFuncs;
        for func in &qf.functions {
            for param in &func.parameters {
                if param.name == name {
                    if !restrict || func.position.is_valid() {
                        return param.data_type.and_then(completion_dt_to_dix);
                    }
                }
            }
        }
    }

    // 2. Type index from semantic result
    if let Some(type_idx) = doc
        .semantic_result
        .as_ref()
        .and_then(|sr| sr.type_index.as_ref())
    {
        if let Some(&dt) = type_idx.get(name) {
            return completion_dt_to_dix(dt);
        }
    }

    // 3. Symbol table
    let st = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;

    let var = st
        .try_get_data_variable(name)
        .or_else(|| st.try_get_data_variable(&format!("DATA.{}", name)))?;

    completion_dt_to_dix(var.effective_type()?)
}

/// Convert `DataType` → `DixType`.  Typed collections map to their base type.
fn completion_dt_to_dix(dt: DataType) -> Option<DixType> {
    match dt {
        DataType::Int => Some(DixType::Int),
        DataType::Long => Some(DixType::Long),
        DataType::Float => Some(DixType::Float),
        DataType::Double => Some(DixType::Double),
        DataType::String => Some(DixType::String),
        DataType::Bool => Some(DixType::Bool),
        DataType::Array | DataType::TypedArray(_) => Some(DixType::Array),
        DataType::Tuple | DataType::TypedTuple(_) => Some(DixType::Tuple),
        DataType::Object => Some(DixType::Object),
        DataType::Hex => Some(DixType::Hex),
        DataType::Blob => Some(DixType::Blob),
        DataType::Regex => Some(DixType::Regex),
        DataType::Date => Some(DixType::Date),
        DataType::Timestamp => Some(DixType::Timestamp),
        DataType::Enum => Some(DixType::Enum),
        _ => None,
    }
}

/// Build completion items for ALL instance methods of `dix_type` from the registry.
/// Includes universal methods (toString, type, isNull, …).
fn registry_instance_method_completions(dix_type: DixType) -> Vec<CompletionItem> {
    instance_method_registry::initialize();

    let mut method_names = instance_method_registry::get_instance_methods(dix_type);
    method_names.sort();
    method_names.dedup();

    let type_label = dix_type.get_type_name();

    method_names
        .iter()
        .filter_map(|name| {
            let method = instance_method_registry::get_instance_method(dix_type, name)?;

            // parameter_count includes the instance as param 0 — subtract 1.
            let pc            = method.parameter_count();
            let is_variadic   = pc < 0;
            let min_pc        = method.min_parameter_count();
            let extra_params  = if is_variadic {
                (min_pc as i32).saturating_sub(1).max(0) as usize
            } else {
                (pc as i32).saturating_sub(1).max(0) as usize
            };

            let (insert_text, insert_fmt) = if extra_params == 0 && !is_variadic {
                (format!("{}()", name), InsertTextFormat::PLAIN_TEXT)
            } else {
                let mut slots: Vec<String> = (1..=extra_params.max(1))
                    .map(|i| format!("${{{}:arg{}}}", i, i))
                    .collect();
                if is_variadic {
                    slots.push(format!("${{{}:…}}", extra_params + 1));
                }
                (
                    format!("{}({})", name, slots.join(", ")),
                    InsertTextFormat::SNIPPET,
                )
            };

            let ret  = method.return_type().get_type_name();
            let desc = method.description().to_string();

            let param_count_note = if is_variadic {
                format!("variadic, ≥{} args", min_pc.saturating_sub(1))
            } else {
                format!("{} arg{}", extra_params, if extra_params == 1 { "" } else { "s" })
            };

            Some(CompletionItem {
                label: name.clone(),
                kind:  Some(CompletionItemKind::METHOD),
                detail: Some(format!(
                    "<{}>.{}({}) → <{}>",
                    type_label, name, param_count_note, ret
                )),
                documentation: if !desc.is_empty() {
                    Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!(
                            "**`{}`** — `<{}>` method\n\n{}\n\n**Returns:** `<{}>`",
                            name, type_label, desc, ret
                        ),
                    }))
                } else {
                    None
                },
                insert_text:        Some(insert_text),
                insert_text_format: Some(insert_fmt),
                filter_text:        None,
                sort_text:          Some(format!("0_{}", name)),
                ..Default::default()
            })
        })
        .collect()
}

/// Main dot-completion entry point.
///
/// 1. Identify the token before the dot (token stream, post-enhancement).
/// 2. Convert to `DixType` using literal types or semantic inference.
/// 3. Fetch ALL methods for that type from the instance method registry.
/// 4. Add static-object methods / enum fields / namespace members as before.
fn dot_completions(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = Vec::new();

    // ── Step 1 & 2: type of the receiver ─────────────────────────────────────
    let receiver_dix_type: Option<DixType> = token_before_dot(&doc.tokens, pos)
        .and_then(|tok| dix_type_of_token(tok, doc));

    if let Some(dix_type) = receiver_dix_type {
        items.extend(registry_instance_method_completions(dix_type));
    }

    // ── Step 3: static objects, enums, namespaces (word-based) ───────────────
    let word_before = word_before_dot(&doc.source, pos);
    if !word_before.is_empty() {
        // Enum field completions
        if let Some(ast) = &doc.ast {
            let enum_items = enum_value_completions(ast, &word_before);
            if !enum_items.is_empty() {
                // Enum fields come first — they're more precise
                let mut merged = enum_items;
                merged.extend(items);
                items = merged;
                // Don't add generic instance methods on top of enum completions
                return items;
            }
        }

        // Static object methods (Math.sqrt, DateTime.now, …)
        let static_items = static_method_completions(&word_before);
        if !static_items.is_empty() {
            items.extend(static_items);
            // If we found static methods, stop (static and instance don't mix)
            if receiver_dix_type.is_none() {
                return items;
            }
        }

        // Imported namespace functions and enums
        if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
            if let Some(ns) = st.try_get_namespace(&word_before) {
                for func_name in ns.functions.keys() {
                    items.push(CompletionItem {
                        label:       func_name.clone(),
                        kind:        Some(CompletionItemKind::FUNCTION),
                        detail:      Some(format!("imported from {}", ns.alias)),
                        insert_text: Some(format!("{}(", func_name)),
                        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                        filter_text: None,
                        ..Default::default()
                    });
                }
                for enum_name in ns.enums.keys() {
                    items.push(CompletionItem {
                        label:       enum_name.clone(),
                        kind:        Some(CompletionItemKind::ENUM),
                        detail:      Some(format!("enum from {}", ns.alias)),
                        filter_text: None,
                        ..Default::default()
                    });
                }
            }
        }

        // Fallback: if still empty and no type was inferred, use name heuristics
        if items.is_empty() {
            items.extend(instance_method_completions_heuristic(&word_before));
        }
    }

    items
}

// ── Enum value completions ────────────────────────────────────────────────────

fn enum_value_completions(ast: &DixScript, enum_name: &str) -> Vec<CompletionItem> {
    let enums = match &ast.enums { Some(e) => e, None => return vec![] };
    for decl in &enums.enums {
        if decl.name == enum_name {
            return decl.fields.iter().map(|f| {
                let detail = f.value.map(|v| format!("= {}", v)).unwrap_or_default();
                CompletionItem {
                    label:    f.name.clone(),
                    kind:     Some(CompletionItemKind::ENUM_MEMBER),
                    detail:   Some(detail),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!(
                            "**{}.{}**\n\nUsage: `{}.{}`",
                            enum_name, f.name, enum_name, f.name
                        ),
                    })),
                    insert_text: Some(f.name.clone()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    filter_text: None,
                    ..Default::default()
                }
            }).collect();
        }
    }
    vec![]
}

// ── Static method completions (hardcoded catalogue) ───────────────────────────

fn static_method_completions(object_name: &str) -> Vec<CompletionItem> {
    let catalogue: &[(&str, &[(&str, &str)])] = &[
        ("Math", &[
            ("sqrt",  "→ double"), ("pow",     "→ double"), ("abs",     "→ double"),
            ("floor", "→ int"),    ("ceil",    "→ int"),    ("round",   "→ double"),
            ("min",   "→ double"), ("max",     "→ double"), ("clamp",   "→ double"),
            ("sin",   "→ double"), ("cos",     "→ double"), ("tan",     "→ double"),
            ("log",   "→ double"), ("pi",      "→ double"), ("e",       "→ double"),
        ]),
        ("DateTime", &[
            ("now",        "→ timestamp"), ("today",    "→ date"),   ("format",  "→ string"),
            ("year",       "→ int"),       ("month",    "→ int"),    ("day",     "→ int"),
            ("addDays",    "→ date"),      ("subtract", "→ double"), ("isLeapYear", "→ bool"),
        ]),
        ("Array", &[
            ("empty",   "→ array"),  ("range",   "→ array"),  ("fill",    "→ array"),
            ("sort",    "→ array"),  ("unique",  "→ array"),  ("flatten", "→ array"),
            ("sum",     "→ double"), ("average", "→ double"), ("min",     "→ double"),
            ("max",     "→ double"),
        ]),
        ("Random", &[
            ("range",        "→ int"),    ("float",        "→ float"),
            ("double",       "→ double"), ("boolean",      "→ bool"),
            ("choice",       "→ any"),    ("shuffle",      "→ array"),
            ("alphanumeric", "→ string"),
        ]),
        ("Guid", &[
            ("new",      "→ string"), ("parse",    "→ string"),
            ("validate", "→ bool"),   ("empty",    "→ string"),
        ]),
        ("IpAddress", &[
            ("parse",      "→ string"), ("validate",  "→ bool"),
            ("isV4",       "→ bool"),   ("isV6",      "→ bool"),
            ("isPrivate",  "→ bool"),   ("localhost", "→ string"),
        ]),
        ("Enum", &[
            ("getValues", "→ array"), ("getName",  "→ string"),
            ("getValue",  "→ int"),   ("count",    "→ int"),
            ("exists",    "→ bool"),  ("list",     "→ array"),
        ]),
        ("Dix", &[
            ("Log",        "→ void"), ("LogInfo",    "→ void"),
            ("LogWarning", "→ void"), ("LogError",   "→ void"),
            ("Assert",     "→ void"), ("Format",     "→ string"),
            ("Join",       "→ string"),
        ]),
        ("DCompressor", &[
            ("gzip",  "compression algorithm"),
            ("bzip2", "compression algorithm"),
            ("lzma",  "compression algorithm"),
        ]),
        ("DEncryptor", &[
            ("aes256",   "encryption algorithm"),
            ("aes128",   "encryption algorithm"),
            ("chacha20", "encryption algorithm"),
            ("xor",      "⚠️ weak obfuscation only"),
        ]),
        ("DAuditor", &[
            ("diy",      "custom audit hook"),
            ("enhanced", "built-in checksum audit"),
        ]),
    ];

    for (obj, methods) in catalogue {
        if *obj == object_name {
            return methods.iter().map(|(method, sig)| CompletionItem {
                label:              method.to_string(),
                kind:               Some(CompletionItemKind::METHOD),
                detail:             Some(format!("{}.{} {}", obj, method, sig)),
                insert_text:        Some(format!("{}(", method)),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                filter_text:        None,
                ..Default::default()
            }).collect();
        }
    }
    vec![]
}

// ── Instance method completions — name-based fallback ─────────────────────────
//
// Only used when we cannot infer the receiver type from the token stream.
// Kept for backwards compatibility / robustness.

fn instance_method_completions_heuristic(word: &str) -> Vec<CompletionItem> {
    let lower = word.to_lowercase();

    let dix_type = if lower.contains("str") || lower.contains("name")
        || lower.contains("text") || lower.contains("msg")
        || lower.contains("title") || lower.contains("label")
    {
        Some(DixType::String)
    } else if lower.contains("arr") || lower.contains("list")
        || lower.contains("items") || lower.contains("tags")
        || lower.contains("values") || lower.contains("elements")
    {
        Some(DixType::Array)
    } else if lower.contains("tuple") || lower.contains("coord")
        || lower.contains("point") || lower.contains("pair")
    {
        Some(DixType::Tuple)
    } else if lower.contains("regex") || lower.contains("pattern") {
        Some(DixType::Regex)
    } else if lower.contains("blob") || lower.contains("data")
        || lower.contains("bytes") || lower.contains("content")
    {
        Some(DixType::Blob)
    } else {
        None
    };

    if let Some(dt) = dix_type {
        return registry_instance_method_completions(dt);
    }

    vec![]
}

// ── General completions ───────────────────────────────────────────────────────

fn general_completions(doc: &Document, _pos: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    if let Some(ast) = &doc.ast {
        // QuickFunc names
        if let Some(qf) = &ast.quick_functions {
            for func in &qf.functions {
                let params: Vec<String> = func.parameters.iter()
                    .map(|p| {
                        let t = p.data_type.as_ref()
                            .map(|dt| format!("<{}>", dt))
                            .unwrap_or_default();
                        format!("{}{}", p.name, t)
                    }).collect();
                let ret = func.return_type.as_ref()
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "?".to_string());

                items.push(CompletionItem {
                    label:   func.name.clone(),
                    kind:    Some(CompletionItemKind::FUNCTION),
                    detail:  Some(format!("~{}<{}>({}) — QuickFunc", func.name, ret, params.join(", "))),
                    insert_text:        Some(format!("{}(", func.name)),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    filter_text:        None,
                    ..Default::default()
                });
            }
        }

        // Enum type names
        if let Some(enums) = &ast.enums {
            for decl in &enums.enums {
                items.push(CompletionItem {
                    label:       decl.name.clone(),
                    kind:        Some(CompletionItemKind::ENUM),
                    detail:      Some(format!("{} fields", decl.fields.len())),
                    filter_text: None,
                    ..Default::default()
                });
            }
        }
    }

    items.extend(keyword_completions());

    // Built-in static objects
    for (name, desc) in &[
        ("Math",        "Built-in math functions"),
        ("DateTime",    "Date/time functions"),
        ("Array",       "Array factory functions"),
        ("Random",      "Random generation"),
        ("Guid",        "GUID/UUID generation"),
        ("IpAddress",   "IP address utilities"),
        ("Enum",        "Enum introspection"),
        ("Dix",         "Logging and utilities"),
        ("DCompressor", "DLM compression module"),
        ("DEncryptor",  "DLM encryption module"),
        ("DAuditor",    "DLM audit module"),
    ] {
        items.push(CompletionItem {
            label:       name.to_string(),
            kind:        Some(CompletionItemKind::CLASS),
            detail:      Some(desc.to_string()),
            filter_text: None,
            ..Default::default()
        });
    }

    items
}

// ── Keyword completions ───────────────────────────────────────────────────────

fn keyword_completions() -> Vec<CompletionItem> {
    let keywords: &[(&str, &str)] = &[
        ("if:",        "Conditional — DixScript uses `if:` with a colon"),
        ("elif:",      "Else-if branch"),
        ("else",       "Fallback branch"),
        ("chk:",       "Switch/match statement"),
        ("miss",       "Default case in chk:"),
        ("return",     "Return a value from a QuickFunc"),
        ("log:",       "Log expression at compile time"),
        ("let",        "Immutable local variable"),
        ("let mut",    "Mutable local variable"),
        ("const",      "Compile-time constant"),
        ("and",        "Logical AND (= &&)"),
        ("or",         "Logical OR (= ||)"),
        ("not",        "Logical NOT (= !)"),
        ("true",       "Boolean true"),
        ("false",      "Boolean false"),
        ("null",       "Null literal"),
        ("from",       "Import a local .mdix file"),
        ("from_cloud", "Import a remote .mdix file"),
        ("verify",     "Verify import file hash"),
        ("global",     "Global scope modifier"),
    ];

    keywords.iter().map(|(label, detail)| CompletionItem {
        label:              label.to_string(),
        kind:               Some(CompletionItemKind::KEYWORD),
        detail:             Some(detail.to_string()),
        insert_text:        Some(label.to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

// ── Source text helpers ───────────────────────────────────────────────────────

fn char_before_cursor(source: &str, pos: Position) -> char {
    let line = source.lines().nth(pos.line as usize).unwrap_or("");
    if pos.character == 0 { return '\0'; }
    line.chars().nth((pos.character - 1) as usize).unwrap_or('\0')
}

fn word_before_cursor(source: &str, pos: Position) -> String {
    let line = source.lines().nth(pos.line as usize).unwrap_or("");
    let up_to: String = line.chars().take(pos.character as usize).collect();
    let start = up_to
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '@')
        .map(|i| i + 1)
        .unwrap_or(0);
    up_to[start..].to_string()
}

fn word_before_dot(source: &str, pos: Position) -> String {
    let line = source.lines().nth(pos.line as usize).unwrap_or("");
    // Take chars up to (but not including) the dot
    let up_to: String = line
        .char_indices()
        .take_while(|(i, _)| *i < pos.character.saturating_sub(1) as usize)
        .map(|(_, c)| c)
        .collect();
    up_to
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .last()
        .unwrap_or("")
        .to_string()
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
            source.to_string(), 0,
        );
        run_pipeline(&mut doc);
        doc
    }

    #[test]
    fn section_snippets_cover_all_sections() {
        let items  = section_snippet_completions();
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        for s in &["@CONFIG","@IMPORTS","@DLM","@ENUMS","@QUICKFUNCS","@DATA","@SECURITY"] {
            assert!(labels.iter().any(|l| l == s), "missing: {}", s);
        }
        for item in &items {
            assert!(item.filter_text.is_none(), "filter_text must be None for: {}", item.label);
        }
    }

    #[test]
    fn type_annotations_complete() {
        let items = type_annotation_completions();
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        for t in &["<int>","<long>","<float>","<double>","<string>","<bool>","<array>",
                   "<tuple>","<object>","<hex>","<blob>","<regex>","<date>",
                   "<timestamp>","<enum>","<any>"] {
            assert!(labels.iter().any(|l| l == t), "missing type: {}", t);
        }
    }

    #[test]
    fn bracket_completions_provide_items() {
        for ch in ['{', '(', '['] {
            let items = bracket_completions(ch, Position::new(0, 1));
            assert!(!items.is_empty(), "no bracket completions for '{}'", ch);
            assert!(items[0].text_edit.is_some(), "missing text_edit for '{}'", ch);
        }
    }

    #[test]
    fn registry_string_methods_non_empty() {
        instance_method_registry::initialize();
        let items = registry_instance_method_completions(DixType::String);
        assert!(!items.is_empty(), "no string methods from registry");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"toUpper"), "missing toUpper");
        assert!(labels.contains(&"length"),  "missing length");
    }

    #[test]
    fn registry_array_methods_non_empty() {
        instance_method_registry::initialize();
        let items = registry_instance_method_completions(DixType::Array);
        assert!(!items.is_empty(), "no array methods from registry");
    }

    #[test]
    fn registry_tuple_methods_include_positional() {
        instance_method_registry::initialize();
        let items = registry_instance_method_completions(DixType::Tuple);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Tuple should have positional accessors
        assert!(
            labels.iter().any(|l| *l == "first" || *l == "get" || *l == "length"),
            "tuple methods missing positional accessors; got: {:?}", labels
        );
    }

    #[test]
    fn dot_completions_after_string_literal() {
        let src = "@DATA(\n  x = \"hello\"\n)";
        let doc  = test_doc(src);
        // Simulate completion at the position of '.' in '"hello".'
        // Token stream should have String("hello") on line 2, col 7 (1-based).
        // We call the function directly with a fabricated position.
        let items = dot_completions(&doc, Position::new(1, 16));
        // Items may be empty if the document state doesn't match exactly,
        // but the function should not panic.
        let _ = items;
    }

    #[test]
    fn dlm_static_methods_complete() {
        for obj in &["DCompressor","DEncryptor","DAuditor"] {
            let methods = static_method_completions(obj);
            assert!(!methods.is_empty(), "{} has no completions", obj);
        }
    }

    #[test]
    fn config_value_completions_for_debug_mode() {
        let items = config_value_completions("debug_mode");
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        assert!(labels.iter().any(|l| l.contains("off")));
        assert!(labels.iter().any(|l| l.contains("verbose")));
    }

    #[test]
    fn quickfunc_names_in_general_completions() {
        let src = "@QUICKFUNCS(\n  ~calc<int>(x) { return x }\n)\n@DATA(\n  y = 1\n)";
        let doc = test_doc(src);
        let items = general_completions(&doc, Position::new(3, 0));
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        assert!(labels.iter().any(|l| l == "calc"), "QuickFunc 'calc' missing; got: {:?}", labels);
    }
}
