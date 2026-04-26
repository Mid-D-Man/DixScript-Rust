// mdix-lsp/src/features/completions.rs
//! Completion provider.
//!
//! Changes from previous version:
//!
//! - `provide` is now wrapped in `catch_unwind` so a panic never kills the server.
//!
//! - CONFIG completions no longer depend on `triggerCharacter` being set.
//!   IntelliJ / Rust Rover often sends completions with `triggerKind = Invoked`
//!   (no trigger character) even when the user pressed a key.  The new flow
//!   uses the section of the token under the cursor first; trigger-character
//!   routing is a secondary fallback.
//!
//! - The CONFIG path now fires when:
//!     a) The nearest token to the cursor is in SectionId::Config, OR
//!     b) The line contains `->` and we are inside what looks like a CONFIG block.
//!
//! - `filter_text` is set to `None` on all section snippet items (was a
//!   named bug: setting it to the bare name without `@` caused VSCode to
//!   hide the items when the user typed `@`).  This was already fixed in the
//!   previous version but is preserved here.

use std::panic;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse,
    Documentation, InsertTextFormat, MarkupContent, MarkupKind, Position,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::Compiler::AST::DixScript;
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
            // ── Step 1: Determine section from token stream ─────────────────
            // This is the most reliable signal and works regardless of whether
            // the client sends a trigger character.
            let section = if !d.tokens.is_empty() {
                section_of_token_at(&d.tokens, pos)
            } else {
                SectionId::None
            };

            // ── Step 2: Section-specific completion ─────────────────────────

            // CONFIG section: provide key completions and value completions.
            // This fires whether the trigger was a character, an invocation,
            // or anything else — the section context is what matters.
            if section == SectionId::Config {
                let config_items = config_completions_at(&d.source, pos);
                if !config_items.is_empty() {
                    return Some(CompletionResponse::Array(config_items));
                }
                return Some(CompletionResponse::Array(config_key_completions()));
            }

            // Fallback: also check if the line looks like a CONFIG entry even
            // when the token stream section hasn't been set yet (e.g. on the
            // very first keystroke before analysis completes).
            if section == SectionId::None {
                if line_looks_like_config_entry(&d.source, pos) {
                    let config_items = config_completions_at(&d.source, pos);
                    if !config_items.is_empty() {
                        return Some(CompletionResponse::Array(config_items));
                    }
                    return Some(CompletionResponse::Array(config_key_completions()));
                }
            }

            // ── Step 3: Trigger-character routing ───────────────────────────
            // Resolve the effective trigger character.  IntelliJ often sends
            // triggerKind=Invoked (no trigger character) even for character
            // triggers, so we also check what the character immediately before
            // the cursor is.
            let trigger_ch: char = trigger
                .and_then(|t| t.chars().next())
                .unwrap_or_else(|| char_before_cursor(&d.source, pos));

            match trigger_ch {
                '@' => section_snippet_completions(),

                '<' => type_annotation_completions(),

                '~' => quickfunc_declaration_snippets(),

                '.' => dot_completions(d, pos),

                _ => {
                    // Word-based routing: if the user started typing `@`, offer
                    // section snippets; otherwise offer general completions.
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
//
// Used when the token stream is empty (analysis not yet complete) or section
// context is `None`.  A CONFIG entry line looks like one of:
//   `  version -> "1.0.0"`
//   `  debug_mode -> `                (still being typed)
//
// We scan backward from the cursor line to find `@CONFIG(` within 20 lines.

fn line_looks_like_config_entry(source: &str, pos: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line = pos.line as usize;

    // Check if we are inside a @CONFIG block by scanning backward.
    let search_start = cursor_line.saturating_sub(20);
    let mut found_config_open = false;

    for line in lines[search_start..=cursor_line.min(lines.len().saturating_sub(1))].iter().rev() {
        let trimmed = line.trim().to_uppercase();
        if trimmed.starts_with("@CONFIG") && trimmed.contains('(') {
            found_config_open = true;
            break;
        }
        // If we hit another section keyword, stop searching.
        if trimmed.starts_with('@') && !trimmed.starts_with("@CONFIG") {
            break;
        }
    }

    found_config_open
}

// ── CONFIG completions ────────────────────────────────────────────────────────

fn config_completions_at(source: &str, pos: Position) -> Vec<CompletionItem> {
    let line_text = source.lines().nth(pos.line as usize).unwrap_or("");
    let up_to: &str = &line_text[..((pos.character as usize).min(line_text.len()))];

    // If the line contains `->`, we are on the value side — offer value choices.
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
            "File creation timestamp. ISO 8601 format.\n\nExample: `created -> \"2025-01-15T10:30:00Z\"`",
        ),
        (
            "encoding",
            "encoding -> \"${1|utf-8,utf-16,ascii,iso-8859-1|}\"",
            "string",
            "Source file character encoding.\n\nSupported: `utf-8` *(default)*, `utf-16`, `ascii`, `iso-8859-1`",
        ),
        (
            "debug_mode",
            "debug_mode -> \"${1|off,regular,verbose|}\"",
            "string",
            "Compiler diagnostic verbosity.\n\n| Value | Effect |\n|-------|--------|\n| `\"off\"` | No debug output *(default)* |\n| `\"regular\"` | Key resolution steps |\n| `\"verbose\"` | Full execution trace |",
        ),
        (
            "error_handling",
            "error_handling -> \"${1|halt,continue,recover|}\"",
            "string",
            "How the compiler responds to errors.\n\n| Value | Behaviour |\n|-------|----------|\n| `\"halt\"` | Stop on first error *(default)* |\n| `\"continue\"` | Collect all, then report |\n| `\"recover\"` | Try to parse past errors |",
        ),
        (
            "compatibility_mode",
            "compatibility_mode -> \"${1|strict,best_effort,permissive|}\"",
            "string",
            "Parser strictness level.\n\n| Value | Behaviour |\n|-------|----------|\n| `\"strict\"` | Reject unknown syntax *(default)* |\n| `\"best_effort\"` | Warn, continue |\n| `\"permissive\"` | Accept anything parseable |",
        ),
        (
            "features",
            "features -> \"${1|advanced,basic|}\"",
            "string",
            "Enabled section features.\n\n| Value | Sections |\n|-------|----------|\n| `\"basic\"` | DATA, SECURITY only |\n| `\"advanced\"` | All sections *(default)* |",
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
        label:  format!("\"{}\"", value),
        kind:   Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some(detail.to_string()),
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
            "Compiler settings and metadata.\n\n**Keys:** `version`, `author`, `created`, `encoding`, `debug_mode` (`off`/`regular`/`verbose`), `error_handling` (`halt`/`continue`/`recover`), `compatibility_mode` (`strict`/`best_effort`/`permissive`), `features`.\n\n```mdix\n@CONFIG(\n  version -> \"1.0.0\"\n  debug_mode -> \"regular\"\n)\n```",
        ),
        (
            "@IMPORTS",
            "IMPORTS(\n  ${1:Alias} from \"${2:path/to/file.mdix}\"\n)",
            "Import other `.mdix` files. The alias becomes a namespace.\n\n```mdix\n@IMPORTS(\n  Utils from \"common/utils.mdix\"\n  Base  from \"../base.mdix\"\n)\n```\n\nCall imported functions: `Utils.myFunc(x)`. Access imported enums: `Utils.Status.ACTIVE`.",
        ),
        (
            "@DLM",
            "DLM(\n  DCompressor.${1|gzip,bzip2,lzma|}\n  DEncryptor.${2|aes256,aes128,chacha20|}\n)",
            "Data Lifecycle Modules — compression and encryption applied at compile time.\n\n**Compressors:** `DCompressor.gzip`, `DCompressor.bzip2`, `DCompressor.lzma`\n**Encryptors:** `DEncryptor.aes256`, `DEncryptor.aes128`, `DEncryptor.chacha20`, `DEncryptor.xor`\n**Auditor:** `DAuditor.diy`, `DAuditor.enhanced`\n\n```mdix\n@DLM(DCompressor.gzip, DEncryptor.aes256)\n```",
        ),
        (
            "@ENUMS",
            "ENUMS(\n  ${1:EnumName} { ${2:VALUE_A} = 0, ${3:VALUE_B} = 1 }\n)",
            "Named integer constants. Auto-increments from 0 if values omitted.\n\n```mdix\n@ENUMS(\n  Difficulty { EASY = 0, NORMAL = 1, HARD = 2 }\n  AIType     { PASSIVE, NEUTRAL, AGGRESSIVE, BOSS }\n)\n```\n\nAccess: `Difficulty.HARD`, use type annotation `<enum>`.",
        ),
        (
            "@QUICKFUNCS",
            "QUICKFUNCS(\n  ~${1:funcName}<${2:object}>(${3:param1}, ${4:param2}) {\n    return {\n      ${5:key} = ${6:param1}\n    }\n  }\n)",
            "Compile-time functions. All computation happens at compile time — zero runtime overhead.\n\n```mdix\n@QUICKFUNCS(\n  ~weapon<object>(id, damage<int>) {\n    return { id = id, damage = damage, range = damage * 2 }\n  }\n)\n```\n\nCall in `@DATA`: `weapon(\"AK47\", 35)`",
        ),
        (
            "@DATA",
            "DATA(\n  ${1:key} = ${2:value}\n\n  ${3:table}: ${4:field} = ${5:value}\n\n  ${6:array}::\n    ${7:item1},\n    ${8:item2}\n)",
            "Data payload. Three entry types:\n\n- **Flat:** `name = value` (primitives)\n- **Table:** `section: field = value, field2 = value2`\n- **Group array:** `items:: value1, value2`\n\nCommas between entries are optional.\n\n```mdix\n@DATA(\n  app_name = \"MyApp\"\n  server: host = \"localhost\", port = 8080\n  tags:: \"alpha\", \"beta\"\n)\n```",
        ),
        (
            "@SECURITY",
            "SECURITY(\n  encryption -> {\n    mode = \"${1|password,keyfile|}\",\n    algorithm = \"${2|aes256-gcm,aes128-gcm,chacha20-poly1305|}\"\n  }\n)",
            "Encryption configuration. Required when `@DLM` includes a `DEncryptor`.\n\n**Modes:** `\"password\"` (user-supplied), `\"keyfile\"` (generates `.key` file)\n\n```mdix\n@SECURITY(\n  encryption -> { mode = \"keyfile\", algorithm = \"aes256-gcm\" }\n)\n```\n\nCompile: `mdix compile secrets.mdix --password`",
        ),
    ];

    sections.iter().map(|(label, snippet, doc)| {
        CompletionItem {
            label:              label.to_string(),
            kind:               Some(CompletionItemKind::MODULE),
            detail:             Some("DixScript section".to_string()),
            // filter_text MUST be None so IntelliJ / VSCode use the label
            // (`@CONFIG` etc.) for filtering.  A non-None filter_text of the
            // bare name without `@` caused completions to disappear when the
            // user typed `@`.
            filter_text:        None,
            documentation:      Some(Documentation::MarkupContent(MarkupContent {
                kind:  MarkupKind::Markdown,
                value: doc.to_string(),
            })),
            insert_text:        Some(snippet.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            sort_text:          Some(format!("0_{}", label)),
            ..Default::default()
        }
    }).collect()
}

// ── Type annotation completions ───────────────────────────────────────────────

fn type_annotation_completions() -> Vec<CompletionItem> {
    let types: &[(&str, &str, &str)] = &[
        ("int",       "32-bit signed integer",             "42, -7, 0"),
        ("float",     "32-bit float (requires f suffix)",  "3.14f, -0.5f"),
        ("double",    "64-bit float (IEEE 754 f64)",       "3.14159, -2.718"),
        ("string",    "UTF-8 string",                      "\"hello\""),
        ("bool",      "Boolean",                           "true, false"),
        ("array",     "Ordered collection (::)",            "\"a\", \"b\", \"c\""),
        ("tuple",     "Mixed-type, max 6 elements",         "t:(1, \"a\", true)"),
        ("object",    "Key-value map  { }",                "{ x = 1, y = 2 }"),
        ("hex",       "Hex colour or integer",             "#FF5733, 0xFF"),
        ("blob",      "Base64-encoded binary",             "b:(\"SGVsbG8=\")"),
        ("regex",     "Regular expression pattern",        "r:(\"^[a-z]+$\")"),
        ("date",      "ISO 8601 date",                     "2025-12-31"),
        ("timestamp", "ISO 8601 timestamp",                "2025-12-31T10:30:00Z"),
        ("enum",      "Enum value from @ENUMS",            "MyEnum.VALUE"),
        ("any",       "Any type (no restriction)",         "anything"),
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
            "Object-returning QuickFunc (most common — for createEnemy, weapon, etc.)",
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

// ── Dot completions ───────────────────────────────────────────────────────────

fn dot_completions(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let word_before = word_before_dot(&doc.source, pos);
    if word_before.is_empty() {
        return items;
    }

    if let Some(ast) = &doc.ast {
        items.extend(enum_value_completions(ast, &word_before));
    }

    items.extend(static_method_completions(&word_before));

    if let Some(sr) = doc.semantic_result.as_ref() {
        if let Some(st) = &sr.symbol_table {
            if let Some(ns) = st.try_get_namespace(&word_before) {
                for func_name in ns.functions.keys() {
                    items.push(CompletionItem {
                        label:  func_name.clone(),
                        kind:   Some(CompletionItemKind::FUNCTION),
                        detail: Some(format!("imported from {}", ns.alias)),
                        filter_text: None,
                        ..Default::default()
                    });
                }
                for enum_name in ns.enums.keys() {
                    items.push(CompletionItem {
                        label:  enum_name.clone(),
                        kind:   Some(CompletionItemKind::ENUM),
                        detail: Some(format!("enum from {}", ns.alias)),
                        filter_text: None,
                        ..Default::default()
                    });
                }
            }
        }
    }

    items.extend(instance_method_completions(&word_before));
    items
}

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
                        value: format!("**{}.{}** — enum member\n\nUsage: `{}.{}`",
                                       enum_name, f.name, enum_name, f.name),
                    })),
                    filter_text: None,
                    ..Default::default()
                }
            }).collect();
        }
    }
    vec![]
}

