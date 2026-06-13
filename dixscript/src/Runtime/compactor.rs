// dixscript/src/Runtime/compactor.rs
//! Utilities for compacting and minifying DixScript files.
//!
//! ## Why token-based `minify`?
//!
//! DixScript has complex lexical rules (kebab-case identifiers, `b:(…)` / `t:(…)`
//! prefixed constructors, interpolated strings `$"…"`, multi-char operators
//! `->` / `::`, section keywords `@DATA(`, etc.).  A character-by-character
//! scanner must re-implement large parts of the lexer to know whether two
//! adjacent characters need a separator space.
//!
//! `minify` uses the DixScript tokenizer and applies one rule: insert a space
//! between two non-empty rendered tokens only when the **last character of the
//! previous token** AND the **first character of the current token** are both
//! "word characters" (alphanumeric or `_`).  This correctly prevents e.g.
//! `789table:` and `trueother` while avoiding spurious spaces around `->`, `=`,
//! `::`, `(`, etc.

use crate::Compiler::Core::Tokenizer::{Tokenizer, Token, TokenType};
use crate::Compiler::Core::Config::OperationalSettings;

/// `true` for alphanumeric characters and `_`.
#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Render one token to its minimal source representation.
///
/// Overrides `Token::get_token_value` for cases where the default is lossy or
/// uses the `Display` fallback (which renders the debug-style
/// `"VariantName(...)"` form instead of the actual source syntax):
///
/// * `Double(d)` — Rust's `f64::to_string` drops `.0` for whole numbers
///   (`4.0` → `"4"`), which re-parses as Integer and silently changes the type.
///   We force `"4.0"`.
///
/// * `Float(f)` — `f32::to_string` omits the required `f` suffix (`3.14f`→`"3.14"`),
///   which re-parses as Double.  We append `"f"`.
///
/// * `String(s)` / `StringSingle(s)` — `get_token_value()` returns the raw
///   inner content with **no surrounding quotes**, so `"Hello World"` minified
///   to `Hello World` — a SEVERE bug: it changes the token stream from a single
///   String token into multiple Identifier tokens on re-lex. We restore the
///   `"..."` / `'...'` delimiters.
///
/// * `InterpolatedString(s)` — same issue; restore the `$"..."` wrapper.
///
/// * `SectionConfig` / `SectionImports` / `SectionDLM` / `SectionEnums` /
///   `SectionQuickFuncs` / `SectionData` / `SectionSecurity` — these have no
///   arm in `get_token_value()`'s explicit match, so they fall through to the
///   `_ => self.token_type.to_string()` fallback, which uses `Display` and
///   produces `"SectionConfig(@CONFIG)"` instead of the actual source text
///   `"@CONFIG"`. We render the real `@SECTION` keyword.
///
/// Returns an empty string for tokens that produce no output in minified text
/// (comments, EOF, parse-context markers).
fn render_token(token: &Token) -> String {
    match &token.token_type {
        TokenType::Double(d) => {
            if d.is_finite() && d.fract() == 0.0 {
                format!("{:.1}", d) // "4.0" not "4"
            } else {
                format!("{}", d)
            }
        }
        TokenType::Float(f) => {
            // Append 'f' suffix so re-parse produces Float, not Double.
            format!("{}f", f)
        }

        // FIX: restore quote delimiters — get_token_value() returns the raw
        // inner content with no quotes, which corrupts the token stream.
        TokenType::String(s) => format!("\"{}\"", s),
        TokenType::StringSingle(s) => format!("'{}'", s),

        // FIX: restore the `$"..."` interpolation wrapper.
        TokenType::InterpolatedString(s) => format!("$\"{}\"", s),

        // FIX: section keywords — render the actual `@SECTION` source text
        // instead of the Display fallback `"SectionConfig(@CONFIG)"`.
        TokenType::SectionConfig     => "@CONFIG".to_string(),
        TokenType::SectionImports    => "@IMPORTS".to_string(),
        TokenType::SectionDLM        => "@DLM".to_string(),
        TokenType::SectionEnums      => "@ENUMS".to_string(),
        TokenType::SectionQuickFuncs => "@QUICKFUNCS".to_string(),
        TokenType::SectionData       => "@DATA".to_string(),
        TokenType::SectionSecurity   => "@SECURITY".to_string(),

        // No output in minified form.
        TokenType::Comment(_) | TokenType::EndOfFile | TokenType::ParseContext(_) => {
            String::new()
        }
        // All other tokens: use the canonical rendering already in Token.
        _ => token.get_token_value(),
    }
}

