//! Document color provider.
//!
//! Scans the token stream for HexColor tokens and returns DocumentColor entries
//! so editors can display an inline color swatch.
//!
//! `color_presentation` receives the source range and returns a TextEdit
//! so the editor's color picker writes the new hex back to the file.
//!
//! Falls back to scanning the raw source text when the pipeline has not yet
//! populated the token stream (e.g. on first open before analysis completes).

use tower_lsp::lsp_types::{
    Color, ColorInformation, ColorPresentation, Position, Range, TextEdit,
};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::document::Document;

// ── Public entry point ────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Vec<ColorInformation> {
    let doc = match doc {
        Some(d) => d,
        None    => return vec![],
    };

    // Prefer the accurate token-based path when tokens are available.
    if !doc.tokens.is_empty() {
        return scan_tokens(&doc.tokens);
    }

    // Fallback: scan the raw source text.  This fires on the first
    // `document_color` request that arrives before the async pipeline
    // has finished populating doc.tokens.
    scan_source(&doc.source)
}

// ── Token-based scan ──────────────────────────────────────────────────────────

fn scan_tokens(tokens: &[Token]) -> Vec<ColorInformation> {
    let mut result = Vec::new();

    for token in tokens {
        if let TokenType::HexColor(hex) = &token.token_type {
            if let Some(color) = parse_hex_color(hex) {
                let line   = token.line.saturating_sub(1) as u32;
                let col    = token.column.saturating_sub(1) as u32;
                // hex already contains the '#' (the lexer stores start_pos at '#')
                let length = hex.len() as u32;

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

/// Walk the source text looking for `#[0-9A-Fa-f]{3|4|6|8}` sequences that
/// appear to be color literals (not preceded by an identifier character).
fn scan_source(source: &str) -> Vec<ColorInformation> {
    let mut result = Vec::new();

    for (line_idx, line) in source.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut col = 0usize;

        while col < bytes.len() {
            if bytes[col] == b'#' {
                // Ensure the '#' is not preceded by an identifier char
                // (guards against things like `url(#anchor)` if ever present).
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
                        // Ensure it is not followed by more hex digits (would
                        // mean it is part of a longer token, not a color).
                        let followed_by_hex = hex_end < bytes.len()
                            && (bytes[hex_end] as char).is_ascii_hexdigit();

                        if !followed_by_hex {
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

// ── Produce color presentation ────────────────────────────────────────────────

/// Called by the editor when the user picks a new color in the swatch.
/// `range` is the source range that originally contained the hex literal.
/// We return TextEdits so the editor replaces the old hex with the new one.
pub fn presentation(color: Color, range: Range) -> Vec<ColorPresentation> {
    let r = (color.red   * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue  * 255.0).round() as u8;
    let a = (color.alpha * 255.0).round() as u8;

    let rgb_label  = format!("#{:02X}{:02X}{:02X}", r, g, b);
    let rgba_label = format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a);

    let mut presentations = vec![ColorPresentation {
        label:             rgb_label.clone(),
        text_edit:         Some(TextEdit { range, new_text: rgb_label }),
        additional_text_edits: None,
    }];

    // Only offer the RGBA variant when alpha is not fully opaque.
    if a != 255 {
        presentations.push(ColorPresentation {
            label:             rgba_label.clone(),
            text_edit:         Some(TextEdit { range, new_text: rgba_label }),
            additional_text_edits: None,
        });
    }

    presentations
}

// ── Hex parsing ───────────────────────────────────────────────────────────────

/// Parses a DixScript HexColor string into an LSP Color.
/// Accepts the leading '#' or without it.
/// Supported: RGB (3), RGBA (4), RRGGBB (6), RRGGBBAA (8).
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');

    let (r, g, b, a): (u8, u8, u8, u8) = match hex.len() {
        3 => {
            let r = expand_nibble(hex.get(0..1)?)?;
            let g = expand_nibble(hex.get(1..2)?)?;
            let b = expand_nibble(hex.get(2..3)?)?;
            (r, g, b, 255)
        }
        4 => {
            let r = expand_nibble(hex.get(0..1)?)?;
            let g = expand_nibble(hex.get(1..2)?)?;
            let b = expand_nibble(hex.get(2..3)?)?;
            let a = expand_nibble(hex.get(3..4)?)?;
            (r, g, b, a)
        }
        6 => {
            let r = u8::from_str_radix(hex.get(0..2)?, 16).ok()?;
            let g = u8::from_str_radix(hex.get(2..4)?, 16).ok()?;
            let b = u8::from_str_radix(hex.get(4..6)?, 16).ok()?;
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(hex.get(0..2)?, 16).ok()?;
            let g = u8::from_str_radix(hex.get(2..4)?, 16).ok()?;
            let b = u8::from_str_radix(hex.get(4..6)?, 16).ok()?;
            let a = u8::from_str_radix(hex.get(6..8)?, 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };

    Some(Color {
        red:   r as f32 / 255.0,
        green: g as f32 / 255.0,
        blue:  b as f32 / 255.0,
        alpha: a as f32 / 255.0,
    })
}

/// Expands a single hex nibble character to a byte (e.g. "F" → 0xFF).
fn expand_nibble(s: &str) -> Option<u8> {
    let nibble = u8::from_str_radix(s, 16).ok()?;
    Some(nibble << 4 | nibble)
}