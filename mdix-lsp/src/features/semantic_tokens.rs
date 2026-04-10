// mdix-lsp/src/features/semantic_tokens.rs

//! Semantic token provider.
//!
//! Uses a single stateful pass through the token stream to assign accurate
//! token types based on section context and surrounding token context:
//!
//!   @ENUMS  — enum type names → TYPE, field names → ENUM_MEMBER
//!   @QUICKFUNCS — name after ~ → FUNCTION, params → PARAMETER, body idents → VARIABLE
//!   @DATA   — property names → PROPERTY, values → appropriate type
//!   @IMPORTS — alias names → NAMESPACE
//!   general operators, strings, numbers, keywords → their respective types

use tower_lsp::lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;

// Token type indices — must match capabilities::TOKEN_TYPES order exactly.
const TT_KEYWORD:     u32 = 0;
const TT_STRING:      u32 = 1;
const TT_NUMBER:      u32 = 2;
const TT_OPERATOR:    u32 = 3;
const TT_VARIABLE:    u32 = 4;
const TT_FUNCTION:    u32 = 5;
const TT_TYPE:        u32 = 6;
const TT_ENUM_MEMBER: u32 = 7;
const TT_COMMENT:     u32 = 8;
const TT_NAMESPACE:   u32 = 9;
const TT_PROPERTY:    u32 = 10;
const TT_PARAMETER:   u32 = 11;

// Token modifier bitmasks — must match capabilities::TOKEN_MODIFIERS order.
const MOD_DECLARATION: u32 = 1 << 0;
const MOD_READONLY:    u32 = 1 << 1;

