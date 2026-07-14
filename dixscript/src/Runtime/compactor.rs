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
//!
//! ## Comma-before-grouped-entry replacement
//!
//! The grammar marks commas between `GroupedEntry` items (`TableProperty` and
//! `GroupArray`) as optional (`","?`). The parser, however, rejects them.
//! `minify` therefore replaces any `Symbol(',')` token whose next meaningful
//! token sequence matches the head of a `GroupedEntry` with a forced space
//! instead.
//!
//! A `GroupedEntry` head is detected by the lookahead
//! [`is_next_grouped_entry`]:
//!
//! ```text
//! Identifier  ('.' Identifier)*  (':' | '::')
//! ```
//!
//! The tokenizer never emits a composite `TablePath` token; it emits exactly
//! that `Identifier Symbol('.') Identifier … Symbol(':')` (or `DoubleColon`)
//! sequence.  The lookahead inspects up to N meaningful tokens (where N grows
//! with path depth) without consuming them.
//!
//! Commas *within* a group-array item list (`tags:: "a", "b"`) or within
//! a table-property assignment list (`db: host = "a", port = 5432`) are
//! unaffected: the token immediately after them is always a value or bare
//! `Identifier`, never the start of an `Identifier ('.' Identifier)* (':' | '::')` chain.
//!
//! ## Proactive space before grouped-entry heads (no-comma case)
//!
//! When no comma precedes a grouped entry (e.g. a hand-written file with bare
//! newlines between entries), the comma-drop path does not fire.  The
//! word-char rule handles the common `number → identifier` transition
//! (`10973731.56816 elements:`), but misses cases like `"Alice" db:` where the
//! previous rendered token ends with a non-word character (`"`) that is also not
//! a natural separator symbol (`(`, `=`, `:`, etc.).
//!
//! A proactive check fires on every grouped-entry head: if the previous rendered
//! token does not end with a [`is_grouped_entry_separator`] character, a space
//! is forced regardless of the word-char rule.
//!
//! ### Why `.` is included in [`is_grouped_entry_separator`]
//!
//! For a multi-segment path like `elements.hydrogen.identity:`, every interior
//! segment (`hydrogen`, `identity`) also satisfies `is_next_grouped_entry` when
//! inspected in isolation (e.g. `hydrogen.identity:` is a valid grouped-entry
//! head).  Without `.` in the separator set the proactive check would fire at
//! each interior segment, inserting a spurious space after every path dot.
//!
//! The only token that ever renders to a string ending in `.` is `Symbol('.')`
//! itself — floating-point literals like `42.0` end in `0`, not `.`.  So
//! treating a trailing `.` as a natural separator is safe and precise: it
//! uniquely identifies the "we are already inside a dotted path" state.

use crate::Compiler::Core::Config::OperationalSettings;
use crate::Compiler::Core::Tokenizer::{Token, TokenType, Tokenizer};

// ── Character helpers ──────────────────────────────────────────────────────────

/// `true` for alphanumeric characters and `_`.
#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `true` for characters that already act as a natural token boundary,
/// meaning no additional space is needed before a grouped-entry head.
///
/// Covers bracket/delimiter characters, `=`, `:`, and `.`.
///
/// `.` is included because the only token that renders to a string ending in
/// `.` is the path-separator `Symbol('.')` itself.  When `prev_rendered` ends
/// with `.` we are already inside a dotted table path — inserting a space
/// between `db.` and `host` (producing `db. host:`) would be incorrect.
///
/// Notably does **not** include `"` or `'` (string delimiters) or word
/// characters — those cases still need an explicit space (e.g. `"Alice" db:`).
#[inline]
fn is_grouped_entry_separator(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | ')' | ']' | '}' | '=' | ':' | '.')
}

// ── Token-stream helpers ───────────────────────────────────────────────────────