fn static_method_completions(object_name: &str) -> Vec<CompletionItem> {
    // Abbreviated table — full method lists live in the completions data above.
    // The important thing is these fire for DLM module names too.
    let catalogue: &[(&str, &[(&str, &str)])] = &[
        ("Math",       &[("sqrt","→ double"),("pow","→ double"),("abs","→ double"),("floor","→ int"),("ceil","→ int"),("round","→ int"),("min","→ double"),("max","→ double"),("clamp","→ double"),("sin","→ double"),("cos","→ double"),("tan","→ double"),("log","→ double"),("pi","→ double"),("e","→ double")]),
        ("DateTime",   &[("now","→ timestamp"),("today","→ date"),("format","→ string"),("year","→ int"),("month","→ int"),("day","→ int"),("addDays","→ date"),("subtract","→ double"),("isLeapYear","→ bool")]),
        ("Array",      &[("empty","→ array"),("range","→ array"),("fill","→ array"),("sort","→ array"),("unique","→ array"),("flatten","→ array"),("sum","→ double"),("average","→ double")]),
        ("Random",     &[("range","→ int"),("float","→ float"),("double","→ double"),("boolean","→ bool"),("choice","→ any"),("shuffle","→ array"),("alphanumeric","→ string")]),
        ("Guid",       &[("new","→ string"),("parse","→ string"),("validate","→ bool"),("empty","→ string")]),
        ("IpAddress",  &[("parse","→ string"),("validate","→ bool"),("isV4","→ bool"),("isV6","→ bool"),("isPrivate","→ bool"),("localhost","→ string")]),
        ("Enum",       &[("getValues","→ array"),("getName","→ string"),("getValue","→ int"),("count","→ int"),("exists","→ bool"),("list","→ array")]),
        ("Dix",        &[("Log","→ void"),("LogInfo","→ void"),("LogWarning","→ void"),("LogError","→ void"),("Assert","→ void"),("Format","→ string"),("Join","→ string")]),
        // DLM module dot-completions.
        ("DCompressor",&[("gzip","compression algorithm"),("bzip2","compression algorithm"),("lzma","compression algorithm")]),
        ("DEncryptor", &[("aes256","encryption algorithm"),("aes128","encryption algorithm"),("chacha20","encryption algorithm"),("xor","⚠️ weak obfuscation only")]),
        ("DAuditor",   &[("diy","custom audit hook"),("enhanced","built-in checksum audit")]),
    ];

    for (obj, methods) in catalogue {
        if *obj == object_name {
            return methods.iter().map(|(method, sig)| CompletionItem {
                label:       method.to_string(),
                kind:        Some(CompletionItemKind::METHOD),
                detail:      Some(format!("{}.{} {}", obj, method, sig)),
                insert_text:        Some(format!("{}(", method)),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                filter_text:        None,
                ..Default::default()
            }).collect();
        }
    }
    vec![]
}

