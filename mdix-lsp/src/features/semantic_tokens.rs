// mdix-lsp/src/features/semantic_tokens.rs
//!
//! ## Changes in this version
//!
//! - `FunctionPrefix` token type removed from the lexer/token.rs.
//!   The `~` character is now always emitted as `Symbol('~')`.
//!   All references updated:
//!   * ClassifierState::advance() — state machine now triggers on Symbol('~')
//!   * classify() — Symbol('~') handled before the catch-all Symbol(_) => None
//!   * token_length() — FunctionPrefix entry removed; Symbol('~') falls through
//!     to the wildcard which returns get_token_value().len() == 1. Correct.
//!
//! ## DLM coloring
//!
//! Uses a single `dlm_dot_seen` flag. Set when any `.` is encountered anywhere.
//! Cleared when an Identifier inside DLM is classified. No SectionId stamp
//! dependency needed for the DLM module/subtype distinction.

use std::collections::HashSet;
use std::panic;

use tower_lsp::lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult};
use dixscript::Compiler::AST::{ConfigValue, DixScript};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;

use crate::capabilities::{
    TT_KEYWORD, TT_STRING, TT_NUMBER, TT_OPERATOR, TT_VARIABLE,
    TT_FUNCTION, TT_TYPE, TT_ENUM_MEMBER, TT_COMMENT, TT_NAMESPACE,
    TT_PROPERTY, TT_PARAMETER, TT_MACRO, TT_DECORATOR,
    TT_REGEXP, TT_EVENT,
    MOD_DECLARATION, MOD_READONLY,
};

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Option<SemanticTokensResult> {
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| provide_inner(doc)));
    match result {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("semantic_tokens panicked: {}", msg);
            None
        }
    }
}

fn provide_inner(doc: Option<&Document>) -> Option<SemanticTokensResult> {
    let doc = doc?;

    let enum_names: HashSet<String> = doc
        .semantic_result.as_ref()
        .and_then(|sr| sr.symbol_table.as_ref())
        .map(|st| st.enums.keys().cloned().collect())
        .unwrap_or_default();

    // NEW: collect confirmed function names from the symbol table.
    // This makes function call coloring authoritative rather than
    // guessing from lookahead for `(`.
    let func_names: HashSet<String> = doc
        .semantic_result.as_ref()
        .and_then(|sr| sr.symbol_table.as_ref())
        .map(|st| st.functions.keys().cloned().collect())
        .unwrap_or_default();

    let data = encode_tokens(doc, &enum_names, &func_names);
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

// ── Stateful classifier ───────────────────────────────────────────────────────

struct ClassifierState<'a> {
    in_enum_body:      bool,
    enum_brace_depth:  i32,
    seen_enum_name:    bool,

    next_is_func_name: bool,
    in_param_list:     bool,
    param_paren_depth: i32,

    next_is_alias:     bool,

    dlm_dot_seen:      bool,

    // Replaced unreliable lookahead with authoritative symbol-table lookup.
    // prev_was_call_ident is now set only when the identifier is confirmed
    // to exist in the symbol table's function map.
    prev_was_call_ident: bool,

    next_is_enum_type:  bool,
    next_is_enum_dot:   bool,
    prev_was_enum_dot:  bool,

    enum_names: &'a HashSet<String>,
    func_names: &'a HashSet<String>,  // NEW
}

impl<'a> ClassifierState<'a> {
    fn new(enum_names: &'a HashSet<String>, func_names: &'a HashSet<String>) -> Self {
        ClassifierState {
            in_enum_body:        false,
            enum_brace_depth:    0,
            seen_enum_name:      false,
            next_is_func_name:   false,
            in_param_list:       false,
            param_paren_depth:   0,
            next_is_alias:       false,
            dlm_dot_seen:        false,
            prev_was_call_ident: false,
            next_is_enum_type:   false,
            next_is_enum_dot:    false,
            prev_was_enum_dot:   false,
            enum_names,
            func_names,
        }
    }