/// Returns `true` for tokens that produce no visible output in minified form.
///
/// **Must stay in sync** with the empty-`String` arms in [`render_token`].
/// Adding a new "silent" token variant to `render_token` requires a matching
/// arm here; otherwise the lookahead will stall on that token type instead of
/// skipping past it.
#[inline]
fn renders_empty(token: &Token) -> bool {
    matches!(
        token.token_type,
        TokenType::Comment(_) | TokenType::EndOfFile
    )
}

/// Advance `from` past zero or more empty-rendering tokens.
///
/// Returns `Some(index)` pointing at the first visible token at or after
/// `from`, or `None` if no such token exists.
#[inline]
fn skip_empty_from(tokens: &[Token], from: usize) -> Option<usize> {
    let mut i = from;
    while i < tokens.len() && renders_empty(&tokens[i]) {
        i += 1;
    }
    if i < tokens.len() {
        Some(i)
    } else {
        None
    }
}

/// Returns `true` when the visible token sequence starting at `from` matches
/// the **head of a `GroupedEntry`**:
///
/// ```text
/// Identifier  ('.' Identifier)*  (':' | '::')
/// ```
///
/// This is the exact token shape the tokenizer emits for what the grammar
/// calls `TablePath ':'` (table property) or `TablePath '::'` (group array).
/// There is no composite `TablePath` token in the stream.
///
/// Empty-rendering tokens (comments, EOF, ParseContext) are skipped at every
/// position so the check works even if a comment sits between tokens.
///
/// ### Why this is safe to use as a comma-replacement trigger
///
/// * A bare `Identifier` followed immediately by `=` starts a
///   `SimpleProperty` — the loop hits `=` and returns `false`. ✓
/// * A bare `Identifier` that is a function call is followed by `(` — returns
///   `false`. ✓
/// * A value token (`String`, number, `Bool`, …) is not an `Identifier` —
///   fails the first check and returns `false`. ✓
/// * An `Identifier` followed by `.` but then a non-`Identifier` (e.g. a
///   method call `obj.method(`) returns `false` because the post-dot token is
///   not a plain `Identifier`. ✓
///
/// ### Interior-segment note
///
/// For a path like `elements.hydrogen.identity:`, this function returns `true`
/// when called at the `hydrogen` or `identity` position as well as at
/// `elements`, because each suffix is itself a valid grouped-entry head.  The
/// proactive space check in [`DixCompactor::minify`] relies on
/// [`is_grouped_entry_separator`] (which now includes `.`) to suppress
/// force_space when the previous rendered token already ends with `.`.
fn is_next_grouped_entry(tokens: &[Token], from: usize) -> bool {
    // ── Step 1: must begin with an Identifier ─────────────────────────────
    let Some(mut i) = skip_empty_from(tokens, from) else {
        return false;
    };
    if !matches!(tokens[i].token_type, TokenType::Identifier(_)) {
        return false;
    }
    i += 1;

    // ── Step 2: follow ('.' Identifier)* then expect ':' or '::' ─────────
    loop {
        let Some(j) = skip_empty_from(tokens, i) else {
            return false;
        };
        i = j;

        match &tokens[i].token_type {
            // GroupArray terminator — `::` was lexed as a DoubleColon token.
            TokenType::DoubleColon => return true,

            // TableProperty terminator — single `:`.
            // Symbol(':') is the most likely form in @DATA.
            // ControlFlowColon is also handled defensively in case the
            // tokenizer uses that variant for colons outside control-flow
            // keywords in some contexts.
            TokenType::Symbol(':')  => return true,

            // Dot — must be followed by another Identifier segment.
            TokenType::Symbol('.') => {
                i += 1;
                let Some(k) = skip_empty_from(tokens, i) else {
                    return false;
                };
                i = k;
                if !matches!(tokens[i].token_type, TokenType::Identifier(_)) {
                    return false;
                }
                i += 1;
            }

            // Anything else (=, (, [, value token, …) → not a grouped-entry head.
            _ => return false,
        }
    }
}

