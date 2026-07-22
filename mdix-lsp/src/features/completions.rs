//! Completion provider.

use std::collections::HashMap;
use std::panic;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    Documentation, InsertTextFormat, MarkupContent, MarkupKind, Position,
    Range, TextEdit,
};
use dixscript::Builtins::Core::DixType;
use dixscript::Builtins::Resolver::{instance_method_registry, static_object_registry};
use dixscript::Compiler::AST::{
    DataEntry, DataType, DeclarationType, DixScript, Expression, QuickFuncStatement,
    QuickFunction, TypeInferenceVisitor, Value,
};
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

            // @CONFIG
            if section == SectionId::Config {
                let config_items = config_completions_at(&d.source, pos);
                if !config_items.is_empty() {
                    return Some(CompletionResponse::Array(config_items));
                }
                return Some(CompletionResponse::Array(config_key_completions()));
            }

            // @SECURITY
            if section == SectionId::Security {
                let sec_items = security_completions_at(&d.source, pos);
                if !sec_items.is_empty() {
                    return Some(CompletionResponse::Array(sec_items));
                }
                return Some(CompletionResponse::Array(security_block_key_completions()));
            }

            // @DLM
            if section == SectionId::Dlm {
                let dlm_items = dlm_completions_at(&d.source, pos);
                if !dlm_items.is_empty() {
                    return Some(CompletionResponse::Array(dlm_items));
                }
                let word = word_before_cursor(&d.source, pos);
                let mut items = dlm_module_type_completions();
                if !word.is_empty() {
                    let lower = word.to_lowercase();
                    items.retain(|item| item.label.to_lowercase().contains(&lower));
                }
                if !items.is_empty() {
                    return Some(CompletionResponse::Array(items));
                }
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
        ("version",            "version -> \"${1:1.0.0}\"",                         "string",    "DixScript format version.\n\nExample: `version -> \"1.0.0\"`"),
        ("author",             "author -> \"${1:name}\"",                           "string",    "File author. Free-form string."),
        ("created",            "created -> \"${1:2025-01-01T00:00:00Z}\"",          "timestamp", "File creation timestamp. ISO 8601 format."),
        ("encoding",           "encoding -> \"${1|utf-8,utf-16,ascii,iso-8859-1|}\"","string",   "Source file character encoding."),
        ("debug_mode",         "debug_mode -> \"${1|off,regular,verbose|}\"",       "string",    "Compiler diagnostic verbosity."),
        ("error_handling",     "error_handling -> \"${1|halt,continue,recover|}\"", "string",    "How the compiler responds to errors."),
        ("compatibility_mode", "compatibility_mode -> \"${1|strict,best_effort,permissive|}\"", "string", "Parser strictness level."),
        ("features",           "features -> \"${1|advanced,basic|}\"",              "string",    "Enabled section features."),
    ];
    keys.iter().map(|(key, snippet, type_hint, doc)| CompletionItem {
        label:              key.to_string(),
        kind:               Some(CompletionItemKind::FIELD),
        detail:             Some(format!("<{}> — CONFIG key", type_hint)),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
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
        "debug_mode"         => &[("off","No debug output (default)"),("regular","Key resolution steps"),("verbose","Full execution trace")],
        "error_handling"     => &[("halt","Stop on first error (default)"),("continue","Collect all errors, then report"),("recover","Try to parse past errors")],
        "compatibility_mode" => &[("strict","Reject unknown syntax (default)"),("best_effort","Warn on unknown, continue"),("permissive","Accept anything parseable")],
        "features"           => &[("advanced","All sections (default)"),("basic","DATA and SECURITY only"),("quickfuncs,enums,data","Explicit section list")],
        "encoding"           => &[("utf-8","Default encoding"),("utf-16","UTF-16"),("ascii","ASCII only"),("iso-8859-1","Latin-1")],
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

// ── @SECURITY section completions ─────────────────────────────────────────────

fn find_enclosing_security_block(source: &str, cursor_line: u32) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor = cursor_line as usize;
    let mut depth = 0i32;

    for i in (0..=cursor.min(lines.len().saturating_sub(1))).rev() {
        let line = lines[i];
        let closes = line.chars().filter(|&c| c == '}').count() as i32;
        let opens  = line.chars().filter(|&c| c == '{').count() as i32;
        depth += closes - opens;

        if depth < 0 {
            if let Some(arrow_pos) = line.find("->") {
                let block_name = line[..arrow_pos].trim();
                if !block_name.is_empty()
                    && !block_name.contains('@')
                    && !block_name.contains('(')
                    && !block_name.contains('/')
                {
                    return Some(block_name.to_string());
                }
            }
            return None;
        }
    }
    None
}

fn security_completions_at(source: &str, pos: Position) -> Vec<CompletionItem> {
    let line_text = source.lines().nth(pos.line as usize).unwrap_or("");
    let up_to = &line_text[..((pos.character as usize).min(line_text.len()))];

    if let Some(eq_pos) = up_to.rfind('=') {
        let before_eq = &up_to[..eq_pos];
        if !before_eq.contains("->") {
            let field_name = before_eq.trim().to_string();
            let items = security_field_value_completions(&field_name);
            if !items.is_empty() { return items; }
        }
    }

    if let Some(block) = find_enclosing_security_block(source, pos.line) {
        return security_field_key_completions(&block);
    }

    vec![]
}

fn security_block_key_completions() -> Vec<CompletionItem> {
    let blocks: &[(&str, &str, &str)] = &[
        (
            "encryption",
            concat!(
                "encryption -> {\n",
                "  mode      = \"${1|keyfile,password|}\",\n",
                "  algorithm = \"${2|aes256-gcm,aes128-gcm,chacha20-poly1305|}\"\n",
                "}"
            ),
            "Encryption configuration. Required when `@DLM` includes a `DEncryptor` module.",
        ),
        (
            "validation",
            concat!(
                "validation -> {\n",
                "  checksum_algorithm = \"${1|sha256,sha512|}\",\n",
                "  auth_tag_length    = ${2:128},\n",
                "  hmac_algorithm     = \"${3|hmac-sha256,hmac-sha512|}\"\n",
                "}"
            ),
            "Content integrity and authentication tag settings.",
        ),
        (
            "keystore",
            concat!(
                "keystore -> {\n",
                "  auto_generate = ${1:true},\n",
                "  backup_count  = ${2:3},\n",
                "  backup_naming = \"${3|timestamp,sequence|}\"\n",
                "}"
            ),
            "Key file management. Set `auto_generate = true` so the compiler produces a `.mdix.key` file.",
        ),
    ];

    blocks.iter().map(|(key, snippet, doc)| CompletionItem {
        label:              key.to_string(),
        kind:               Some(CompletionItemKind::MODULE),
        detail:             Some("@SECURITY block".to_string()),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: format!("**`{}`** — @SECURITY block\n\n{}", key, doc),
        })),
        insert_text:        Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        sort_text:          Some(format!("0_{}", key)),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

fn security_field_key_completions(block_name: &str) -> Vec<CompletionItem> {
    let fields: &[(&str, &str, &str)] = match block_name.to_lowercase().as_str() {
        "encryption" => &[
            ("mode",            "\"${1|keyfile,password|}\"",               "How the key is supplied."),
            ("algorithm",       "\"${1|aes256-gcm,aes128-gcm,chacha20-poly1305,xor|}\"", "Encryption algorithm."),
            ("key_length",      "${1:32}",                                  "Key length in bytes."),
            ("kdf",             "\"${1|argon2id,argon2i,argon2d|}\"",       "Key derivation function."),
            ("kdf_memory",      "${1:65536}",                               "Argon2 memory cost in KiB."),
            ("kdf_iterations",  "${1:3}",                                   "Argon2 iteration count."),
            ("kdf_parallelism", "${1:4}",                                   "Argon2 parallelism factor."),
        ],
        "validation" => &[
            ("checksum_algorithm", "\"${1|sha256,sha512|}\"",              "Hash algorithm for content-integrity check."),
            ("auth_tag_length",    "${1:128}",                              "Authentication tag length in bits."),
            ("hmac_algorithm",     "\"${1|hmac-sha256,hmac-sha512|}\"",    "HMAC algorithm for message authentication."),
        ],
        "keystore" => &[
            ("auto_generate", "${1:true}",                                  "When `true`, auto-generates a `.mdix.key` file."),
            ("backup_count",  "${1:3}",                                     "Number of previous key file backups to retain."),
            ("backup_naming", "\"${1|timestamp,sequence|}\"",               "Naming convention for backup key files."),
        ],
        _ => &[],
    };

    fields.iter().map(|(key, snippet, doc)| {
        CompletionItem {
            label:              key.to_string(),
            kind:               Some(CompletionItemKind::FIELD),
            detail:             Some(format!("{} field", block_name)),
            documentation:      Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: format!("**`{}`** — `{}` entry\n\n{}", key, block_name, doc),
            })),
            insert_text:        Some(format!("{} = {}", key, snippet)),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            sort_text:          Some(format!("0_{}", key)),
            filter_text:        None,
            ..Default::default()
        }
    }).collect()
}

