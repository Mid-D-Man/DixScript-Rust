//! Hover provider.
//!
//! Shows type info for DATA variables, enum values, QuickFunc signatures,
//! built-in method signatures, regex validation, blob previews,
//! and formatted Date/Timestamp literals.

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::document::Document;

pub fn provide(doc: Option<&Document>, pos: Position) -> Option<Hover> {
    let doc = doc?;
    let (token, index) = token_and_index_at(&doc.tokens, pos)?;
    let content = hover_content_for(token, index, doc)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind:  MarkupKind::Markdown,
            value: content,
        }),
        range: None,
    })
}

// ── Token dispatch ─────────────────────────────────────────────────────────────

fn hover_content_for(token: &Token, index: usize, doc: &Document) -> Option<String> {
    match &token.token_type {
        TokenType::EnumAccess { enum_name, value } => {
            hover_enum_access(doc, enum_name, value)
        }

        TokenType::Identifier(name) => {
            hover_identifier(doc, name)
        }

        TokenType::Date(d) => {
            hover_date(d)
        }

        TokenType::Timestamp(ts) => {
            hover_timestamp(ts)
        }

        TokenType::StaticFunction { class, method } => {
            hover_static_method(class, method)
        }

        // Regex constructor: validate the pattern found in the adjacent string token.
        TokenType::RegexConstructor(_) => {
            Some(hover_regex(&doc.tokens, index))
        }

        // Blob constructor: show decoded size and hex preview.
        TokenType::BlobConstructor(_) => {
            Some(hover_blob(&doc.tokens, index))
        }

        // HexColor: full RGBA breakdown alongside the swatch (color swatch is
        // handled by the documentColor provider; hover shows the numeric values).
        TokenType::HexColor(hex) => {
            hover_hex_color(hex)
        }

        TokenType::SectionConfig     => Some(section_doc("@CONFIG",     "Compiler settings. Uses -> arrows.")),
        TokenType::SectionImports    => Some(section_doc("@IMPORTS",    "Import other .mdix files.")),
        TokenType::SectionDLM        => Some(section_doc("@DLM",        "Data Lifecycle Modules: compression, encryption, auditing.")),
        TokenType::SectionEnums      => Some(section_doc("@ENUMS",      "Named integer constants. Access via EnumName.VALUE.")),
        TokenType::SectionQuickFuncs => Some(section_doc("@QUICKFUNCS", "Compile-time functions. Prefix with ~.")),
        TokenType::SectionData       => Some(section_doc("@DATA",       "Data payload. Flat properties first, then grouped entries.")),
        TokenType::SectionSecurity   => Some(section_doc("@SECURITY",   "Encryption configuration.")),

        _ => None,
    }
}

// ── Enum access hover ──────────────────────────────────────────────────────────

fn hover_enum_access(doc: &Document, enum_name: &str, field: &str) -> Option<String> {
    let sr    = doc.semantic_result.as_ref()?;
    let st    = sr.symbol_table.as_ref()?;
    let value = st.try_get_enum_field_value(enum_name, field)?;

    Some(format!(
        "**{}.{}**\n\n```\n(enum) {} = {}\n```",
        enum_name, field, field, value
    ))
}

// ── Identifier hover ───────────────────────────────────────────────────────────

fn hover_identifier(doc: &Document, name: &str) -> Option<String> {
    if let Some(ast) = &doc.ast {
        if let Some(qf) = &ast.quick_functions {
            for func in &qf.functions {
                if func.name != name {
                    continue;
                }
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
                    name, ret, params.join(", ")
                ));
            }
        }
    }

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
                    name,
                    type_str.to_lowercase(),
                    inferred
                ));
            }
        }
    }

    None
}

// ── Date / Timestamp hover ─────────────────────────────────────────────────────