pub struct DixCompactor;

impl DixCompactor {
    /// Minify DixScript content — remove all unnecessary whitespace.
    ///
    /// Uses the DixScript tokenizer so that keyword, identifier, and literal
    /// boundaries are always respected.  A single space is inserted between two
    /// consecutive tokens only when both their adjacent characters are word chars.
    ///
    /// Preserves:
    /// - String contents (whitespace and `//` inside strings are kept verbatim)
    /// - Mandatory spaces between adjacent word tokens (`true other` ≠ `trueother`)
    pub fn minify(content: &str) -> String {
        if content.trim().is_empty() {
            return String::new();
        }

        let settings = OperationalSettings::default();
        let tokenizer = Tokenizer::new(content, &settings);
        let tok_result = tokenizer.tokenize();

        let mut result = String::with_capacity(content.len());
        let mut prev_rendered: Option<String> = None;

        for token in &tok_result.tokens {
            let rendered = render_token(token);
            if rendered.is_empty() {
                continue;
            }

            if let Some(ref prev) = prev_rendered {
                let prev_ends_word  = prev.chars().last().map(is_word_char).unwrap_or(false);
                let curr_starts_word = rendered.chars().next().map(is_word_char).unwrap_or(false);
                if prev_ends_word && curr_starts_word {
                    result.push(' ');
                }
            }

            result.push_str(&rendered);
            prev_rendered = Some(rendered);
        }

        result
    }

    /// Compact DixScript — remove trailing whitespace and collapse consecutive
    /// blank lines to at most one.  Does NOT modify indentation or code.
    pub fn compact(content: &str) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let mut result = String::with_capacity(content.len());
        let mut consecutive_blank = 0usize;

