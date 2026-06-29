
//!
//! ## Token coloring scheme
//! - `TT_NAMESPACE`   — static object receivers (Math, DateTime, …) & import aliases
//! - `TT_FUNCTION`    — QuickFunc declarations, direct calls, static method calls (+MOD_STATIC)
//! - `TT_METHOD`      — instance method calls (.toUpper(), .push() etc.)
//! - `TT_PROPERTY`    — property access after dot (non-call), CONFIG keys, SECURITY keys
//! - `TT_MACRO`       — DLM module names (DCompressor, DEncryptor, DAuditor)
//! - `TT_DECORATOR`   — DLM subtype names AND @DATA table/group-array path segments
//!
//! ## Pre-scan approach for ALL dot patterns
//! `build_position_sets()` scans the raw token stream in two passes BEFORE encoding:
//!
//!   Pass 1 — @DATA table/group-array paths:
//!     Pattern: Identifier (Symbol('.') Identifier)* (Symbol(':') | DoubleColon)
//!     → table_path_start and table_path_segment positions → TT_DECORATOR
//!
//!   Pass 2 — all dot-access patterns (all sections):
//!     For each Symbol('.') at index i:
//!       receiver = tokens[i-1], member = tokens[i+1]
//!       receiver is static object name  → static_receiver / static_method / static_property
//!       receiver is enum name           → enum_type / enum_field
//!       receiver is ')' or ']'          → instance_method / instance_property (chained)
//!       otherwise                       → instance_method / instance_property
//!     member is_call_site? → method position. Otherwise → property position.
//!
//! `ClassifierState` retains only truly stateful things:
//!   @ENUMS body tracking, @QUICKFUNCS declaration tracking, @IMPORTS alias tracking,
//!   @DLM subtype dot tracking, and a direct-call lookahead flag for identifiers
//!   not covered by the position sets.

use std::collections::{HashMap, HashSet};
use std::panic;

use tower_lsp::lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult};
use dixscript::Compiler::AST::DataType;
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

const STATIC_OBJECT_NAMES: &[&str] = &[
    "Math", "DateTime", "Array", "Random", "Guid", "IpAddress", "Enum", "Dix",
];

// ─────────────────────────────────────────────────────────────────────────────
// Position sets — all built from the raw token stream before encoding
// ─────────────────────────────────────────────────────────────────────────────

/// All identifier positions determined by pre-scanning the token stream.
/// Keys are (1-based line, 1-based column) matching Token.line / Token.column.
#[derive(Default)]
struct PositionSets {
    // @DATA table/group-array paths
    table_path_start:   HashSet<(usize, usize)>,  // 'player'       in  player.config:
    table_path_segment: HashSet<(usize, usize)>,  // 'config'       in  player.config:

    // Enum access
    enum_type:          HashSet<(usize, usize)>,  // 'AIType'       in  AIType.AGGRESSIVE
    enum_field:         HashSet<(usize, usize)>,  // 'AGGRESSIVE'   in  AIType.AGGRESSIVE

    // Static-object access
    static_receiver:    HashSet<(usize, usize)>,  // 'Math'         in  Math.sqrt(5)
    static_method:      HashSet<(usize, usize)>,  // 'sqrt'         in  Math.sqrt(5)
    static_property:    HashSet<(usize, usize)>,  // non-call static member access

    // Instance access (via dot on non-static, non-enum receiver)
    instance_method:    HashSet<(usize, usize)>,  // 'toUpper'      in  str.toUpper()
    instance_property:  HashSet<(usize, usize)>,  // 'name'         in  obj.name
}

