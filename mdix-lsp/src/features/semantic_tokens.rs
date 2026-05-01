// mdix-lsp/src/features/semantic_tokens.rs
//! Semantic token provider.
//!
//! Key improvements over original:
//!   - @CONFIG tokens synthesised from AST (stripped before tokenisation, so no tokens exist).
//!   - Enum type names at usage sites (e.g. `AIType` in `AIType.BOSS`) coloured as TT_TYPE.
//!   - Enum field names at usage sites (e.g. `BOSS` in `AIType.BOSS`) coloured as TT_ENUM_MEMBER.
//!   - Function call identifiers (name immediately before `(`) coloured as TT_FUNCTION.
//!   - Lookahead skips type-annotation tokens (`<`, DataType, `>`) when searching for `(`.
//!   - All paths wrapped in catch_unwind.

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

    // Build the set of declared enum type names for usage-site coloring.
    let enum_names: HashSet<String> = doc
        .semantic_result
        .as_ref()
        .and_then(|sr| sr.symbol_table.as_ref())
        .map(|st| st.enums.keys().cloned().collect())
        .unwrap_or_default();

    let data = encode_tokens(doc, &enum_names);
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

// ── Stateful classifier ───────────────────────────────────────────────────────

struct ClassifierState<'a> {
    in_enum_body:        bool,
    enum_brace_depth:    i32,
    seen_enum_name:      bool,
    next_is_func_name:   bool,
    in_param_list:       bool,
    param_paren_depth:   i32,
    next_is_alias:       bool,
    next_is_dlm_module:  bool,
    next_is_dlm_subtype: bool,
    /// True when the current Identifier token is followed by `(` (a call site).
    prev_was_call_ident: bool,
    /// True when the current Identifier is a known enum type name followed by `.`.
    next_is_enum_type:   bool,
    /// True after `EnumTypeName` — persists until `.` is consumed.
    next_is_enum_dot:    bool,
    /// True after `EnumTypeName.` — the next Identifier is a field name.
    prev_was_enum_dot:   bool,
    /// Known enum type names from semantic analysis.
    enum_names: &'a HashSet<String>,
}

impl<'a> ClassifierState<'a> {
    fn new(enum_names: &'a HashSet<String>) -> Self {
        ClassifierState {
            in_enum_body:        false,
            enum_brace_depth:    0,
            seen_enum_name:      false,
            next_is_func_name:   false,
            in_param_list:       false,
            param_paren_depth:   0,
            next_is_alias:       false,
            next_is_dlm_module:  false,
            next_is_dlm_subtype: false,
            prev_was_call_ident: false,
            next_is_enum_type:   false,
            next_is_enum_dot:    false,
            prev_was_enum_dot:   false,
            enum_names,
        }
    }

