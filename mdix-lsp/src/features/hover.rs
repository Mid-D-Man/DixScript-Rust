//! Hover provider.
//!
//! Shows: type info for DATA variables, enum value integers,
//! QuickFunc signatures, built-in method signatures,
//! and human-readable formatting for Date / Timestamp tokens.

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::document::Document;

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<Hover> {
    let doc = doc?;
    let token = token_at(&doc.tokens, pos)?;
    let content = hover_content_for(token, doc)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: content,
        }),
        range: None,
    })
}

// ── Token dispatch ─────────────────────────────────────────────────────────────

fn hover_content_for(token: &Token, doc: &Document) -> Option<String> {
    match &token.token_type {
        // Enum access: show the integer value
        TokenType::EnumAccess { enum_name, value } => {
            hover_enum_access(doc, enum_name, value)
        }

        // Identifier: could be a DATA variable or a QuickFunc call
        TokenType::Identifier(name) => {
            hover_identifier(doc, name)
        }

        // Date literal: show human-readable form
        TokenType::Date(d) => {
            hover_date(d)
        }

        // Timestamp: show formatted version
        TokenType::Timestamp(ts) => {
            hover_timestamp(ts)
        }

        // Static function call: show method signature
        TokenType::StaticFunction { class, method } => {
            hover_static_method(class, method)
        }

        // Section keywords: show a short description
        TokenType::SectionConfig     => Some(section_doc("@CONFIG",     "Compiler settings. Uses -> arrows.")),
        TokenType::SectionImports    => Some(section_doc("@IMPORTS",    "Import other .mdix files.")),
        TokenType::SectionDLM        => Some(section_doc("@DLM",        "Data Lifecycle Modules: compression, encryption, auditing.")),
        TokenType::SectionEnums      => Some(section_doc("@ENUMS",      "Named integer constants. Access via EnumName.VALUE.")),
        TokenType::SectionQuickFuncs => Some(section_doc("@QUICKFUNCS", "Compile-time functions. Prefix with ~.")),
        TokenType::SectionData       => Some(section_doc("@DATA",       "Data payload. Flat properties first, then grouped entries.")),
        TokenType::SectionSecurity   => Some(section_doc("@SECURITY",   "Encryption configuration. Auto-generated when DEncryptor is used.")),

        _ => None,
    }
}

// ── Enum access hover ──────────────────────────────────────────────────────────

fn hover_enum_access(doc: &Document, enum_name: &str, field: &str) -> Option<String> {
    // Look up the integer value from the symbol table.
    let sr = doc.semantic_result.as_ref()?;
    let st = sr.symbol_table.as_ref()?;
    let value = st.try_get_enum_field_value(enum_name, field)?;

    Some(format!(
        "**{}.{}**\n\n```\n(enum) {} = {}\n```",
        enum_name, field, field, value
    ))
}

// ── Identifier hover ───────────────────────────────────────────────────────────

fn hover_identifier(doc: &Document, name: &str) -> Option<String> {
    // Check QuickFuncs first.
    if let Some(ast) = &doc.ast {
        if let Some(qf) = &ast.quick_functions {
            for func in &qf.functions {
                if func.name == name {
                    let params: Vec<String> = func
                        .parameters
                        .iter()
                        .map(|p| {
                            let type_str = p
                                .data_type
                                .as_ref()
                                .map(|t| format!("<{:?}>", t))
                                .unwrap_or_default();
                            format!("{}{}", p.name, type_str)
                        })
                        .collect();

                    let ret = func
                        .return_type
                        .as_ref()
                        .map(|t| format!("{:?}", t))
                        .unwrap_or_else(|| "?".to_string());

                    return Some(format!(
                        "**~{}<{}>**({})\n\n*QuickFunc — compile-time function*",
                        name,
                        ret,
                        params.join(", ")
                    ));
                }
            }
        }
    }

    // Check DATA variables in the symbol table.
    if let Some(sr) = &doc.semantic_result {
        if let Some(st) = &sr.symbol_table {
            if let Some(var) = st.try_get_data_variable(name) {
                let type_str = var
                    .effective_type()
                    .map(|t| format!("{:?}", t))
                    .unwrap_or_else(|| "unknown".to_string());
                let inferred = if var.is_inferred { " *(inferred)*" } else { "" };
                return Some(format!(
                    "**{}**: `<{}>`{}\n\n*DATA variable*",
                    name, type_str.to_lowercase(), inferred
                ));
            }
        }
    }

    None
}

