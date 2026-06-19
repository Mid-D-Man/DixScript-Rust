// mdix-lsp/src/features/formatting.rs
// Document formatting provider.
//
// Returns a single full-document TextEdit that replaces the source with a
// normalized version. The formatter is **token-based**: it runs the
// DixScript tokenizer once and uses the resulting
// `Symbol('('|')'|'['|']'|'{'|'}')` tokens to compute an accurate nesting
// depth per line — this is what drives indentation.
//
// Anything inside string or comment tokens is invisible to this pass (the
// tokenizer already classified it as such), so brackets that merely *look*
// like brackets inside a string never throw off the depth count — unlike
// the previous character-scanning approach, which only special-cased
// `{`/`}` and missed `(`/`)`/`[`/`]` entirely. That was the main cause of
// multi-line arrays / objects / tuples rendering with no extra indentation
// at all ("slammed at the edge").
//
// ## Passes
//
// 1. Tokenize the whole source once.
// 2. For every (1-based) source line, collect the ordered list of bracket
//    characters from `Symbol` tokens on that line.
// 3. Walk lines top to bottom with a single running depth counter:
//    - A line's own indent = `depth - (leading closers on that line)`.
//    - Then every bracket on the line updates `depth`
//      (+1 per opener, -1 per closer, clamped at 0).
//    This naturally handles `} else {`-style lines: the leading `}`
//    de-indents the line itself, the trailing `{` re-indents the body,
//    net depth change is zero.
// 4. `@SECTION(` / `)` lines participate in the *same* counter. If the
//    tokenizer doesn't happen to emit a separate `Symbol('(')` for a bare
//    `@SECTION(` line, a synthetic `(` is added so depth still balances
//    against its matching `)`.
// 5. Lines that are continuations of a multi-line `/* ... */` comment are
//    copied verbatim — no re-indentation, no operator normalization — so
//    any deliberate internal formatting in long comments survives.
// 6. Operator spacing (`->`, `::`, `=`, and the compound assignment
//    operators `+=` `-=` `*=` `/=` `%=`) is normalized per line,
//    string-aware.
//
// ## Control-flow keywords
//
// DixScript uses `if:`, `elif:`, `chk:`, `log:` — the colon is part of the
// keyword syntax, not a table-property delimiter. The tokenizer emits
// `Keyword("if")` + `Symbol(':')` as two separate tokens. The formatter
// treats them as normal characters: neither triggers bracket-depth changes
// nor operator normalization. The `normalize_operators` function explicitly
// avoids rewriting a lone `:` (it only rewrites `::` for group-array
// syntax).
//
// ## Known limitations
//
// - Table-property / group-array continuation lines without braces
//   (`server:\n  host = "x"\n  port = 8080`) stay at the same depth as
//   their `path:` / `path::` line — there's no closing delimiter to anchor
//   extra indentation to, and guessing where such a block "ends" would
//   require heuristics that are easy to get wrong. This matches the
//   previous formatter's behaviour (not a regression).
// - A multi-line `t:(...)` / `b:(...)` / `r:(...)` constructor whose
//   prefix+parens are lexed as a single combined token (rather than
//   separate `Symbol('(')`/`Symbol(')')`) won't get extra indentation for
//   its body, and its closing `)` may end up de-indented by one level via
//   the leading-closer rule. Canonical (`Display`-generated) output for
//   these is always single-line, so this is a rare hand-written edge case.

use std::collections::{HashMap, HashSet};
use std::panic;

use tower_lsp::lsp_types::{FormattingOptions, Position, Range, TextEdit};
use dixscript::Compiler::Core::OperationalSettings;
use dixscript::Compiler::Core::Tokenizer::{Tokenizer, TokenType};

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

// ── Bracket classification ──────────────────────────────────────────────────
//
// Only `(` `)` `[` `]` `{` `}` drive nesting depth. Type-annotation angle
// brackets (`<int>`, `<array<int>>`, ...) are folded by the tokenizer into a
// single `DataType` token and never appear as `Symbol('<'|'>')`, so they
// never need special-casing here.
//
// DixScript control-flow colons (`if:`, `elif:`, `chk:`, `log:`) are emitted
// as `Keyword("if")` + `Symbol(':')`. The `:` is a Symbol but NOT a depth
// bracket, so it is invisible to this counter.

