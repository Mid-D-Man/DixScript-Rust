//! CommentFilter — reusable utility for stripping comment tokens from a
//! token stream.
//!
//! Extracted from `GeneralParser` so it can be used in other contexts
//! (formatters, linters, IDE language-server pipelines, etc.) without
//! spinning up a full parser.

use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::ErrorManager::Helpers::ParseException;

/// Strips comment tokens from a token stream and guarantees an EOF sentinel.
pub struct CommentFilter;

impl CommentFilter {
    /// Remove all `Comment(_)` tokens from `tokens` and ensure the stream ends
    /// with an `EndOfFile` token.
    ///
    /// Consumes the input `Vec<Token>` and returns a new, comment-free `Vec`.
    /// This is an O(n) in-place retain pass — no intermediate allocation.
    pub fn filter(mut tokens: Vec<Token>) -> Result<Vec<Token>, ParseException> {
        tokens.retain(|t| !matches!(t.token_type, TokenType::Comment(_)));

        // Guarantee an EOF sentinel is present.
        if tokens.is_empty()
            || !matches!(
                tokens.last().unwrap().token_type,
                TokenType::EndOfFile
            )
        {
            let line   = tokens.last().map(|t| t.line).unwrap_or(1);
            let column = tokens.last().map(|t| t.column + 1).unwrap_or(1);
            tokens.push(Token::eof(line, column));
        }

        Ok(tokens)
    }

    /// Returns `true` if `token` is any kind of comment token.
    #[inline]
    pub fn is_comment(token: &Token) -> bool {
        matches!(token.token_type, TokenType::Comment(_))
    }
}