fn build_position_sets(tokens: &[Token], enum_names: &HashSet<String>) -> PositionSets {
    let mut ps = PositionSets::default();
    let n = tokens.len();

    // ── Pass 1: @DATA table/group-array paths ─────────────────────────────────
    // Pattern: Identifier (Symbol('.') Identifier)* (Symbol(':') | DoubleColon)
    let mut i = 0;
    while i < n {
        if tokens[i].section != SectionId::Data {
            i += 1;
            continue;
        }
        if !matches!(tokens[i].token_type, TokenType::Identifier(_)) {
            i += 1;
            continue;
        }

        let start = (tokens[i].line, tokens[i].column);
        let mut j = i + 1;
        let mut segments: Vec<(usize, usize)> = Vec::new();
        let mut expect_ident = false;
        let mut is_path = false;

        while j < n && (j - i) < 24 {
            match (&tokens[j].token_type, expect_ident) {
                (TokenType::Symbol('.'), false) => {
                    expect_ident = true;
                    j += 1;
                }
                (TokenType::Identifier(_), true) => {
                    segments.push((tokens[j].line, tokens[j].column));
                    expect_ident = false;
                    j += 1;
                }
                (TokenType::Symbol(':'), false) | (TokenType::DoubleColon, false) => {
                    is_path = true;
                    break;
                }
                _ => break,
            }
        }

        if is_path {
            ps.table_path_start.insert(start);
            for seg in segments {
                ps.table_path_segment.insert(seg);
            }
        }

        i += 1;
    }

    // ── Pass 2: dot-access patterns (all sections) ────────────────────────────
    // For each Symbol('.') at index i:
    //   receiver = tokens[i-1]  (what the dot is applied to)
    //   member   = tokens[i+1]  (what is being accessed)
    let mut i = 1;
    while i + 1 < n {
        if !matches!(tokens[i].token_type, TokenType::Symbol('.')) {
            i += 1;
            continue;
        }

        // Member must be an Identifier
        let member = match &tokens[i + 1].token_type {
            TokenType::Identifier(_) => &tokens[i + 1],
            _ => { i += 1; continue; }
        };
        let member_pos = (member.line, member.column);

        // Skip members already classified as table path segments
        if ps.table_path_segment.contains(&member_pos) {
            i += 1;
            continue;
        }

        // Is the member accessed as a call (followed by '(')?
        let is_call = is_followed_by_paren(tokens, i + 2);

        // Classify by receiver token
        match &tokens[i - 1].token_type {
            TokenType::Identifier(recv_name) => {
                let recv_pos = (tokens[i - 1].line, tokens[i - 1].column);

                // Skip if receiver is part of a table path — those dots are path separators
                if ps.table_path_start.contains(&recv_pos)
                    || ps.table_path_segment.contains(&recv_pos)
                {
                    i += 1;
                    continue;
                }

                if enum_names.contains(recv_name.as_str()) {
                    // Enum access: AIType.AGGRESSIVE
                    ps.enum_type.insert(recv_pos);
                    ps.enum_field.insert(member_pos);
                } else if STATIC_OBJECT_NAMES.contains(&recv_name.as_str()) {
                    // Static access: Math.sqrt(5), DateTime.now()
                    ps.static_receiver.insert(recv_pos);
                    if is_call {
                        ps.static_method.insert(member_pos);
                    } else {
                        ps.static_property.insert(member_pos);
                    }
                } else {
                    // Instance access: arr.push(x), obj.name
                    if is_call {
                        ps.instance_method.insert(member_pos);
                    } else {
                        ps.instance_property.insert(member_pos);
                    }
                }
            }

            // Chained: result of a previous call or index expression
            // e.g. getPlayer().name  or  items[0].toUpper()
            TokenType::Symbol(')') | TokenType::Symbol(']') => {
                if is_call {
                    ps.instance_method.insert(member_pos);
                } else {
                    ps.instance_property.insert(member_pos);
                }
            }

            _ => {}
        }

        i += 1;
    }

    ps
}

// ─────────────────────────────────────────────────────────────────────────────
// Stateful classifier — only truly stateful things remain here
// ─────────────────────────────────────────────────────────────────────────────

struct ClassifierState<'a> {
    // @ENUMS body tracking
    in_enum_body:      bool,
    enum_brace_depth:  i32,
    seen_enum_name:    bool,

    // @QUICKFUNCS declaration tracking
    next_is_func_name: bool,
    in_param_list:     bool,
    param_paren_depth: i32,

    // @IMPORTS alias tracking
    next_is_alias: bool,

    // @DLM subtype dot tracking
    dlm_dot_seen: bool,

    // Direct call lookahead (for identifiers not resolved via position sets)
    is_call_site: bool,

    // Pre-scanned position sets and function name registry
    positions:  &'a PositionSets,
    func_names: &'a HashSet<String>,
}

impl<'a> ClassifierState<'a> {
    fn new(positions: &'a PositionSets, func_names: &'a HashSet<String>) -> Self {
        ClassifierState {
            in_enum_body:      false,
            enum_brace_depth:  0,
            seen_enum_name:    false,
            next_is_func_name: false,
            in_param_list:     false,
            param_paren_depth: 0,
            next_is_alias:     false,
            dlm_dot_seen:      false,
            is_call_site:      false,
            positions,
            func_names,
        }
    }