pub fn provide(doc: Option<&Document>) -> Option<SemanticTokensResult> {
    let doc  = doc?;
    let data = encode_tokens(&doc.tokens);
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Stateful classifier context
// ─────────────────────────────────────────────────────────────────────────────

/// Tracks just enough state to classify identifiers correctly as we walk the
/// token stream linearly.  Kept small — one pass, O(n).
#[derive(Default)]
struct ClassifierState {
    // @ENUMS state
    enum_brace_depth: i32,       // >0 means we're inside enum { … }

    // @QUICKFUNCS state
    next_ident_is_func_name: bool, // true immediately after we see FunctionPrefix (~)
    in_param_list:           bool, // true between ( and ) of a function signature
    func_paren_depth:        i32,  // paren depth for the function argument list

    // @IMPORTS state
    next_ident_is_alias:     bool, // alias comes first in "Alias from …"
}

impl ClassifierState {
    fn update(&mut self, token: &Token) {
        match &token.token_type {
            // ── @ENUMS brace tracking ─────────────────────────────────────────
            TokenType::Symbol('{') if token.section == SectionId::Enums => {
                self.enum_brace_depth += 1;
            }
            TokenType::Symbol('}') if token.section == SectionId::Enums => {
                self.enum_brace_depth = (self.enum_brace_depth - 1).max(0);
            }

            // ── @QUICKFUNCS function prefix ──────────────────────────────────
            TokenType::FunctionPrefix => {
                self.next_ident_is_func_name = true;
                self.in_param_list           = false;
            }
            // After the function name, the next `(` opens the param list.
            TokenType::Symbol('(') if token.section == SectionId::QuickFuncs => {
                if !self.next_ident_is_func_name {
                    // Could be a nested call inside the body; track depth.
                    if self.in_param_list {
                        self.func_paren_depth += 1;
                    } else {
                        self.in_param_list    = true;
                        self.func_paren_depth = 1;
                    }
                }
            }
            TokenType::Symbol(')') if token.section == SectionId::QuickFuncs => {
                if self.in_param_list {
                    self.func_paren_depth -= 1;
                    if self.func_paren_depth <= 0 {
                        self.in_param_list    = false;
                        self.func_paren_depth = 0;
                    }
                }
            }

            // ── @IMPORTS alias tracking ──────────────────────────────────────
            TokenType::SectionImports => {
                self.next_ident_is_alias = true;
            }
            TokenType::Keyword(k) if *k == "from" || *k == "from_cloud" => {
                // After the alias and the `from` keyword the next token is
                // the path string, not an alias.
                self.next_ident_is_alias = false;
            }

            _ => {}
        }
    }

    /// Classify an `Identifier` token using current state.
    fn classify_identifier(&mut self, token: &Token) -> (u32, u32) {
        match token.section {
            // ── @ENUMS ────────────────────────────────────────────────────────
            SectionId::Enums => {
                if self.enum_brace_depth > 0 {
                    (TT_ENUM_MEMBER, MOD_DECLARATION)
                } else {
                    (TT_TYPE, MOD_DECLARATION)
                }
            }

            // ── @QUICKFUNCS ───────────────────────────────────────────────────
            SectionId::QuickFuncs => {
                if self.next_ident_is_func_name {
                    self.next_ident_is_func_name = false;
                    // The `(` that follows will open the param list.
                    (TT_FUNCTION, MOD_DECLARATION)
                } else if self.in_param_list && self.func_paren_depth <= 1 {
                    (TT_PARAMETER, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            // ── @IMPORTS ─────────────────────────────────────────────────────
            SectionId::Imports => {
                if self.next_ident_is_alias {
                    self.next_ident_is_alias = false;
                    (TT_NAMESPACE, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            // ── @DATA ─────────────────────────────────────────────────────────
            SectionId::Data => {
                // Property names and table-path segments appear before `=`, `:`, `::`.
                // Everything else is a value reference.
                (TT_PROPERTY, 0)
            }

            // ── Everything else ───────────────────────────────────────────────
            _ => (TT_VARIABLE, 0),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Encoder
// ─────────────────────────────────────────────────────────────────────────────

fn encode_tokens(tokens: &[Token]) -> Vec<SemanticToken> {
    let mut data: Vec<SemanticToken> = Vec::with_capacity(tokens.len());
    let mut prev_line: u32 = 0;
    let mut prev_col:  u32 = 0;
    let mut state = ClassifierState::default();

    for token in tokens {
        // Always update state (even for tokens we'll skip for highlighting).
        state.update(token);

        let (token_type, modifiers) = match classify(token, &mut state) {
            Some(t) => t,
            None    => continue,
        };

        // Convert 1-based source position to 0-based LSP deltas.
        let line = token.line.saturating_sub(1) as u32;
        let col  = token.column.saturating_sub(1) as u32;

        let delta_line  = line - prev_line;
        let delta_start = if delta_line == 0 { col.saturating_sub(prev_col) } else { col };

        let length = token_length(token) as u32;
        if length == 0 {
            continue;
        }

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

/// Returns `(token_type_index, modifier_bitmask)` or `None` to skip the token.
/// `state` is passed mutably so `classify_identifier` can consume
/// `next_ident_is_func_name` etc.  State MUTATION from `ClassifierState::update`
/// is done in the outer loop before this call, so by the time we're here the
/// state already reflects the current token.
fn classify(token: &Token, state: &mut ClassifierState) -> Option<(u32, u32)> {
    match &token.token_type {
        // ── Section keywords ──────────────────────────────────────────────────
        TokenType::SectionConfig
        | TokenType::SectionImports
        | TokenType::SectionDLM
        | TokenType::SectionEnums
        | TokenType::SectionQuickFuncs
        | TokenType::SectionData
        | TokenType::SectionSecurity => Some((TT_KEYWORD, MOD_READONLY)),

        // ── Language keywords ─────────────────────────────────────────────────
        TokenType::Keyword(_) => Some((TT_KEYWORD, 0)),

        // ── Boolean / null literals ───────────────────────────────────────────
        TokenType::Bool(_) => Some((TT_KEYWORD, 0)),

        // ── Type annotations ──────────────────────────────────────────────────
        TokenType::DataType(_) => Some((TT_TYPE, 0)),

        // ── Strings ───────────────────────────────────────────────────────────
        TokenType::String(_)
        | TokenType::StringSingle(_)
        | TokenType::InterpolatedString(_) => Some((TT_STRING, 0)),

        // ── Dates and timestamps (string-like) ────────────────────────────────
        TokenType::Date(_) | TokenType::Timestamp(_) => Some((TT_STRING, 0)),

        // ── Numbers ───────────────────────────────────────────────────────────
        TokenType::Integer(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_)
        | TokenType::HexLiteral(_) => Some((TT_NUMBER, 0)),

        // ── Hex colours (number-like, rendered as colour swatches separately) ─
        TokenType::HexColor(_) => Some((TT_NUMBER, MOD_READONLY)),

        // ── Operators and punctuation ─────────────────────────────────────────
        TokenType::ArithmeticOp(_)
        | TokenType::ArithmeticAssignOp(_)
        | TokenType::ComparisonOp(_)
        | TokenType::LogicalOp(_)
        | TokenType::BitwiseOp(_)
        | TokenType::MultiCharSymbol(_)
        | TokenType::Arrow
        | TokenType::SwitchCase
        | TokenType::DoubleColon
        | TokenType::FunctionPrefix
        | TokenType::ControlFlowColon => Some((TT_OPERATOR, 0)),

        // ── Comments ──────────────────────────────────────────────────────────
        TokenType::Comment(_) => Some((TT_COMMENT, 0)),

        // ── Enum access: EnumName.VALUE ───────────────────────────────────────
        TokenType::EnumAccess { .. } => Some((TT_ENUM_MEMBER, MOD_READONLY)),

        // ── Table paths: user.profile.settings ───────────────────────────────
        TokenType::TablePath(_) => Some((TT_PROPERTY, 0)),

        // ── Static method calls: Math.sqrt ────────────────────────────────────
        TokenType::StaticFunction { .. } => Some((TT_FUNCTION, 0)),

        // ── Dix built-in functions ────────────────────────────────────────────
        TokenType::DixFunction(_) => Some((TT_FUNCTION, 0)),

        // ── Special constructors ──────────────────────────────────────────────
        TokenType::BlobConstructor(_)
        | TokenType::RegexConstructor(_)
        | TokenType::TupleConstructor(_)
        | TokenType::PrefixedConstructor { .. } => Some((TT_KEYWORD, 0)),

        // ── Context-aware identifier classification ───────────────────────────
        TokenType::Identifier(_) => Some(state.classify_identifier(token)),

        // ── Skip: raw symbols, EOF, errors ────────────────────────────────────
        TokenType::Symbol(_)
        | TokenType::EndOfFile
        | TokenType::Error(_)
        | TokenType::ParseContext(_)
        | TokenType::ScopeDeclaration(_)
        | TokenType::ObjectAccess(_)
        | TokenType::BuiltinMethod(_)
        | TokenType::ConfigAccess(_) => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Source length of a token
// ─────────────────────────────────────────────────────────────────────────────

fn token_length(token: &Token) -> usize {
    match &token.token_type {
        TokenType::String(s)             => s.len() + 2,   // include quotes
        TokenType::StringSingle(s)       => s.len() + 2,
        TokenType::InterpolatedString(s) => s.len() + 3,   // $"..."
        TokenType::HexColor(h)           => h.len() + 1,   // include #
        TokenType::Comment(c)            => c.len() + 2,   // // + content
        TokenType::SectionConfig         => 7,    // @CONFIG
        TokenType::SectionImports        => 8,    // @IMPORTS
        TokenType::SectionDLM            => 4,    // @DLM
        TokenType::SectionEnums          => 6,    // @ENUMS
        TokenType::SectionQuickFuncs     => 11,   // @QUICKFUNCS
        TokenType::SectionData           => 5,    // @DATA
        TokenType::SectionSecurity       => 9,    // @SECURITY
        TokenType::DoubleColon           => 2,    // ::
        TokenType::Arrow                 => 2,    // =>
        TokenType::SwitchCase            => 2,    // ->
        TokenType::FunctionPrefix        => 1,    // ~
        TokenType::Bool(b)               => if *b { 4 } else { 5 }, // true / false
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}