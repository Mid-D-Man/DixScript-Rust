// mdix-lsp/src/features/semantic_tokens.rs

//! Semantic token classifier.
//!
//! One linear pass through the token stream assigns (type_index, modifier_bitmask)
//! to every token.  Indices map to TOKEN_TYPES in capabilities.rs.
//!
//! ## Full token → color mapping (Ferrous dark reference)
//!
//! TT_KEYWORD     (0)  #569CD6  blue   — @sections, control-flow, literals
//! TT_STRING      (1)  #CE9178  orange — "strings", 'strings', $"interpolated"
//! TT_NUMBER      (2)  #B5CEA8  green  — 42, 3.14f, 0xDEAD
//! TT_OPERATOR    (3)  #D4D4D4  white  — =, ->, ::, ~, +, -, ==
//! TT_VARIABLE    (4)  #9CDCFE  l-blue — plain identifiers, DATA property names
//! TT_FUNCTION    (5)  #DCDCAA  yellow — QuickFunc call/decl, static calls
//! TT_TYPE        (6)  #4EC9B0  teal   — <int>, <string>, <enum> …
//! TT_ENUM_MEMBER (7)  #4FC1FF  l-blue — enum fields + enum access
//! TT_COMMENT     (8)  #6A9955  green  — // line comments
//! TT_NAMESPACE   (9)  #9CDCFE  l-blue — import alias names
//! TT_PROPERTY   (10)  #C586C0  purple — table.path segments, group arrays::
//! TT_PARAMETER  (11)  #9CDCFE  l-blue — QuickFunc parameters
//! TT_MACRO      (12)  #C586C0  purple — DCompressor, DEncryptor, DAuditor
//! TT_DECORATOR  (13)  #CE9178  orange — .gzip, .aes256, .chacha20 subtypes
//! TT_STRUCT     (14)  #4EC9B0  teal   — enum TYPE names in @ENUMS declaration
//! TT_REGEXP     (15)  #D16969  red    — r:() regex constructor token
//! TT_EVENT      (16)  #B5CEA8  green  — date / timestamp literals

use tower_lsp::lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;

use crate::capabilities::{
    TT_KEYWORD, TT_STRING, TT_NUMBER, TT_OPERATOR, TT_VARIABLE,
    TT_FUNCTION, TT_TYPE, TT_ENUM_MEMBER, TT_COMMENT, TT_NAMESPACE,
    TT_PROPERTY, TT_PARAMETER, TT_MACRO, TT_DECORATOR, TT_STRUCT,
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
    // @ENUMS tracking
    in_enum_body:      bool,
    enum_brace_depth:  i32,
    seen_enum_name:    bool,

    // @QUICKFUNCS tracking
    next_is_func_name: bool,
    in_param_list:     bool,
    param_paren_depth: i32,

    // @IMPORTS tracking
    next_is_alias:     bool,

    // @DLM tracking
    next_is_dlm_module:  bool,
    next_is_dlm_subtype: bool,
}