fn security_field_value_completions(field_name: &str) -> Vec<CompletionItem> {
    let values: &[(&str, &str)] = match field_name.trim() {
        "mode" => &[
            ("keyfile",  "Compiler auto-generates a `.mdix.key` file — recommended"),
            ("password", "Compiler prompts for a passphrase at compile time"),
        ],
        "algorithm" => &[
            ("aes256-gcm",        "AES-256-GCM — recommended"),
            ("aes128-gcm",        "AES-128-GCM — faster, slightly smaller keys"),
            ("chacha20-poly1305", "ChaCha20-Poly1305 — excellent on mobile / ARM"),
            ("xor",               "XOR — obfuscation only, NOT cryptographically secure"),
        ],
        "kdf" => &[
            ("argon2id", "Argon2id — recommended"),
            ("argon2i",  "Argon2i"),
            ("argon2d",  "Argon2d"),
        ],
        "backup_naming" => &[
            ("timestamp", "Embed a timestamp in the backup key filename"),
            ("sequence",  "Use an incrementing sequence number"),
        ],
        "checksum_algorithm" => &[
            ("sha256", "SHA-256 — recommended"),
            ("sha512", "SHA-512 — stronger, larger output"),
        ],
        "hmac_algorithm" => &[
            ("hmac-sha256", "HMAC-SHA256 — recommended"),
            ("hmac-sha512", "HMAC-SHA512"),
        ],
        "auto_generate" => &[
            ("true",  "Automatically write `.mdix.key` during compilation"),
            ("false", "Require the key file to exist already"),
        ],
        _ => &[],
    };

    values.iter().map(|(val, detail)| CompletionItem {
        label:              format!("\"{}\"", val),
        kind:               Some(CompletionItemKind::ENUM_MEMBER),
        detail:             Some(detail.to_string()),
        insert_text:        Some(format!("\"{}\"", val)),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

// ── @DLM section completions ──────────────────────────────────────────────────
//
// `@DLM(...)` entries are `ModuleType ("." ModuleSubtype)?` — see
// dlm_section_parser.rs. There are exactly three module types, and each one
// only accepts a specific subset of subtypes (checked against the actual
// Auditor/Compressor/Encryptor implementations, not just the parser's shared
// subtype token list):
//   DCompressor -> gzip | bzip2 | lzma
//   DEncryptor  -> xor | aes128 | aes256 | chacha20
//   DAuditor    -> diy | enhanced
//
// Previously there was no dedicated completion path for this section at all,
// so typing inside `@DLM(` fell through to `general_completions`, and typing
// `DAuditor.` fell through to `dot_completions`'s word-based fallback chain —
// neither of which knows about DLM modules, so nothing useful ever appeared.

fn dlm_completions_at(source: &str, pos: Position) -> Vec<CompletionItem> {
    if char_before_cursor(source, pos) == '.' {
        let module_name = word_before_dot(source, pos);
        let items = dlm_subtype_completions(&module_name);
        if !items.is_empty() {
            return items;
        }
    }
    vec![]
}

fn dlm_module_type_completions() -> Vec<CompletionItem> {
    let modules: &[(&str, &str)] = &[
        ("DCompressor", "DLM compression module — subtypes: gzip, bzip2, lzma"),
        ("DEncryptor",  "DLM encryption module — subtypes: xor, aes128, aes256, chacha20"),
        ("DAuditor",    "DLM audit module — subtypes: diy, enhanced"),
    ];
    modules.iter().map(|(name, detail)| CompletionItem {
        label:              name.to_string(),
        kind:               Some(CompletionItemKind::CLASS),
        detail:             Some(detail.to_string()),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: format!("**`{}`** — DLM module type\n\n{}", name, detail),
        })),
        insert_text:        Some(name.to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        filter_text:        None,
        sort_text:          Some(format!("0_{}", name)),
        ..Default::default()
    }).collect()
}

fn dlm_subtype_completions(module_name: &str) -> Vec<CompletionItem> {
    let subtypes: &[(&str, &str)] = match module_name {
        "DCompressor" => &[
            ("gzip",  "DEFLATE-based compression — default balance of speed/ratio"),
            ("bzip2", "Block-sorting compression — higher ratio, slower"),
            ("lzma",  "Highest compression ratio, slowest"),
        ],
        "DEncryptor" => &[
            ("xor",      "XOR cipher — fast, NOT cryptographically secure"),
            ("aes128",   "AES-128 symmetric encryption, 128-bit key"),
            ("aes256",   "AES-256 symmetric encryption, 256-bit key (recommended)"),
            ("chacha20", "ChaCha20 stream cipher"),
        ],
        "DAuditor" => &[
            ("diy",      "Minimal/custom audit trail format"),
            ("enhanced", "Full structured audit trail with richer metadata"),
        ],
        _ => return vec![],
    };
    subtypes.iter().map(|(name, detail)| CompletionItem {
        label:              name.to_string(),
        kind:               Some(CompletionItemKind::ENUM_MEMBER),
        detail:             Some(detail.to_string()),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: format!("**`{}.{}`**\n\n{}", module_name, name, detail),
        })),
        insert_text:        Some(name.to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

// ── Section snippets ──────────────────────────────────────────────────────────

fn section_snippet_completions() -> Vec<CompletionItem> {
    let sections: &[(&str, &str, &str)] = &[
        ("@CONFIG",     "CONFIG(\n  version -> \"1.0.0\"\n  author -> \"${1:name}\"\n  debug_mode -> \"off\"\n  error_handling -> \"halt\"\n  compatibility_mode -> \"strict\"\n  features -> \"advanced\"\n)",    "Compiler settings and metadata."),
        ("@IMPORTS",    "IMPORTS(\n  ${1:Alias} from \"${2:path/to/file.mdix}\"\n)",                                                                                                                                   "Import other `.mdix` files."),
        ("@DLM",        "DLM(\n  DCompressor.${1|gzip,bzip2,lzma|}\n  DEncryptor.${2|aes256,aes128,chacha20|}\n)",                                                                                                    "Data Lifecycle Modules."),
        ("@ENUMS",      "ENUMS(\n  ${1:EnumName} { ${2:VALUE_A} = 0, ${3:VALUE_B} = 1 }\n)",                                                                                                                          "Named integer constants."),
        ("@QUICKFUNCS", "QUICKFUNCS(\n  ~${1:funcName}<${2:object}>(${3:param1}, ${4:param2}) {\n    return {\n      ${5:key} = ${6:param1}\n    }\n  }\n)",                                                           "Compile-time functions."),
        ("@DATA",       "DATA(\n  ${1:key} = ${2:value}\n\n  ${3:table}: ${4:field} = ${5:value}\n\n  ${6:array}::\n    ${7:item1},\n    ${8:item2}\n)",                                                              "Data payload."),
        ("@SECURITY",   "SECURITY(\n  encryption -> {\n    mode = \"${1|password,keyfile|}\",\n    algorithm = \"${2|aes256-gcm,aes128-gcm,chacha20-poly1305|}\"\n  }\n  keystore -> {\n    auto_generate = true\n  }\n)", "@SECURITY section — required when @DLM includes DEncryptor."),
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
        ("int",       "32-bit signed integer",       "42, -7, 0"),
        ("long",      "64-bit signed integer (L)",   "9_000_000_000L"),
        ("float",     "32-bit float (f suffix)",     "3.14f"),
        ("double",    "64-bit float (f64)",          "3.14159"),
        ("string",    "UTF-8 string",                "\"hello\""),
        ("bool",      "Boolean",                     "true, false"),
        ("array",     "Ordered collection",          "\"a\", \"b\""),
        ("tuple",     "Mixed-type, max 6 elements",  "t:(1, \"a\", true)"),
        ("object",    "Key-value map { }",           "{ x = 1, y = 2 }"),
        ("hex",       "Hex colour or integer",       "#FF5733, 0xFF"),
        ("blob",      "Base64-encoded binary",       "b:(\"SGVsbG8=\")"),
        ("regex",     "Regular expression",          "r:(\"^[a-z]+$\")"),
        ("date",      "ISO 8601 date",               "2025-12-31"),
        ("timestamp", "ISO 8601 timestamp",          "2025-12-31T10:30:00Z"),
        ("enum",      "Enum value from @ENUMS",      "MyEnum.VALUE"),
        ("any",       "Any type (no restriction)",   "anything"),
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
        ("~funcName<object>", "~${1:funcName}<object>(${2:param1}, ${3:param2}) {\n    return {\n      ${4:key} = ${5:param1}\n    }\n  }", "Object-returning QuickFunc"),
        ("~funcName<int>",    "~${1:funcName}<int>(${2:x}<int>, ${3:y}<int>) {\n    return ${4:x + y}\n  }",                                "Integer-returning QuickFunc"),
        ("~funcName<string>", "~${1:funcName}<string>(${2:value}) {\n    return ${3:value}\n  }",                                           "String-returning QuickFunc"),
        ("~funcName<bool>",   "~${1:funcName}<bool>(${2:condition}) {\n    return ${3:condition}\n  }",                                     "Boolean-returning QuickFunc"),
        ("~funcName<array>",  "~${1:funcName}<array>(${2:items}) {\n    return ${3:items}\n  }",                                            "Array-returning QuickFunc"),
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

fn bracket_completions(ch: char, pos: Position) -> Vec<CompletionItem> {
    let (close, label_inline, snippet_inline, label_multi, snippet_multi) = match ch {
        '{' => ('}', "{ } — same line",    "{ $1 }$0",        "{ } — multi-line block", "{\n  $1\n}$0"),
        '(' => (')', "( ) — close paren",  "($1)$0",          "( ) — multi-line",       "(\n  $1\n)$0"),
        '[' => (']', "[ ] — close bracket","[$1]$0",          "[ ] — multi-line",       "[\n  $1\n]$0"),
        _ => return vec![],
    };
    let start_char = pos.character.saturating_sub(1);
    let make = |label: &str, snippet: &str, sort: &str| CompletionItem {
        label: label.to_string(),
        kind:  Some(CompletionItemKind::SNIPPET),
        detail: Some(format!("Auto-close with '{}'", close)),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        filter_text: Some(ch.to_string()),
        sort_text: Some(sort.to_string()),
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
    vec![make(label_inline, snippet_inline, "000"), make(label_multi, snippet_multi, "001")]
}

// ─────────────────────────────────────────────────────────────────────────────
// LOCAL VARIABLE TYPE LOOKUP
// ─────────────────────────────────────────────────────────────────────────────

fn find_local_var_dt_in_stmts(stmts: &[QuickFuncStatement], name: &str) -> Option<DataType> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration { variable_name, data_type, value, .. }
                if *variable_name == name =>
            {
                if let Some(dt) = data_type {
                    return Some(*dt);
                }
                return infer_datatype_from_expr_simple(value);
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(dt) = find_local_var_dt_in_stmts(then_branch, name) { return Some(dt); }
                if let Some(eb) = else_branch {
                    if let Some(dt) = find_local_var_dt_in_stmts(eb, name) { return Some(dt); }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(dt) = find_local_var_dt_in_stmts(&case.statements, name) { return Some(dt); }
                }
                if let Some(dc) = default_case {
                    if let Some(dt) = find_local_var_dt_in_stmts(&dc.statements, name) { return Some(dt); }
                }
            }
            _ => {}
        }
    }
    None
}

fn infer_datatype_from_expr_simple(expr: &Expression) -> Option<DataType> {
    match expr {
        Expression::Value { value, .. } => infer_datatype_from_value_simple(value),
        Expression::Parenthesized { expression, .. } => infer_datatype_from_expr_simple(expression),
        _ => None,
    }
}

fn infer_datatype_from_value_simple(value: &Value) -> Option<DataType> {
    match value {
        Value::Integer { .. }                                       => Some(DataType::Int),
        Value::Long { .. }                                          => Some(DataType::Long),
        Value::Float { .. }                                         => Some(DataType::Float),
        Value::Double { .. } | Value::ScientificNotation { .. }    => Some(DataType::Double),
        Value::String { .. } | Value::InterpolatedString { .. }    => Some(DataType::String),
        Value::Boolean { .. }                                       => Some(DataType::Bool),
        Value::HexColor { .. }                                      => Some(DataType::Hex),
        Value::Date { .. }                                          => Some(DataType::Date),
        Value::Timestamp { .. }                                     => Some(DataType::Timestamp),
        Value::EnumValue { .. }                                     => Some(DataType::Enum),
        Value::Object { .. }                                        => Some(DataType::Object),
        Value::Array { .. } | Value::NestedArray { .. }            => Some(DataType::Array),
        Value::PrefixedConstructor { prefix, .. } => match prefix.as_str() {
            "t" => Some(DataType::Tuple),
            "b" => Some(DataType::Blob),
            "r" => Some(DataType::Regex),
            _   => None,
        },
        Value::Expression { expr, .. } => infer_datatype_from_expr_simple(expr),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OBJECT PROPERTY TYPE RESOLUTION
// ─────────────────────────────────────────────────────────────────────────────

fn resolve_object_property_type(
    doc:       &Document,
    obj_name:  &str,
    prop_name: &str,
) -> Option<DixType> {
    let qf = doc.ast.as_ref()?.quick_functions.as_ref()?;

    for func in &qf.functions {
        if let Some(obj_value) = find_object_literal_for_var(&func.body, obj_name) {
            if let Value::Object { properties, .. } = obj_value {
                for prop in properties {
                    if prop.key == prop_name {
                        let dt = infer_datatype_from_value_simple(&prop.value)?;
                        return completion_dt_to_dix(dt);
                    }
                }
            }
        }
    }
    None
}

fn find_object_literal_for_var<'a>(
    stmts:    &'a [QuickFuncStatement],
    var_name: &str,
) -> Option<&'a Value> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration { variable_name, value, .. }
                if *variable_name == var_name =>
            {
                return extract_object_value(value);
            }
            QuickFuncStatement::Assignment { variable, value, .. }
                if *variable == var_name =>
            {
                return extract_object_value(value);
            }
            // `object: Value` here (not `Expression`), unlike Assignment/
            // VariableDeclaration above — the parser doesn't currently emit
            // this variant, but every other AST pass (semantic analyzer,
            // interpreter, call collector) already treats it as live, so we
            // handle it defensively too rather than silently dropping it.
            QuickFuncStatement::ObjectCreation { variable, object, .. }
                if *variable == var_name =>
            {
                return match object {
                    Value::Object { .. } => Some(object),
                    _ => None,
                };
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(v) = find_object_literal_for_var(then_branch, var_name) { return Some(v); }
                if let Some(eb) = else_branch {
                    if let Some(v) = find_object_literal_for_var(eb, var_name) { return Some(v); }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(v) = find_object_literal_for_var(&case.statements, var_name) { return Some(v); }
                }
                if let Some(dc) = default_case {
                    if let Some(v) = find_object_literal_for_var(&dc.statements, var_name) { return Some(v); }
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_object_value(expr: &Expression) -> Option<&Value> {
    match expr {
        Expression::Value { value: v @ Value::Object { .. }, .. } => Some(v),
        Expression::Parenthesized { expression, .. } => extract_object_value(expression),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OBJECT USER-PROPERTY COMPLETIONS
// ─────────────────────────────────────────────────────────────────────────────

fn make_field_completion(key: &str, detail: Option<&str>) -> CompletionItem {
    CompletionItem {
        label:              key.to_string(),
        kind:               Some(CompletionItemKind::FIELD),
        detail:             detail.as_ref().map(|s| s.to_string()),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: format!("**`{}`**{}", key,
                detail.map(|d| format!(" — `{}`", d)).unwrap_or_default()),
        })),
        insert_text:        Some(key.to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        sort_text:          Some(format!("000_{}", key)),
        filter_text:        None,
        ..Default::default()
    }
}

fn object_user_properties_completions(doc: &Document, var_name: &str) -> Vec<CompletionItem> {
    // 1. QuickFunc local variable
    if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        for func in &qf.functions {
            if let Some(Value::Object { properties, .. }) =
                find_object_literal_for_var(&func.body, var_name)
            {
                return properties.iter().map(|prop| {
                    let detail = infer_datatype_from_value_simple(&prop.value)
                        .map(|dt| format!("<{}>", dt));
                    make_field_completion(&prop.key, detail.as_deref())
                }).collect();
            }
        }
    }

    // 2. @DATA ObjectProperty
    if let Some(data) = doc.ast.as_ref().and_then(|a| a.data.as_ref()) {
        for entry in &data.entries {
            match entry {
                DataEntry::ObjectProperty { name, object, .. } if *name == var_name => {
                    if let Value::Object { properties, .. } = object.as_ref() {
                        return properties.iter().map(|prop| {
                            let detail = infer_datatype_from_value_simple(&prop.value)
                                .map(|dt| format!("<{}>", dt));
                            make_field_completion(&prop.key, detail.as_deref())
                        }).collect();
                    }
                }
                // 3. @DATA SimpleProperty whose value is a QF call
                DataEntry::SimpleProperty { name, value, .. } if *name == var_name => {
                    let qf_name = match value {
                        Value::QuickFuncCall { function_name, .. } => Some(function_name.clone()),
                        Value::Expression { expr, .. } => {
                            if let Expression::QuickFuncCall { name: fn_name, .. } = expr.as_ref() {
                                Some(fn_name.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(fname) = qf_name {
                        let props = quickfunc_return_object_properties(doc, &fname);
                        if !props.is_empty() { return props; }
                    }
                }
                _ => {}
            }
        }
    }

    vec![]
}

fn quickfunc_return_object_properties(doc: &Document, func_name: &str) -> Vec<CompletionItem> {
    let qf = match doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        Some(q) => q,
        None => return vec![],
    };
    let func = match qf.functions.iter().find(|f| f.name == func_name) {
        Some(f) => f,
        None => return vec![],
    };
    collect_object_props_from_stmts(&func.body)
}

fn collect_object_props_from_stmts(stmts: &[QuickFuncStatement]) -> Vec<CompletionItem> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::Return { value, .. } => {
                if let Expression::Value { value: Value::Object { properties, .. }, .. } = value {
                    return properties.iter().map(|prop| {
                        let detail = infer_datatype_from_value_simple(&prop.value)
                            .map(|dt| format!("<{}>", dt));
                        make_field_completion(&prop.key, detail.as_deref())
                    }).collect();
                }
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                let props = collect_object_props_from_stmts(then_branch);
                if !props.is_empty() { return props; }
                if let Some(eb) = else_branch {
                    let props = collect_object_props_from_stmts(eb);
                    if !props.is_empty() { return props; }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    let props = collect_object_props_from_stmts(&case.statements);
                    if !props.is_empty() { return props; }
                }
                if let Some(dc) = default_case {
                    let props = collect_object_props_from_stmts(&dc.statements);
                    if !props.is_empty() { return props; }
                }
            }
            _ => {}
        }
    }
    vec![]
}

fn func_name_from_paren_close(tokens: &[Token], close_idx: usize) -> Option<String> {
    let mut depth = 0i32;
    let mut i = close_idx;

    loop {
        match &tokens[i].token_type {
            TokenType::Symbol(')') => depth += 1,
            TokenType::Symbol('(') => {
                depth -= 1;
                if depth == 0 && i > 0 {
                    let mut j = i - 1;
                    while j > 0 {
                        match &tokens[j].token_type {
                            TokenType::Symbol('>') => {
                                while j > 0 && !matches!(&tokens[j].token_type, TokenType::Symbol('<')) {
                                    j -= 1;
                                }
                                if j > 0 { j -= 1; }
                            }
                            TokenType::Identifier(name) => {
                                return Some(name.clone());
                            }
                            _ => break,
                        }
                    }
                    if let TokenType::Identifier(name) = &tokens[j].token_type {
                        return Some(name.clone());
                    }
                    return None;
                }
            }
            _ => {}
        }
        if i == 0 { break; }
        i -= 1;
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// RECURSIVE OBJECT SHAPE RESOLUTION
// ─────────────────────────────────────────────────────────────────────────────

/// A field of an object literal, carrying its raw AST `Value` so that
/// sub-property resolution can recurse into nested objects.
struct FieldInfo {
    name:  String,
    value: Value,
}

/// Recursively resolve the object fields of the expression whose last token
/// is at global index `tok_idx` in the full token stream.
///
/// Handles all chain patterns:
///   • `a`          – direct variable lookup
///   • `a.b`        – recurse into owner `a`, descend into field `b`
///   • `a.b.c`      – arbitrary depth nesting
///   • `func()`     – QuickFunc return object
///   • `func().b`   – field of a function return value
///
/// Returns `None` when the expression is not object-typed or its shape
/// cannot be statically determined.
fn get_object_fields(
    tokens:  &[Token],
    tok_idx: usize,
    doc:     &Document,
) -> Option<Vec<FieldInfo>> {
    let tok  = tokens.get(tok_idx)?;
    let line = tok.line;

    match &tok.token_type {
        TokenType::Identifier(name) => {
            // Check: is this identifier a property access (`owner.name`)?
            // It is when preceded by `.` on the same line.
            let preceded_by_dot = tok_idx
                .checked_sub(1)
                .and_then(|i| tokens.get(i))
                .map(|t| t.line == line && matches!(t.token_type, TokenType::Symbol('.')))
                .unwrap_or(false);

            if preceded_by_dot && tok_idx >= 2 {
                // Owner expression ends at tok_idx - 2 (tok_idx - 1 is the dot).
                if tokens.get(tok_idx - 2).map(|t| t.line == line).unwrap_or(false) {
                    // Recursively get owner's fields, then descend into `name`.
                    let owner_fields = get_object_fields(tokens, tok_idx - 2, doc)?;
                    for field in &owner_fields {
                        if field.name == *name {
                            return extract_object_fields_from_value(&field.value, doc);
                        }
                    }
                    return None; // `name` found but not an object
                }
            }

            // Direct variable — look up in AST.
            find_variable_object_fields(doc, name)
        }

        TokenType::Symbol(')') => {
            // Function / method call result.
            let fname = func_name_from_paren_close(tokens, tok_idx)?;
            find_quickfunc_return_fields(doc, &fname)
        }

        _ => None,
    }
}

/// Bound on identifier-alias chasing (`let a = b` where `b` is itself an
/// object elsewhere). Purely a safety net against pathological/circular
/// aliases (`@DATA a = b`, `@DATA b = a`) — real code never nests this deep.
const MAX_ALIAS_DEPTH: u8 = 8;

/// Find the object fields of `var_name` by searching QuickFunc bodies and
/// `@DATA` entries.
fn find_variable_object_fields(doc: &Document, var_name: &str) -> Option<Vec<FieldInfo>> {
    find_variable_object_fields_bounded(doc, var_name, MAX_ALIAS_DEPTH)
}

fn find_variable_object_fields_bounded(
    doc:      &Document,
    var_name: &str,
    depth:    u8,
) -> Option<Vec<FieldInfo>> {
    if depth == 0 {
        return None;
    }

    // 1. QuickFunc local variable (let / assignment to an object literal)
    if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        for func in &qf.functions {
            if let Some(val) = find_object_literal_for_var(&func.body, var_name) {
                if let Value::Object { properties, .. } = val {
                    return Some(
                        properties.iter()
                            .map(|p| FieldInfo { name: p.key.clone(), value: p.value.clone() })
                            .collect(),
                    );
                }
            }
        }
    }

    // 2. @DATA entries
    if let Some(data) = doc.ast.as_ref().and_then(|a| a.data.as_ref()) {
        for entry in &data.entries {
            match entry {
                DataEntry::SimpleProperty { name, value, .. } if *name == var_name => {
                    return extract_object_fields_from_value_bounded(value, doc, depth - 1);
                }
                DataEntry::ObjectProperty { name, object, .. } if *name == var_name => {
                    if let Value::Object { properties, .. } = object.as_ref() {
                        return Some(
                            properties.iter()
                                .map(|p| FieldInfo { name: p.key.clone(), value: p.value.clone() })
                                .collect(),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    None
}

/// Return the fields produced by QuickFunc `func_name`'s `return { … }`.
fn find_quickfunc_return_fields(doc: &Document, func_name: &str) -> Option<Vec<FieldInfo>> {
    let qf   = doc.ast.as_ref()?.quick_functions.as_ref()?;
    let func = qf.functions.iter().find(|f| f.name == func_name)?;
    find_return_object_fields_in_stmts(&func.body)
}

/// Recursively search `stmts` for the first `return { … }` and extract its fields.
fn find_return_object_fields_in_stmts(stmts: &[QuickFuncStatement]) -> Option<Vec<FieldInfo>> {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::Return { value, .. } => {
                if let Expression::Value {
                    value: Value::Object { properties, .. },
                    ..
                } = value
                {
                    return Some(
                        properties.iter()
                            .map(|p| FieldInfo { name: p.key.clone(), value: p.value.clone() })
                            .collect(),
                    );
                }
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                if let Some(r) = find_return_object_fields_in_stmts(then_branch) { return Some(r); }
                if let Some(eb) = else_branch {
                    if let Some(r) = find_return_object_fields_in_stmts(eb) { return Some(r); }
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    if let Some(r) = find_return_object_fields_in_stmts(&case.statements) { return Some(r); }
                }
                if let Some(dc) = default_case {
                    if let Some(r) = find_return_object_fields_in_stmts(&dc.statements) { return Some(r); }
                }
            }
            _ => {}
        }
    }
    None
}

/// If `value` is (or delegates to) an object, return its field list.
fn extract_object_fields_from_value(value: &Value, doc: &Document) -> Option<Vec<FieldInfo>> {
    extract_object_fields_from_value_bounded(value, doc, MAX_ALIAS_DEPTH)
}

fn extract_object_fields_from_value_bounded(
    value: &Value,
    doc:   &Document,
    depth: u8,
) -> Option<Vec<FieldInfo>> {
    if depth == 0 {
        return None;
    }
    match value {
        Value::Object { properties, .. } => Some(
            properties.iter()
                .map(|p| FieldInfo { name: p.key.clone(), value: p.value.clone() })
                .collect(),
        ),
        Value::QuickFuncCall { function_name, .. } => {
            find_quickfunc_return_fields(doc, function_name)
        }
        // Alias: the property's value is itself just a reference to another
        // variable, e.g. `{ user = otherVar }`. Chase it so `thing.user.`
        // still offers `otherVar`'s shape instead of dead-ending here.
        Value::Identifier { value: name, .. } => {
            find_variable_object_fields_bounded(doc, name, depth - 1)
        }
        Value::Expression { expr, .. } => {
            match expr.as_ref() {
                Expression::QuickFuncCall { name, .. } => find_quickfunc_return_fields(doc, name),
                Expression::Identifier { name, .. } => {
                    find_variable_object_fields_bounded(doc, name, depth - 1)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Convert a `FieldInfo` list into LSP completion items.
fn fields_to_completions(fields: &[FieldInfo], _doc: &Document) -> Vec<CompletionItem> {
    fields.iter().map(|f| {
        let detail = infer_datatype_from_value_simple(&f.value)
            .map(|dt| format!("<{}>", dt));
        make_field_completion(&f.name, detail.as_deref())
    }).collect()
}

/// Infer the `DixType` of field `field_name` in the return object of QuickFunc
/// `func_name`. Used by `chain_dix_type` for the `func().prop.` pattern.
fn infer_field_type_in_quickfunc_return(
    doc:        &Document,
    func_name:  &str,
    field_name: &str,
) -> Option<DixType> {
    let fields = find_quickfunc_return_fields(doc, func_name)?;
    for field in &fields {
        if field.name == field_name {
            return infer_datatype_from_value_simple(&field.value)
                .and_then(|dt| completion_dt_to_dix(dt));
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// CHAINED CALL TYPE RESOLUTION
// ─────────────────────────────────────────────────────────────────────────────

fn resolve_paren_close_dix_type(
    tokens:    &[Token],
    close_idx: usize,
    doc:       &Document,
) -> Option<DixType> {
    let mut depth: i32 = 0;
    let mut i = close_idx;

    loop {
        let is_close = matches!(&tokens[i].token_type, TokenType::Symbol(')'));
        let is_open  = matches!(&tokens[i].token_type, TokenType::Symbol('('));

        if is_close {
            depth += 1;
        } else if is_open {
            depth -= 1;
            if depth == 0 {
                if i == 0 { return None; }
                let pre    = i - 1;
                let pre_tt = tokens[pre].token_type.clone();

                return match pre_tt {
                    TokenType::Identifier(name) => {
                        if pre >= 2 {
                            if matches!(&tokens[pre - 1].token_type, TokenType::Symbol('.')) {
                                return resolve_dot_call_type(tokens, pre - 2, &name, doc);
                            }
                        }
                        resolve_direct_call_return(&name, doc)
                    }
                    TokenType::Symbol(':') => {
                        if pre >= 1 {
                            if let TokenType::Identifier(prefix) = tokens[pre - 1].token_type.clone() {
                                return match prefix.as_str() {
                                    "t" => Some(DixType::Tuple),
                                    "b" => Some(DixType::Blob),
                                    "r" => Some(DixType::Regex),
                                    _   => None,
                                };
                            }
                        }
                        None
                    }
                    TokenType::Symbol(')') => resolve_paren_close_dix_type(tokens, pre, doc),
                    _ => None,
                };
            }
        }

        if i == 0 { break; }
        i -= 1;
    }

    None
}

fn resolve_dot_call_type(
    tokens:       &[Token],
    receiver_idx: usize,
    method_name:  &str,
    doc:          &Document,
) -> Option<DixType> {
    if let TokenType::Identifier(recv_name) = tokens[receiver_idx].token_type.clone() {
        static_object_registry::initialize_static_registry();
        if static_object_registry::has_static_object(&recv_name) {
            return static_object_registry::get_method_info(&recv_name, method_name)
                .and_then(|info| {
                    TypeInferenceVisitor::convert_dix_type_to_data_type(info.return_type)
                        .and_then(completion_dt_to_dix)
                });
        }
    }

    let recv_dix = get_token_dix_type_for_chain(tokens, receiver_idx, doc)?;

    instance_method_registry::initialize();
    if let Some(method) = instance_method_registry::get_instance_method(recv_dix, method_name) {
        let ret = method.return_type();
        return match ret {
            DixType::Void | DixType::Null => None,
            DixType::Any => {
                if is_same_type_returning_method(method_name) { Some(recv_dix) } else { None }
            }
            dt => TypeInferenceVisitor::convert_dix_type_to_data_type(dt).and_then(completion_dt_to_dix),
        };
    }

    None
}

fn get_token_dix_type_for_chain(tokens: &[Token], idx: usize, doc: &Document) -> Option<DixType> {
    match tokens[idx].token_type.clone() {
        TokenType::Identifier(name) =>
            completion_identifier_dix_type(&name, doc, tokens[idx].section),
        TokenType::String(_) | TokenType::StringSingle(_) | TokenType::InterpolatedString(_) =>
            Some(DixType::String),

        TokenType::Long(_)   => Some(DixType::Long),
        TokenType::Float(_)  => Some(DixType::Float),
        TokenType::Double(_) | TokenType::ScientificNotation(_) => Some(DixType::Double),
        TokenType::Bool(_)   => Some(DixType::Bool),
        TokenType::HexColor(_) => Some(DixType::Hex),
        TokenType::Date(_)     => Some(DixType::Date),
        TokenType::Timestamp(_) => Some(DixType::Timestamp),
        TokenType::Symbol(']')  => Some(DixType::Array),
        TokenType::Symbol(')')  => resolve_paren_close_dix_type(tokens, idx, doc),
        TokenType::TupleConstructor(_) => Some(DixType::Tuple),
        TokenType::BlobConstructor(_)  => Some(DixType::Blob),
        TokenType::RegexConstructor(_) => Some(DixType::Regex),
        _ => None,
    }
}

fn resolve_direct_call_return(name: &str, doc: &Document) -> Option<DixType> {
    if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        for func in &qf.functions {
            if func.name == name {
                return func.return_type.and_then(completion_dt_to_dix);
            }
        }
    }
    if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        if let Some(sig) = st.try_get_function(name) {
            return sig.return_type.and_then(completion_dt_to_dix);
        }
    }
    None
}

fn is_same_type_returning_method(name: &str) -> bool {
    matches!(
        name,
        | "toUpper" | "toLower" | "trim" | "trimStart" | "trimEnd"
        | "replace" | "replaceAll" | "padLeft" | "padRight"
        | "sort" | "reverse" | "shuffle" | "distinct" | "filter"
        | "concat" | "flatten" | "push" | "unshift"
        | "clone" | "defaultIfNull" | "defaultIfEmpty"
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// TOKEN-BEFORE-DOT HELPERS
// ─────────────────────────────────────────────────────────────────────────────

fn token_before_dot_with_idx<'a>(
    tokens: &'a [Token],
    pos:    Position,
) -> Option<(&'a Token, usize)> {
    let target_line_1 = (pos.line + 1) as usize;
    let dot_col_0     = pos.character as usize;

    tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            t.line == target_line_1
                && (t.column.saturating_sub(1)) < dot_col_0
                && !matches!(t.token_type, TokenType::Symbol('.'))
        })
        .last()
        .map(|(i, t)| (t, i))
}

// ─────────────────────────────────────────────────────────────────────────────
// TOKEN-TO-DIXTYPE MAPPING
// ─────────────────────────────────────────────────────────────────────────────

fn dix_type_of_token(token: &Token, doc: &Document) -> Option<DixType> {
    match &token.token_type {
        TokenType::String(_) | TokenType::StringSingle(_) | TokenType::InterpolatedString(_) =>
            Some(DixType::String),

        TokenType::Long(_)    => Some(DixType::Long),
        TokenType::Float(_)   => Some(DixType::Float),
        TokenType::Double(_) | TokenType::ScientificNotation(_) => Some(DixType::Double),
        TokenType::Bool(_)    => Some(DixType::Bool),
        TokenType::HexColor(_) => Some(DixType::Hex),
        TokenType::Date(_)     => Some(DixType::Date),
        TokenType::Timestamp(_) => Some(DixType::Timestamp),
        TokenType::RegexConstructor(_) => Some(DixType::Regex),
        TokenType::BlobConstructor(_)  => Some(DixType::Blob),
        TokenType::TupleConstructor(_) => Some(DixType::Tuple),

        TokenType::Symbol(']') => Some(DixType::Array),

        TokenType::Identifier(name) =>
            completion_identifier_dix_type(name, doc, token.section),
        _ => None,
    }
}

fn completion_identifier_dix_type(
    name:    &str,
    doc:     &Document,
    section: SectionId,
) -> Option<DixType> {
    // 1. QuickFunc parameter type annotation
    if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
        for func in &qf.functions {
            for param in &func.parameters {
                if param.name == name {
                    return param.data_type.and_then(completion_dt_to_dix);
                }
            }
        }
    }

    // 2. QuickFunc body local variable declaration
    if section == SectionId::QuickFuncs {
        if let Some(qf) = doc.ast.as_ref().and_then(|a| a.quick_functions.as_ref()) {
            for func in &qf.functions {
                if let Some(dt) = find_local_var_dt_in_stmts(&func.body, name) {
                    return completion_dt_to_dix(dt);
                }
            }
        }
    }

    // 3. Type index from semantic result
    if let Some(type_idx) = doc
        .semantic_result
        .as_ref()
        .and_then(|sr| sr.type_index.as_ref())
    {
        if let Some(&dt) = type_idx.get(name) {
            return completion_dt_to_dix(dt);
        }
    }

    // 4. Symbol table DATA variable
    let st = doc.semantic_result.as_ref()?.symbol_table.as_ref()?;
    let var = st
        .try_get_data_variable(name)
        .or_else(|| st.try_get_data_variable(&format!("DATA.{}", name)))?;
    completion_dt_to_dix(var.effective_type()?)
}

fn completion_dt_to_dix(dt: DataType) -> Option<DixType> {
    match dt {
        DataType::Int                              => Some(DixType::Int),
        DataType::Long                             => Some(DixType::Long),
        DataType::Float                            => Some(DixType::Float),
        DataType::Double                           => Some(DixType::Double),
        DataType::String                           => Some(DixType::String),
        DataType::Bool                             => Some(DixType::Bool),
        DataType::Array | DataType::TypedArray(_)  => Some(DixType::Array),
        DataType::Tuple | DataType::TypedTuple(_)  => Some(DixType::Tuple),
        DataType::Object                           => Some(DixType::Object),
        DataType::Hex                              => Some(DixType::Hex),
        DataType::Blob                             => Some(DixType::Blob),
        DataType::Regex                            => Some(DixType::Regex),
        DataType::Date                             => Some(DixType::Date),
        DataType::Timestamp                        => Some(DixType::Timestamp),
        DataType::Enum                             => Some(DixType::Enum),
        _                                          => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// REGISTRY-BACKED INSTANCE METHOD COMPLETIONS
// ─────────────────────────────────────────────────────────────────────────────

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

            let pc           = method.parameter_count();
            let is_variadic  = pc < 0;
            let min_pc       = method.min_parameter_count();
            let extra_params = if is_variadic {
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
                (format!("{}({})", name, slots.join(", ")), InsertTextFormat::SNIPPET)
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
                            "**`{}`** — `<{}>` instance method\n\n{}\n\n**Returns:** `<{}>`",
                            name, type_label, desc, ret
                        ),
                    }))
                } else {
                    None
                },
                insert_text:        Some(insert_text),
                insert_text_format: Some(insert_fmt),
                filter_text:        None,
                sort_text:          Some(format!("1_{}", name)),
                ..Default::default()
            })
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// DOT COMPLETIONS  (main entry point for `.` trigger)
// ─────────────────────────────────────────────────────────────────────────────

fn dot_completions(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = Vec::new();

    // ── Step 1: identify the receiver token AND its index ────────────────────
    let receiver_with_idx = token_before_dot_with_idx(&doc.tokens, pos);

    // ── NEW: Compute the recursive object shape for the receiver ──────────────
    //
    // get_object_fields handles all chain patterns via recursion:
    //   • a.         – direct variable
    //   • a.b.       – nested object property      (was broken before)
    //   • func().    – QuickFunc return object
    //   • func().b.  – property of a return value  (was broken before)
    //   • a.b.c.     – arbitrary depth nesting      (was broken before)
    let object_shape: Vec<CompletionItem> = receiver_with_idx
        .as_ref()
        .and_then(|(_, idx)| get_object_fields(&doc.tokens, *idx, doc))
        .map(|fields| fields_to_completions(&fields, doc))
        .unwrap_or_default();

    // ── Step 2: chain DixType — handles a.prop. AND func().prop. ─────────────
    //
    // Uses recv_idx directly (not second_token_before_dot) so we can also
    // handle `)` as the owner token for the `func().prop.` pattern.
    let chain_dix_type: Option<DixType> =
        receiver_with_idx.as_ref().and_then(|(recv, recv_idx)| {
            let prop_name = match &recv.token_type {
                TokenType::Identifier(n) => n,
                _ => return None,
            };
            let recv_line = recv.line;
            let tokens    = &doc.tokens;

            // Receiver must be preceded by `.` on the same line to be a property.
            let has_dot_before = recv_idx
                .checked_sub(1)
                .and_then(|i| tokens.get(i))
                .map(|t| t.line == recv_line && matches!(t.token_type, TokenType::Symbol('.')))
                .unwrap_or(false);

            if !has_dot_before || *recv_idx < 2 {
                return None;
            }

            let owner_tok = tokens.get(*recv_idx - 2).filter(|t| t.line == recv_line)?;

            match &owner_tok.token_type {
                TokenType::Identifier(obj_name) => {
                    // a.prop — look up prop's type via the owner's shape.
                    resolve_object_property_type(doc, obj_name, prop_name)
                        .or_else(|| {
                            static_object_registry::get_method_info(obj_name, prop_name)
                                .and_then(|info| {
                                    TypeInferenceVisitor::convert_dix_type_to_data_type(
                                        info.return_type,
                                    )
                                    .and_then(|dt| completion_dt_to_dix(dt))
                                })
                        })
                }
                TokenType::Symbol(')') => {
                    // func().prop — infer prop's DixType from the return object.
                    let owner_idx = *recv_idx - 2;
                    let func_dt   = resolve_paren_close_dix_type(tokens, owner_idx, doc);
                    if func_dt == Some(DixType::Object) {
                        func_name_from_paren_close(tokens, owner_idx)
                            .and_then(|fname| {
                                infer_field_type_in_quickfunc_return(doc, &fname, prop_name)
                            })
                            // We know the owner returns Object; fall back so registry
                            // methods for Object still appear even if we can't infer
                            // the precise field type.
                            .or(Some(DixType::Object))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        });

    // ── Step 3: resolve receiver DixType (with chained-call support) ─────────
    let receiver_dix_type: Option<DixType> = chain_dix_type
        .or_else(|| {
            receiver_with_idx.as_ref().and_then(|(tok, idx)| {
                match &tok.token_type {
                    TokenType::Symbol(')') => {
                        resolve_paren_close_dix_type(&doc.tokens, *idx, doc)
                    }
                    TokenType::Symbol(']') => Some(DixType::Array),
                    _ => dix_type_of_token(tok, doc),
                }
            })
        })
        // If the recursive shape resolver found an object but the type system
        // couldn't determine the type (e.g. 3+ levels deep), treat as Object so
        // that Object registry methods are still offered alongside the shape items.
        .or_else(|| {
            if !object_shape.is_empty() { Some(DixType::Object) } else { None }
        });

    // ── Step 4: registry instance methods ────────────────────────────────────
    if let Some(dix_type) = receiver_dix_type {
        items.extend(registry_instance_method_completions(dix_type));
    }

    // ── Step 5: user-defined object properties (prepended, highest priority) ──
    //
    // object_shape is already fully resolved by get_object_fields above.
    // Prepend before registry methods so user fields appear first.
    if !object_shape.is_empty() {
        let registry_items = std::mem::replace(&mut items, object_shape);
        items.extend(registry_items);
    }

    // ── Step 6: static objects, enums, namespaces (word-based) ───────────────
    let word_before = word_before_dot(&doc.source, pos);
    if !word_before.is_empty() {
        // Enum field completions (local file's own @ENUMS section)
        if let Some(ast) = &doc.ast {
            let enum_items = enum_value_completions(ast, &word_before);
            if !enum_items.is_empty() {
                let mut merged = enum_items;
                merged.extend(items);
                return merged;
            }
        }

        // Imported (namespaced) enum field completions — `Namespace.EnumName.`
        //
        // Mirrors the compiler's own resolution rule for qualified identifiers
        // (see QualifiedIdentifierType::ImportedEnumAccess in
        // quickfuncs_section_analyzer.rs): a 3-part chain `ns.enum.value` is
        // only an enum access once `ns` is a confirmed imported namespace.
        // `word_before` here is just "EnumName" — we walk back through the
        // token stream to confirm it's actually `Namespace.EnumName.` and not
        // a bare `EnumName.` before trusting the namespace lookup.
        if let Some((_, recv_idx)) = receiver_with_idx.as_ref() {
            let imported_enum_items =
                imported_enum_value_completions(doc, &doc.tokens, *recv_idx, &word_before);
            if !imported_enum_items.is_empty() {
                let mut merged = imported_enum_items;
                merged.extend(items);
                return merged;
            }
        }

        // Static object methods
        let static_items = static_method_completions(&word_before);
        if !static_items.is_empty() {
            items.extend(static_items);
            if receiver_dix_type.is_none() {
                return items;
            }
        }

        // Imported namespace functions and enums
        if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
            if let Some(ns) = st.try_get_namespace(&word_before) {
                for (func_name, func_info) in &ns.functions {
                    let ret = func_info.signature.return_type
                        .map(|t| format!("{}", t))
                        .unwrap_or_else(|| "?".to_string());
                    let params: Vec<String> = func_info.signature.parameters.iter()
                        .map(|p| {
                            let t = p.param_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
                            format!("{}{}", p.name, t)
                        }).collect();
                    items.push(CompletionItem {
                        label:       func_name.clone(),
                        kind:        Some(CompletionItemKind::FUNCTION),
                        detail:      Some(format!("{}({}) → <{}>", func_name, params.join(", "), ret)),
                        insert_text: Some(format!("{}(", func_name)),
                        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                        filter_text: None,
                        documentation: Some(Documentation::MarkupContent(MarkupContent {
                            kind:  MarkupKind::Markdown,
                            value: format!(
                                "**`{ns}.{name}<{ret}>({params})`** — imported QuickFunc\n\nFile: `{file}`",
                                ns     = word_before,
                                name   = func_name,
                                ret    = ret,
                                params = params.join(", "),
                                file   = ns.file_path,
                            ),
                        })),
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

        // Fallback name-heuristic when no type was resolved
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
                        value: format!("**{}.{}**\n\nUsage: `{}.{}`", enum_name, f.name, enum_name, f.name),
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

/// Enum value completions for an **imported** (namespaced) enum, e.g.
/// `Namespace.Color.` — the third level that plain `enum_value_completions`
/// can't reach because it only looks at the local file's own `@ENUMS`.
///
/// `enum_recv_idx` is the token index of the enum-name identifier itself
/// (i.e. the receiver just before the trigger dot — "Color" in
/// `Namespace.Color.`). We walk back two tokens to confirm the chain is
/// actually `Namespace . Color .` and that `Namespace` resolves to a real
/// imported namespace in the symbol table before trusting the lookup —
/// otherwise `Foo.Bar.` where `Bar` merely happens to share a name with some
/// unrelated imported enum would incorrectly surface completions.
fn imported_enum_value_completions(
    doc:           &Document,
    tokens:        &[Token],
    enum_recv_idx: usize,
    enum_name:     &str,
) -> Vec<CompletionItem> {
    if enum_recv_idx < 2 {
        return vec![];
    }

    let dot_idx = enum_recv_idx - 1;
    let is_dot = tokens.get(dot_idx)
        .map(|t| matches!(t.token_type, TokenType::Symbol('.')))
        .unwrap_or(false);
    if !is_dot {
        return vec![];
    }

    let ns_idx = enum_recv_idx - 2;
    let ns_token = match tokens.get(ns_idx) {
        Some(t) => t,
        None => return vec![],
    };
    // All three tokens (namespace, dot, enum name) must be on the same
    // source line to be a genuine chain rather than coincidental adjacency.
    if ns_token.line != tokens[enum_recv_idx].line {
        return vec![];
    }
    let ns_alias = match &ns_token.token_type {
        TokenType::Identifier(name) => name.clone(),
        _ => return vec![],
    };

    let st = match doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        Some(st) => st,
        None => return vec![],
    };
    let fields = match st.get_namespaced_enum(&ns_alias, enum_name) {
        Some(f) => f,
        None => return vec![],
    };

    let mut sorted: Vec<(&String, &i32)> = fields.iter().collect();
    sorted.sort_by_key(|(_, v)| **v);

    sorted.into_iter().map(|(field_name, value)| CompletionItem {
        label:              field_name.clone(),
        kind:               Some(CompletionItemKind::ENUM_MEMBER),
        detail:             Some(format!("= {}", value)),
        documentation:      Some(Documentation::MarkupContent(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: format!(
                "**{ns}.{enum_name}.{field}**\n\nUsage: `{ns}.{enum_name}.{field}`\n\nImported from `{ns}`.",
                ns = ns_alias, enum_name = enum_name, field = field_name,
            ),
        })),
        insert_text:        Some(field_name.clone()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

// ── Static method completions ─────────────────────────────────────────────────

fn static_method_completions(object_name: &str) -> Vec<CompletionItem> {
    static_object_registry::initialize_static_registry();
    if static_object_registry::has_static_object(object_name) {
        let method_names = static_object_registry::get_method_names(object_name);
        if !method_names.is_empty() {
            return method_names.iter().filter_map(|method_name| {
                let info = static_object_registry::get_method_info(object_name, method_name)?;
                let param_count = info.parameter_count.max(0) as usize;
                let (insert_text, insert_fmt) = if param_count == 0 {
                    (format!("{}()", method_name), InsertTextFormat::PLAIN_TEXT)
                } else {
                    let slots: Vec<String> = (1..=param_count)
                        .map(|i| format!("${{{}:arg{}}}", i, i))
                        .collect();
                    (format!("{}({})", method_name, slots.join(", ")), InsertTextFormat::SNIPPET)
                };
                Some(CompletionItem {
                    label:   method_name.clone(),
                    kind:    Some(CompletionItemKind::METHOD),
                    detail:  Some(format!("{}.{} → <{}>", object_name, method_name, info.return_type.get_type_name())),
                    documentation: if !info.description.is_empty() {
                        Some(Documentation::MarkupContent(MarkupContent {
                            kind:  MarkupKind::Markdown,
                            value: format!("**`{}.{}`**\n\n{}\n\n**Returns:** `<{}>`",
                                object_name, method_name, info.description, info.return_type.get_type_name()),
                        }))
                    } else { None },
                    insert_text:        Some(insert_text),
                    insert_text_format: Some(insert_fmt),
                    filter_text:        None,
                    ..Default::default()
                })
            }).collect();
        }
    }
    vec![]
}

// ── Instance method completions — name-based fallback ─────────────────────────

fn instance_method_completions_heuristic(word: &str) -> Vec<CompletionItem> {
    let lower = word.to_lowercase();
    let dix_type = if lower.contains("str") || lower.contains("name") || lower.contains("text")
        || lower.contains("msg") || lower.contains("title") || lower.contains("label")
        || lower.contains("key") || lower.contains("val") || lower.contains("desc")
    {
        Some(DixType::String)
    } else if lower.contains("arr") || lower.contains("list") || lower.contains("items")
        || lower.contains("tags") || lower.contains("values") || lower.contains("elements")
        || lower.contains("col") || lower.contains("set")
    {
        Some(DixType::Array)
    } else if lower.contains("tuple") || lower.contains("coord") || lower.contains("point")
        || lower.contains("pair") || lower.contains("vec")
    {
        Some(DixType::Tuple)
    } else if lower.contains("regex") || lower.contains("pattern") || lower.contains("rule") {
        Some(DixType::Regex)
    } else if lower.contains("blob") || lower.contains("bytes") || lower.contains("buf")
        || lower.contains("bin") || lower.contains("raw")
    {
        Some(DixType::Blob)
    } else if lower.contains("num") || lower.contains("count") || lower.contains("size")
        || lower.contains("len") || lower.contains("idx") || lower.contains("index")
    {
        Some(DixType::Int)
    } else if lower.contains("date") || lower.contains("time") || lower.contains("when") {
        Some(DixType::Date)
    } else if lower.contains("stamp") || lower.contains("ts") {
        Some(DixType::Timestamp)
    } else {
        None
    };

    if let Some(dt) = dix_type {
        return registry_instance_method_completions(dt);
    }
    vec![]
}
// ─────────────────────────────────────────────────────────────────────────────
// ENCLOSING QUICKFUNC RESOLUTION (for local variable / parameter completions)
// ─────────────────────────────────────────────────────────────────────────────

/// Find the `QuickFunction` whose body contains the cursor position, by
/// walking the token stream and matching `~name(...) { ... }` brace ranges.
///
/// `QuickFunction` only carries a start `Position` (no end), so this
/// reconstructs the body's line range from tokens: it locates each `~`
/// declaration's body-opening `{` (skipping over the parameter list's
/// parens) and then tracks brace depth to its matching `}`.
///
/// Functions are matched to `ast.quick_functions.functions` by source order,
/// since both come from the same sequential token stream.
///
/// If the closing `}` hasn't been typed yet (mid-edit), the range is treated
/// as extending to the end of the document instead of collapsing to a single
/// line — this keeps completions working while typing inside an unfinished
/// function body.
fn find_enclosing_quickfunc<'a>(
    ast: &'a DixScript,
    tokens: &[Token],
    pos: Position,
) -> Option<&'a QuickFunction> {
    let qf = ast.quick_functions.as_ref()?;
    let cursor_line = (pos.line as usize) + 1; // tokens are 1-indexed
    let eof_line = tokens.last().map(|t| t.line).unwrap_or(cursor_line);

    let mut func_idx = 0usize;
    let mut i = 0usize;

    while i < tokens.len() {
        let tok = &tokens[i];

        if tok.section != SectionId::QuickFuncs || !matches!(tok.token_type, TokenType::Symbol('~')) {
            i += 1;
            continue;
        }

        let start_line = tok.line;

        // Find the body-opening `{`, skipping over the parameter list's parens.
        let mut j = i + 1;
        let mut paren_depth = 0i32;
        let mut open_brace_idx: Option<usize> = None;
        while j < tokens.len() {
            match &tokens[j].token_type {
                TokenType::Symbol('(') => paren_depth += 1,
                TokenType::Symbol(')') => paren_depth -= 1,
                TokenType::Symbol('{') if paren_depth <= 0 => {
                    open_brace_idx = Some(j);
                    break;
                }
                _ => {}
            }
            j += 1;
        }

        let open_idx = match open_brace_idx {
            Some(idx) => idx,
            None => {
                i += 1;
                continue;
            }
        };

        // Track brace depth to find the matching closing `}`.
        let mut depth = 1i32;
        let mut k = open_idx + 1;
        let mut end_line = eof_line; // fallback: unfinished body extends to EOF
        while k < tokens.len() {
            match &tokens[k].token_type {
                TokenType::Symbol('{') => depth += 1,
                TokenType::Symbol('}') => {
                    depth -= 1;
                    if depth == 0 {
                        end_line = tokens[k].line;
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }

        if cursor_line >= start_line && cursor_line <= end_line {
            return qf.functions.get(func_idx);
        }

        func_idx += 1;
        i = if depth == 0 { k + 1 } else { tokens.len() };
    }

    None
}

fn function_param_completions(func: &QuickFunction) -> Vec<CompletionItem> {
    func.parameters.iter().map(|p| {
        let detail = p.data_type.map(|dt| format!("<{}>", dt));
        CompletionItem {
            label:              p.name.clone(),
            kind:               Some(CompletionItemKind::VARIABLE),
            detail:             Some(detail.clone().unwrap_or_else(|| "parameter".to_string())),
            documentation:      Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: format!(
                    "**`{}`**{} — function parameter",
                    p.name,
                    detail.map(|d| format!(" {}", d)).unwrap_or_default()
                ),
            })),
            insert_text:        Some(p.name.clone()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            sort_text:          Some(format!("0_{}", p.name)),
            filter_text:        None,
            ..Default::default()
        }
    }).collect()
}

fn collect_local_variable_completions(stmts: &[QuickFuncStatement]) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    collect_local_variables_into(stmts, &mut items);
    items
}

fn collect_local_variables_into(stmts: &[QuickFuncStatement], items: &mut Vec<CompletionItem>) {
    for stmt in stmts {
        match stmt {
            QuickFuncStatement::VariableDeclaration {
                declaration_type,
                variable_name,
                data_type,
                value,
                ..
            } => {
                let dt = data_type.or_else(|| infer_datatype_from_expr_simple(value));
                let detail = dt.map(|d| format!("<{}>", d));
                let kw = match declaration_type {
                    DeclarationType::Let   => "let",
                    DeclarationType::Const => "const",
                };
                items.push(CompletionItem {
                    label:              variable_name.clone(),
                    kind:               Some(CompletionItemKind::VARIABLE),
                    detail:             Some(detail.clone().unwrap_or_else(|| format!("{} variable", kw))),
                    documentation:      Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!(
                            "**`{}`**{} — local variable (`{}`)",
                            variable_name,
                            detail.map(|d| format!(" {}", d)).unwrap_or_default(),
                            kw
                        ),
                    })),
                    insert_text:        Some(variable_name.clone()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    sort_text:          Some(format!("0_{}", variable_name)),
                    filter_text:        None,
                    ..Default::default()
                });
            }
            QuickFuncStatement::If { then_branch, else_branch, .. } => {
                collect_local_variables_into(then_branch, items);
                if let Some(eb) = else_branch {
                    collect_local_variables_into(eb, items);
                }
            }
            QuickFuncStatement::Switch { cases, default_case, .. } => {
                for case in cases {
                    collect_local_variables_into(&case.statements, items);
                }
                if let Some(dc) = default_case {
                    collect_local_variables_into(&dc.statements, items);
                }
            }
            _ => {}
        }
    }
                }
// ── General completions ───────────────────────────────────────────────────────

fn general_completions(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let word = word_before_cursor(&doc.source, pos);
    let word_lower = word.to_lowercase();
    let filter_by_word = word.len() >= 2;

    // Local variables and parameters of the enclosing QuickFunc.
    // Without this, typing inside a function body never suggested the
    // function's own parameters or any `let`/`const` declared above the
    // cursor — only QuickFunc names, enums, and builtins showed up.
    if let Some(ast) = &doc.ast {
        if let Some(func) = find_enclosing_quickfunc(ast, &doc.tokens, pos) {
            let mut local_items = function_param_completions(func);
            local_items.extend(collect_local_variable_completions(&func.body));
            if filter_by_word {
                local_items.retain(|item| item.label.to_lowercase().contains(&word_lower));
            }
            items.extend(local_items);
        }
    }

    if let Some(ast) = &doc.ast {
        // QuickFunc names (local)
        if let Some(qf) = &ast.quick_functions {
            for func in &qf.functions {
                if filter_by_word && !func.name.to_lowercase().contains(&word_lower) {
                    continue;
                }
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

                let param_snippets: Vec<String> = func.parameters.iter().enumerate()
                    .map(|(i, p)| format!("${{{}:{}}}", i + 1, p.name))
                    .collect();

                items.push(CompletionItem {
                    label:   func.name.clone(),
                    kind:    Some(CompletionItemKind::FUNCTION),
                    detail:  Some(format!("~{}<{}>({}) — QuickFunc", func.name, ret, params.join(", "))),
                    insert_text:        Some(format!("{}({})", func.name, param_snippets.join(", "))),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    filter_text:        None,
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!("**`~{}<{}>({})` — local QuickFunc**\n\nCompile-time function.",
                            func.name, ret, params.join(", ")),
                    })),
                    sort_text: Some(format!("0_{}", func.name)),
                    ..Default::default()
                });
            }
        }

        // Enum type names
        if let Some(enums) = &ast.enums {
            for decl in &enums.enums {
                if filter_by_word && !decl.name.to_lowercase().contains(&word_lower) {
                    continue;
                }
                items.push(CompletionItem {
                    label:       decl.name.clone(),
                    kind:        Some(CompletionItemKind::ENUM),
                    detail:      Some(format!("{} fields", decl.fields.len())),
                    filter_text: None,
                    sort_text:   Some(format!("1_{}", decl.name)),
                    ..Default::default()
                });
            }
        }
    }

    // Symbol table callables (imported namespace functions)
    if let Some(st) = doc.semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        for (ns_alias, ns) in &st.namespaces {
            for (func_name, func_info) in &ns.functions {
                let full_name = format!("{}.{}", ns_alias, func_name);
                if filter_by_word && !full_name.to_lowercase().contains(&word_lower)
                    && !func_name.to_lowercase().contains(&word_lower) {
                    continue;
                }
                let ret = func_info.signature.return_type
                    .map(|t| format!("{}", t))
                    .unwrap_or_else(|| "?".to_string());
                let params: Vec<String> = func_info.signature.parameters.iter()
                    .map(|p| {
                        let t = p.param_type.map(|dt| format!("<{}>", dt)).unwrap_or_default();
                        format!("{}{}", p.name, t)
                    }).collect();
                let param_snippets: Vec<String> = func_info.signature.parameters.iter().enumerate()
                    .map(|(i, p)| format!("${{{}:{}}}", i + 1, p.name))
                    .collect();
                items.push(CompletionItem {
                    label:   full_name.clone(),
                    kind:    Some(CompletionItemKind::FUNCTION),
                    detail:  Some(format!("{} → <{}>", full_name, ret)),
                    insert_text: Some(format!("{}({})", full_name, param_snippets.join(", "))),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    filter_text: None,
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind:  MarkupKind::Markdown,
                        value: format!(
                            "**`{ns}.{name}<{ret}>({params})`** — imported QuickFunc\n\nFile: `{file}`",
                            ns     = ns_alias,
                            name   = func_name,
                            ret    = ret,
                            params = params.join(", "),
                            file   = ns.file_path,
                        ),
                    })),
                    sort_text: Some(format!("1_{}", full_name)),
                    ..Default::default()
                });
            }

            // Imported enum types
            for enum_name in ns.enums.keys() {
                let full_name = format!("{}.{}", ns_alias, enum_name);
                if filter_by_word && !full_name.to_lowercase().contains(&word_lower)
                    && !enum_name.to_lowercase().contains(&word_lower) {
                    continue;
                }
                items.push(CompletionItem {
                    label:       full_name.clone(),
                    kind:        Some(CompletionItemKind::ENUM),
                    detail:      Some(format!("imported enum from {}", ns_alias)),
                    filter_text: None,
                    sort_text:   Some(format!("2_{}", full_name)),
                    ..Default::default()
                });
            }
        }
    }

    items.extend(keyword_completions_filtered(&word_lower, filter_by_word));

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
        if filter_by_word && !name.to_lowercase().contains(&word_lower) {
            continue;
        }
        items.push(CompletionItem {
            label:       name.to_string(),
            kind:        Some(CompletionItemKind::CLASS),
            detail:      Some(desc.to_string()),
            filter_text: None,
            sort_text:   Some(format!("3_{}", name)),
            ..Default::default()
        });
    }

    items
}

// ── Keyword completions ───────────────────────────────────────────────────────

fn keyword_completions_filtered(word_lower: &str, do_filter: bool) -> Vec<CompletionItem> {
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
    keywords.iter()
        .filter(|(kw, _)| !do_filter || kw.to_lowercase().contains(word_lower))
        .map(|(label, detail)| CompletionItem {
            label:              label.to_string(),
            kind:               Some(CompletionItemKind::KEYWORD),
            detail:             Some(detail.to_string()),
            insert_text:        Some(label.to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            filter_text:        None,
            sort_text:          Some(format!("9_{}", label)),
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
    let up_to_dot: String = line
        .char_indices()
        .take_while(|(i, _)| *i < pos.character.saturating_sub(1) as usize)
        .map(|(_, c)| c)
        .collect();
    up_to_dot
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .last()
        .unwrap_or("")
        .to_string()
            }
