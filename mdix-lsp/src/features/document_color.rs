// mdix-lsp/src/features/document_color.rs
//! Document color provider.
//!
//! ## Range note
//! HexColor tokens are stored WITHOUT the leading '#' in the token value,
//! but the source text includes '#'. All ranges must therefore use
//! `hex.len() + 1` to cover the full literal including '#'.

use std::panic;

use tower_lsp::lsp_types::{Color, ColorInformation, ColorPresentation, Position, Range, TextEdit};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

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

// ── Token-based scan ──────────────────────────────────────────────────────────

fn scan_tokens(tokens: &[Token]) -> Vec<ColorInformation> {
    let mut result = Vec::new();
    for token in tokens {
        if let TokenType::HexColor(hex) = &token.token_type {
            if let Some(color) = parse_hex_color(hex) {
                let line = token.line.saturating_sub(1) as u32;
                let col  = token.column.saturating_sub(1) as u32;

                // HexColor is stored WITHOUT '#' (token_length in semantic_tokens.rs
                // confirms this by adding +1 to h.len()). The source text IS:
                //   #FF5733   → col points to '#', length = 6 + 1 = 7
                //   #FF573380 → col points to '#', length = 8 + 1 = 9
                let length = hex.len() as u32 + 1;

                result.push(ColorInformation {
                    range: Range::new(
                        Position::new(line, col),
                        Position::new(line, col + length),
                    ),
                    color,
                });
            }
        }
    }
    result
}

// ── Source-text fallback scan ─────────────────────────────────────────────────
// Used when no tokens are available (e.g. before first analysis completes).

fn scan_source(source: &str) -> Vec<ColorInformation> {
    let mut result = Vec::new();
    for (line_idx, line) in source.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut col = 0usize;
        while col < bytes.len() {
            if bytes[col] == b'#' {
                // Don't match '#' that follows an identifier char (e.g. CSS ID selectors).
                let preceded_by_ident = col > 0 && {
                    let prev = bytes[col - 1] as char;
                    prev.is_alphanumeric() || prev == '_'
                };
                if !preceded_by_ident {
                    let hex_start = col + 1; // skip '#'
                    let mut hex_end = hex_start;
                    while hex_end < bytes.len()
                        && (bytes[hex_end] as char).is_ascii_hexdigit()
                        && hex_end - hex_start < 8
                    {
                        hex_end += 1;
                    }
                    let hex_len = hex_end - hex_start;
                    if matches!(hex_len, 3 | 4 | 6 | 8) {
                        // Make sure no additional hex digit follows.
                        let followed = hex_end < bytes.len()
                            && (bytes[hex_end] as char).is_ascii_hexdigit();
                        if !followed {
                            let hex_str = &line[col..hex_end]; // includes '#'
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

// ── Color presentation (picker → text edit) ───────────────────────────────────

pub fn presentation(color: Color, range: Range) -> Vec<ColorPresentation> {
    let r = (color.red   * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue  * 255.0).round() as u8;
    let a = (color.alpha * 255.0).round() as u8;

    let rgb_label  = format!("#{:02X}{:02X}{:02X}", r, g, b);
    let rgba_label = format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a);

    // Determine original format from the range length so we can offer the
    // most appropriate replacement first.
    let range_len = range.end.character.saturating_sub(range.start.character) as usize;

    // Was the original an 8-char hex (with alpha)?
    let original_had_alpha = range_len == 9; // '#' + 8 hex digits

    if original_had_alpha || a != 255 {
        // Offer RGBA first when the original had alpha or the picked color has transparency.
        vec![
            ColorPresentation {
                label:                 rgba_label.clone(),
                text_edit:             Some(TextEdit { range, new_text: rgba_label }),
                additional_text_edits: None,
            },
            ColorPresentation {
                label:                 rgb_label.clone(),
                text_edit:             Some(TextEdit { range, new_text: rgb_label }),
                additional_text_edits: None,
            },
        ]
    } else {
        // Fully opaque — offer RGB only (keeps output compact).
        // Also offer RGBA so the user can add transparency.
        vec![
            ColorPresentation {
                label:                 rgb_label.clone(),
                text_edit:             Some(TextEdit { range, new_text: rgb_label }),
                additional_text_edits: None,
            },
            ColorPresentation {
                label:                 rgba_label.clone(),
                text_edit:             Some(TextEdit { range, new_text: rgba_label }),
                additional_text_edits: None,
            },
        ]
    }
}

// ── Hex color parser ──────────────────────────────────────────────────────────

/// Parse a hex color string (with or without leading '#') into an LSP Color.
/// Supports 3, 4, 6, and 8 hex-digit forms.
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let digits = hex.trim_start_matches('#');

    let (r, g, b, a): (u8, u8, u8, u8) = match digits.len() {
        3 => {
            let expand = |s: &str| -> Option<u8> {
                u8::from_str_radix(s, 16).ok().map(|n| n << 4 | n)
            };
            (expand(&digits[0..1])?, expand(&digits[1..2])?, expand(&digits[2..3])?, 255)
        }
        4 => {
            let expand = |s: &str| -> Option<u8> {
                u8::from_str_radix(s, 16).ok().map(|n| n << 4 | n)
            };
            (
                expand(&digits[0..1])?,
                expand(&digits[1..2])?,
                expand(&digits[2..3])?,
                expand(&digits[3..4])?,
            )
        }
        6 => (
            u8::from_str_radix(&digits[0..2], 16).ok()?,
            u8::from_str_radix(&digits[2..4], 16).ok()?,
            u8::from_str_radix(&digits[4..6], 16).ok()?,
            255,
        ),
        8 => (
            u8::from_str_radix(&digits[0..2], 16).ok()?,
            u8::from_str_radix(&digits[2..4], 16).ok()?,
            u8::from_str_radix(&digits[4..6], 16).ok()?,
            u8::from_str_radix(&digits[6..8], 16).ok()?,
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
