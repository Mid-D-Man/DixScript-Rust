// mdix-lsp/src/features/semantic_tokens.rs

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

// ── Public entry point ────────────────────────────────────────────────────────

pub fn provide(doc: Option<&Document>) -> Option<SemanticTokensResult> {
    let doc  = doc?;
    let data = encode_tokens(&doc.tokens);
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

// ── Stateful classifier ───────────────────────────────────────────────────────

#[derive(Default)]
struct ClassifierState {
    in_enum_body:        bool,
    enum_brace_depth:    i32,
    seen_enum_name:      bool,
    next_is_func_name:   bool,
    in_param_list:       bool,
    param_paren_depth:   i32,
    next_is_alias:       bool,
    next_is_dlm_module:  bool,
    next_is_dlm_subtype: bool,
}

impl ClassifierState {
    fn advance(&mut self, token: &Token) {
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

fn encode_tokens(tokens: &[Token]) -> Vec<SemanticToken> {
    let mut data: Vec<SemanticToken> = Vec::with_capacity(tokens.len());
    let mut prev_line: u32 = 0;
    let mut prev_col:  u32 = 0;
    let mut state = ClassifierState::default();

    for token in tokens {
        state.advance(token);

        // Interpolated strings get special splitting so variables inside {}
        // receive TT_VARIABLE colouring instead of TT_STRING.
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

        let delta_line  = line - prev_line;
        let delta_start = if delta_line == 0 { col.saturating_sub(prev_col) } else { col };

        let length = token_length(token) as u32;
        if length == 0 { continue; }

        data.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: modifiers,
        });

        prev_line = line;
        prev_col  = col;
    }

    data
}

// ── Interpolated string splitter ──────────────────────────────────────────────
//
// For  $"Hello {name}!"  the token starts at `$`.
// Content (stored in the token) = `Hello {name}!`  (no $" prefix, no closing ")
// Offsets from base_col:  0=$  1="  2=H  3=e … 8={  9=n … 12=e  13=}  14=!  15="
//
// We emit:
//   [0 , 8)  TT_STRING   →  $"Hello
//   [8 , 9)  TT_OPERATOR →  {
//   [9 , 13) TT_VARIABLE →  name
//   [13, 14) TT_OPERATOR →  }
//   [14, 16) TT_STRING   →  !"   (closing quote included)

fn emit_interpolated_tokens(
    token: &Token,
    content: &str,
    prev_line: &mut u32,
    prev_col: &mut u32,
    data: &mut Vec<SemanticToken>,
) {
    let base_line = token.line.saturating_sub(1) as u32;
    let base_col  = token.column.saturating_sub(1) as u32;

    // Multiline interpolated strings are uncommon and complex to split correctly;
    // fall back to a single TT_STRING span for them.
    if content.contains('\n') {
        push_raw(data, prev_line, prev_col,
                 base_line, base_col,
                 (content.len() + 3) as u32,   // $" + content + "
                 TT_STRING, 0);
        return;
    }

    // seg_start  = offset from base_col where the current string segment begins
    // char_offset = offset from base_col of the character currently being examined
    // Content chars start at offset 2 (after $")
    let mut seg_start:   u32 = 0; // begins at `$`
    let mut char_offset: u32 = 2; // first content char
    let mut in_brace          = false;
    let mut brace_start: u32  = 0;

    for ch in content.chars() {
        match ch {
            '{' if !in_brace => {
                // Emit string segment up to (exclusive) the opening brace.
                let seg_len = char_offset - seg_start;
                push_raw(data, prev_line, prev_col,
                         base_line, base_col + seg_start, seg_len, TT_STRING, 0);
                // Emit `{` as an operator.
                push_raw(data, prev_line, prev_col,
                         base_line, base_col + char_offset, 1, TT_OPERATOR, 0);
                in_brace     = true;
                brace_start  = char_offset + 1;
                char_offset += 1;
            }
            '}' if in_brace => {
                // Emit expression content.
                let expr_len = char_offset - brace_start;
                push_raw(data, prev_line, prev_col,
                         base_line, base_col + brace_start, expr_len, TT_VARIABLE, 0);
                // Emit `}` as an operator.
                push_raw(data, prev_line, prev_col,
                         base_line, base_col + char_offset, 1, TT_OPERATOR, 0);
                in_brace     = false;
                seg_start    = char_offset + 1;
                char_offset += 1;
            }
            _ => { char_offset += 1; }
        }
    }

    // Emit the remaining string segment plus the closing `"`.
    if !in_brace {
        let end_offset = char_offset + 1; // +1 for the closing `"`
        let seg_len    = end_offset - seg_start;
        push_raw(data, prev_line, prev_col,
                 base_line, base_col + seg_start, seg_len, TT_STRING, 0);
    }
}

/// Push a single semantic token, computing the LSP delta from the running
/// previous-position state.
fn push_raw(
    data: &mut Vec<SemanticToken>,
    prev_line: &mut u32,
    prev_col:  &mut u32,
    line: u32, col: u32, len: u32, tt: u32, mods: u32,
) {
    if len == 0 { return; }
    let dl = line - *prev_line;
    let ds = if dl == 0 { col.saturating_sub(*prev_col) } else { col };
    data.push(SemanticToken {
        delta_line:              dl,
        delta_start:             ds,
        length:                  len,
        token_type:              tt,
        token_modifiers_bitset:  mods,
    });
    *prev_line = line;
    *prev_col  = col;
}

// ── Per-token classification ──────────────────────────────────────────────────

fn classify(token: &Token, state: &mut ClassifierState) -> Option<(u32, u32)> {
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
        // InterpolatedString is handled before classify() is reached in encode_tokens.
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

        TokenType::Identifier(_)         => Some(state.classify_identifier(token)),
        TokenType::ScopeDeclaration(_)   => Some((TT_TYPE, 0)),
        TokenType::ConfigAccess(_)       => Some((TT_PROPERTY, 0)),
        TokenType::BuiltinMethod(_)      => Some((TT_FUNCTION, 0)),
        TokenType::ParseContext(_)       => None,
        TokenType::Symbol(_)             => None,
        TokenType::EndOfFile             => None,
        TokenType::Error(_)              => None,
    }
}

// ── Source-text length of a token ─────────────────────────────────────────────

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
        TokenType::DoubleColon            => 2,
        TokenType::Arrow                  => 2,
        TokenType::SwitchCase             => 2,
        TokenType::ControlFlowColon       => 1,
        TokenType::FunctionPrefix         => 1,
        TokenType::Bool(b)                => if *b { 4 } else { 5 },
        TokenType::BlobConstructor(_)     => 2,
        TokenType::RegexConstructor(_)    => 2,
        TokenType::TupleConstructor(_)    => 2,
        TokenType::EnumAccess { enum_name, value } => enum_name.len() + 1 + value.len(),
        TokenType::TablePath(s)           => s.len(),
        TokenType::ObjectAccess(parts)    => parts.join(".").len(),
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}