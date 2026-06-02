// mdix-lsp/src/features/semantic_tokens.rs
//!
//! ## Token coloring scheme
//! - `TT_NAMESPACE`   — static object receivers (Math, DateTime, …) + table/group-array paths
//! - `TT_FUNCTION`    — QuickFunc calls and static method calls (MOD_STATIC modifier)
//! - `TT_METHOD`      — instance method calls (.toUpper(), .length(), .get(k) …)
//! - `TT_PROPERTY`    — property access after dot (non-call), CONFIG keys, SECURITY keys
//! - `TT_MACRO`       — DLM module names (DCompressor, DEncryptor, DAuditor)
//! - `TT_DECORATOR`   — DLM subtype names (gzip, aes256, …)
//!
//! ## Approach B — real @CONFIG tokens
//! @CONFIG tokens are in the full token stream with SectionId::Config and
//! accurate positions. No synthetic token emission needed.
//!
//! ## Long token
//! `TokenType::Long(_)` → TT_NUMBER (same as Integer).
//!
//! ## Control-flow keyword colouring
//! Any Identifier in @QUICKFUNCS immediately followed by ControlFlowColon
//! (`log:`, `if:`, `chk:`, `elif:`) is classified as TT_KEYWORD.

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
    TT_REGEXP, TT_EVENT, TT_METHOD,
    MOD_DECLARATION, MOD_READONLY, MOD_STATIC,
};

