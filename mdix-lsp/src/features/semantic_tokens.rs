//! Semantic token provider.
//!
//! Maps DixScript TokenType variants to LSP SemanticTokenType indices
//! (defined in capabilities.rs) so editors can apply accurate syntax
//! highlighting beyond what a TextMate grammar can express.
//!
//! Encoding follows the LSP spec: each token is represented as 5 u32 values —
//! [deltaLine, deltaStartChar, length, tokenType, tokenModifiers].

use tower_lsp::lsp_types::{SemanticTokens, SemanticTokensResult};
use dixscript::Compiler::Core::Tokenizer::{Token, TokenType};
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use crate::document::Document;

// Token type indices — must match the order in capabilities::TOKEN_TYPES.
const TT_KEYWORD:    u32 = 0;
const TT_STRING:     u32 = 1;
const TT_NUMBER:     u32 = 2;
const TT_OPERATOR:   u32 = 3;
const TT_VARIABLE:   u32 = 4;
const TT_FUNCTION:   u32 = 5;
const TT_TYPE:       u32 = 6;
const TT_ENUM_MEMBER:u32 = 7;
const TT_COMMENT:    u32 = 8;
const TT_NAMESPACE:  u32 = 9;
const TT_PROPERTY:   u32 = 10;
const TT_PARAMETER:  u32 = 11;

// Token modifier bitmasks — must match capabilities::TOKEN_MODIFIERS.
const MOD_DECLARATION: u32 = 1 << 0;
const MOD_READONLY:    u32 = 1 << 1;

