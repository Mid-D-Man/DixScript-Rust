// mdix-lsp/src/features/semantic_tokens.rs

//! Semantic token classifier.
//!
//! One linear pass through the token stream assigns a (type_index, modifier_bitmask)
//! to every token.  The indices map to TOKEN_TYPES in capabilities.rs; the names
//! there are what the editor theme uses to pick a color.
//!
//! ## Full token → color mapping (Ferrous dark as reference)
//!
//! TT_KEYWORD     (0)  #569CD6  blue   — @sections, return/if/let/const
//! TT_STRING      (1)  #CE9178  orange — "strings", 'strings', $"interpolated"
//! TT_NUMBER      (2)  #B5CEA8  green  — 42, 3.14f, 0xDEAD
//! TT_OPERATOR    (3)  #D4D4D4  white  — =, ->, ::, ~, +, -, ==
//! TT_VARIABLE    (4)  #9CDCFE  l-blue — plain identifiers, DATA property names
//! TT_FUNCTION    (5)  #DCDCAA  yellow — QuickFunc call/decl
//! TT_TYPE        (6)  #4EC9B0  teal   — <int>, <string>, <enum>, …
//! TT_ENUM_MEMBER (7)  #4FC1FF  l-blue — enum fields + enum access (PASSIVE, AIType.BOSS)
//! TT_COMMENT     (8)  #6A9955  green  — // line comments
//! TT_NAMESPACE   (9)  #9CDCFE  l-blue — import alias names
//! TT_PROPERTY   (10)  #C586C0  purple — table.path segments, group arrays::
//! TT_PARAMETER  (11)  #9CDCFE  l-blue — QuickFunc parameters
//! TT_MACRO      (12)  #C586C0  purple — DCompressor, DEncryptor, DAuditor
//! TT_DECORATOR  (13)  #CE9178  orange — .gzip, .aes256, .chacha20 subtypes
//! TT_STRUCT     (14)  #4EC9B0  teal   — enum TYPE name (Environment, Tier, …)
//! TT_REGEXP     (15)  #D16969  red    — r:() constructor token
//! TT_EVENT      (16)  #B5CEA8  green  — date (2025-01-15), timestamp (…Z)

use tower_lsp::lsp_types::{SemanticToken, SemanticTokens, SemanticTokensResult};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;

// Import constants from capabilities — single source of truth for indices.
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

/// Tracks just enough context to classify identifiers without a second pass.
/// One instance per `encode_tokens` call — never shared across calls.
#[derive(Default)]
struct ClassifierState {
    // @ENUMS tracking
    in_enum_body:     bool,  // true inside { … } of an enum declaration
    enum_brace_depth: i32,
    seen_enum_name:   bool,  // first ident after @ENUMS( is the type name

    // @QUICKFUNCS tracking
    next_is_func_name: bool, // true immediately after FunctionPrefix (~)
    in_param_list:     bool, // true between ( and ) of function signature
    param_paren_depth: i32,

    // @IMPORTS tracking
    next_is_alias:     bool, // first ident in each import line is the alias

    // @DLM tracking — next identifier after a section open is a module name
    next_is_dlm_module:   bool,
    next_is_dlm_subtype:  bool, // true after a DLM module name (before the .subtype)
}

impl ClassifierState {
    /// Update internal state BEFORE classifying `token`.
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
                    self.seen_enum_name = false; // ready for next enum decl
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
                    // Opening paren of param list — the func name was just consumed
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
            // 'from' / 'from_cloud' / 'verify' reset alias expectation
            TokenType::Keyword(kw)
                if matches!(kw.as_str(), "from" | "from_cloud" | "verify") =>
            {
                self.next_is_alias = false;
            }
            // A string ends the import line — next non-string, non-keyword
            // ident starts a new alias
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
            // After a DLM module name the next meaningful token should be
            // the `.subtype` (a MultiCharSymbol or ObjectAccess token) or a comma.
            // We handle it in classify() where ObjectAccess lands.