    fn advance(&mut self, token: &Token, tokens: &[Token], index: usize) {
        // ── Per-token resets ────────────────────────────────────────────────
        // These flags are specific to the current token and must be recomputed.
        self.prev_was_call_ident = false;
        self.next_is_enum_type   = false;

        // Enum-chain flags (next_is_enum_dot, prev_was_enum_dot) persist across
        // Identifier and Symbol('.') tokens.  Reset on anything else that
        // breaks the EnumName.FIELD chain.
        match &token.token_type {
            TokenType::Identifier(_) | TokenType::Symbol('.') => {
                // Don't reset enum chain flags here.
            }
            _ => {
                self.next_is_enum_dot  = false;
                self.prev_was_enum_dot = false;
            }
        }

        // ── Identifier-specific logic ───────────────────────────────────────
        if let TokenType::Identifier(name) = &token.token_type {
            // Function call detection: peek ahead for '(' skipping type annotations.
            // Handles: funcName(, funcName<RetType>(, ~funcName<RetType>(
            let is_call = tokens.iter()
                .skip(index + 1)
                .take(8)
                .filter(|t| !matches!(&t.token_type,
                    TokenType::DataType(_)
                    | TokenType::Symbol('<')
                    | TokenType::Symbol('>')))
                .take(3)
                .any(|t| matches!(t.token_type, TokenType::Symbol('(')));
            self.prev_was_call_ident = is_call;

            // Enum type name detection: known enum followed by '.'.
            if self.enum_names.contains(name.as_str()) {
                let has_dot = tokens.iter()
                    .skip(index + 1)
                    .take(2)
                    .any(|t| matches!(t.token_type, TokenType::Symbol('.')));
                if has_dot {
                    self.next_is_enum_type = true;
                    self.next_is_enum_dot  = true;
                } else {
                    // Enum name not followed by '.' — standalone reference.
                    self.next_is_enum_dot = false;
                }
            } else {
                // Not an enum type name; clear enum-dot tracking.
                self.next_is_enum_dot = false;
            }
        }

        // ── Dot: transition from enum-name to enum-field ───────────────────
        if let TokenType::Symbol('.') = &token.token_type {
            if self.next_is_enum_dot {
                self.prev_was_enum_dot = true;
                self.next_is_enum_dot  = false;
            }
        }

        // ── Section-level state machine ─────────────────────────────────────
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

            TokenType::FunctionPrefix => {
                self.next_is_func_name = true;
                self.in_param_list     = false;
                self.param_paren_depth = 0;
            }
            TokenType::Symbol('(') if token.section == SectionId::QuickFuncs => {
                if self.next_is_func_name {
                    self.in_param_list     = true;
                    self.param_paren_depth = 1;
                } else if self.in_param_list {
                    self.param_paren_depth += 1;
                }
            }
            TokenType::Symbol(')') if token.section == SectionId::QuickFuncs => {
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
                self.next_is_dlm_module  = true;
                self.next_is_dlm_subtype = false;
            }
            TokenType::Symbol('.') if token.section == SectionId::Dlm => {
                self.next_is_dlm_subtype = true;
            }

            _ => {}
        }
    }

    fn classify_identifier(&mut self, token: &Token) -> (u32, u32) {
        // Priority 1: enum field at usage site (e.g. BOSS in AIType.BOSS).
        if self.prev_was_enum_dot {
            self.prev_was_enum_dot = false; // consume
            return (TT_ENUM_MEMBER, 0);
        }

        // Priority 2: enum type name at usage site (e.g. AIType in AIType.BOSS).
        if self.next_is_enum_type {
            // next_is_enum_type is reset at the start of the next advance() call.
            return (TT_TYPE, 0);
        }

        // Priority 3: function call identifier (outside a declaration).
        if self.prev_was_call_ident && !self.next_is_func_name {
            return (TT_FUNCTION, 0);
        }

        // Priority 4: section-specific classification.
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
                if self.next_is_dlm_subtype {
                    self.next_is_dlm_subtype = false;
                    (TT_DECORATOR, 0)
                } else if self.next_is_dlm_module {
                    self.next_is_dlm_module  = false;
                    self.next_is_dlm_subtype = true;
                    (TT_MACRO, MOD_DECLARATION)
                } else {
                    (TT_MACRO, 0)
                }
            }

            SectionId::Data     => (TT_VARIABLE, 0),
            SectionId::Security => (TT_PROPERTY, 0),
            _                   => (TT_VARIABLE, 0),
        }
    }
}

// ── Encoder ───────────────────────────────────────────────────────────────────

fn encode_tokens(doc: &Document, enum_names: &HashSet<String>) -> Vec<SemanticToken> {
    let mut data: Vec<SemanticToken> = Vec::with_capacity(doc.tokens.len() + 32);
    let mut prev_line: u32 = 0;
    let mut prev_col:  u32 = 0;
    let mut state = ClassifierState::new(enum_names);

    // ── Emit @CONFIG section tokens from AST ──────────────────────────────
    // @CONFIG is stripped (replaced with blank lines) before tokenisation, so
    // no Token entries exist for it in doc.tokens.  We synthesise them here
    // directly from doc.ast.config, reading positions from the original source.
    if let Some(ast) = &doc.ast {
        emit_config_tokens(ast, &doc.source, &mut prev_line, &mut prev_col, &mut data);
    }

    // ── Emit tokens from the regular token stream ─────────────────────────
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

        let line = token.line.saturating_sub(1) as u32;
        let col  = token.column.saturating_sub(1) as u32;

        let length = token_length(token) as u32;
        if length == 0 { continue; }

        push_raw(&mut data, &mut prev_line, &mut prev_col, line, col, length, token_type, modifiers);
    }

    data
}

// ── CONFIG token synthesis ────────────────────────────────────────────────────

