//! Tokenizer - Lexical analysis

pub mod lexer;
pub mod token;

/// Compile-time character classification tables.
/// Shared by the lexer's hot paths for branch-free byte classification.
pub mod char_tables;

/// Platform-specific whitespace-end finder used by `skip_whitespace`.
/// Kept `pub(crate)` — callers outside the tokenizer always go through
/// `Tokenizer::tokenize`, never through the platform internals directly.
pub(crate) mod platform;

pub use lexer::{
    Tokenizer,
    TokenizationResult,
    TokenizationMetadata,
    PrefixedConstructorInfo,
    StaticCallInfo,
};
pub use token::{Token, TokenType, TokenExtensions};