    fn advance(&mut self, token: &Token, tokens: &[Token], index: usize) {
        // Per-token reset
        self.is_call_site = false;

        // Direct call-site detection: identifier followed by '('
        if let TokenType::Identifier(name) = &token.token_type {
            self.is_call_site = self.func_names.contains(name.as_str())
                || is_followed_by_paren(tokens, index + 1);
        }

        // Structural state transitions
        match &token.token_type {

            // @ENUMS brace tracking
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

            // QuickFunc declaration (~)
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

            // Import alias
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

            // DLM subtype dot tracking
            TokenType::SectionDLM => {
                self.dlm_dot_seen = false;
            }
            TokenType::Symbol('.') if token.section == SectionId::Dlm => {
                self.dlm_dot_seen = true;
            }
            TokenType::Symbol(',') if token.section == SectionId::Dlm => {
                self.dlm_dot_seen = false;
            }

            _ => {}
        }
    }

    /// Classify an Identifier token.
    ///
    /// Priority order:
    ///   0. DLM section (absolute override — module names and subtype names)
    ///   1. Pre-scanned position sets (covers all dot patterns + table paths)
    ///   2. Control-flow keyword in @QUICKFUNCS (followed by ':')
    ///   3. Section-specific stateful fallback
    fn classify_identifier(&mut self, token: &Token, tokens: &[Token], index: usize) -> (u32, u32) {
        let pos = (token.line, token.column);

        // ── 0. DLM section ────────────────────────────────────────────────────
        if token.section == SectionId::Dlm {
            let result = if self.dlm_dot_seen {
                (TT_DECORATOR, 0)           // subtype: gzip, aes256, …
            } else {
                (TT_MACRO, MOD_DECLARATION) // module:  DCompressor, DEncryptor, …
            };
            self.dlm_dot_seen = false;
            return result;
        }

        // ── 1. Pre-scanned position sets ──────────────────────────────────────

        // @DATA table paths
        if self.positions.table_path_start.contains(&pos) {
            return (TT_DECORATOR, 0);
        }
        if self.positions.table_path_segment.contains(&pos) {
            return (TT_DECORATOR, 0);
        }

        // Enum access
        if self.positions.enum_type.contains(&pos) {
            return (TT_TYPE, 0);
        }
        if self.positions.enum_field.contains(&pos) {
            return (TT_ENUM_MEMBER, 0);
        }

        // Static access
        if self.positions.static_receiver.contains(&pos) {
            return (TT_NAMESPACE, 0);
        }
        if self.positions.static_method.contains(&pos) {
            return (TT_FUNCTION, MOD_STATIC);
        }
        if self.positions.static_property.contains(&pos) {
            return (TT_PROPERTY, MOD_STATIC);
        }

        // Instance access
        if self.positions.instance_method.contains(&pos) {
            return (TT_METHOD, 0);
        }
        if self.positions.instance_property.contains(&pos) {
            return (TT_PROPERTY, 0);
        }

        // ── 2. Control-flow keyword in @QUICKFUNCS ────────────────────────────
        if token.section == SectionId::QuickFuncs {
            let next_is_colon = tokens.get(index + 1)
                .map(|t| matches!(t.token_type,
                    TokenType::Symbol(':')))
                .unwrap_or(false);
            if next_is_colon {
                return (TT_KEYWORD, 0);
            }
        }

        // ── 3. Section-specific stateful fallback ─────────────────────────────
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
                } else if self.is_call_site {
                    (TT_FUNCTION, 0)
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

            SectionId::Data => {
                if self.is_call_site {
                    (TT_FUNCTION, 0)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            SectionId::Security => (TT_PROPERTY, 0),

            _ => {
                if self.is_call_site {
                    (TT_FUNCTION, 0)
                } else {
                    (TT_VARIABLE, 0)
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Call-site lookahead
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Entry points
// ─────────────────────────────────────────────────────────────────────────────

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

    let positions = build_position_sets(&doc.tokens, &enum_names);
    let data      = encode_tokens(doc, &func_names, &positions);

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Encoder
// ─────────────────────────────────────────────────────────────────────────────

fn encode_tokens(
    doc:        &Document,
    func_names: &HashSet<String>,
    positions:  &PositionSets,
) -> Vec<SemanticToken> {
    let mut data: Vec<SemanticToken> = Vec::with_capacity(doc.tokens.len());
    let mut prev_line: u32 = 0;
    let mut prev_col:  u32 = 0;
    let mut state = ClassifierState::new(positions, func_names);

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

        push_raw(&mut data, &mut prev_line, &mut prev_col,
                 line, col, length, token_type, modifiers);
    }

    data
}

// ─────────────────────────────────────────────────────────────────────────────
// Interpolated string emitter
// ─────────────────────────────────────────────────────────────────────────────

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
        push_raw(data, prev_line, prev_col,
            base_line, base_col, (content.len() + 3) as u32, TT_STRING, 0);
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
                    push_raw(data, prev_line, prev_col,
                        base_line, base_col + seg_start, seg_len, TT_STRING, 0);
                }
                push_raw(data, prev_line, prev_col,
                    base_line, base_col + char_offset, 1, TT_OPERATOR, 0);
                in_brace     = true;
                brace_start  = char_offset + 1;
                char_offset += 1;
            }
            '}' if in_brace => {
                let expr_len = char_offset - brace_start;
                if expr_len > 0 {
                    push_raw(data, prev_line, prev_col,
                        base_line, base_col + brace_start, expr_len, TT_VARIABLE, 0);
                }
                push_raw(data, prev_line, prev_col,
                    base_line, base_col + char_offset, 1, TT_OPERATOR, 0);
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
            push_raw(data, prev_line, prev_col,
                base_line, base_col + seg_start, seg_len, TT_STRING, 0);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Raw token emitter
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Per-token classification dispatch
// ─────────────────────────────────────────────────────────────────────────────

fn classify(
    token:  &Token,
    state:  &mut ClassifierState<'_>,
    tokens: &[Token],
    index:  usize,
) -> Option<(u32, u32)> {
    match &token.token_type {
        // Section keywords
        TokenType::SectionConfig
        | TokenType::SectionImports
        | TokenType::SectionDLM
        | TokenType::SectionEnums
        | TokenType::SectionQuickFuncs
        | TokenType::SectionData
        | TokenType::SectionSecurity     => Some((TT_KEYWORD, MOD_READONLY)),

        // Language keywords
        TokenType::Keyword(_)            => Some((TT_KEYWORD, 0)),
        TokenType::Bool(_)               => Some((TT_KEYWORD, MOD_READONLY)),

        // String literals
        TokenType::String(_)
        | TokenType::StringSingle(_)     => Some((TT_STRING, 0)),
        TokenType::InterpolatedString(_) => Some((TT_STRING, 0)),

        // Temporal values
        TokenType::Date(_)
        | TokenType::Timestamp(_)        => Some((TT_EVENT, 0)),

        // Numeric literals
        TokenType::Integer(_)
        | TokenType::Long(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_) => Some((TT_NUMBER, 0)),
        TokenType::HexColor(_)             => Some((TT_NUMBER, MOD_READONLY)),

        // Operators
        TokenType::ArithmeticOp(_)
        | TokenType::ArithmeticAssignOp(_)
        | TokenType::ComparisonOp(_)
        | TokenType::LogicalOp(_)
        | TokenType::BitwiseOp(_)  => Some((TT_OPERATOR, 0)),

        TokenType::Arrow
        | TokenType::SwitchCase
        | TokenType::DoubleColon   => Some((TT_OPERATOR, 0)),

        TokenType::Symbol('~')           => Some((TT_OPERATOR, 0)),

        // Comments
        TokenType::Comment(_)            => Some((TT_COMMENT, 0)),


        // Prefixed constructors
        TokenType::RegexConstructor(_)   => Some((TT_REGEXP, 0)),
        TokenType::BlobConstructor(_)
        | TokenType::TupleConstructor(_) => Some((TT_KEYWORD, 0)),


        // Plain identifiers: position-set lookup then stateful fallback
        TokenType::Identifier(_) => Some(state.classify_identifier(token, tokens, index)),

        // Structural / ignored
        TokenType::Symbol(_)
        | TokenType::EndOfFile
        | TokenType::Error(_)          => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Token source-text length
// ─────────────────────────────────────────────────────────────────────────────

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
        TokenType::Bool(b)                => if *b { 4 } else { 5 },
        TokenType::BlobConstructor(_)     =>  2,
        TokenType::RegexConstructor(_)    =>  2,
        TokenType::TupleConstructor(_)    =>  2,
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
        }