/// Synthesise semantic tokens for `@CONFIG(...)` entries directly from the AST.
///
/// @CONFIG is stripped before tokenisation (replaced with blank newlines that
/// preserve line numbers for all subsequent tokens).  We read positions from
/// `ast.config` and scan the original `source` text to locate `->` and values.
fn emit_config_tokens(
    ast:       &DixScript,
    source:    &str,
    prev_line: &mut u32,
    prev_col:  &mut u32,
    data:      &mut Vec<SemanticToken>,
) {
    let config = match ast.config.as_ref() {
        Some(c) => c,
        None    => return,
    };

    let source_lines: Vec<&str> = source.lines().collect();

    // Emit @CONFIG section keyword.
    if config.position.is_valid() {
        let lsp_line = (config.position.line - 1) as u32;
        let lsp_col  = (config.position.column - 1) as u32;
        push_raw(data, prev_line, prev_col, lsp_line, lsp_col, 7, TT_KEYWORD, MOD_READONLY);
        // "@CONFIG" = 7 chars; the "(" is a plain symbol, leave uncoloured.
    }

    // Sort entries by position to ensure strictly increasing delta encoding.
    let mut entries: Vec<&dixscript::Compiler::AST::ConfigEntry> =
        config.entries.iter().collect();
    entries.sort_by_key(|e| (e.position.line, e.position.column));

    for entry in entries {
        if !entry.position.is_valid() { continue; }

        let lsp_line = (entry.position.line - 1) as u32;
        let line_text = match source_lines.get(entry.position.line - 1) {
            Some(l) => l,
            None    => continue,
        };

        let key_start_0 = entry.position.column.saturating_sub(1);
        let key_col     = key_start_0 as u32;
        let key_len     = entry.key.len() as u32;
        if key_len == 0 { continue; }

        // Key token → TT_PROPERTY.
        push_raw(data, prev_line, prev_col, lsp_line, key_col, key_len, TT_PROPERTY, 0);

        // Scan for `->` after the key.
        let search_start = key_start_0 + entry.key.len();
        if search_start >= line_text.len() { continue; }

        if let Some(arrow_rel) = line_text[search_start..].find("->") {
            let arrow_col = (search_start + arrow_rel) as u32;
            push_raw(data, prev_line, prev_col, lsp_line, arrow_col, 2, TT_OPERATOR, 0);

            let after_arrow = search_start + arrow_rel + 2;
            if after_arrow >= line_text.len() { continue; }

            let value_raw  = &line_text[after_arrow..];
            let trim_len   = value_raw.len() - value_raw.trim_start().len();
            let value_col  = (after_arrow + trim_len) as u32;
            let value_text = value_raw.trim_start();

            let (tt, len) = classify_config_value(&entry.value, value_text);
            if len > 0 {
                push_raw(data, prev_line, prev_col, lsp_line, value_col, len as u32, tt, 0);
            }
        }
    }
}

/// Classify a `@CONFIG` value into a (token_type, source_length) pair.
fn classify_config_value(value: &ConfigValue, text: &str) -> (u32, usize) {
    match value {
        ConfigValue::String(_) | ConfigValue::Features(_) => {
            // String literal: count chars inside quotes.
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
            if text.starts_with("true")  { (TT_KEYWORD, 4) }
            else if text.starts_with("false") { (TT_KEYWORD, 5) }
            else { (TT_KEYWORD, 4) }
        }
        ConfigValue::Date(_) | ConfigValue::Timestamp(_) => {
            if text.starts_with('"') {
                if let Some(end) = text[1..].find('"') {
                    return (TT_EVENT, end + 2);
                }
            }
            (TT_EVENT, text.split_whitespace().next().map(|s| s.len()).unwrap_or(0))
        }
        ConfigValue::ErrorHandling(_) | ConfigValue::Compatibility(_) | ConfigValue::Debug(_) => {
            // These are always string literals like "halt", "strict", "off".
            if text.starts_with('"') {
                if let Some(end) = text[1..].find('"') {
                    return (TT_TYPE, end + 2);
                }
            }
            (TT_TYPE, text.split_whitespace().next().map(|s| s.len()).unwrap_or(0))
        }
    }
}

// ── Interpolated string splitter ──────────────────────────────────────────────