#[inline]
fn is_depth_bracket(c: char) -> bool {
    matches!(c, '(' | ')' | '[' | ']' | '{' | '}')
}

#[inline]
fn is_opener(c: char) -> bool {
    matches!(c, '(' | '[' | '{')
}

#[inline]
fn is_closer(c: char) -> bool {
    matches!(c, ')' | ']' | '}')
}

// ── Formatter ─────────────────────────────────────────────────────────────────

pub fn format_source(source: &str, indent_size: usize) -> String {
    let unit = " ".repeat(indent_size.max(1));

    // ── Pass 1: tokenize once ────────────────────────────────────────────────
    let settings   = OperationalSettings::default();
    let tokenizer  = Tokenizer::new(source, &settings);
    let tok_result = tokenizer.tokenize();

    // Per (1-based) source line: ordered bracket characters from Symbol tokens.
    let mut line_brackets: HashMap<usize, Vec<char>> = HashMap::new();

    // Lines that are continuations of a multi-line `/* ... */` comment —
    // copied verbatim, untouched by indentation or operator normalization.
    let mut verbatim_lines: HashSet<usize> = HashSet::new();

    for token in &tok_result.tokens {
        match &token.token_type {
            TokenType::Symbol(c) if is_depth_bracket(*c) => {
                line_brackets.entry(token.line).or_default().push(*c);
            }
            TokenType::Comment(text) => {
                let span = text.matches('\n').count();
                for offset in 1..=span {
                    verbatim_lines.insert(token.line + offset);
                }
            }
            _ => {}
        }
    }

    // ── Pass 2: walk lines, tracking depth ──────────────────────────────────
    let lines: Vec<&str> = source.lines().collect();
    let mut out = String::with_capacity(source.len());

    let mut depth: i32 = 0;
    let mut prev_blank = false;

    for (idx, raw_line) in lines.iter().enumerate() {
        let line_no = idx + 1; // 1-based, matches Token::line

        // Multi-line comment continuation — pass through untouched.
        if verbatim_lines.contains(&line_no) {
            out.push_str(raw_line);
            out.push('\n');
            prev_blank = false;
            continue;
        }

        let trimmed = raw_line.trim();

        if trimmed.is_empty() {
            if !prev_blank {
                out.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        let mut brackets = line_brackets.get(&line_no).cloned().unwrap_or_default();

        // Synthetic compensation for a bare `@SECTION(` opener whose `(`
        // wasn't captured as its own Symbol token by the tokenizer. Only
        // fires if the token list doesn't already end with `(` — never
        // double-counts.
        let is_bare_section_open =
            trimmed.starts_with('@') && trimmed.ends_with('(') && !trimmed.contains(')');
        if is_bare_section_open && brackets.last() != Some(&'(') {
            brackets.push('(');
        }

        // A line's own indent is reduced by however many closers it *starts*
        // with — this is what makes `}`, `]`, `)`, and compound forms like
        // `} else {` or `}],` align with the construct they're closing,
        // while their *contents* (already emitted on previous lines) stay
        // one level deeper.
        let leading_close = brackets.iter()
            .take_while(|&&c| is_closer(c))
            .count() as i32;

        let indent_level = (depth - leading_close).max(0);
        let indent = unit.repeat(indent_level as usize);

        let normalised = normalize_operators(trimmed);

        out.push_str(&indent);
        out.push_str(&normalised);
        out.push('\n');

        for c in brackets {
            if is_opener(c) {
                depth += 1;
            } else if is_closer(c) {
                depth = (depth - 1).max(0);
            }
        }
    }

    // Ensure exactly one trailing newline.
    out.trim_end_matches('\n').to_string() + "\n"
}

/// Normalize spacing around `->`, `::`, `=`, and the compound assignment
/// operators `+=` `-=` `*=` `/=` `%=`. Does NOT modify content inside string
/// literals.
///
/// A lone `:` (as in `if:`, `elif:`, `chk:`, `log:`) is intentionally NOT
/// rewritten — only `::` (group-array double-colon) triggers normalization.
fn normalize_operators(line: &str) -> String {
    let mut result       = String::with_capacity(line.len() + 8);
    let chars: Vec<char> = line.chars().collect();
    let len              = chars.len();
    let mut i            = 0;
    let mut in_string    = false;
    let mut str_char     = '"';

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

        // `->` arrow (config entries, switch cases)
        if c == '-' && next == '>' {
            let trimmed_result = result.trim_end().to_string();
            result.clear();
            result.push_str(&trimmed_result);
            result.push_str(" -> ");
            i += 2;
            while i < len && chars[i] == ' ' { i += 1; }
            continue;
        }

        // `::` double-colon (group arrays) — single space after, no forced
        // line break. The previous version injected a hardcoded "\n    "
        // here, which ignored `indent_size`/depth entirely and produced
        // mis-indented continuation lines for anything but 2-space,
        // top-level group arrays.
        if c == ':' && next == ':' {
            let trimmed_result = result.trim_end().to_string();
            result.clear();
            result.push_str(&trimmed_result);
            result.push_str(":: ");
            i += 2;
            while i < len && chars[i] == ' ' { i += 1; }
            continue;
        }

        // Compound assignment: `+=` `-=` `*=` `/=` `%=`.
        // Must be checked BEFORE the plain `=` branch — otherwise e.g.
        // `x += 1` was previously split into `x + = 1`.
        if c == '=' && next != '=' && matches!(prev, '+' | '-' | '*' | '/' | '%') {
            result.pop(); // remove the operator char just pushed
            let trimmed_result = result.trim_end().to_string();
            result.clear();
            result.push_str(&trimmed_result);
            result.push(' ');
            result.push(prev);
            result.push_str("= ");
            i += 1;
            while i < len && chars[i] == ' ' { i += 1; }
            continue;
        }

        // `=` assignment (not `==`, `!=`, `<=`, `>=`, and not part of a
        // compound assignment operator — those are handled above).
        if c == '=' && next != '=' && !matches!(prev, '!' | '<' | '>' | '=' | '+' | '-' | '*' | '/' | '%') {
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

    // ── New: bracket-driven indentation ──────────────────────────────────────

    #[test]
    fn format_indents_nested_array() {
        let src = "@DATA(\nmatrix = [\n[1, 2],\n[3, 4]\n]\n)\n";
        let out = format_source(src, 2);
        assert!(out.contains("\n  matrix = [\n"),  "got: {}", out);
        assert!(out.contains("\n    [1, 2],\n"),   "got: {}", out);
        assert!(out.contains("\n    [3, 4]\n"),    "got: {}", out);
        assert!(out.contains("\n  ]\n"),           "got: {}", out);
    }

    #[test]
    fn format_indents_nested_object() {
        let src = "@DATA(\nuser: {\nname = \"Bob\",\nage = 30\n}\n)\n";
        let out = format_source(src, 2);
        assert!(out.contains("\n  user: {\n"),       "got: {}", out);
        assert!(out.contains("\n    name = \"Bob\",\n"), "got: {}", out);
        assert!(out.contains("\n    age = 30\n"),    "got: {}", out);
        assert!(out.contains("\n  }\n"),             "got: {}", out);
    }

    #[test]
    fn format_indents_doubly_nested_object_in_array() {
        let src = "@DATA(\nenemies::\n{\nname = \"Goblin\",\nhp = 50\n},\n{\nname = \"Orc\",\nhp = 100\n}\n)\n";
        let out = format_source(src, 2);
        assert!(out.contains("\n  {\n"),                  "got: {}", out);
        assert!(out.contains("\n    name = \"Goblin\",\n"), "got: {}", out);
        assert!(out.contains("\n  },\n"),                 "got: {}", out);
    }

    #[test]
    fn format_if_else_braces_align() {
        // DixScript control-flow keywords carry a colon suffix: `if:`, `elif:`,
        // `chk:`, `log:`. The tokenizer emits them as Keyword + Symbol(':').
        // The `:` is NOT a depth bracket, so it has no effect on indentation.
        //
        // Depth trace (indent_size = 2):
        //
        //   @QUICKFUNCS(      depth 0 → indent 0, then ( opens → depth 1
        //   ~test(x<int>) {   depth 1 → indent 2, ( and ) cancel, { opens → depth 2
        //   if: x > 0 {       depth 2 → indent 4, { opens → depth 3
        //   return 1          depth 3 → indent 6
        //   } else {          leading } de-indents: (3-1)=2 → indent 4;
        //                     then } closes (depth 2) and { opens (depth 3)
        //   return 0          depth 3 → indent 6
        //   }                 leading } de-indents: (3-1)=2 → indent 4; } → depth 2
        //   }                 leading } de-indents: (2-1)=1 → indent 2; } → depth 1
        //   )                 leading ) de-indents: (1-1)=0 → indent 0; ) → depth 0
        let src = "@QUICKFUNCS(\n~test(x<int>) {\nif: x > 0 {\nreturn 1\n} else {\nreturn 0\n}\n}\n)\n";
        let out = format_source(src, 2);

        // `if:` sits at depth 2 inside ~test's body — 4 spaces.
        assert!(out.contains("\n    if: x > 0 {\n"), "got: {}", out);

        // `} else {` de-indents by one leading closer — same 4-space level as `if:`.
        assert!(out.contains("\n    } else {\n"), "got: {}", out);

        // Bodies (return statements) are one level deeper — 6 spaces.
        assert!(out.contains("\n      return 1\n"), "got: {}", out);
        assert!(out.contains("\n      return 0\n"), "got: {}", out);

        // Function body's own closing brace is one level shallower — 2 spaces.
        assert!(out.contains("\n  }\n"), "got: {}", out);
    }

    // ── New: operator-normalization fixes ────────────────────────────────────

    #[test]
    fn format_compound_assignment_plus_equals() {
        let src = "@QUICKFUNCS(\n~test() {\nx += 1\nreturn x\n}\n)\n";
        let out = format_source(src, 2);
        assert!(out.contains("x += 1"),   "got: {}", out);
        assert!(!out.contains("x + = 1"), "got: {}", out);
    }

    #[test]
    fn format_compound_assignment_all_operators() {
        for op in ["+=", "-=", "*=", "/=", "%="] {
            let src = format!("@QUICKFUNCS(\n~test() {{\nx {} 1\nreturn x\n}}\n)\n", op);
            let out = format_source(&src, 2);
            assert!(out.contains(&format!("x {} 1", op)), "op {}: got {}", op, out);
        }
    }

    #[test]
    fn format_double_colon_no_forced_newline() {
        let src = "@DATA(\ntags:: \"a\", \"b\"\n)\n";
        let out = format_source(src, 2);
        // `tags:: "a", "b"` stays on one line, just gets a space after `::`.
        assert!(out.contains("tags:: \"a\", \"b\""), "got: {}", out);
        assert!(!out.contains("::\n"), "unexpected forced newline after '::': {}", out);
    }

    #[test]
    fn format_double_colon_respects_indent_size() {
        let src = "@DATA(\ntags:: \"a\", \"b\"\n)\n";
        let out = format_source(src, 4);
        assert!(out.contains("    tags:: \"a\", \"b\""), "got: {}", out);
    }

    #[test]
    fn format_control_flow_colon_not_treated_as_operator() {
        // `if:` must survive normalize_operators unchanged — the lone `:` after
        // the keyword must NOT be rewritten as `::` or have spaces injected.
        let src = "@QUICKFUNCS(\n~f(x<int>) {\nif: x > 0 {\nreturn 1\n}\n}\n)\n";
        let out = format_source(src, 2);
        assert!(out.contains("if: x > 0 {"), "if: was mangled: {}", out);
        assert!(!out.contains("if :: "), "if: was wrongly doubled: {}", out);
    }
             }