// ── Date / Timestamp hover ─────────────────────────────────────────────────────

fn hover_date(date_str: &str) -> Option<String> {
    // Parse YYYY-MM-DD manually — no external date crate needed.
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year:  u32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day:   u32 = parts[2].parse().ok()?;

    let month_name = month_name(month)?;
    let day_suffix = ordinal_suffix(day);

    Some(format!(
        "**Date**: `{}`\n\n{} {}{}, {}",
        date_str, month_name, day, day_suffix, year
    ))
}

fn hover_timestamp(ts: &str) -> Option<String> {
    // Show the raw value with a note about the timezone designator.
    let tz_note = if ts.ends_with('Z') {
        "UTC"
    } else if ts.contains('+') || (ts.len() > 20 && ts.chars().nth(19) == Some('-')) {
        "with UTC offset"
    } else {
        "local time"
    };

    Some(format!(
        "**Timestamp**: `{}`\n\n*{}*",
        ts, tz_note
    ))
}

// ── Static method hover ────────────────────────────────────────────────────────

fn hover_static_method(class: &str, method: &str) -> Option<String> {
    let sig = STATIC_SIGS
        .iter()
        .find(|(c, m, _)| *c == class && *m == method)
        .map(|(_, _, sig)| *sig)?;

    Some(format!(
        "**{}.{}**\n\n```\n{}\n```\n\n*Built-in static method*",
        class, method, sig
    ))
}

// ── Section doc helper ─────────────────────────────────────────────────────────

fn section_doc(name: &str, description: &str) -> String {
    format!("**{}**\n\n{}", name, description)
}

// ── Lookup tables ──────────────────────────────────────────────────────────────

static STATIC_SIGS: &[(&str, &str, &str)] = &[
    ("Math", "sqrt",     "Math.sqrt(x: double) -> double"),
    ("Math", "round",    "Math.round(x: double) -> int"),
    ("Math", "abs",      "Math.abs(x: number) -> number"),
    ("Math", "floor",    "Math.floor(x: double) -> int"),
    ("Math", "ceil",     "Math.ceil(x: double) -> int"),
    ("Math", "min",      "Math.min(a: number, b: number) -> number"),
    ("Math", "max",      "Math.max(a: number, b: number) -> number"),
    ("Math", "pow",      "Math.pow(base: double, exp: double) -> double"),
    ("Math", "clamp",    "Math.clamp(v: number, min: number, max: number) -> number"),
    ("DateTime", "now",      "DateTime.now() -> timestamp"),
    ("DateTime", "today",    "DateTime.today() -> date"),
    ("DateTime", "format",   "DateTime.format(ts: timestamp, pattern: string) -> string"),
    ("DateTime", "year",     "DateTime.year(d: date) -> int"),
    ("DateTime", "month",    "DateTime.month(d: date) -> int"),
    ("DateTime", "day",      "DateTime.day(d: date) -> int"),
    ("DateTime", "subtract", "DateTime.subtract(a: date, b: date) -> int"),
    ("Array", "sort",    "Array.sort(arr: array) -> array"),
    ("Array", "reverse", "Array.reverse(arr: array) -> array"),
    ("Array", "slice",   "Array.slice(arr: array, start: int, end: int) -> array"),
    ("Array", "sum",     "Array.sum(arr: array) -> double"),
    ("Array", "range",   "Array.range(start: int, end: int) -> array"),
    ("Array", "length",  "Array.length(arr: array) -> int"),
    ("Array", "first",   "Array.first(arr: array) -> any"),
    ("Array", "last",    "Array.last(arr: array) -> any"),
    ("Random", "range",  "Random.range(min: int, max: int) -> int"),
    ("Random", "choice", "Random.choice(arr: array) -> any"),
    ("Guid", "new",      "Guid.new() -> string"),
];