fn emit_interpolated_tokens(
    token:     &Token,
    content:   &str,
    prev_line: &mut u32,
    prev_col:  &mut u32,
    data:      &mut Vec<SemanticToken>,
) {
    let base_line = token.line.saturating_sub(1) as u32;
    let base_col  = token.column.saturating_sub(1) as u32;

    if content.contains('\n') {
        push_raw(data, prev_line, prev_col, base_line, base_col,
            (content.len() + 3) as u32, TT_STRING, 0);
        return;
    }

    let mut seg_start:   u32 = 0;
    let mut char_offset: u32 = 2; // $"
    let mut in_brace          = false;
    let mut brace_start: u32  = 0;

    for ch in content.chars() {
        match ch {
            '{' if !in_brace => {
                let seg_len = char_offset - seg_start;
                push_raw(data, prev_line, prev_col, base_line,
                    base_col + seg_start, seg_len, TT_STRING, 0);
                push_raw(data, prev_line, prev_col, base_line,
                    base_col + char_offset, 1, TT_OPERATOR, 0);
                in_brace     = true;
                brace_start  = char_offset + 1;
                char_offset += 1;
            }
            '}' if in_brace => {
                let expr_len = char_offset - brace_start;
                push_raw(data, prev_line, prev_col, base_line,
                    base_col + brace_start, expr_len, TT_VARIABLE, 0);
                push_raw(data, prev_line, prev_col, base_line,
                    base_col + char_offset, 1, TT_OPERATOR, 0);
                in_brace     = false;
                seg_start    = char_offset + 1;
                char_offset += 1;
            }
            _ => { char_offset += 1; }
        }
    }

    if !in_brace {
        let end_offset = char_offset + 1;
        let seg_len    = end_offset - seg_start;
        push_raw(data, prev_line, prev_col, base_line,
            base_col + seg_start, seg_len, TT_STRING, 0);
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
    // Skip out-of-order tokens (e.g. invalid AST positions).
    if line < *prev_line || (line == *prev_line && col < *prev_col) {
        return;
    }
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
        TokenType::SectionConfig
        | TokenType::SectionImports
        | TokenType::SectionDLM
        | TokenType::SectionEnums
        | TokenType::SectionQuickFuncs
        | TokenType::SectionData
        | TokenType::SectionSecurity     => Some((TT_KEYWORD, MOD_READONLY)),

        TokenType::Keyword(_)            => Some((TT_KEYWORD, 0)),
        TokenType::Bool(_)               => Some((TT_KEYWORD, MOD_READONLY)),
        TokenType::DataType(_)           => Some((TT_TYPE, 0)),

        TokenType::String(_)
        | TokenType::StringSingle(_)     => Some((TT_STRING, 0)),
        TokenType::InterpolatedString(_) => Some((TT_STRING, 0)),

        TokenType::Date(_)
        | TokenType::Timestamp(_)        => Some((TT_EVENT, 0)),

        TokenType::Integer(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_) => Some((TT_NUMBER, 0)),

        TokenType::HexLiteral(_)         => Some((TT_NUMBER, 0)),
        TokenType::HexColor(_)           => Some((TT_NUMBER, MOD_READONLY)),

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

        TokenType::FunctionPrefix        => Some((TT_OPERATOR, 0)),
        TokenType::Comment(_)            => Some((TT_COMMENT, 0)),
        TokenType::EnumAccess { .. }     => Some((TT_ENUM_MEMBER, 0)),
        TokenType::TablePath(_)          => Some((TT_PROPERTY, 0)),

        TokenType::StaticFunction { .. } => Some((TT_FUNCTION, 0)),
        TokenType::DixFunction(_)        => Some((TT_FUNCTION, 0)),
        TokenType::BuiltinMethod(_)      => Some((TT_FUNCTION, 0)),

        TokenType::RegexConstructor(_)   => Some((TT_REGEXP, 0)),

        TokenType::BlobConstructor(_)
        | TokenType::TupleConstructor(_)
        | TokenType::PrefixedConstructor { .. } => Some((TT_KEYWORD, 0)),

        TokenType::ObjectAccess(_) => {
            if token.section == SectionId::Dlm {
                Some((TT_DECORATOR, 0))
            } else {
                Some((TT_PROPERTY, 0))
            }
        }

        TokenType::Identifier(_)       => Some(state.classify_identifier(token)),
        TokenType::ScopeDeclaration(_) => Some((TT_TYPE, 0)),
        TokenType::ConfigAccess(_)     => Some((TT_PROPERTY, 0)),

        TokenType::ParseContext(_)
        | TokenType::Symbol(_)
        | TokenType::EndOfFile
        | TokenType::Error(_)          => None,
    }
}

// ── Token source-text length ──────────────────────────────────────────────────

fn token_length(token: &Token) -> usize {
    match &token.token_type {
        TokenType::String(s)              => s.len() + 2,
        TokenType::StringSingle(s)        => s.len() + 2,
        TokenType::InterpolatedString(s)  => s.len() + 3,
        TokenType::HexColor(h)            => h.len() + 1,
        TokenType::Comment(c)             => c.len() + 2,
        TokenType::SectionConfig          =>  7,
        TokenType::SectionImports         =>  8,
        TokenType::SectionDLM             =>  4,
        TokenType::SectionEnums           =>  6,
        TokenType::SectionQuickFuncs      => 11,
        TokenType::SectionData            =>  5,
        TokenType::SectionSecurity        =>  9,
        TokenType::DoubleColon            =>  2,
        TokenType::Arrow                  =>  2,
        TokenType::SwitchCase             =>  2,
        TokenType::ControlFlowColon       =>  1,
        TokenType::FunctionPrefix         =>  1,
        TokenType::Bool(b)                => if *b { 4 } else { 5 },
        TokenType::BlobConstructor(_)     =>  2,
        TokenType::RegexConstructor(_)    =>  2,
        TokenType::TupleConstructor(_)    =>  2,
        TokenType::EnumAccess { enum_name, value } => enum_name.len() + 1 + value.len(),
        TokenType::TablePath(s)           => s.len(),
        TokenType::ObjectAccess(parts)    => parts.join(".").len(),
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}
