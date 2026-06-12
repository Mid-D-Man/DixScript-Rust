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
/// Overrides `Token::get_token_value` for two cases where the default is lossy:
///
/// * `Double(d)` — Rust's `f64::to_string` drops `.0` for whole numbers
///   (`4.0` → `"4"`), which re-parses as Integer and silently changes the type.
///   We force `"4.0"`.
///
/// * `Float(f)` — `f32::to_string` omits the required `f` suffix (`3.14f`→`"3.14"`),
///   which re-parses as Double.  We append `"f"`.
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