    fn advance(&mut self, token: &Token, tokens: &[Token], index: usize) {
        self.prev_was_call_ident = false;
        self.next_is_enum_type   = false;

        match &token.token_type {
            TokenType::Identifier(_) | TokenType::Symbol('.') => {}
            _ => {
                self.next_is_enum_dot  = false;
                self.prev_was_enum_dot = false;
            }
        }

        if let TokenType::Identifier(name) = &token.token_type {
            // FIX: use symbol table to confirm it's a function, not lookahead for `(`.
            // This means QuickFunc calls in @DATA and @QUICKFUNCS both get
            // TT_FUNCTION color, matching the declaration color from `~`.
            // Lookahead is kept as a fallback for call sites that aren't in the
            // symbol table yet (e.g. during first-pass before analysis completes).
            let in_symbol_table = self.func_names.contains(name.as_str());
            let lookahead_paren = !in_symbol_table && tokens.iter()
                .skip(index + 1)
                .take(8)
                .filter(|t| !matches!(&t.token_type,
                    TokenType::DataType(_)
                    | TokenType::Symbol('<')
                    | TokenType::Symbol('>')))
                .take(3)
                .any(|t| matches!(t.token_type, TokenType::Symbol('(')));
            self.prev_was_call_ident = in_symbol_table || lookahead_paren;

            if self.enum_names.contains(name.as_str()) {
                let has_dot = tokens.iter()
                    .skip(index + 1)
                    .take(2)
                    .any(|t| matches!(t.token_type, TokenType::Symbol('.')));
                if has_dot {
                    self.next_is_enum_type = true;
                    self.next_is_enum_dot  = true;
                } else {
                    self.next_is_enum_dot = false;
                }
            } else {
                self.next_is_enum_dot = false;
            }
        }

        if let TokenType::Symbol('.') = &token.token_type {
            if self.next_is_enum_dot {
                self.prev_was_enum_dot = true;
                self.next_is_enum_dot  = false;
            }
        }

        match &token.token_type {
            TokenType::SectionEnums => {
                self.seen_enum_name = false;
            }
            TokenType::Symbol('{') if token.section == SectionId::Enums => {
                self.in_enum_body     = true;
                self.enum_brace_depth += 1;
            }
            TokenType::Symbol('}') if token.section == SectionId::Enums => {
                self.enum_brace_depth = (self.enum_brace_depth - 1).max(0);
                if self.enum_brace_depth == 0 {
                    self.in_enum_body   = false;
                    self.seen_enum_name = false;
                }
            }
            TokenType::Symbol('~') => {
                self.next_is_func_name = true;
                self.in_param_list     = false;
                self.param_paren_depth = 0;
            }
            TokenType::Symbol('(')
                if token.section == SectionId::QuickFuncs =>
            {
                if self.next_is_func_name {
                    self.in_param_list     = true;
                    self.param_paren_depth = 1;
                } else if self.in_param_list {
                    self.param_paren_depth += 1;
                }
            }
            TokenType::Symbol(')')
                if token.section == SectionId::QuickFuncs =>
            {
                if self.in_param_list {
                    self.param_paren_depth -= 1;
                    if self.param_paren_depth <= 0 {
                        self.in_param_list     = false;
                        self.param_paren_depth = 0;
                    }
                }
            }
            TokenType::SectionImports => {
                self.next_is_alias = true;
            }
            TokenType::Keyword(kw)
                if *kw == "from" || *kw == "from_cloud" || *kw == "verify" =>
            {
                self.next_is_alias = false;
            }
            TokenType::String(_) | TokenType::StringSingle(_)
                if token.section == SectionId::Imports =>
            {
                self.next_is_alias = true;
            }
            TokenType::SectionDLM => {
                self.dlm_dot_seen = false;
            }
            TokenType::Symbol('.') => {
                self.dlm_dot_seen = true;
            }
            TokenType::Symbol(',') if token.section == SectionId::Dlm => {
                self.dlm_dot_seen = false;
            }
            _ => {}
        }
    }