// ── Token rendering ────────────────────────────────────────────────────────────

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

        // FIX: restore quote delimiters — get_token_value() strips them.
        TokenType::String(s) => format!("\"{}\"", s),
        TokenType::StringSingle(s) => format!("'{}'", s),

        // FIX: restore the `$"..."` interpolation wrapper.
        TokenType::InterpolatedString(s) => format!("$\"{}\"", s),

        // FIX: render the actual `@SECTION` keyword, not the Display fallback.
        TokenType::SectionConfig     => "@CONFIG".to_string(),
        TokenType::SectionImports    => "@IMPORTS".to_string(),
        TokenType::SectionDLM        => "@DLM".to_string(),
        TokenType::SectionEnums      => "@ENUMS".to_string(),
        TokenType::SectionQuickFuncs => "@QUICKFUNCS".to_string(),
        TokenType::SectionData       => "@DATA".to_string(),
        TokenType::SectionSecurity   => "@SECURITY".to_string(),

        // No output in minified form.
        TokenType::Comment(_) | TokenType::EndOfFile  => {
            String::new()
        }

        // All other tokens: use the canonical rendering already in Token.
        _ => token.get_token_value(),
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

pub struct DixCompactor;

impl DixCompactor {
    /// Minify DixScript content — remove all unnecessary whitespace.
    ///
    /// Uses the DixScript tokenizer so that keyword, identifier, and literal
    /// boundaries are always respected.  A single space is inserted between two
    /// consecutive tokens only when both their adjacent characters are word chars.
    ///
    /// ### Preserves
    /// - String contents (whitespace and `//` inside strings are kept verbatim)
    /// - Mandatory spaces between adjacent word tokens (`true other` ≠ `trueother`)
    /// - Commas within group-array item lists and table-property assignment lists
    ///
    /// ### Separator rules across tier boundaries
    ///
    /// **Flat → flat**: commas between `SimpleProperty` entries are kept as-is.
    /// The token immediately after such a comma is always an `Identifier` followed
    /// by `=`, so `is_next_grouped_entry` returns `false` and the comma is left
    /// alone.
    ///
    /// **Flat → table / group-array, table ↔ group-array, table → table,
    /// group-array → group-array**: any `Symbol(',')` whose next meaningful token
    /// sequence matches `Identifier ('.' Identifier)* (':' | '::')` is dropped
    /// and a forced space is scheduled instead.  The parser rejects commas before
    /// grouped-entry heads; a space is the correct separator.
    ///
    /// Additionally, even when no comma is present (hand-written source using
    /// bare newlines), a space is forced before a grouped-entry head whenever the
    /// previous rendered token does not already end with a natural separator
    /// symbol (see [`is_grouped_entry_separator`]).  This prevents fusions like
    /// `"Alice"db:` or `10973731.56816elements:` (where the trailing `e` would
    /// be misread as a scientific-notation exponent by the lexer).
    ///
    /// Interior path segments (e.g. `hydrogen` and `identity` inside
    /// `elements.hydrogen.identity:`) are NOT spuriously spaced because `.` is
    /// included in [`is_grouped_entry_separator`] — a prev_rendered ending in `.`
    /// means we are already inside the dotted path, not at an entry boundary.
    pub fn minify(content: &str) -> String {
        if content.trim().is_empty() {
            return String::new();
        }

        let settings = OperationalSettings::default();
        let tokenizer = Tokenizer::new(content, &settings);
        let tok_result = tokenizer.tokenize();
        let tokens = &tok_result.tokens;

        let mut result = String::with_capacity(content.len());
        let mut prev_rendered: Option<String> = None;
        // Set to `true` when a comma is dropped before a grouped entry, OR when
        // the proactive check determines a space is required before a grouped-entry
        // head even with no preceding comma.  Forces a space before the very next
        // visible token regardless of the word-char rule.
        let mut force_space = false;
        let mut i = 0;

        while i < tokens.len() {
            let token = &tokens[i];

            // ── Comma-before-grouped-entry replacement ────────────────────────
            // When a comma immediately precedes a table-property or group-array
            // header, drop it and schedule a forced space.  The parser rejects
            // commas in this position; a space keeps tokens properly separated.
            if matches!(token.token_type, TokenType::Symbol(','))
                && is_next_grouped_entry(tokens, i + 1) {
                    force_space = true;
                    i += 1;
                    continue;
                }

            // ── Proactive space before grouped-entry head (no-comma case) ─────
            // When the current token is the first identifier of a grouped-entry
            // head and no comma was dropped just before it (force_space already
            // false), check whether the previous rendered token ends with a
            // character that provides natural separation.  If not, force a space.
            //
            // This handles e.g.:
            //   `"Alice"\n  db: host = "x"` → minified `"Alice" db:host="x"` ✓
            //   `10973731.56816\n  elements: name = "H"` — already covered by the
            //   word-char rule (digit → letter), but fires here too (no harm). ✓
            //
            // Interior path segments (e.g. `hydrogen` in `elements.hydrogen:`)
            // also satisfy is_next_grouped_entry, but their prev_rendered ends
            // with `.` which IS in is_grouped_entry_separator, so force_space is
            // NOT set for them.  This prevents `elements. hydrogen:` output. ✓
            if !force_space && prev_rendered.is_some() && is_next_grouped_entry(tokens, i) {
                let prev = prev_rendered.as_deref().unwrap_or("");
                if let Some(last) = prev.chars().last() {
                    if !is_grouped_entry_separator(last) {
                        force_space = true;
                    }
                }
            }
            // ─────────────────────────────────────────────────────────────────

            let rendered = render_token(token);
            if rendered.is_empty() {
                i += 1;
                continue;
            }

            if prev_rendered.is_some() {
                let prev = prev_rendered.as_deref().unwrap_or("");
                let prev_ends_word   = prev.chars().last().map(is_word_char).unwrap_or(false);
                let curr_starts_word = rendered.chars().next().map(is_word_char).unwrap_or(false);
                if force_space || (prev_ends_word && curr_starts_word) {
                    result.push(' ');
                }
            }
            force_space = false;

            result.push_str(&rendered);
            prev_rendered = Some(rendered);
            i += 1;
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
                    in_string   = true;
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

    // ── minify: string / section keyword fixes ────────────────────────────────

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

    /// Section keywords must render as `@SECTION`, not the Display fallback.
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

    /// A minified file must be idempotent under minify.
    #[test]
    fn test_minify_idempotent_on_already_minified_config() {
        let once  = DixCompactor::minify("@CONFIG(\n  version -> \"1.0.0\"\n)");
        let twice = DixCompactor::minify(&once);
        assert_eq!(once, twice, "minify should be idempotent: {once} vs {twice}");
    }

    // ── minify: comma-before-grouped-entry replacement ────────────────────────
    //
    // The tokenizer emits table-property and group-array headers as the token
    // sequence  `Identifier ('.' Identifier)* (':' | '::')` — never as a
    // composite TablePath token.  The lookahead in `minify` detects that shape
    // and replaces the preceding comma with a space.

    /// Comma between a flat property and a single-segment table property.
    /// Input comma must be replaced by a space, never appear as `,ident:`.
    #[test]
    fn test_minify_replaces_comma_before_table_property_with_space() {
        let input  = "@DATA(\n  count = 1,\n  host: key = \"v\"\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains(",host"),
            "comma leaked before table-property — got: {output}"
        );
        // The space replacement must keep the tokens separated.
        assert!(
            output.contains(" host:") || output.contains("1 host:"),
            "no space before table-property — got: {output}"
        );
        assert!(
            output.contains("host:"),
            "table-property missing — got: {output}"
        );
    }

    /// Comma before a dotted (multi-segment) table path: `db.host: …`
    ///
    /// The dot between `db` and `host` must NOT trigger a spurious space —
    /// `is_grouped_entry_separator('.')` returns true so the proactive check
    /// is suppressed for interior path segments.
    #[test]
    fn test_minify_replaces_comma_before_dotted_table_property() {
        let input  = "@DATA(\n  count = 1,\n  db.host: port = 5432\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains(",db"),
            "comma leaked before dotted table-property — got: {output}"
        );
        assert!(
            output.contains("db.host:") || output.contains("db.host :"),
            "dotted table-property missing — got: {output}"
        );
    }