            _ => {}
        }
    }

    /// Classify an `Identifier` given the current state.
    /// Returns (type_index, modifier_bitmask).
    fn classify_identifier(&mut self, token: &Token) -> (u32, u32) {
        match token.section {

            // ── @ENUMS ────────────────────────────────────────────────────────
            SectionId::Enums => {
                if self.in_enum_body {
                    // Fields inside the { … } block
                    (TT_ENUM_MEMBER, MOD_DECLARATION)
                } else if !self.seen_enum_name {
                    // The enum type name itself (Environment, Tier, …)
                    self.seen_enum_name = true;
                    (TT_STRUCT, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            // ── @QUICKFUNCS ───────────────────────────────────────────────────
            SectionId::QuickFuncs => {
                if self.next_is_func_name {
                    self.next_is_func_name = false;
                    (TT_FUNCTION, MOD_DECLARATION)
                } else if self.in_param_list && self.param_paren_depth <= 1 {
                    (TT_PARAMETER, MOD_DECLARATION)
                } else {
                    // Body identifiers: could be a function call or a variable
                    (TT_VARIABLE, 0)
                }
            }

            // ── @IMPORTS ─────────────────────────────────────────────────────
            SectionId::Imports => {
                if self.next_is_alias {
                    self.next_is_alias = false;
                    (TT_NAMESPACE, MOD_DECLARATION)
                } else {
                    (TT_VARIABLE, 0)
                }
            }

            // ── @DLM ──────────────────────────────────────────────────────────
            SectionId::Dlm => {
                if self.next_is_dlm_module {
                    self.next_is_dlm_module  = false;
                    self.next_is_dlm_subtype = true;
                    (TT_MACRO, MOD_DECLARATION)
                } else {
                    (TT_MACRO, 0)
                }
            }

            // ── @DATA ─────────────────────────────────────────────────────────
            SectionId::Data => {
                // In DATA everything before = / : / :: is a property name.
                // After the operator it's a value reference.
                // The parser has already split these into distinct token types
                // (Identifier vs call site), so here we just use property color.
                (TT_VARIABLE, 0)
            }

            // ── @CONFIG, @SECURITY, root ───────────────────────────────────────
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
        // Advance state BEFORE classifying so the state reflects the
        // context established by the current token.
        state.advance(token);

        let (token_type, modifiers) = match classify(token, &mut state) {
            Some(t) => t,
            None    => continue,
        };

        // DixScript positions are 1-based; LSP deltas are 0-based.
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

/// Map a single token to (type_index, modifier_bitmask).
/// Returns `None` to skip the token (no highlight).
fn classify(token: &Token, state: &mut ClassifierState) -> Option<(u32, u32)> {
    match &token.token_type {

        // ── Section keywords (@CONFIG, @DATA, …) ─────────────────────────────
        // MOD_READONLY signals "this is a keyword, not a declaration site"
        TokenType::SectionConfig
        | TokenType::SectionImports
        | TokenType::SectionDLM
        | TokenType::SectionEnums
        | TokenType::SectionQuickFuncs
        | TokenType::SectionData
        | TokenType::SectionSecurity     => Some((TT_KEYWORD, MOD_READONLY)),

        // ── Language keywords (return, if:, let, const, …) ───────────────────
        TokenType::Keyword(_)            => Some((TT_KEYWORD, 0)),

        // ── Boolean / null literals ───────────────────────────────────────────
        // true / false / null — in Ferrous these get #569CD6 (same as bool in VS Dark+)
        TokenType::Bool(_)               => Some((TT_KEYWORD, MOD_READONLY)),

        // ── Type annotations <int>, <string>, … ──────────────────────────────
        TokenType::DataType(_)           => Some((TT_TYPE, 0)),

        // ── String literals ───────────────────────────────────────────────────
        TokenType::String(_)
        | TokenType::StringSingle(_)
        | TokenType::InterpolatedString(_) => Some((TT_STRING, 0)),

        // ── Date / timestamp literals — dedicated slot (TT_EVENT) ────────────
        // This gives them a distinct color (#B5CEA8 green in Ferrous) rather
        // than sharing the string orange.
        TokenType::Date(_)
        | TokenType::Timestamp(_)        => Some((TT_EVENT, 0)),

        // ── Numeric literals ──────────────────────────────────────────────────
        TokenType::Integer(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_) => Some((TT_NUMBER, 0)),

        // ── Hex integer literals (0xDEAD) — number color ─────────────────────
        TokenType::HexLiteral(_)         => Some((TT_NUMBER, 0)),

        // ── Hex color literals (#FF5733) — number slot + readonly modifier ────
        // The color-swatch provider handles the inline swatch separately.
        // MOD_READONLY lets themes distinguish #color from a plain number.
        TokenType::HexColor(_)           => Some((TT_NUMBER, MOD_READONLY)),

        // ── Operators ─────────────────────────────────────────────────────────
        TokenType::ArithmeticOp(_)
        | TokenType::ArithmeticAssignOp(_)
        | TokenType::ComparisonOp(_)
        | TokenType::LogicalOp(_)
        | TokenType::BitwiseOp(_)        => Some((TT_OPERATOR, 0)),

        // ── Multi-char operators (::, ->, =>) ────────────────────────────────
        TokenType::MultiCharSymbol(_)
        | TokenType::Arrow
        | TokenType::SwitchCase
        | TokenType::DoubleColon
        | TokenType::ControlFlowColon   => Some((TT_OPERATOR, 0)),

        // ── Function prefix (~) ───────────────────────────────────────────────
        // The ~ is an operator; the identifier that follows it gets
        // TT_FUNCTION|MOD_DECLARATION from classify_identifier.
        TokenType::FunctionPrefix        => Some((TT_OPERATOR, 0)),

        // ── Comments ──────────────────────────────────────────────────────────
        TokenType::Comment(_)            => Some((TT_COMMENT, 0)),

        // ── Enum access (AIType.BOSS, Environment.PROD) ───────────────────────
        // The whole "EnumName.FIELD" token gets the enum member color.
        // TT_ENUM_MEMBER without MOD_DECLARATION = access site (not declaration).
        TokenType::EnumAccess { .. }     => Some((TT_ENUM_MEMBER, 0)),

        // ── Table paths (server.primary, user.profile) ────────────────────────
        TokenType::TablePath(_)          => Some((TT_PROPERTY, 0)),

        // ── Static method calls (Math.sqrt, DateTime.now) ─────────────────────
        // These are function calls — yellow in Ferrous.
        TokenType::StaticFunction { .. } => Some((TT_FUNCTION, 0)),

        // ── Built-in Dix functions ────────────────────────────────────────────
        TokenType::DixFunction(_)        => Some((TT_FUNCTION, 0)),

        // ── Regex constructor r:() ─────────────────────────────────────────────
        // Gets its own red slot so it's visually distinct from blob/tuple.
        TokenType::RegexConstructor(_)   => Some((TT_REGEXP, 0)),

        // ── Blob / Tuple constructors ─────────────────────────────────────────
        // b:() and t:() — keyword color (blue), same as other built-in literals.
        TokenType::BlobConstructor(_)
        | TokenType::TupleConstructor(_)
        | TokenType::PrefixedConstructor { .. } => Some((TT_KEYWORD, 0)),

        // ── ObjectAccess / DLM subtype (.gzip, .aes256) ──────────────────────
        // ObjectAccess tokens appear as ".subtype" in @DLM — decorator color.
        // In @DATA they're table path continuations — property color.
        TokenType::ObjectAccess(_) => {
            if token.section == SectionId::Dlm {
                Some((TT_DECORATOR, 0))
            } else {
                Some((TT_PROPERTY, 0))
            }
        }

        // ── Context-aware identifier classification ───────────────────────────
        TokenType::Identifier(_)         => Some(state.classify_identifier(token)),

        // ── Scope declarations (=> global, => server.config) ──────────────────
        // These are like type annotations for QuickFunc scope.
        TokenType::ScopeDeclaration(_)   => Some((TT_TYPE, 0)),

        // ── Config access tokens ──────────────────────────────────────────────
        TokenType::ConfigAccess(_)       => Some((TT_PROPERTY, 0)),

        // ── Built-in method tokens ────────────────────────────────────────────
        TokenType::BuiltinMethod(_)      => Some((TT_FUNCTION, 0)),

        // ── ParseContext — internal parser marker, skip ───────────────────────
        TokenType::ParseContext(_)       => None,

        // ── Raw symbols ( ) { } [ ] , — skip ─────────────────────────────────
        // Most editors already give brackets their own color from bracket-pair
        // coloring; emitting them as operators would fight that.
        TokenType::Symbol(_)             => None,

        // ── End of file ───────────────────────────────────────────────────────
        TokenType::EndOfFile             => None,

        // ── Lexer error tokens — skip, already reported as diagnostics ────────
        TokenType::Error(_)              => None,
    }
}

// ── Source-text length of a token ─────────────────────────────────────────────

/// How many source characters this token occupies.
/// Used for the `length` field of SemanticToken — must be exact.
fn token_length(token: &Token) -> usize {
    match &token.token_type {
        // Quoted strings: add 2 for the surrounding quote chars
        TokenType::String(s)              => s.len() + 2,
        TokenType::StringSingle(s)        => s.len() + 2,
        // Interpolated: $"..." = 3 extra chars
        TokenType::InterpolatedString(s)  => s.len() + 3,
        // HexColor includes the leading #
        TokenType::HexColor(h)            => h.len() + 1,
        // Comments: // + content (no newline)
        TokenType::Comment(c)             => c.len() + 2,
        // Section keywords — fixed widths
        TokenType::SectionConfig          => 7,   // @CONFIG
        TokenType::SectionImports         => 8,   // @IMPORTS
        TokenType::SectionDLM             => 4,   // @DLM
        TokenType::SectionEnums           => 6,   // @ENUMS
        TokenType::SectionQuickFuncs      => 11,  // @QUICKFUNCS
        TokenType::SectionData            => 5,   // @DATA
        TokenType::SectionSecurity        => 9,   // @SECURITY
        // Multi-char operators
        TokenType::DoubleColon            => 2,   // ::
        TokenType::Arrow                  => 2,   // ->  or =>
        TokenType::SwitchCase             => 2,   // ->
        TokenType::ControlFlowColon       => 2,   // if: elif: etc
        TokenType::FunctionPrefix         => 1,   // ~
        // Boolean literals
        TokenType::Bool(b)                => if *b { 4 } else { 5 },
        // Everything else: use the token's own value length,
        // defaulting to 1 so we never emit a zero-length token.
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}