    fn classify_identifier(&mut self, token: &Token) -> (u32, u32) {
        if self.prev_was_enum_dot {
            self.prev_was_enum_dot = false;
            return (TT_ENUM_MEMBER, 0);
        }
        if self.next_is_enum_type {
            return (TT_TYPE, 0);
        }
        // Function call sites now use TT_FUNCTION, same as declarations.
        // prev_was_call_ident is true when the symbol table confirms it's a
        // function (authoritative) or lookahead found a `(` (fallback).
        if self.prev_was_call_ident && !self.next_is_func_name {
            return (TT_FUNCTION, 0);
        }

        match token.section {
            SectionId::Config => (TT_PROPERTY, 0),

            SectionId::Enums => {
                if self.in_enum_body {
                    (TT_ENUM_MEMBER, MOD_DECLARATION)
                } else if !self.seen_enum_name {
                    self.seen_enum_name = true;
                    (TT_TYPE, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            SectionId::QuickFuncs => {
                if self.next_is_func_name {
                    self.next_is_func_name = false;
                    (TT_FUNCTION, MOD_DECLARATION)
                } else if self.in_param_list && self.param_paren_depth <= 1 {
                    (TT_PARAMETER, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            SectionId::Imports => {
                if self.next_is_alias {
                    self.next_is_alias = false;
                    (TT_NAMESPACE, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            SectionId::Dlm => {
                let result = if self.dlm_dot_seen {
                    (TT_DECORATOR, 0)
                } else {
                    (TT_MACRO, MOD_DECLARATION)
                };
                self.dlm_dot_seen = false;
                result
            }

            SectionId::Data     => (TT_VARIABLE, 0),
            SectionId::Security => (TT_PROPERTY, 0),

            _ => {
                if self.dlm_dot_seen {
                    self.dlm_dot_seen = false;
                    return (TT_DECORATOR, 0);
                }
                (TT_VARIABLE, 0)
            }
        }
    }
}
// ── Encoder ───────────────────────────────────────────────────────────────────

fn encode_tokens(doc: &Document, enum_names: &HashSet<String>, func_names: &HashSet<String>) -> Vec<SemanticToken> {
    let mut data: Vec<SemanticToken> = Vec::with_capacity(doc.tokens.len() + 32);
    let mut prev_line: u32 = 0;
    let mut prev_col:  u32 = 0;
    let mut state = ClassifierState::new(enum_names, func_names);

    if let Some(ast) = &doc.ast {
        emit_config_tokens(
            ast,
            &doc.source,
            doc.config_line_range,
            &mut prev_line,
            &mut prev_col,
            &mut data,
        );
    }

    for (index, token) in doc.tokens.iter().enumerate() {
        state.advance(token, &doc.tokens, index);

        if let TokenType::InterpolatedString(content) = &token.token_type {
            emit_interpolated_tokens(token, content, &mut prev_line, &mut prev_col, &mut data);
            continue;
        }

        let (token_type, modifiers) = match classify(token, &mut state) {
            Some(t) => t,
            None    => continue,
        };

        let line   = token.line.saturating_sub(1) as u32;
        let col    = token.column.saturating_sub(1) as u32;
        let length = token_length(token) as u32;
        if length == 0 { continue; }

        push_raw(
            &mut data, &mut prev_line, &mut prev_col,
            line, col, length, token_type, modifiers,
        );
    }

    data
}
// ── @CONFIG token synthesis ───────────────────────────────────────────────────
//
// @CONFIG is stripped before tokenization — no tokens exist for it.
// We synthesise semantic tokens directly from the AST's ConfigSection,
// which now has correct positions thanks to config_section_handler's
// position fixup pass.

fn emit_config_tokens(
    ast:       &DixScript,
    source:    &str,
    prev_line: &mut u32,
    prev_col:  &mut u32,
    data:      &mut Vec<SemanticToken>,
) {
    let config = match ast.config.as_ref() { Some(c) => c, None => return };
    let source_lines: Vec<&str> = source.lines().collect();

    // @CONFIG section keyword — position is now set by the fixup pass.
    if config.position.is_valid() {
        let lsp_line = (config.position.line - 1) as u32;
        let lsp_col  = (config.position.column - 1) as u32;
        push_raw(data, prev_line, prev_col, lsp_line, lsp_col, 7, TT_KEYWORD, MOD_READONLY);
    }

    // Sort entries by position so delta encoding is monotone.
    let mut sorted_entries: Vec<&dixscript::Compiler::AST::ConfigEntry> = config.entries.iter()
        .filter(|e| e.position.is_valid())
        .collect();
    sorted_entries.sort_by_key(|e| (e.position.line, e.position.column));

    for entry in sorted_entries {
        let lsp_line  = (entry.position.line - 1) as u32;

        // Guard: line must be in the source.
        let line_text = match source_lines.get(entry.position.line - 1) {
            Some(l) => l,
            None    => continue,
        };

        // ── Key token ─────────────────────────────────────────────────────────
        let key_col = (entry.position.column - 1) as u32;
        let key_len = entry.key.len() as u32;
        if key_len == 0 { continue; }
        push_raw(data, prev_line, prev_col, lsp_line, key_col, key_len, TT_PROPERTY, 0);

        // ── Arrow token ───────────────────────────────────────────────────────
        // Search for `->` starting just after the key in this line.
        let search_start = (entry.position.column - 1) + entry.key.len();
        if search_start >= line_text.len() { continue; }

        if let Some(arrow_rel) = line_text[search_start..].find("->") {
            let arrow_col = (search_start + arrow_rel) as u32;
            push_raw(data, prev_line, prev_col, lsp_line, arrow_col, 2, TT_OPERATOR, 0);

            // ── Value token ───────────────────────────────────────────────────
            let after_arrow = search_start + arrow_rel + 2;
            if after_arrow >= line_text.len() { continue; }

            let value_raw  = &line_text[after_arrow..];
            let trim_len   = value_raw.len() - value_raw.trim_start().len();
            let value_col  = (after_arrow + trim_len) as u32;
            let value_text = value_raw.trim_start();

            let (tt, len) = classify_config_value(&entry.value, value_text);
            if len > 0 {
                push_raw(
                    data, prev_line, prev_col,
                    lsp_line, value_col, len as u32,
                    tt, 0,
                );
            }
        }
    }
}

fn classify_config_value(value: &ConfigValue, text: &str) -> (u32, usize) {
    match value {
        ConfigValue::String(_) | ConfigValue::Features(_) => {
            if text.starts_with('"') {
                if let Some(end) = text[1..].find('"') {
                    return (TT_STRING, end + 2);
                }
            }
            (TT_STRING, text.split_whitespace().next().map(|s| s.len()).unwrap_or(0))
        }
        ConfigValue::Integer(_) => {
            let len = text.chars()
                .take_while(|c| c.is_ascii_digit() || *c == '-')
                .count();
            (TT_NUMBER, len.max(1))
        }
        ConfigValue::Float(_) => {
            let len = text.chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                .count();
            (TT_NUMBER, len.max(1))
        }
        ConfigValue::Boolean(_) => {
            if text.starts_with("true") { (TT_KEYWORD, 4) } else { (TT_KEYWORD, 5) }
        }
        ConfigValue::Date(_) | ConfigValue::Timestamp(_) => {
            if text.starts_with('"') {
                if let Some(end) = text[1..].find('"') {
                    return (TT_EVENT, end + 2);
                }
            }
            (TT_EVENT, text.split_whitespace().next().map(|s| s.len()).unwrap_or(0))
        }
        ConfigValue::ErrorHandling(_)
        | ConfigValue::Compatibility(_)
        | ConfigValue::Debug(_) => {
            if text.starts_with('"') {
                if let Some(end) = text[1..].find('"') {
                    return (TT_TYPE, end + 2);
                }
            }
            (TT_TYPE, text.split_whitespace().next().map(|s| s.len()).unwrap_or(0))
        }
    }
}

// ── Interpolated string ───────────────────────────────────────────────────────

fn emit_interpolated_tokens(
    token:     &Token,
    content:   &str,
    prev_line: &mut u32,
    prev_col:  &mut u32,
    data:      &mut Vec<SemanticToken>,
) {
    let base_line = token.line.saturating_sub(1) as u32;
    let base_col  = token.column.saturating_sub(1) as u32;

    // Multi-line: emit as a single string token.
    if content.contains('\n') {
        push_raw(
            data, prev_line, prev_col,
            base_line, base_col, (content.len() + 3) as u32,
            TT_STRING, 0,
        );
        return;
    }

    // Single-line: split into string segments, operators ({/}), and variable spans.
    let mut seg_start:   u32 = 0;
    let mut char_offset: u32 = 2; // $" prefix = 2 chars
    let mut in_brace          = false;
    let mut brace_start: u32  = 0;

    for ch in content.chars() {
        match ch {
            '{' if !in_brace => {
                let seg_len = char_offset - seg_start;
                if seg_len > 0 {
                    push_raw(
                        data, prev_line, prev_col,
                        base_line, base_col + seg_start, seg_len,
                        TT_STRING, 0,
                    );
                }
                push_raw(
                    data, prev_line, prev_col,
                    base_line, base_col + char_offset, 1,
                    TT_OPERATOR, 0,
                );
                in_brace     = true;
                brace_start  = char_offset + 1;
                char_offset += 1;
            }
            '}' if in_brace => {
                let expr_len = char_offset - brace_start;
                if expr_len > 0 {
                    push_raw(
                        data, prev_line, prev_col,
                        base_line, base_col + brace_start, expr_len,
                        TT_VARIABLE, 0,
                    );
                }
                push_raw(
                    data, prev_line, prev_col,
                    base_line, base_col + char_offset, 1,
                    TT_OPERATOR, 0,
                );
                in_brace     = false;
                seg_start    = char_offset + 1;
                char_offset += 1;
            }
            _ => { char_offset += 1; }
        }
    }

    // Trailing segment (includes closing `"`).
    if !in_brace {
        let seg_len = char_offset + 1 - seg_start;
        if seg_len > 0 {
            push_raw(
                data, prev_line, prev_col,
                base_line, base_col + seg_start, seg_len,
                TT_STRING, 0,
            );
        }
    }
}

// ── Raw token emitter ─────────────────────────────────────────────────────────

fn push_raw(
    data:      &mut Vec<SemanticToken>,
    prev_line: &mut u32,
    prev_col:  &mut u32,
    line: u32, col: u32, len: u32, tt: u32, mods: u32,
) {
    if len == 0 { return; }
    // LSP requires monotone ordering — skip tokens that go backwards.
    if line < *prev_line || (line == *prev_line && col < *prev_col) { return; }
    let dl = line - *prev_line;
    let ds = if dl == 0 { col.saturating_sub(*prev_col) } else { col };
    data.push(SemanticToken {
        delta_line:             dl,
        delta_start:            ds,
        length:                 len,
        token_type:             tt,
        token_modifiers_bitset: mods,
    });
    *prev_line = line;
    *prev_col  = col;
}

// ── Per-token classification ──────────────────────────────────────────────────

fn classify(token: &Token, state: &mut ClassifierState<'_>) -> Option<(u32, u32)> {
    match &token.token_type {
        // ── Section keywords ──────────────────────────────────────────────────
        TokenType::SectionConfig
        | TokenType::SectionImports
        | TokenType::SectionDLM
        | TokenType::SectionEnums
        | TokenType::SectionQuickFuncs
        | TokenType::SectionData
        | TokenType::SectionSecurity     => Some((TT_KEYWORD, MOD_READONLY)),

        // ── Language keywords ──────────────────────────────────────────────────
        TokenType::Keyword(_)            => Some((TT_KEYWORD, 0)),
        TokenType::Bool(_)               => Some((TT_KEYWORD, MOD_READONLY)),
        TokenType::DataType(_)           => Some((TT_TYPE, 0)),

        // ── String types ──────────────────────────────────────────────────────
        TokenType::String(_)
        | TokenType::StringSingle(_)     => Some((TT_STRING, 0)),
        // InterpolatedString handled separately in encode_tokens before classify().
        TokenType::InterpolatedString(_) => Some((TT_STRING, 0)),

        // ── Date / Timestamp ──────────────────────────────────────────────────
        TokenType::Date(_)
        | TokenType::Timestamp(_)        => Some((TT_EVENT, 0)),

        // ── Numeric types ──────────────────────────────────────────────────────
        TokenType::Integer(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_) => Some((TT_NUMBER, 0)),
        TokenType::HexLiteral(_)          => Some((TT_NUMBER, 0)),

        // HexColor — stored without '#'; source has '#'.
        TokenType::HexColor(_)           => Some((TT_NUMBER, MOD_READONLY)),

        // ── Operators ──────────────────────────────────────────────────────────
        TokenType::ArithmeticOp(_)
        | TokenType::ArithmeticAssignOp(_)
        | TokenType::ComparisonOp(_)
        | TokenType::LogicalOp(_)
        | TokenType::BitwiseOp(_)
        | TokenType::MultiCharSymbol(_)  => Some((TT_OPERATOR, 0)),

        TokenType::Arrow
        | TokenType::SwitchCase
        | TokenType::DoubleColon
        | TokenType::ControlFlowColon    => Some((TT_OPERATOR, 0)),

        // FIX: `~` is Symbol('~') — treat as operator regardless of section.
        // The ClassifierState::advance() state machine fires on Symbol('~') and
        // sets next_is_func_name so the following Identifier gets TT_FUNCTION.
        // The `~` itself is just the declaration sigil → operator color.
        // IMPORTANT: this arm must come BEFORE the catch-all Symbol(_) => None arm.
        TokenType::Symbol('~')           => Some((TT_OPERATOR, 0)),

        // ── Comments ──────────────────────────────────────────────────────────
        TokenType::Comment(_)            => Some((TT_COMMENT, 0)),

        // ── Enum access (pre-resolved by semantic analysis) ────────────────────
        TokenType::EnumAccess { .. }     => Some((TT_ENUM_MEMBER, 0)),

        // ── Table paths ────────────────────────────────────────────────────────
        TokenType::TablePath(_)          => Some((TT_PROPERTY, 0)),

        // ── Static function calls / DLM ───────────────────────────────────────
        TokenType::StaticFunction { .. } if token.section == SectionId::Dlm
                                         => Some((TT_MACRO, 0)),
        TokenType::StaticFunction { .. } => Some((TT_FUNCTION, 0)),
        TokenType::DixFunction(_)        => Some((TT_FUNCTION, 0)),
        TokenType::BuiltinMethod(_)      => Some((TT_FUNCTION, 0)),

        // ── Prefixed constructors ──────────────────────────────────────────────
        TokenType::RegexConstructor(_)   => Some((TT_REGEXP, 0)),
        TokenType::BlobConstructor(_)
        | TokenType::TupleConstructor(_)
        | TokenType::PrefixedConstructor { .. } => Some((TT_KEYWORD, 0)),

        // ── Object access ─────────────────────────────────────────────────────
        TokenType::ObjectAccess(_) => {
            if token.section == SectionId::Dlm {
                Some((TT_MACRO, 0))
            } else {
                Some((TT_PROPERTY, 0))
            }
        }

        // ── Identifiers: section-aware classification via state machine ────────
        TokenType::Identifier(_)       => Some(state.classify_identifier(token)),
        TokenType::ScopeDeclaration(_) => Some((TT_TYPE, 0)),
        TokenType::ConfigAccess(_)     => Some((TT_PROPERTY, 0)),

        // ── Skipped / invisible tokens ────────────────────────────────────────
        // Symbol('~') is handled above. All other Symbol variants (punctuation,
        // brackets, etc.) are not colored — they take the editor's default.
        TokenType::ParseContext(_)
        | TokenType::Symbol(_)
        | TokenType::EndOfFile
        | TokenType::Error(_)          => None,
    }
}

// ── Token source-text length ──────────────────────────────────────────────────

fn token_length(token: &Token) -> usize {
    match &token.token_type {
        // String types include their delimiter characters.
        TokenType::String(s)              => s.len() + 2,   // "..."
        TokenType::StringSingle(s)        => s.len() + 2,   // '...'
        TokenType::InterpolatedString(s)  => s.len() + 3,   // $"..."

        // HexColor stored without '#'; source has '#'.
        TokenType::HexColor(h)            => h.len() + 1,

        // Comments include their `//` prefix.
        TokenType::Comment(c)             => c.len() + 2,

        // Section keywords: exact character counts for "@KEYWORD".
        TokenType::SectionConfig          =>  7,  // @CONFIG
        TokenType::SectionImports         =>  8,  // @IMPORTS
        TokenType::SectionDLM             =>  4,  // @DLM
        TokenType::SectionEnums           =>  6,  // @ENUMS
        TokenType::SectionQuickFuncs      => 11,  // @QUICKFUNCS
        TokenType::SectionData            =>  5,  // @DATA
        TokenType::SectionSecurity        =>  9,  // @SECURITY

        // Fixed-length operators.
        TokenType::DoubleColon            =>  2,  // ::
        TokenType::Arrow                  =>  2,  // =>
        TokenType::SwitchCase             =>  2,  // ->
        TokenType::ControlFlowColon       =>  1,  // :

        // NOTE: Symbol('~') falls through to the wildcard below.
        // get_token_value() returns "~", len = 1. Correct.

        // Bool literals.
        TokenType::Bool(b)                => if *b { 4 } else { 5 }, // true / false

        // Prefixed constructors include their prefix+colon.
        TokenType::BlobConstructor(_)     =>  2,  // b:
        TokenType::RegexConstructor(_)    =>  2,  // r:
        TokenType::TupleConstructor(_)    =>  2,  // t:

        // Compound tokens.
        TokenType::EnumAccess { enum_name, value } => enum_name.len() + 1 + value.len(),
        TokenType::TablePath(s)           => s.len(),
        TokenType::ObjectAccess(parts)    => parts.join(".").len(),

        // Everything else: use the display value length.
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}