fn hover_date(date_str: &str) -> Option<String> {
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

// ── Regex constructor hover ────────────────────────────────────────────────────

/// Show regex pattern validation status and a brief description.
/// The pattern is read from the string token that follows the r:( constructor.
fn hover_regex(tokens: &[Token], constructor_index: usize) -> String {
    let pattern = find_adjacent_string(tokens, constructor_index);

    match pattern {
        None => "**r:(...)** — Regular expression constructor\n\nThe pattern is validated at compile time.".to_string(),
        Some(pat) => {
            match regex::Regex::new(&pat) {
                Ok(_) => format!(
                    "**r:(...)** — Regular expression\n\n```\n{}\n```\n\n*Pattern syntax: valid*",
                    pat
                ),
                Err(e) => format!(
                    "**r:(...)** — Regular expression\n\n```\n{}\n```\n\n*Pattern syntax: invalid — {}*",
                    pat,
                    // Trim the verbose regex error to keep the hover concise.
                    e.to_string()
                        .lines()
                        .next()
                        .unwrap_or("parse error")
                ),
            }
        }
    }
}

// ── Blob constructor hover ─────────────────────────────────────────────────────

/// Show decoded byte count and a hex preview of the first 8 bytes.
fn hover_blob(tokens: &[Token], constructor_index: usize) -> String {
    let data = find_adjacent_string(tokens, constructor_index);

    match data {
        None => "**b:(...)** — Binary blob constructor\n\nData is stored as base64-encoded binary.".to_string(),
        Some(b64) => {
            use base64::{Engine as _, engine::general_purpose};
            match general_purpose::STANDARD.decode(&b64) {
                Ok(bytes) => {
                    let preview: Vec<String> = bytes
                        .iter()
                        .take(8)
                        .map(|b| format!("{:02X}", b))
                        .collect();
                    let ellipsis = if bytes.len() > 8 { " …" } else { "" };
                    format!(
                        "**b:(...)** — Binary blob\n\n{} bytes ({} base64 chars)\n\nFirst bytes: `{}{}`",
                        bytes.len(),
                        b64.len(),
                        preview.join(" "),
                        ellipsis,
                    )
                }
                Err(_) => format!(
                    "**b:(...)** — Binary blob\n\n`{}` base64 chars *(invalid encoding)*",
                    b64.len()
                ),
            }
        }
    }
}

// ── HexColor hover ─────────────────────────────────────────────────────────────

/// Show parsed RGBA channel values alongside the hex string.
fn hover_hex_color(hex: &str) -> Option<String> {
    let stripped = hex.trim_start_matches('#');

    let (r, g, b, a): (u8, u8, u8, u8) = match stripped.len() {
        3 => {
            let r = expand_nibble(stripped.get(0..1)?)?;
            let g = expand_nibble(stripped.get(1..2)?)?;
            let b = expand_nibble(stripped.get(2..3)?)?;
            (r, g, b, 255)
        }
        4 => {
            let r = expand_nibble(stripped.get(0..1)?)?;
            let g = expand_nibble(stripped.get(1..2)?)?;
            let b = expand_nibble(stripped.get(2..3)?)?;
            let a = expand_nibble(stripped.get(3..4)?)?;
            (r, g, b, a)
        }
        6 => {
            let r = u8::from_str_radix(stripped.get(0..2)?, 16).ok()?;
            let g = u8::from_str_radix(stripped.get(2..4)?, 16).ok()?;
            let b = u8::from_str_radix(stripped.get(4..6)?, 16).ok()?;
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(stripped.get(0..2)?, 16).ok()?;
            let g = u8::from_str_radix(stripped.get(2..4)?, 16).ok()?;
            let b = u8::from_str_radix(stripped.get(4..6)?, 16).ok()?;
            let a = u8::from_str_radix(stripped.get(6..8)?, 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };

    let alpha_line = if a == 255 {
        "Alpha: 255 (fully opaque)".to_string()
    } else {
        format!("Alpha: {} ({:.0}%)", a, a as f32 / 255.0 * 100.0)
    };

    Some(format!(
        "**HexColor**: `#{}`\n\nRed: {}\nGreen: {}\nBlue: {}\n{}",
        stripped.to_uppercase(),
        r, g, b,
        alpha_line,
    ))
}

fn expand_nibble(s: &str) -> Option<u8> {
    let nibble = u8::from_str_radix(s, 16).ok()?;
    Some(nibble << 4 | nibble)
}

// ── Section doc helper ─────────────────────────────────────────────────────────

fn section_doc(name: &str, description: &str) -> String {
    format!("**{}**\n\n{}", name, description)
}

// ── Token-at-position lookup ───────────────────────────────────────────────────

/// Returns the token and its index whose source span contains `pos`.
pub fn token_and_index_at(tokens: &[Token], pos: Position) -> Option<(&Token, usize)> {
    let target_line = pos.line as usize + 1;
    let target_col  = pos.character as usize + 1;

    let mut best: Option<(&Token, usize)> = None;

    for (i, token) in tokens.iter().enumerate() {
        if token.line != target_line {
            continue;
        }
        if token.column > target_col {
            break;
        }
        let value_len = token.get_token_value().len();
        if target_col <= token.column + value_len {
            best = Some((token, i));
        }
    }

    best
}

/// Compatibility shim used by goto_definition and other features that only need the token.
pub fn token_at(tokens: &[Token], pos: Position) -> Option<&Token> {
    token_and_index_at(tokens, pos).map(|(t, _)| t)
}

/// Find the first string literal token within the next few tokens after `start_index`.
/// Used to extract the argument to r:(...) and b:(...) constructors.
fn find_adjacent_string(tokens: &[Token], start_index: usize) -> Option<String> {
    for token in tokens.iter().skip(start_index + 1).take(4) {
        match &token.token_type {
            TokenType::String(s) | TokenType::StringSingle(s) => return Some(s.clone()),
            // Stop searching if we hit another meaningful construct.
            TokenType::Identifier(_) | TokenType::SectionData | TokenType::EndOfFile => break,
            _ => {}
        }
    }
    None
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
    fn hover_whitespace_does_not_panic() {
        let doc = test_doc("@DATA(\n  x = 1\n)");
        let _ = provide(Some(&doc), Position::new(1, 0));
    }

    #[test]
    fn token_at_returns_none_past_end_of_file() {
        let doc    = test_doc("@DATA(\n  x = 1\n)");
        let result = token_at(&doc.tokens, Position::new(999, 0));
        assert!(result.is_none());
    }

    #[test]
    fn hover_hex_color_parses_rgb() {
        let result = hover_hex_color("#FF5733");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("255"), "red channel should be 255");
        assert!(text.contains("fully opaque"), "alpha should be fully opaque");
    }

    #[test]
    fn hover_hex_color_parses_rgba() {
        let result = hover_hex_color("#FF573380");
        assert!(result.is_some());
        let text = result.unwrap();
        assert!(text.contains("128") || text.contains("50%"), "alpha ~50% should appear");
    }

    #[test]
    fn find_adjacent_string_finds_next_string() {
        use dixscript::Compiler::Core::Tokenizer::token::SectionId;
        let tokens = vec![
            Token::new(TokenType::RegexConstructor(String::new()), 1, 1, SectionId::Data),
            Token::new(TokenType::Symbol('('), 1, 3, SectionId::Data),
            Token::new(TokenType::String("[a-z]+".to_string()), 1, 4, SectionId::Data),
        ];
        let result = find_adjacent_string(&tokens, 0);
        assert_eq!(result, Some("[a-z]+".to_string()));
    }
}
