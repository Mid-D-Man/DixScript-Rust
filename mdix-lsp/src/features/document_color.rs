//! Document color provider.
//!
//! Scans the token stream for HexColor tokens and returns DocumentColor entries
//! so editors can display an inline color swatch.
//! color_presentation converts an edited RGBA value back to a DixScript hex string.

use tower_lsp::lsp_types::{
    Color, ColorInformation, ColorPresentation, Position, Range,
};
use dixscript::Compiler::Core::Tokenizer::TokenType;
use crate::document::Document;

pub fn provide(doc: Option<&Document>) -> Vec<ColorInformation> {
    let doc = match doc {
        Some(d) => d,
        None    => return vec![],
    };

    let mut result = Vec::new();

    for token in &doc.tokens {
        if let TokenType::HexColor(hex) = &token.token_type {
            if let Some(color) = parse_hex_color(hex) {
                // Token position is 1-based; LSP Range is 0-based.
                let line   = token.line.saturating_sub(1) as u32;
                let col    = token.column.saturating_sub(1) as u32;
                // +1 for the leading '#'
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

/// Called by the editor when the user picks a new color in the swatch.
/// Converts the RGBA float color back to a DixScript hex string.
pub fn presentation(color: Color) -> Vec<ColorPresentation> {
    let r = (color.red   * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue  * 255.0).round() as u8;
    let a = (color.alpha * 255.0).round() as u8;

    let label = if a == 255 {
        format!("#{:02X}{:02X}{:02X}", r, g, b)
    } else {
        format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a)
    };

    vec![ColorPresentation {
        label,
        text_edit:               None,
        additional_text_edits:   None,
    }]
}

// ── Hex parsing ───────────────────────────────────────────────────────────────

/// Parses a DixScript HexColor string (without the leading '#') into an LSP Color.
///
/// Supported formats: RGB (3), RGBA (4), RRGGBB (6), RRGGBBAA (8).
fn parse_hex_color(hex: &str) -> Option<Color> {
    // Strip leading '#' if present (the lexer may include it in the stored string).
    let hex = hex.trim_start_matches('#');

    let (r, g, b, a): (u8, u8, u8, u8) = match hex.len() {
        3 => {
            // RGB shorthand — expand each nibble.
            let r = expand_nibble(hex.get(0..1)?)?;
            let g = expand_nibble(hex.get(1..2)?)?;
            let b = expand_nibble(hex.get(2..3)?)?;
            (r, g, b, 255)
        }
        4 => {
            // RGBA shorthand.
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
