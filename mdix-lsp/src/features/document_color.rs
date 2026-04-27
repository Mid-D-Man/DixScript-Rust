// mdix-lsp/src/features/document_color.rs
//! Document color provider. Wrapped in catch_unwind.

use std::panic;

use tower_lsp::lsp_types::{Color, ColorInformation, ColorPresentation, Position, Range, TextEdit};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::document::Document;

pub fn provide(doc: Option<&Document>) -> Vec<ColorInformation> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("document_color panicked: {}", msg);
            vec![]
        }
    }
}

fn provide_inner(doc: Option<&Document>) -> Vec<ColorInformation> {
    let doc = match doc { Some(d) => d, None => return vec![] };

    if !doc.tokens.is_empty() {
        return scan_tokens(&doc.tokens);
    }
    scan_source(&doc.source)
}

fn scan_tokens(tokens: &[Token]) -> Vec<ColorInformation> {
    let mut result = Vec::new();
    for token in tokens {
        if let TokenType::HexColor(hex) = &token.token_type {
            if let Some(color) = parse_hex_color(hex) {
                let line   = token.line.saturating_sub(1) as u32;
                let col    = token.column.saturating_sub(1) as u32;
                let length = hex.len() as u32;
                result.push(ColorInformation {
                    range: Range::new(Position::new(line, col), Position::new(line, col + length)),
                    color,
                });
            }
        }
    }
    result
}

fn scan_source(source: &str) -> Vec<ColorInformation> {
    let mut result = Vec::new();
    for (line_idx, line) in source.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut col = 0usize;
        while col < bytes.len() {
            if bytes[col] == b'#' {
                let preceded_by_ident = col > 0 && {
                    let prev = bytes[col - 1] as char;
                    prev.is_alphanumeric() || prev == '_'
                };
                if !preceded_by_ident {
                    let hex_start = col + 1;
                    let mut hex_end = hex_start;
                    while hex_end < bytes.len()
                        && (bytes[hex_end] as char).is_ascii_hexdigit()
                        && hex_end - hex_start < 8
                    {
                        hex_end += 1;
                    }
                    let hex_len = hex_end - hex_start;
                    if matches!(hex_len, 3 | 4 | 6 | 8) {
                        let followed = hex_end < bytes.len()
                            && (bytes[hex_end] as char).is_ascii_hexdigit();
                        if !followed {
                            let hex_str = &line[col..hex_end];
                            if let Some(color) = parse_hex_color(hex_str) {
                                result.push(ColorInformation {
                                    range: Range::new(
                                        Position::new(line_idx as u32, col as u32),
                                        Position::new(line_idx as u32, hex_end as u32),
                                    ),
                                    color,
                                });
                            }
                        }
                    }
                }
            }
            col += 1;
        }
    }
    result
}

pub fn presentation(color: Color, range: Range) -> Vec<ColorPresentation> {
    let r = (color.red   * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue  * 255.0).round() as u8;
    let a = (color.alpha * 255.0).round() as u8;

    let rgb_label  = format!("#{:02X}{:02X}{:02X}", r, g, b);
    let rgba_label = format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a);

    let mut out = vec![ColorPresentation {
        label:                 rgb_label.clone(),
        text_edit:             Some(TextEdit { range, new_text: rgb_label }),
        additional_text_edits: None,
    }];

    if a != 255 {
        out.push(ColorPresentation {
            label:                 rgba_label.clone(),
            text_edit:             Some(TextEdit { range, new_text: rgba_label }),
            additional_text_edits: None,
        });
    }
    out
}

pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    let (r, g, b, a): (u8, u8, u8, u8) = match hex.len() {
        3 => {
            let expand = |s: &str| u8::from_str_radix(s, 16).ok().map(|n| n << 4 | n);
            (expand(&hex[0..1])?, expand(&hex[1..2])?, expand(&hex[2..3])?, 255)
        }
        4 => {
            let expand = |s: &str| u8::from_str_radix(s, 16).ok().map(|n| n << 4 | n);
            (expand(&hex[0..1])?, expand(&hex[1..2])?, expand(&hex[2..3])?,
             expand(&hex[3..4])?)
        }
        6 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            255,
        ),
        8 => (
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
            u8::from_str_radix(&hex[6..8], 16).ok()?,
        ),
        _ => return None,
    };
    Some(Color {
        red:   r as f32 / 255.0,
        green: g as f32 / 255.0,
        blue:  b as f32 / 255.0,
        alpha: a as f32 / 255.0,
    })
}