pub fn provide(doc: Option<&Document>) -> Option<SemanticTokensResult> {
    let doc = doc?;
    let data = encode_tokens(&doc.tokens);
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

fn encode_tokens(tokens: &[Token]) -> Vec<u32> {
    let mut data = Vec::with_capacity(tokens.len() * 5);
    let mut prev_line: u32 = 0;
    let mut prev_col:  u32 = 0;

    for token in tokens {
        let (token_type, modifiers) = match classify(token) {
            Some(t) => t,
            None    => continue,
        };

        // Convert 1-based token position to 0-based LSP deltas.
        let line = token.line.saturating_sub(1) as u32;
        let col  = token.column.saturating_sub(1) as u32;

        let delta_line = line - prev_line;
        let delta_col  = if delta_line == 0 { col - prev_col } else { col };

        let length = token_length(token) as u32;
        if length == 0 {
            continue;
        }

        data.push(delta_line);
        data.push(delta_col);
        data.push(length);
        data.push(token_type);
        data.push(modifiers);

        prev_line = line;
        prev_col  = col;
    }

    data
}

/// Returns `(token_type_index, modifier_bitmask)` for a token,
/// or `None` to skip the token entirely (e.g. synthetic/EOF tokens).
fn classify(token: &Token) -> Option<(u32, u32)> {
    match &token.token_type {
        // Section keywords
        TokenType::SectionConfig
        | TokenType::SectionImports
        | TokenType::SectionDLM
        | TokenType::SectionEnums
        | TokenType::SectionQuickFuncs
        | TokenType::SectionData
        | TokenType::SectionSecurity => Some((TT_KEYWORD, MOD_READONLY)),

        // Language keywords
        TokenType::Keyword(_) => Some((TT_KEYWORD, 0)),

        // Type annotations  <int>, <string>, …
        TokenType::DataType(_) => Some((TT_TYPE, 0)),

        // String literals (all variants)
        TokenType::String(_)
        | TokenType::StringSingle(_)
        | TokenType::InterpolatedString(_) => Some((TT_STRING, 0)),

        // Numeric literals
        TokenType::Integer(_)
        | TokenType::Float(_)
        | TokenType::Double(_)
        | TokenType::ScientificNotation(_)
        | TokenType::HexLiteral(_) => Some((TT_NUMBER, 0)),

        // Boolean / null — treat as keywords
        TokenType::Bool(_) => Some((TT_KEYWORD, 0)),

        // Operators
        TokenType::ArithmeticOp(_)
        | TokenType::ArithmeticAssignOp(_)
        | TokenType::ComparisonOp(_)
        | TokenType::LogicalOp(_)
        | TokenType::BitwiseOp(_)
        | TokenType::Arrow
        | TokenType::SwitchCase
        | TokenType::DoubleColon => Some((TT_OPERATOR, 0)),

        // Function prefix ~ and function declarations
        TokenType::FunctionPrefix => Some((TT_OPERATOR, 0)),

        // QuickFunc names — identifiers in QUICKFUNCS section
        TokenType::Identifier(name) if token.section == SectionId::QuickFuncs => {
            Some((TT_FUNCTION, MOD_DECLARATION))
        }

        // Enum access: EnumName.VALUE
        TokenType::EnumAccess { .. } => Some((TT_ENUM_MEMBER, MOD_READONLY)),

        // Imported namespace aliases
        TokenType::ConfigAccess(_) => Some((TT_NAMESPACE, MOD_READONLY)),

        // Table paths  (user.profile.settings)
        TokenType::TablePath(_) => Some((TT_PROPERTY, 0)),

        // Static function calls (Math.sqrt, DateTime.now, …)
        TokenType::StaticFunction { .. } => Some((TT_FUNCTION, 0)),

        // Dix built-in functions
        TokenType::DixFunction(_) => Some((TT_FUNCTION, 0)),

        // General identifiers in DATA section = variables
        TokenType::Identifier(_) if token.section == SectionId::Data => {
            Some((TT_VARIABLE, 0))
        }

        // Identifiers anywhere else
        TokenType::Identifier(_) => Some((TT_VARIABLE, 0)),

        // Special constructors
        TokenType::BlobConstructor(_)
        | TokenType::RegexConstructor(_)
        | TokenType::TupleConstructor(_)
        | TokenType::PrefixedConstructor { .. } => Some((TT_KEYWORD, 0)),

        // Dates and timestamps — highlight as strings
        TokenType::Date(_) | TokenType::Timestamp(_) => Some((TT_STRING, 0)),

        // Hex colors — highlight as numbers
        TokenType::HexColor(_) => Some((TT_NUMBER, 0)),

        // Comments
        TokenType::Comment(_) => Some((TT_COMMENT, 0)),

        // Skip: symbols, EOF, errors, parse context
        TokenType::Symbol(_)
        | TokenType::EndOfFile
        | TokenType::Error(_)
        | TokenType::ParseContext(_) => None,

        // Skip: scope declarations, object/array access (already covered
        // by higher-level tokens in practice)
        TokenType::ScopeDeclaration(_)
        | TokenType::ObjectAccess(_)
        | TokenType::BuiltinMethod(_)
        | TokenType::ControlFlowColon => None,

        TokenType::MultiCharSymbol(_) => Some((TT_OPERATOR, 0)),
    }
}

/// Approximates the source length of a token from its value.
/// Used for the length field in the encoded output.
fn token_length(token: &Token) -> usize {
    match &token.token_type {
        TokenType::String(s)             => s.len() + 2,  // include quotes
        TokenType::StringSingle(s)       => s.len() + 2,
        TokenType::InterpolatedString(s) => s.len() + 3,  // $"..."
        TokenType::HexColor(h)           => h.len() + 1,  // include #
        TokenType::Comment(c)            => c.len() + 2,  // include //
        TokenType::SectionConfig         => 7,   // @CONFIG
        TokenType::SectionImports        => 8,   // @IMPORTS
        TokenType::SectionDLM            => 4,   // @DLM
        TokenType::SectionEnums          => 6,   // @ENUMS
        TokenType::SectionQuickFuncs     => 11,  // @QUICKFUNCS
        TokenType::SectionData           => 5,   // @DATA
        TokenType::SectionSecurity       => 9,   // @SECURITY
        TokenType::DoubleColon           => 2,
        TokenType::Arrow                 => 2,   // =>
        TokenType::SwitchCase            => 2,   // ->
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}