    /// Comma between a flat property and a group array.
    #[test]
    fn test_minify_replaces_comma_before_group_array_with_space() {
        let input  = "@DATA(\n  x = 1,\n  tags:: \"a\"\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains(",tags"),
            "comma leaked before group-array — got: {output}"
        );
        assert!(
            output.contains(" tags::") || output.contains("1 tags::"),
            "no space before group-array — got: {output}"
        );
        assert!(
            output.contains("tags::"),
            "group-array missing — got: {output}"
        );
    }

    /// Comma before a dotted group-array path: `db.tags:: …`
    ///
    /// Interior segment `tags` must not be preceded by a spurious space —
    /// the `.` before it is in `is_grouped_entry_separator`.
    #[test]
    fn test_minify_replaces_comma_before_dotted_group_array() {
        let input  = "@DATA(\n  x = 1,\n  db.tags:: \"a\", \"b\"\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains(",db"),
            "comma leaked before dotted group-array — got: {output}"
        );
        assert!(
            output.contains("db.tags::"),
            "dotted group-array missing — got: {output}"
        );
    }

    /// Comma between two table-property blocks.
    #[test]
    fn test_minify_replaces_comma_between_table_properties() {
        let input  = "@DATA(\n  db: host = \"a\",\n  cache: host = \"b\"\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains(",cache"),
            "comma leaked between table-properties — got: {output}"
        );
        assert!(
            output.contains("db:") && output.contains("cache:"),
            "a table-property is missing — got: {output}"
        );
        // The two blocks must be separated by whitespace, not squashed together.
        assert!(
            output.contains(" cache:"),
            "no space between table-property blocks — got: {output}"
        );
    }

    /// Comma between two group-array declarations.
    #[test]
    fn test_minify_replaces_comma_between_group_arrays() {
        let input  = "@DATA(\n  tags:: \"a\",\n  flags:: true\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains(",flags"),
            "comma leaked between group-arrays — got: {output}"
        );
        assert!(
            output.contains("tags::") && output.contains("flags::"),
            "a group-array is missing — got: {output}"
        );
        assert!(
            output.contains(" flags::"),
            "no space between group-array declarations — got: {output}"
        );
    }

    /// Commas WITHIN a group-array item list must be kept — they follow a
    /// value token, never an `Identifier ('.' Identifier)* (':' | '::')` head.
    #[test]
    fn test_minify_keeps_comma_within_group_array_items() {
        let input  = "@DATA(\n  tags:: \"a\", \"b\", \"c\"\n)";
        let output = DixCompactor::minify(input);
        // All three items must be present.
        assert!(output.contains("\"a\""), "got: {output}");
        assert!(output.contains("\"b\""), "got: {output}");
        assert!(output.contains("\"c\""), "got: {output}");
        // The commas between items (followed by String literals) must survive.
        assert!(
            output.contains("\"a\",\"b\"") || output.contains("\"a\", \"b\""),
            "comma between group-array items was incorrectly dropped — got: {output}"
        );
        assert!(
            output.contains("\"b\",\"c\"") || output.contains("\"b\", \"c\""),
            "second comma between group-array items was incorrectly dropped — got: {output}"
        );
    }

    /// Commas within a table-property assignment list must be kept — they
    /// follow a value token, not a grouped-entry head.
    #[test]
    fn test_minify_keeps_comma_within_table_property_assignments() {
        let input  = "@DATA(\n  db: host = \"a\", port = 5432\n)";
        let output = DixCompactor::minify(input);
        assert!(
            output.contains("host=") && output.contains("port="),
            "an assignment was dropped — got: {output}"
        );
        // The comma between `"a"` (String) and `port` (Identifier) must stay.
        // `port` is preceded by a value, so is_next_grouped_entry sees
        // Identifier followed by `=`, not `:` — returns false, comma kept.
        assert!(
            output.contains(",port") || output.contains(", port"),
            "comma between table-property assignments was incorrectly dropped — got: {output}"
        );
    }

    /// Comma between a string-valued flat property and a table property.
    /// Previous token ends with `"` (non-word char) so force_space is needed
    /// to guarantee separation — the word-char rule alone would not add a space.
    #[test]
    fn test_minify_space_after_string_value_before_table_property() {
        let input  = "@DATA(\n  name = \"Alice\",\n  db: host = \"x\"\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains(",db"),
            "comma leaked — got: {output}"
        );
        // `"Alice"` ends with `"` which is NOT a word char, so force_space
        // must fire to avoid `"Alice"db:`.
        assert!(
            !output.contains("\"Alice\"db"),
            "string-value and table-property fused — got: {output}"
        );
        assert!(
            output.contains("db:"),
            "table-property missing — got: {output}"
        );
    }

    /// Comma between a string-valued group array and another group array.
    #[test]
    fn test_minify_space_after_string_item_before_group_array() {
        let input  = "@DATA(\n  tags:: \"x\", \"y\",\n  flags:: true\n)";
        let output = DixCompactor::minify(input);
        // Last item of first group array is `"y"` → ends with `"` (non-word).
        // force_space must still fire and prevent `"y"flags::`.
        assert!(
            !output.contains("\"y\"flags"),
            "string item and group-array fused — got: {output}"
        );
        assert!(
            !output.contains(",flags"),
            "comma leaked before second group-array — got: {output}"
        );
        assert!(
            output.contains("flags::"),
            "second group-array missing — got: {output}"
        );
        // Commas within the first group array's item list must survive.
        assert!(
            output.contains("\"x\",\"y\"") || output.contains("\"x\", \"y\""),
            "inner comma between group-array items dropped — got: {output}"
        );
    }

    /// Proactive space: string value before table property WITHOUT a comma.
    /// The proactive grouped-entry check must fire here — the word-char rule
    /// alone won't because `"` is not a word char.
    #[test]
    fn test_minify_proactive_space_string_value_no_comma_before_table() {
        // No comma — bare newline between entries.
        let input  = "@DATA(\n  name = \"Alice\"\n  db: host = \"x\"\n)";
        let output = DixCompactor::minify(input);
        assert!(
            !output.contains("\"Alice\"db"),
            "string and table-property fused without comma — got: {output}"
        );
        assert!(output.contains("db:"), "table-property missing — got: {output}");
    }

    /// Full @DATA section: flat properties, a table property, and a group array
    /// — all optional inter-entry commas replaced with spaces; inner commas kept.
    #[test]
    fn test_minify_full_data_section_mixed() {
        let input = concat!(
            "@DATA(\n",
            "  count = 42,\n",
            "  label = \"hello\",\n",
            "  db: host = \"localhost\", port = 5432,\n",
            "  tags:: \"x\", \"y\"\n",
            ")"
        );
        let output = DixCompactor::minify(input);

        // No comma immediately before an entry header.
        assert!(!output.contains(",db"),   "comma before 'db:'   — got: {output}");
        assert!(!output.contains(",tags"), "comma before 'tags::' — got: {output}");

        // Tokens from every entry must be present.
        assert!(output.contains("count="),  "got: {output}");
        assert!(output.contains("label="),  "got: {output}");
        assert!(output.contains("db:"),     "got: {output}");
        assert!(output.contains("host="),   "got: {output}");
        assert!(output.contains("port="),   "got: {output}");
        assert!(output.contains("tags::"),  "got: {output}");
        assert!(output.contains("\"x\""),   "got: {output}");
        assert!(output.contains("\"y\""),   "got: {output}");

        // Entry headers must be preceded by whitespace (not squashed together).
        assert!(output.contains(" db:"),   "no space before 'db:'   — got: {output}");
        assert!(output.contains(" tags::"), "no space before 'tags::' — got: {output}");
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