impl ClassifierState {
    fn advance(&mut self, token: &Token) {
        match &token.token_type {
            // ── @ENUMS ────────────────────────────────────────────────────────
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

            // ── @QUICKFUNCS ───────────────────────────────────────────────────
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

            // ── @IMPORTS ──────────────────────────────────────────────────────
            TokenType::SectionImports => {
                self.next_is_alias = true;
            }
            // FIX: use direct equality, not .as_str() (unstable on &'static str)
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

            // ── @DLM ──────────────────────────────────────────────────────────
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

            SectionId::Enums => {
                if self.in_enum_body {
                    (TT_ENUM_MEMBER, MOD_DECLARATION)
                } else if !self.seen_enum_name {
                    self.seen_enum_name = true;
                    (TT_STRUCT, MOD_DECLARATION)
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

            SectionId::Data => (TT_VARIABLE, 0),

            _ => (TT_VARIABLE, 0),
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

// ── Per-token classification ──────────────────────────────────────────────────

fn classify(token: &Token, state: &mut ClassifierState) -> Option<(u32, u32)> {
    match &token.token_type {

        // ── Section keywords (@CONFIG, @DATA, …) ─────────────────────────────
        TokenType::SectionConfig
        | TokenType::SectionImports
        | TokenType::SectionDLM
        | TokenType::SectionEnums
        | TokenType::SectionQuickFuncs
        | TokenType::SectionData
        | TokenType::SectionSecurity     => Some((TT_KEYWORD, MOD_READONLY)),

        // ── Language keywords (return, if, let, const, …) ────────────────────
        TokenType::Keyword(_)            => Some((TT_KEYWORD, 0)),

        // ── Boolean / null literals ───────────────────────────────────────────
        TokenType::Bool(_)               => Some((TT_KEYWORD, MOD_READONLY)),

        // ── Type annotations ─────────────────────────────────────────────────
        TokenType::DataType(_)           => Some((TT_TYPE, 0)),

        // ── String literals ───────────────────────────────────────────────────
        TokenType::String(_)
        | TokenType::StringSingle(_)
        | TokenType::InterpolatedString(_) => Some((TT_STRING, 0)),

        // ── Date / timestamp ─────────────────────────────────────────────────
        TokenType::Date(_)
        | TokenType::Timestamp(_)        => Some((TT_EVENT, 0)),

        // ── Numeric literals ─────────────────────────────────────────────────
        TokenType::Integer(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_) => Some((TT_NUMBER, 0)),

        // ── Hex integer literals (0xDEAD) ─────────────────────────────────────
        TokenType::HexLiteral(_)         => Some((TT_NUMBER, 0)),

        // ── Hex color literals (#FF5733) ──────────────────────────────────────
        TokenType::HexColor(_)           => Some((TT_NUMBER, MOD_READONLY)),

        // ── Operators ─────────────────────────────────────────────────────────
        TokenType::ArithmeticOp(_)
        | TokenType::ArithmeticAssignOp(_)
        | TokenType::ComparisonOp(_)
        | TokenType::LogicalOp(_)
        | TokenType::BitwiseOp(_)
        | TokenType::MultiCharSymbol(_)  => Some((TT_OPERATOR, 0)),

        // ── Multi-char structural operators ───────────────────────────────────
        TokenType::Arrow
        | TokenType::SwitchCase
        | TokenType::DoubleColon
        | TokenType::ControlFlowColon   => Some((TT_OPERATOR, 0)),

        // ── Function prefix (~) ───────────────────────────────────────────────
        TokenType::FunctionPrefix        => Some((TT_OPERATOR, 0)),

        // ── Comments ──────────────────────────────────────────────────────────
        TokenType::Comment(_)            => Some((TT_COMMENT, 0)),

        // ── Enum access (AIType.BOSS, Environment.PROD) ───────────────────────
        TokenType::EnumAccess { .. }     => Some((TT_ENUM_MEMBER, 0)),

        // ── Table paths (server.primary, user.profile) ────────────────────────
        // NEW: TablePath was previously unhandled
        TokenType::TablePath(_)          => Some((TT_PROPERTY, 0)),

        // ── Static method calls (Math.sqrt, DateTime.now) ─────────────────────
        TokenType::StaticFunction { .. } => Some((TT_FUNCTION, 0)),

        // ── Built-in Dix functions ────────────────────────────────────────────
        TokenType::DixFunction(_)        => Some((TT_FUNCTION, 0)),

        // ── Regex constructor r:() ─────────────────────────────────────────────
        TokenType::RegexConstructor(_)   => Some((TT_REGEXP, 0)),

        // ── Blob / Tuple constructors ─────────────────────────────────────────
        TokenType::BlobConstructor(_)
        | TokenType::TupleConstructor(_)
        | TokenType::PrefixedConstructor { .. } => Some((TT_KEYWORD, 0)),

        // ── ObjectAccess / DLM subtype ────────────────────────────────────────
        TokenType::ObjectAccess(_) => {
            if token.section == SectionId::Dlm {
                Some((TT_DECORATOR, 0))
            } else {
                Some((TT_PROPERTY, 0))
            }
        }

        // ── Context-aware identifier classification ───────────────────────────
        TokenType::Identifier(_)         => Some(state.classify_identifier(token)),

        // ── Scope declarations ────────────────────────────────────────────────
        TokenType::ScopeDeclaration(_)   => Some((TT_TYPE, 0)),

        // ── Config access tokens ──────────────────────────────────────────────
        TokenType::ConfigAccess(_)       => Some((TT_PROPERTY, 0)),

        // ── Built-in method tokens ────────────────────────────────────────────
        TokenType::BuiltinMethod(_)      => Some((TT_FUNCTION, 0)),

        // ── ParseContext — internal parser marker ─────────────────────────────
        TokenType::ParseContext(_)       => None,

        // ── Raw symbols — skip (bracket-pair coloring handles these) ──────────
        TokenType::Symbol(_)             => None,

        // ── End of file ───────────────────────────────────────────────────────
        TokenType::EndOfFile             => None,

        // ── Lexer error tokens — already reported as diagnostics ──────────────
        TokenType::Error(_)              => None,
    }
}

// ── Source-text length of a token ─────────────────────────────────────────────

fn token_length(token: &Token) -> usize {
    match &token.token_type {
        TokenType::String(s)              => s.len() + 2,   // "..."
        TokenType::StringSingle(s)        => s.len() + 2,   // '...'
        TokenType::InterpolatedString(s)  => s.len() + 3,   // $"..."
        TokenType::HexColor(h)            => h.len() + 1,   // #RRGGBB
        TokenType::Comment(c)             => c.len() + 2,   // // ...

        // Section keywords — exact lengths
        TokenType::SectionConfig          =>  7, // @CONFIG
        TokenType::SectionImports         =>  8, // @IMPORTS
        TokenType::SectionDLM             =>  4, // @DLM
        TokenType::SectionEnums           =>  6, // @ENUMS
        TokenType::SectionQuickFuncs      => 11, // @QUICKFUNCS
        TokenType::SectionData            =>  5, // @DATA
        TokenType::SectionSecurity        =>  9, // @SECURITY

        // Multi-char operators
        TokenType::DoubleColon            => 2, // ::
        TokenType::Arrow                  => 2, // -> or =>
        TokenType::SwitchCase             => 2, // ->
        TokenType::ControlFlowColon       => 1, // :  (just the colon suffix)
        TokenType::FunctionPrefix         => 1, // ~

        // Boolean literals
        TokenType::Bool(b)                => if *b { 4 } else { 5 },

        // Prefix constructors: b:, r:, t:
        TokenType::BlobConstructor(_)     => 2,
        TokenType::RegexConstructor(_)    => 2,
        TokenType::TupleConstructor(_)    => 2,

        // EnumAccess: "EnumName.FIELD"
        TokenType::EnumAccess { enum_name, value } => enum_name.len() + 1 + value.len(),

        // TablePath: "a.b.c"
        TokenType::TablePath(s) => s.len(),

        // ObjectAccess: ".subtype" segments
        TokenType::ObjectAccess(parts) => parts.join(".").len(),

        // Everything else: use the token value length, min 1
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}