fn instance_method_completions(word: &str) -> Vec<CompletionItem> {
    // Instance methods are keyed by heuristic variable name suffixes.
    let lower = word.to_lowercase();
    let kind = if lower.contains("str") || lower.contains("name") || lower.contains("text") {
        "string"
    } else if lower.contains("arr") || lower.contains("list") || lower.contains("items") {
        "array"
    } else {
        return vec![];
    };

    let methods: &[(&str, &str)] = match kind {
        "string" => &[
            ("toUpper","→ string"), ("toLower","→ string"), ("trim","→ string"),
            ("length","→ int"), ("contains","→ bool"), ("startsWith","→ bool"),
            ("endsWith","→ bool"), ("replace","→ string"), ("split","→ array"),
            ("substring","→ string"), ("isEmpty","→ bool"),
        ],
        "array" => &[
            ("length","→ int"), ("isEmpty","→ bool"), ("contains","→ bool"),
            ("get","→ any"), ("push","→ array"), ("pop","→ array"),
            ("join","→ string"), ("reverse","→ array"), ("sort","→ array"),
            ("first","→ any"), ("last","→ any"), ("sum","→ double"),
            ("average","→ double"), ("min","→ double"), ("max","→ double"),
        ],
        _ => &[],
    };

    methods.iter().map(|(method, sig)| CompletionItem {
        label:       method.to_string(),
        kind:        Some(CompletionItemKind::METHOD),
        detail:      Some(format!("{} {}", method, sig)),
        insert_text:        Some(format!("{}(", method)),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

// ── General completions ───────────────────────────────────────────────────────

fn general_completions(doc: &Document, _pos: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    if let Some(ast) = &doc.ast {
        // QuickFunc names.
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

        // Enum type names.
        if let Some(enums) = &ast.enums {
            for decl in &enums.enums {
                items.push(CompletionItem {
                    label:   decl.name.clone(),
                    kind:    Some(CompletionItemKind::ENUM),
                    detail:  Some(format!("{} fields", decl.fields.len())),
                    filter_text: None,
                    ..Default::default()
                });
            }
        }
    }

    items.extend(keyword_completions());

    // Built-in static objects.
    for (name, desc) in &[
        ("Math",      "Built-in math functions"),
        ("DateTime",  "Date/time functions"),
        ("Array",     "Array factory functions"),
        ("Random",    "Random generation"),
        ("Guid",      "GUID/UUID generation"),
        ("IpAddress", "IP address utilities"),
        ("Enum",      "Enum introspection"),
        ("Dix",       "Logging and utilities"),
        // DLM module names — useful when editing inside @DLM.
        ("DCompressor", "DLM compression module"),
        ("DEncryptor",  "DLM encryption module"),
        ("DAuditor",    "DLM audit module"),
    ] {
        items.push(CompletionItem {
            label:   name.to_string(),
            kind:    Some(CompletionItemKind::CLASS),
            detail:  Some(desc.to_string()),
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
        ("and",        "Logical AND (word form, = &&)"),
        ("or",         "Logical OR (word form, = ||)"),
        ("not",        "Logical NOT (word form, = !)"),
        ("true",       "Boolean true"),
        ("false",      "Boolean false"),
        ("null",       "Null literal"),
        ("from",       "Import a local .mdix file"),
        ("from_cloud", "Import a remote .mdix file"),
        ("verify",     "Verify import file hash"),
        ("global",     "Global scope modifier"),
    ];

    keywords.iter().map(|(label, detail)| CompletionItem {
        label:       label.to_string(),
        kind:        Some(CompletionItemKind::KEYWORD),
        detail:      Some(detail.to_string()),
        insert_text:        Some(label.to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        filter_text:        None,
        ..Default::default()
    }).collect()
}

// ── Source text helpers ───────────────────────────────────────────────────────

/// The character immediately before the cursor position.
/// Used as a fallback when the client does not send a trigger character.
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
        for t in &["<int>","<float>","<double>","<string>","<bool>","<array>",
                   "<tuple>","<object>","<hex>","<blob>","<regex>","<date>",
                   "<timestamp>","<enum>","<any>"] {
            assert!(labels.iter().any(|l| l == t), "missing type: {}", t);
        }
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
    fn config_key_completions_covers_all_keys() {
        let items = config_key_completions();
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        for k in &["version","author","debug_mode","error_handling",
                   "compatibility_mode","features"] {
            assert!(labels.iter().any(|l| l == k), "missing config key: {}", k);
        }
    }

    #[test]
    fn quickfunc_names_in_general_completions() {
        let src = "@QUICKFUNCS(\n  ~calc<int>(x) { return x }\n)\n@DATA(\n  y = 1\n)";
        let doc = test_doc(src);
        let items = general_completions(&doc, Position::new(3, 0));
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        assert!(labels.iter().any(|l| l == "calc"), "QuickFunc 'calc' missing; got: {:?}", labels);
    }

    #[test]
    fn line_looks_like_config_entry_detects_config_block() {
        let src = "@CONFIG(\n  version -> \"1.0.0\"\n  debug_mode -> \n)";
        // Line 2 (0-indexed) is `  debug_mode -> ` — inside @CONFIG
        assert!(line_looks_like_config_entry(src, Position::new(2, 14)));
    }

    #[test]
    fn line_looks_like_config_entry_rejects_data_block() {
        let src = "@DATA(\n  version -> \"1.0.0\"\n)";
        // Even though there is an arrow, we are not inside @CONFIG
        assert!(!line_looks_like_config_entry(src, Position::new(1, 10)));
    }
}