// ── Known built-in static objects (colour receiver as TT_NAMESPACE) ───────────
// DLM modules (DCompressor/DEncryptor/DAuditor) are intentionally excluded here;
// they are handled by the dedicated DLM section logic (TT_MACRO / TT_DECORATOR).
const STATIC_OBJECT_NAMES: &[&str] = &[
    "Math", "DateTime", "Array", "Random", "Guid", "IpAddress", "Enum", "Dix",
];

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
    // ── Enum body tracking ────────────────────────────────────────────────────
    in_enum_body:      bool,
    enum_brace_depth:  i32,
    seen_enum_name:    bool,

    // ── QuickFunc declaration tracking ────────────────────────────────────────
    next_is_func_name: bool,
    in_param_list:     bool,
    param_paren_depth: i32,

    // ── Import alias tracking ─────────────────────────────────────────────────
    next_is_alias:     bool,

    // ── DLM dot tracking ──────────────────────────────────────────────────────
    dlm_dot_seen:      bool,

    // ── Call-site detection ───────────────────────────────────────────────────
    is_call_site:      bool,

    // ── Enum access dot tracking ──────────────────────────────────────────────
    next_is_enum_type:  bool,
    next_is_enum_dot:   bool,
    prev_was_enum_dot:  bool,

    // ── Static / instance dot tracking (NEW) ──────────────────────────────────
    /// Set when current identifier is a known static object name (Math, DateTime …).
    next_is_static_obj:  bool,
    /// Set when the preceding `.` followed a static object receiver.
    after_static_dot:    bool,
    /// Set when the preceding `.` followed a non-static, non-enum identifier.
    after_instance_dot:  bool,

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
            next_is_static_obj:  false,
            after_static_dot:    false,
            after_instance_dot:  false,
            enum_names,
            func_names,
        }
    }

    fn advance(&mut self, token: &Token, tokens: &[Token], index: usize) {
        // Reset per-token flags.
        self.is_call_site      = false;
        self.next_is_enum_type = false;

        // Reset enum-dot state for non-identifier, non-dot tokens so that
        // `prev_was_enum_dot` doesn't linger across unrelated tokens.
        match &token.token_type {
            TokenType::Identifier(_) | TokenType::Symbol('.') => {}
            _ => {
                // Preserve dot context across type-annotation tokens (<type>)
                // but clear it for everything else.
                match &token.token_type {
                    TokenType::Symbol('<') | TokenType::Symbol('>')
                    | TokenType::DataType(_) => {}
                    _ => {
                        // Clear after_static/instance dot on structural tokens
                        // (arithmetic ops, commas, newlines, etc.) so stale
                        // state from a previous line cannot bleed forward.
                        match &token.token_type {
                            TokenType::ArithmeticOp(_)
                            | TokenType::ComparisonOp(_)
                            | TokenType::LogicalOp(_)
                            | TokenType::BitwiseOp(_)
                            | TokenType::ArithmeticAssignOp(_)
                            | TokenType::Arrow
                            | TokenType::SwitchCase
                            | TokenType::DoubleColon
                            | TokenType::ControlFlowColon
                            | TokenType::Symbol(';') => {
                                self.after_static_dot   = false;
                                self.after_instance_dot = false;
                                self.next_is_static_obj = false;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // ── Identifier handling ───────────────────────────────────────────────
        if let TokenType::Identifier(name) = &token.token_type {
            // Track whether this identifier is a known static-object receiver.
            self.next_is_static_obj = STATIC_OBJECT_NAMES.contains(&name.as_str());

            // Call-site detection: is this identifier being called with `(`?
            let in_symbol_table = self.func_names.contains(name.as_str());
            let lookahead_paren = if in_symbol_table {
                true
            } else {
                is_followed_by_paren(tokens, index + 1)
            };
            // Never mark a function DECLARATION (after ~) as a call site.
            self.is_call_site = lookahead_paren && !self.next_is_func_name;

            // Enum type detection.
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

        // ── Structural token state transitions ────────────────────────────────
        match &token.token_type {

            // ── Dot (.) — classify what follows as static/instance/enum method ─
            TokenType::Symbol('.') => {
                if self.next_is_enum_dot {
                    // Enum field access: Status.ACTIVE
                    self.prev_was_enum_dot  = true;
                    self.next_is_enum_dot   = false;
                    self.after_static_dot   = false;
                    self.after_instance_dot = false;
                } else {
                    self.prev_was_enum_dot  = false;
                    self.after_static_dot   = self.next_is_static_obj;
                    self.after_instance_dot = !self.next_is_static_obj;
                }
                // Consumed — reset so the next identifier doesn't carry it.
                self.next_is_static_obj = false;
                // DLM subtype tracking.
                self.dlm_dot_seen = true;
            }

            // ── @ENUMS brace tracking ─────────────────────────────────────────
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

            // ── QuickFunc declaration (~) ────────────────────────────────────
            TokenType::Symbol('~') => {
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

            // ── Import alias ──────────────────────────────────────────────────
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

            // ── DLM section reset ─────────────────────────────────────────────
            TokenType::SectionDLM => {
                self.dlm_dot_seen = false;
            }
            TokenType::Symbol(',') if token.section == SectionId::Dlm => {
                self.dlm_dot_seen = false;
            }

            _ => {}
        }
    }

    /// Classify an `Identifier` token, consuming relevant state flags.
    ///
    /// Priority order:
    ///  0. DLM section (absolute override for module/subtype colouring)
    ///  1. Static-object receiver with lookahead dot  → TT_NAMESPACE
    ///  2. Control-flow keyword with lookahead colon  → TT_KEYWORD
    ///  3. Enum member after dot                      → TT_ENUM_MEMBER
    ///  4. Enum type name                             → TT_TYPE
    ///  5. After static dot                           → TT_FUNCTION+MOD_STATIC or TT_PROPERTY
    ///  6. After instance dot                         → TT_METHOD or TT_PROPERTY
    ///  7. Regular QuickFunc call site                → TT_FUNCTION
    ///  8. Section-specific                           → various
    fn classify_identifier(&mut self, token: &Token, tokens: &[Token], index: usize) -> (u32, u32) {

        // ── 0. DLM section — absolute priority ────────────────────────────────
        if token.section == SectionId::Dlm {
            let result = if self.dlm_dot_seen {
                (TT_DECORATOR, 0)
            } else {
                (TT_MACRO, MOD_DECLARATION)
            };
            self.dlm_dot_seen       = false;
            self.after_static_dot   = false;
            self.after_instance_dot = false;
            return result;
        }

        // ── 1. Static-object receiver (lookahead for '.') ─────────────────────
        if let TokenType::Identifier(name) = &token.token_type {
            if STATIC_OBJECT_NAMES.contains(&name.as_str()) {
                let next_is_dot = tokens.get(index + 1)
                    .map(|t| matches!(t.token_type, TokenType::Symbol('.')))
                    .unwrap_or(false);
                if next_is_dot {
                    return (TT_NAMESPACE, 0);
                }
            }
        }

        // ── 2. Control-flow keyword detection in QuickFuncs ───────────────────
        // Handles `log:`, `if:`, `chk:`, `elif:` when tokenised as Identifier + ControlFlowColon.
        if token.section == SectionId::QuickFuncs {
            let next_is_colon = tokens.get(index + 1)
                .map(|t| matches!(t.token_type, TokenType::ControlFlowColon))
                .unwrap_or(false);
            if next_is_colon {
                return (TT_KEYWORD, 0);
            }
        }

        // ── 3. Enum member after dot (Status.ACTIVE) ──────────────────────────
        if self.prev_was_enum_dot {
            self.prev_was_enum_dot  = false;
            self.after_instance_dot = false;
            self.after_static_dot   = false;
            return (TT_ENUM_MEMBER, 0);
        }

        // ── 4. Enum type name (Status in Status.ACTIVE) ───────────────────────
        if self.next_is_enum_type {
            // next_is_enum_type was reset to false at start of advance(); it is
            // re-set within advance() for enum-name identifiers and consumed here.
            return (TT_TYPE, 0);
        }

        // ── 5. After static-object dot (Math.sqrt, DateTime.now) ─────────────
        if self.after_static_dot {
            let result = if self.is_call_site {
                (TT_FUNCTION, MOD_STATIC)
            } else {
                (TT_PROPERTY, MOD_STATIC)
            };
            self.after_static_dot   = false;
            self.after_instance_dot = false;
            return result;
        }

        // ── 6. After instance dot (myStr.toUpper, arr.length) ────────────────
        if self.after_instance_dot {
            let result = if self.is_call_site {
                (TT_METHOD, 0)
            } else {
                (TT_PROPERTY, 0)
            };
            self.after_instance_dot = false;
            return result;
        }

        // ── 7. Regular call site (direct QuickFunc call) ──────────────────────
        if self.is_call_site {
            return (TT_FUNCTION, 0);
        }

        // ── 8. Section-specific classification ───────────────────────────────
        match token.section {
            // @CONFIG keys: real tokens with SectionId::Config, colour as properties.
            SectionId::Config => (TT_PROPERTY, 0),

            // @ENUMS: enum type declaration names and field values.
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

            // @QUICKFUNCS: function names, parameters, local variables.
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

            // @IMPORTS: alias declarations and path tokens.
            SectionId::Imports => {
                if self.next_is_alias {
                    self.next_is_alias = false;
                    (TT_NAMESPACE, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            // @DATA: data variable identifiers.
            SectionId::Data => (TT_VARIABLE, 0),

            // @SECURITY: security block keys.
            SectionId::Security => (TT_PROPERTY, 0),

            // No section (document-level or unknown).
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
            // scope arrow before param list: ~funcName => ScopeA, ScopeB(params)
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

    for (index, token) in doc.tokens.iter().enumerate() {
        state.advance(token, &doc.tokens, index);

        if let TokenType::InterpolatedString(content) = &token.token_type {
            emit_interpolated_tokens(token, content, &mut prev_line, &mut prev_col, &mut data);
            continue;
        }

        let (token_type, modifiers) = match classify(token, &mut state, &doc.tokens, index) {
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

fn classify(
    token: &Token,
    state: &mut ClassifierState<'_>,
    tokens: &[Token],
    index: usize,
) -> Option<(u32, u32)> {
    match &token.token_type {
        // ── Section keywords ──────────────────────────────────────────────────
        TokenType::SectionConfig
        | TokenType::SectionImports
        | TokenType::SectionDLM
        | TokenType::SectionEnums
        | TokenType::SectionQuickFuncs
        | TokenType::SectionData
        | TokenType::SectionSecurity     => Some((TT_KEYWORD, MOD_READONLY)),

        // ── Language keywords ─────────────────────────────────────────────────
        TokenType::Keyword(_)            => Some((TT_KEYWORD, 0)),
        TokenType::Bool(_)               => Some((TT_KEYWORD, MOD_READONLY)),
        TokenType::DataType(_)           => Some((TT_TYPE, 0)),

        // ── String literals ───────────────────────────────────────────────────
        TokenType::String(_)
        | TokenType::StringSingle(_)     => Some((TT_STRING, 0)),
        TokenType::InterpolatedString(_) => Some((TT_STRING, 0)),

        // ── Temporal values ───────────────────────────────────────────────────
        TokenType::Date(_)
        | TokenType::Timestamp(_)        => Some((TT_EVENT, 0)),

        // ── Numeric literals ──────────────────────────────────────────────────
        TokenType::Integer(_)
        | TokenType::Long(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_) => Some((TT_NUMBER, 0)),
        TokenType::HexLiteral(_)           => Some((TT_NUMBER, 0)),
        TokenType::HexColor(_)             => Some((TT_NUMBER, MOD_READONLY)),

        // ── Operators ─────────────────────────────────────────────────────────
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

        // ── Comments ──────────────────────────────────────────────────────────
        TokenType::Comment(_)            => Some((TT_COMMENT, 0)),

        // ── Enum access (pre-analysed by tokeniser) ───────────────────────────
        TokenType::EnumAccess { .. }     => Some((TT_ENUM_MEMBER, 0)),

        // ── Table/group-array paths — colour as NAMESPACE, not plain property ─
        // This gives `server:` and `enemies::` paths a distinct teal/purple tone
        // that visually separates them from regular identifiers and properties.
        TokenType::TablePath(_)          => Some((TT_NAMESPACE, MOD_DECLARATION)),

        // ── Pre-analysed static/builtin calls ─────────────────────────────────
        // StaticFunction tokens: Dix.Log, Math.sqrt emitted by tokeniser.
        TokenType::StaticFunction { .. } if token.section == SectionId::Dlm
                                         => Some((TT_MACRO, 0)),
        TokenType::StaticFunction { .. } => Some((TT_FUNCTION, MOD_STATIC)),  // was plain FUNCTION

        // Dix.Something — always static.
        TokenType::DixFunction(_)        => Some((TT_FUNCTION, MOD_STATIC)),   // was plain FUNCTION

        // Built-in instance methods (e.g. .length, .contains) emitted pre-analysed.
        TokenType::BuiltinMethod(_)      => Some((TT_METHOD, 0)),              // was plain FUNCTION

        // ── Prefixed constructors ─────────────────────────────────────────────
        TokenType::RegexConstructor(_)   => Some((TT_REGEXP, 0)),
        TokenType::BlobConstructor(_)
        | TokenType::TupleConstructor(_)
        | TokenType::PrefixedConstructor { .. } => Some((TT_KEYWORD, 0)),

        // ── Object / config access paths ──────────────────────────────────────
        TokenType::ObjectAccess(_) => {
            if token.section == SectionId::Dlm {
                Some((TT_MACRO, 0))
            } else {
                Some((TT_PROPERTY, 0))
            }
        }

        // ── Plain identifiers — full stateful classification ───────────────────
        TokenType::Identifier(_) => Some(state.classify_identifier(token, tokens, index)),

        // ── Scope declarations (@QUICKFUNCS => ScopeA, ScopeB) ───────────────
        TokenType::ScopeDeclaration(_) => Some((TT_TYPE, 0)),

        // ── Config access paths ────────────────────────────────────────────────
        TokenType::ConfigAccess(_)     => Some((TT_PROPERTY, 0)),

        // ── Ignored / structural ───────────────────────────────────────────────
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