        for line in &lines {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                consecutive_blank += 1;
                if consecutive_blank <= 1 {
                    result.push('\n');
                }
            } else {
                consecutive_blank = 0;
                result.push_str(trimmed);
                result.push('\n');
            }
        }

        result
    }

    /// Remove single-line (`//`) and multi-line (`/* */`) comments from
    /// DixScript source.  Comment markers inside string literals are preserved.
    pub fn remove_comments(content: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let chars: Vec<char> = content.chars().collect();
        let mut i = 0;

        let mut in_string  = false;
        let mut string_char = '\0';

        while i < chars.len() {
            let c    = chars[i];
            let next = if i + 1 < chars.len() { chars[i + 1] } else { '\0' };
            let prev = if i > 0 { chars[i - 1] } else { '\0' };

            // Track string context (handles interpolated strings — `$` is a
            // plain character and the `"` that follows sets in_string normally).
            if (c == '"' || c == '\'') && prev != '\\' {
                if !in_string {
                    in_string  = true;
                    string_char = c;
                } else if c == string_char {
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

            // Single-line comment
            if c == '/' && next == '/' {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }

            // Multi-line comment
            if c == '/' && next == '*' {
                i += 2;
                while i + 1 < chars.len() {
                    if chars[i] == '*' && chars[i + 1] == '/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            result.push(c);
            i += 1;
        }

        result
    }

    /// Compression ratio in `[0.0, 1.0]`.  `1.0` = 100 % reduction.
    pub fn get_compression_ratio(original: &str, compressed: &str) -> f64 {
        if original.is_empty() {
            return 0.0;
        }
        1.0 - (compressed.len() as f64 / original.len() as f64)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── minify: core correctness ──────────────────────────────────────────────

    #[test]
    fn test_minify_basic_config() {
        let input  = "@CONFIG(\n  version -> \"1.0.0\"\n)";
        let output = DixCompactor::minify(input);
        assert_eq!(output, "@CONFIG(version->\"1.0.0\")");
    }

    #[test]
    fn test_minify_preserves_strings_with_spaces() {
        let input  = "name = \"Hello   World\"";
        let output = DixCompactor::minify(input);
        assert_eq!(output, "name=\"Hello   World\"");
    }

    #[test]
    fn test_minify_keeps_space_between_let_and_identifier() {
        let input  = "let x = 5";
        let output = DixCompactor::minify(input);
        assert_eq!(output, "let x=5");
    }

    /// Regression: `789\n  table:` must not fuse to `789table:`.
    #[test]
    fn test_minify_no_fusion_integer_table_path() {
        let input  = "@DATA(\n  count = 789\n  table: host = \"x\"\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains("789table"),
            "integer and table-path fused — got: {output}"
        );
        assert!(
            output.contains("789 table:"),
            "expected '789 table:' in output, got: {output}"
        );
    }

    /// Regression: `true\n  other` must not fuse to `trueother`.
    #[test]
    fn test_minify_no_fusion_bool_identifier() {
        let input  = "@DATA(\n  flag = true\n  other = 5\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains("trueother"),
            "bool and identifier fused — got: {output}"
        );
        assert!(
            output.contains("true") && output.contains("other"),
            "tokens gone — got: {output}"
        );
    }

    /// Wide indentation must not fuse adjacent tokens.
    #[test]
    fn test_minify_deep_indentation_no_fusion() {
        let input  = "@DATA(\n      deeply = 1\n      nested = 2\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains("1nested"),
            "deep-indent tokens fused — got: {output}"
        );
    }

    /// Two keyword tokens (`let result`) must retain a separator.
    #[test]
    fn test_minify_keyword_identifier_space() {
        let input  = "let result = 42";
        let output = DixCompactor::minify(input);
        assert!(output.contains("let result"), "got: {output}");
    }

    // ── minify: comment stripping ─────────────────────────────────────────────

    #[test]
    fn test_minify_strips_single_line_comments() {
        let input  = "x = 5 // comment\ny = 10";
        let output = DixCompactor::minify(input);
        assert!(!output.contains("comment"), "got: {output}");
        assert!(output.contains("x=5"),      "got: {output}");
        assert!(output.contains("y=10"),     "got: {output}");
    }

    #[test]
    fn test_minify_strips_multi_line_comments() {
        let input  = "x = 5 /* a multi\nline comment */ y = 10";
        let output = DixCompactor::minify(input);
        assert!(!output.contains("multi"), "got: {output}");
        assert!(output.contains("x=5"),    "got: {output}");
        assert!(output.contains("y=10"),   "got: {output}");
    }

    /// `//` inside a string literal must NOT be stripped.
    #[test]
    fn test_minify_preserves_url_in_string() {
        let input  = "url = \"https://example.com/path\"";
        let output = DixCompactor::minify(input);
        assert!(
            output.contains("https://example.com/path"),
            "URL inside string was incorrectly stripped — got: {output}"
        );
    }

    // ── minify: operator spacing ──────────────────────────────────────────────

    #[test]
    fn test_minify_arrow_operator_no_spaces() {
        let input  = "@CONFIG(\n  version -> \"2.0\"\n)";
        let output = DixCompactor::minify(input);
        assert!(output.contains("version->\"2.0\""), "got: {output}");
    }

    #[test]
    fn test_minify_double_colon_array() {
        let input  = "@DATA(\n  tags:: \"a\", \"b\"\n)";
        let output = DixCompactor::minify(input);
        assert!(output.contains("tags::"), "got: {output}");
    }

    // ── minify: edge cases ────────────────────────────────────────────────────

    #[test]
    fn test_minify_empty_and_whitespace_only() {
        assert_eq!(DixCompactor::minify(""), "");
        assert_eq!(DixCompactor::minify("   \n  \n"), "");
    }

    #[test]
    fn test_minify_only_comments_returns_empty() {
        let input  = "// single line\n/* multi\nline */";
        let output = DixCompactor::minify(input);
        assert!(output.trim().is_empty(), "expected empty, got: {output:?}");
    }

    // ── minify: new coverage for the String/Section fixes ─────────────────────

    /// Single-quoted strings must keep their delimiters.
    #[test]
    fn test_minify_preserves_single_quoted_string() {
        let input  = "@DATA(\n  name = 'Hello World'\n)";
        let output = DixCompactor::minify(input);
        assert!(
            output.contains("'Hello World'"),
            "single-quoted string lost its delimiters — got: {output}"
        );
    }

    /// Section keywords other than @CONFIG must also render as `@SECTION`.
    #[test]
    fn test_minify_data_section_keyword() {
        let input  = "@DATA(\n  x = 1\n)";
        let output = DixCompactor::minify(input);
        assert!(
            output.starts_with("@DATA("),
            "expected output to start with '@DATA(', got: {output}"
        );
        assert!(
            !output.contains("SectionData"),
            "Display fallback leaked into minified output — got: {output}"
        );
    }

    /// A minified, already-minimal file must be idempotent under minify.
    #[test]
    fn test_minify_idempotent_on_already_minified_config() {
        let once  = DixCompactor::minify("@CONFIG(\n  version -> \"1.0.0\"\n)");
        let twice = DixCompactor::minify(&once);
        assert_eq!(once, twice, "minify should be idempotent: {once} vs {twice}");
    }

    // ── compact ───────────────────────────────────────────────────────────────

    #[test]
    fn test_compact_removes_trailing_whitespace() {
        let input  = "line1   \nline2\t\t";
        let output = DixCompactor::compact(input);
        assert_eq!(output, "line1\nline2\n");
    }

    #[test]
    fn test_compact_collapses_many_blank_lines() {
        let input  = "line1\n\n\n\nline2";
        let output = DixCompactor::compact(input);
        assert_eq!(output, "line1\n\nline2\n");
    }

    #[test]
    fn test_compact_single_blank_line_preserved() {
        let output = DixCompactor::compact("a\n\nb");
        assert_eq!(output, "a\n\nb\n");
    }

    #[test]
    fn test_compact_preserves_indentation() {
        let input  = "@DATA(  \n  x = 1  \n)";
        let output = DixCompactor::compact(input);
        assert!(output.contains("  x = 1"), "indentation lost: {output}");
    }

    // ── remove_comments ───────────────────────────────────────────────────────

    #[test]
    fn test_remove_comments_single_line() {
        let output = DixCompactor::remove_comments("x = 5 // a comment\ny = 10");
        assert_eq!(output, "x = 5 \ny = 10");
    }

    #[test]
    fn test_remove_comments_multi_line() {
        let output = DixCompactor::remove_comments("x = 5 /* comment */ y = 10");
        assert_eq!(output, "x = 5  y = 10");
    }

    #[test]
    fn test_remove_comments_preserves_url_in_string() {
        let output = DixCompactor::remove_comments("url = \"http://example.com\" // comment");
        assert_eq!(output, "url = \"http://example.com\" ");
    }

    #[test]
    fn test_remove_comments_preserves_comment_text_in_string() {
        let input  = "s = \"/* not a comment */\" // real comment";
        let output = DixCompactor::remove_comments(input);
        assert!(output.contains("/* not a comment */"), "got: {output}");
        assert!(!output.contains("real comment"),       "got: {output}");
    }

    // ── compression ratio ─────────────────────────────────────────────────────

    #[test]
    fn test_compression_ratio() {
        let original   = "hello world";
        let compressed = "hello";
        let ratio = DixCompactor::get_compression_ratio(original, compressed);
        assert!((ratio - (1.0 - 5.0 / 11.0)).abs() < 0.001);
    }

    #[test]
    fn test_compression_ratio_empty_original() {
        assert_eq!(DixCompactor::get_compression_ratio("", ""), 0.0);
    }

    #[test]
    fn test_compression_ratio_no_change() {
        let s = "abc";
        assert_eq!(DixCompactor::get_compression_ratio(s, s), 0.0);
    }
            }
