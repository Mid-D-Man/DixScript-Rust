// mdix-lsp/src/features/document_color.rs
//! Document color provider.
//!
//! ## Range note
//! HexColor tokens may store the value WITH or WITHOUT the leading '#'.
//! We always normalise via `trim_start_matches('#')` so that the computed
//! length is always `hex_digits + 1`, covering the full `#RRGGBB` literal
//! regardless of how the lexer stores it.
//!
//! ## CONFIG section
//! @CONFIG is stripped before tokenisation, so HexColor tokens are never
//! emitted for CONFIG lines.  We supplement token-based results with a
//! lightweight source-text scan of the CONFIG line range.

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
    let mut result = Vec::new();

    if !doc.tokens.is_empty() {
        // Primary scan: HexColor tokens from all tokenised sections.
        result.extend(scan_tokens(&doc.tokens));

        // Supplementary scan: @CONFIG section has no tokens — check source.
        if let Some((start, end)) = doc.config_line_range {
            result.extend(scan_source_range(&doc.source, start, end));
        }
    } else {
        // No tokens yet (document not yet analysed): scan full source text.
        result.extend(scan_source(&doc.source));
    }

    // Remove any duplicate positions that could arise from combined scanning.
    result.sort_by_key(|c| (c.range.start.line, c.range.start.character));
    result.dedup_by(|a, b| {
        a.range.start.line      == b.range.start.line &&
        a.range.start.character == b.range.start.character
    });

    tracing::debug!("document_color: {} color(s) found", result.len());
    result
}

// ── Token-based scan ──────────────────────────────────────────────────────────

fn scan_tokens(tokens: &[Token]) -> Vec<ColorInformation> {
    let mut result = Vec::new();
    for token in tokens {
        if let TokenType::HexColor(hex) = &token.token_type {
            if let Some(color) = parse_hex_color(hex) {
                let line = token.line.saturating_sub(1) as u32;
                let col  = token.column.saturating_sub(1) as u32;

                // Normalise: strip any leading '#' before computing digit count.
                // The lexer may store "FF5733" or "#FF5733" depending on version;
                // we always want length = digits + 1 to cover the full "#RRGGBB".
                let digits = hex.trim_start_matches('#');
                let length = digits.len() as u32 + 1;

                tracing::debug!(
                    "document_color token: #{} at L{}:C{} length={}",
                    digits.to_uppercase(), line + 1, col + 1, length
                );

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

// ── Source range scan (used for @CONFIG and no-token fallback) ────────────────

/// Scan only the lines in `[start_lsp, end_lsp]` (0-based LSP line indices).
fn scan_source_range(source: &str, start_lsp: u32, end_lsp: u32) -> Vec<ColorInformation> {
    let mut result = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let lsp_line = idx as u32;
        if lsp_line < start_lsp { continue; }
        if lsp_line > end_lsp   { break;    }
        result.extend(scan_line(line, lsp_line));
    }
    result
}

/// Full source-text scan — used when no tokens are available.
fn scan_source(source: &str) -> Vec<ColorInformation> {
    let mut result = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        result.extend(scan_line(line, idx as u32));
    }
    result
}

/// Scan a single source line for `#RRGGBB` / `#RGB` / `#RRGGBBAA` / `#RGBA` patterns.
fn scan_line(line: &str, lsp_line: u32) -> Vec<ColorInformation> {
    let mut result  = Vec::new();
    let bytes       = line.as_bytes();
    let mut col     = 0usize;

    while col < bytes.len() {
        if bytes[col] != b'#' {
            col += 1;
            continue;
        }

        // Don't match '#' that immediately follows an identifier character.
        if col > 0 {
            let prev = bytes[col - 1] as char;
            if prev.is_alphanumeric() || prev == '_' {
                col += 1;
                continue;
            }
        }

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
            // Ensure no additional hex digit follows.
            let followed = hex_end < bytes.len()
                && (bytes[hex_end] as char).is_ascii_hexdigit();
            if !followed {
                let hex_str = &line[col..hex_end]; // includes '#'
                if let Some(color) = parse_hex_color(hex_str) {
                    let length = (hex_len as u32) + 1; // digits + '#'
                    result.push(ColorInformation {
                        range: Range::new(
                            Position::new(lsp_line, col as u32),
                            Position::new(lsp_line, col as u32 + length),
                        ),
                        color,
                    });
                }
            }
        }
        col += 1;
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

    // Determine whether the original was 8-digit (had an alpha channel)
    // by checking the range width.
    let range_len = range.end.character.saturating_sub(range.start.character) as usize;
    let original_had_alpha = range_len == 9; // '#' + 8 hex digits

    if original_had_alpha || a != 255 {
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
