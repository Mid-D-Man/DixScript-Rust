// mdix-lsp/src/features/document_color.rs
use std::panic;

use tower_lsp::lsp_types::{
    Color, ColorInformation, ColorPresentation, Position, Range,
};
use dixscript::Compiler::Core::Tokenizer::TokenType;

use crate::document::Document;

pub fn provide(doc: Option<&Document>) -> Vec<ColorInformation> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)));
    result.unwrap_or_else(|payload| {
        let msg = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown panic".to_string());
        tracing::error!("document_color panicked: {}", msg);
        vec![]
    })
}

fn provide_inner(doc: Option<&Document>) -> Vec<ColorInformation> {
    let doc = match doc {
        Some(d) => d,
        None    => return vec![],
    };

    let mut colors: Vec<ColorInformation> = Vec::new();

    for token in &doc.tokens {
        let hex = match &token.token_type {
            TokenType::HexColor(h) => h,
            _                      => continue,
        };

        let lsp_color = match parse_hex_color(hex) {
            Some(c) => c,
            None    => continue,
        };

        // Token positions are 1-based; LSP positions are 0-based.
        let line = token.line.saturating_sub(1) as u32;
        let col  = token.column.saturating_sub(1) as u32;

        // HexColor token value includes the '#' prefix.
        // Length = full stored string length (e.g. "#FF5733" = 7 chars).
        let length = hex.len() as u32;

        colors.push(ColorInformation {
            range: Range::new(
                Position::new(line, col),
                Position::new(line, col + length),
            ),
            color: lsp_color,
        });
    }

    colors
}

// 2026-08-07 — ROOT CAUSE of "can't add alpha via the color picker from a
// color that started without one": VS Code applies presentations[0] as the
// actual document text edit automatically -- the rest of the list is only
// reachable through the small format-cycle control next to the picker, not
// applied by default. This function always put hex_rgb (no alpha) first,
// regardless of what alpha the user had actually just dragged the slider
// to. So: drag alpha down -> Color{alpha: 0.5} comes in here -> we compute
// hex_rgba correctly -> but presentations[0] was still the alpha-less
// 6-digit form -> THAT's what got written to the document -> the alpha the
// user just picked was silently thrown away, on every single edit. Next
// time the picker opened, it parsed back from that same alpha-less text and
// started fresh at full opacity -- looking exactly like "can't add alpha at
// all", because it never actually landed.
//
// Fix: order by whether the color is actually opaque. Once alpha isn't
// 255/fully-opaque, the 8-digit form goes first, so IT's what gets written.
// This is also the most likely cause of the picker closing after one edit:
// flipping between a 6-digit and 8-digit replacement changes the edited
// range's text length on every interaction, which can be enough to make the
// picker lose track of the range it's anchored to.
pub fn presentation(color: Color, range: Range) -> Vec<ColorPresentation> {
    let r = (color.red   * 255.0).round() as u8;
    let g = (color.green * 255.0).round() as u8;
    let b = (color.blue  * 255.0).round() as u8;
    let a = (color.alpha * 255.0).round() as u8;

    let hex_rgb  = format!("#{:02X}{:02X}{:02X}",       r, g, b);
    let hex_rgba = format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a);

    let mut presentations = if a != 255 {
        vec![make_presentation(hex_rgba, range), make_presentation(hex_rgb, range)]
    } else {
        vec![make_presentation(hex_rgb, range), make_presentation(hex_rgba, range)]
    };

    // 3-digit shorthand when both nibbles of each channel are equal.
    if (r >> 4) == (r & 0x0F)
        && (g >> 4) == (g & 0x0F)
        && (b >> 4) == (b & 0x0F)
    {
        let hex3 = format!("#{:X}{:X}{:X}", r >> 4, g >> 4, b >> 4);
        presentations.push(make_presentation(hex3, range));
    }

    presentations
}

fn parse_hex_color(hex: &str) -> Option<Color> {
    let digits = hex.trim_start_matches('#');

    let (r, g, b, a): (u8, u8, u8, u8) = match digits.len() {
        3 => {
            let expand = |s: &str| -> Option<u8> {
                u8::from_str_radix(s, 16).ok().map(|n| (n << 4) | n)
            };
            (expand(&digits[0..1])?, expand(&digits[1..2])?, expand(&digits[2..3])?, 255)
        }
        4 => {
            let expand = |s: &str| -> Option<u8> {
                u8::from_str_radix(s, 16).ok().map(|n| (n << 4) | n)
            };
            (expand(&digits[0..1])?, expand(&digits[1..2])?, expand(&digits[2..3])?, expand(&digits[3..4])?)
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

fn make_presentation(label: String, range: Range) -> ColorPresentation {
    use tower_lsp::lsp_types::TextEdit;
    ColorPresentation {
        label:                label.clone(),
        text_edit:            Some(TextEdit { range, new_text: label }),
        additional_text_edits: None,
    }
}
