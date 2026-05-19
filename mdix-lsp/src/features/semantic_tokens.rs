// mdix-lsp/src/features/semantic_tokens.rs
//!
//! ## Approach B — real @CONFIG tokens
//! @CONFIG tokens are now in the full token stream with SectionId::Config and
//! accurate positions. No synthetic token emission needed. The classifier
//! already handles SectionId::Config identifiers as TT_PROPERTY, SectionConfig
//! as TT_KEYWORD, SwitchCase as TT_OPERATOR, and all value types naturally.
//!
//! ## Function call coloring
//! Any Identifier immediately followed by `(` (optionally with a `<type>`
//! annotation between them) is classified as TT_FUNCTION regardless of section.
//! Symbol-table registered functions are always TT_FUNCTION even without
//! lookahead.
//!
//! ## Long token
//! `TokenType::Long(_)` → TT_NUMBER (same as Integer).
//!
//! ## DLM coloring
//! Single `dlm_dot_seen` flag distinguishes module name (TT_MACRO) from
//! subtype (TT_DECORATOR).

use std::collections::HashSet;
use std::panic;

use tower_lsp::lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult};
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

    is_call_site:      bool,

    next_is_enum_type:  bool,
    next_is_enum_dot:   bool,
    prev_was_enum_dot:  bool,

    enum_names: &'a HashSet<String>,
    func_names: &'a HashSet<String>,
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
            is_call_site:        false,
            next_is_enum_type:   false,
            next_is_enum_dot:    false,
            prev_was_enum_dot:   false,
            enum_names,
            func_names,
        }
    }

    fn advance(&mut self, token: &Token, tokens: &[Token], index: usize) {
        self.is_call_site      = false;
        self.next_is_enum_type = false;

        match &token.token_type {
            TokenType::Identifier(_) | TokenType::Symbol('.') => {}
            _ => {
                self.next_is_enum_dot  = false;
                self.prev_was_enum_dot = false;
            }
        }

        if let TokenType::Identifier(name) = &token.token_type {
            let in_symbol_table = self.func_names.contains(name.as_str());

            let lookahead_paren = if in_symbol_table {
                true
            } else {
                is_followed_by_paren(tokens, index + 1)
            };

            self.is_call_site = lookahead_paren && !self.next_is_func_name;

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

        if self.is_call_site {
            return (TT_FUNCTION, 0);
        }

        match token.section {
            // @CONFIG keys — real tokens now, classified as properties.
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

// ── Call-site lookahead ───────────────────────────────────────────────────────

fn is_followed_by_paren(tokens: &[Token], start: usize) -> bool {
    let mut i = start;
    let mut angle_depth: i32 = 0;

    while i < tokens.len() {
        match &tokens[i].token_type {
            TokenType::Symbol('<') => { angle_depth += 1; i += 1; }
            TokenType::Symbol('>') => { angle_depth -= 1; i += 1; }
            _ if angle_depth > 0  => { i += 1; }
            TokenType::Symbol('(') => return true,
            TokenType::Arrow => {
                i += 1;
                while i < tokens.len() {
                    match &tokens[i].token_type {
                        TokenType::Symbol('(') => return true,
                        TokenType::Identifier(_) | TokenType::Symbol(',') => { i += 1; }
                        _ => break,
                    }
                }
                break;
            }
            _ => break,
        }
        if i > start + 16 { break; }
    }
    false
}

// ── Encoder ───────────────────────────────────────────────────────────────────

fn encode_tokens(
    doc:        &Document,
    enum_names: &HashSet<String>,
    func_names: &HashSet<String>,
) -> Vec<SemanticToken> {
    let mut data: Vec<SemanticToken> = Vec::with_capacity(doc.tokens.len());
    let mut prev_line: u32 = 0;
    let mut prev_col:  u32 = 0;
    let mut state = ClassifierState::new(enum_names, func_names);

    // doc.tokens is the full stream including @CONFIG tokens with real positions.
    // No synthetic emission needed — the loop below handles everything.
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

    if content.contains('\n') {
        push_raw(
            data, prev_line, prev_col,
            base_line, base_col, (content.len() + 3) as u32,
            TT_STRING, 0,
        );
        return;
    }

    let mut seg_start:   u32 = 0;
    let mut char_offset: u32 = 2;
    let mut in_brace          = false;
    let mut brace_start: u32  = 0;

    for ch in content.chars() {
        match ch {
            '{' if !in_brace => {
                let seg_len = char_offset - seg_start;
                if seg_len > 0 {
                    push_raw(data, prev_line, prev_col, base_line, base_col + seg_start, seg_len, TT_STRING, 0);
                }
                push_raw(data, prev_line, prev_col, base_line, base_col + char_offset, 1, TT_OPERATOR, 0);
                in_brace     = true;
                brace_start  = char_offset + 1;
                char_offset += 1;
            }
            '}' if in_brace => {
                let expr_len = char_offset - brace_start;
                if expr_len > 0 {
                    push_raw(data, prev_line, prev_col, base_line, base_col + brace_start, expr_len, TT_VARIABLE, 0);
                }
                push_raw(data, prev_line, prev_col, base_line, base_col + char_offset, 1, TT_OPERATOR, 0);
                in_brace     = false;
                seg_start    = char_offset + 1;
                char_offset += 1;
            }
            _ => { char_offset += 1; }
        }
    }

    if !in_brace {
        let seg_len = char_offset + 1 - seg_start;
        if seg_len > 0 {
            push_raw(data, prev_line, prev_col, base_line, base_col + seg_start, seg_len, TT_STRING, 0);
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
        | TokenType::Long(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_) => Some((TT_NUMBER, 0)),
        TokenType::HexLiteral(_)           => Some((TT_NUMBER, 0)),
        TokenType::HexColor(_)             => Some((TT_NUMBER, MOD_READONLY)),

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

        TokenType::Symbol('~')           => Some((TT_OPERATOR, 0)),

        TokenType::Comment(_)            => Some((TT_COMMENT, 0)),

        TokenType::EnumAccess { .. }     => Some((TT_ENUM_MEMBER, 0)),

        TokenType::TablePath(_)          => Some((TT_PROPERTY, 0)),

        TokenType::StaticFunction { .. } if token.section == SectionId::Dlm
                                         => Some((TT_MACRO, 0)),
        TokenType::StaticFunction { .. } => Some((TT_FUNCTION, 0)),
        TokenType::DixFunction(_)        => Some((TT_FUNCTION, 0)),
        TokenType::BuiltinMethod(_)      => Some((TT_FUNCTION, 0)),

        TokenType::RegexConstructor(_)   => Some((TT_REGEXP, 0)),
        TokenType::BlobConstructor(_)
        | TokenType::TupleConstructor(_)
        | TokenType::PrefixedConstructor { .. } => Some((TT_KEYWORD, 0)),

        TokenType::ObjectAccess(_) => {
            if token.section == SectionId::Dlm {
                Some((TT_MACRO, 0))
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
        TokenType::HexColor(h)            => h.trim_start_matches('#').len() + 1,
        TokenType::Comment(c)             => c.len() + 2,
        TokenType::Long(l)                => format!("{}L", l).len(),
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