fn month_name(m: u32) -> Option<&'static str> {
    match m {
        1  => Some("January"),  2  => Some("February"), 3  => Some("March"),
        4  => Some("April"),    5  => Some("May"),       6  => Some("June"),
        7  => Some("July"),     8  => Some("August"),    9  => Some("September"),
        10 => Some("October"),  11 => Some("November"),  12 => Some("December"),
        _  => None,
    }
}

fn ordinal_suffix(d: u32) -> &'static str {
    match d {
        11 | 12 | 13 => "th",
        n if n % 10 == 1 => "st",
        n if n % 10 == 2 => "nd",
        n if n % 10 == 3 => "rd",
        _ => "th",
    }
}

// ── Token-at-position lookup ───────────────────────────────────────────────────

/// Returns the token whose source span contains `pos`.
/// Tokens don't carry an end position, so we find the closest token
/// whose start is at or before the cursor and whose value length
/// covers the cursor column.
pub fn token_at(tokens: &[Token], pos: Position) -> Option<&Token> {
    let target_line = pos.line as usize + 1;   // LSP is 0-based; tokens are 1-based
    let target_col  = pos.character as usize + 1;

    let mut best: Option<&Token> = None;

    for token in tokens {
        if token.line != target_line {
            continue;
        }
        if token.column > target_col {
            break;
        }
        let value_len = token.get_token_value().len();
        if target_col <= token.column + value_len {
            best = Some(token);
        }
    }

    best
      }
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::run_pipeline;
    use crate::document::Document;
    use tower_lsp::lsp_types::{HoverContents, Position, Url};

    fn test_doc(source: &str) -> Document {
        let mut doc = Document::new(
            Url::parse("file:///test.mdix").unwrap(),
            source.to_string(),
            0,
        );
        run_pipeline(&mut doc);
        doc
    }

    #[test]
    fn hover_none_doc_returns_none() {
        assert!(provide(None, Position::new(0, 0)).is_none());
    }

    #[test]
    fn hover_data_section_keyword_returns_markup() {
        let doc    = test_doc("@DATA(\n  x = 1\n)");
        let result = provide(Some(&doc), Position::new(0, 1));
        assert!(result.is_some(), "@DATA should produce a hover result");
        if let Some(h) = result {
            match h.contents {
                HoverContents::Markup(m) => {
                    assert!(m.value.contains("DATA"), "hover should mention DATA");
                }
                _ => panic!("expected MarkupContent"),
            }
        }
    }

    #[test]
    fn hover_enums_section_keyword_returns_markup() {
        let doc    = test_doc("@ENUMS(\n  Status { ACTIVE = 1 }\n)");
        let result = provide(Some(&doc), Position::new(0, 1));
        assert!(result.is_some(), "@ENUMS should produce a hover result");
        if let Some(h) = result {
            if let HoverContents::Markup(m) = h.contents {
                assert!(m.value.contains("ENUMS") || m.value.contains("enum"),
                    "hover should mention enums");
            }
        }
    }

    #[test]
    fn hover_whitespace_does_not_panic() {
        let doc = test_doc("@DATA(\n  x = 1\n)");
        // Position inside the parentheses on an empty area — result may be None, that's fine
        let _ = provide(Some(&doc), Position::new(1, 0));
    }

    #[test]
    fn token_at_returns_none_past_end_of_file() {
        let doc    = test_doc("@DATA(\n  x = 1\n)");
        let result = token_at(&doc.tokens, Position::new(999, 0));
        assert!(result.is_none());
    }

    #[test]
    fn hover_quickfunc_section_keyword() {
        let source = "@QUICKFUNCS(\n  ~add<int>(a, b) {\n    return a\n  }\n)";
        let doc    = test_doc(source);
        let result = provide(Some(&doc), Position::new(0, 1));
        assert!(result.is_some(), "@QUICKFUNCS should produce a hover result");
    }
            }
