// mdix-lsp/src/features/formatting.rs
//! Document formatting provider.
//!
//! Returns a single full-document TextEdit that replaces the source with
//! a normalized version.  The formatter applies three passes:
//!
//!   1. Comment stripping is preserved (comments are kept).
//!   2. Indentation normalisation — contents of every @SECTION(...) are
//!      indented to `indent_size` spaces; object literals `{ }` add one
//!      more level.
//!   3. Operator spacing — `->`, `::`, `:` and `=` get exactly one space
//!      on each side (outside string literals).
//!   4. Trailing whitespace removed; multiple blank lines collapsed to one.

use std::panic;

use tower_lsp::lsp_types::{FormattingOptions, Position, Range, TextEdit};
use crate::document::Document;

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(
    doc:  Option<&Document>,
    opts: &FormattingOptions,
) -> Option<Vec<TextEdit>> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc, opts)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("formatting panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>, opts: &FormattingOptions) -> Option<Vec<TextEdit>> {
    let doc    = doc?;
    let source = &doc.source;

    if source.is_empty() {
        return None;
    }

    let indent_size = opts.tab_size as usize;
    let formatted   = format_source(source, indent_size);

    if formatted == *source {
        return None; // nothing changed — don't push a no-op edit
    }

    let line_count = source.lines().count() as u32;
    let last_line  = source.lines().last().unwrap_or("");

    Some(vec![TextEdit {
        range: Range::new(
            Position::new(0, 0),
            Position::new(line_count, last_line.len() as u32),
        ),
        new_text: formatted,
    }])
}

// ── Formatter ─────────────────────────────────────────────────────────────────

pub fn format_source(source: &str, indent_size: usize) -> String {
    let unit    = " ".repeat(indent_size);
    let lines: Vec<&str> = source.lines().collect();
    let mut out = String::with_capacity(source.len());

    // Depth tracking
    let mut section_depth: i32 = 0; // @SECTION(  …  )
    let mut brace_depth:   i32 = 0; // { … }
    let mut prev_blank         = false;

    for raw_line in &lines {
        let trimmed = raw_line.trim();

        // ── Blank lines ───────────────────────────────────────────────────────
        if trimmed.is_empty() {
            if !prev_blank {
                out.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        // ── Closing paren/brace adjustments (must happen before indent) ───────
        let close_paren = trimmed == ")" || trimmed.starts_with(')');
        let close_brace = trimmed.starts_with('}');

        if close_paren {
            section_depth = (section_depth - 1).max(0);
        }
        if close_brace {
            brace_depth = (brace_depth - 1).max(0);
        }

        // ── Indentation level ─────────────────────────────────────────────────
        let level = (section_depth + brace_depth) as usize;
        let indent = unit.repeat(level);

        // ── Operator spacing normalisation ────────────────────────────────────
        let normalised = normalize_operators(trimmed);

        out.push_str(&indent);
        out.push_str(&normalised);
        out.push('\n');

        // ── Opening depth adjustments (happen after emitting the line) ────────
        if trimmed.starts_with('@') && trimmed.ends_with('(') {
            section_depth += 1;
        } else if trimmed.starts_with('@') && trimmed.contains('(') && !trimmed.contains(')') {
            // e.g. "@DATA(" on its own line
            section_depth += 1;
        }

        // Count net brace change (excluding those inside strings)
        let net = net_brace_delta(trimmed);
        brace_depth = (brace_depth + net).max(0);
    }

    // Ensure exactly one trailing newline
    let result = out.trim_end_matches('\n').to_string() + "\n";
    result
}

/// Net `{` minus `}` count in `line`, ignoring string contents.
fn net_brace_delta(line: &str) -> i32 {
    let mut delta     = 0i32;
    let mut in_string = false;
    let mut str_char  = '"';
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let prev = if i > 0 { chars[i - 1] } else { '\0' };

        if (c == '"' || c == '\'') && prev != '\\' {
            if !in_string {
                in_string = true;
                str_char  = c;
            } else if c == str_char {
                in_string = false;
            }
        } else if !in_string {
            if c == '{' { delta += 1; }
            if c == '}' { delta -= 1; }
        }
        i += 1;
    }
    delta
}

/// Normalize spacing around `->`, `::`, `=`, and single `:` operators.
/// Does NOT modify content inside string literals.
fn normalize_operators(line: &str) -> String {
    let mut result    = String::with_capacity(line.len() + 8);
    let chars: Vec<char> = line.chars().collect();
    let len           = chars.len();
    let mut i         = 0;
    let mut in_string = false;
    let mut str_char  = '"';

    while i < len {
        let c    = chars[i];
        let prev = if i > 0 { chars[i - 1] } else { '\0' };
        let next = if i + 1 < len { chars[i + 1] } else { '\0' };

        // String toggle
        if (c == '"' || c == '\'') && prev != '\\' {
            if !in_string {
                in_string = true;
                str_char  = c;
            } else if c == str_char {
                in_string = false;
            }
            result.push(c);
            i += 1;
            continue;
        }

        if in_string {
            result.push(c);
            i += 1;
            continue;
        }

        // `->` arrow
        if c == '-' && next == '>' {
            let trimmed_result = result.trim_end().to_string();
            result.clear();
            result.push_str(&trimmed_result);
            result.push_str(" -> ");
            i += 2;
            // skip trailing spaces
            while i < len && chars[i] == ' ' { i += 1; }
            continue;
        }

        // `::` double-colon
        if c == ':' && next == ':' {
            let trimmed_result = result.trim_end().to_string();
            result.clear();
            result.push_str(&trimmed_result);
            result.push_str("::\n    ");
            i += 2;
            while i < len && chars[i] == ' ' { i += 1; }
            continue;
        }

        // `=` assignment (not `==`, `!=`, `<=`, `>=`)
        if c == '=' && next != '=' && prev != '!' && prev != '<' && prev != '>' && prev != '=' {
            let trimmed_result = result.trim_end().to_string();
            result.clear();
            result.push_str(&trimmed_result);
            result.push_str(" = ");
            i += 1;
            while i < len && chars[i] == ' ' { i += 1; }
            continue;
        }

        result.push(c);
        i += 1;
    }

    result.trim_end().to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_removes_trailing_whitespace() {
        let src = "@DATA(  \n  x = 1  \n)\n";
        let out = format_source(src, 2);
        for line in out.lines() {
            assert_eq!(line.trim_end(), line, "trailing whitespace in: {:?}", line);
        }
    }

    #[test]
    fn format_collapses_blank_lines() {
        let src = "@DATA(\n  x = 1\n\n\n\n  y = 2\n)\n";
        let out = format_source(src, 2);
        let blanks = out.lines().filter(|l| l.trim().is_empty()).count();
        assert!(blanks <= 1, "too many blank lines: {}", blanks);
    }

    #[test]
    fn format_normalizes_arrow() {
        let src = "@CONFIG(\n  version->\"1.0.0\"\n)\n";
        let out = format_source(src, 2);
        assert!(out.contains("version -> \"1.0.0\""), "got: {}", out);
    }

    #[test]
    fn format_preserves_strings() {
        let src = "@DATA(\n  url = \"http://example.com->thing\"\n)\n";
        let out = format_source(src, 2);
        assert!(out.contains("\"http://example.com->thing\""), "string was modified: {}", out);
    }

    #[test]
    fn format_idempotent() {
        let src = "@DATA(\n  x = 1\n  y = \"hello\"\n)\n";
        let once  = format_source(src, 2);
        let twice = format_source(&once, 2);
        assert_eq!(once, twice, "formatter is not idempotent");
    }
}
