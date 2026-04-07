
use crate::Compiler::Core::Tokenizer::Token;

/// Extension trait for extracting source lines from token streams
pub trait SourceLineExtensions {
    /// Get the source line for a given error token
    fn get_source_line(&self, error_token: &Token, context_size: usize) -> String;
}

impl SourceLineExtensions for Vec<Token> {
    fn get_source_line(&self, error_token: &Token, _context_size: usize) -> String {
        // Get all tokens on the same line as the error token
        let line_tokens: Vec<&Token> = self
            .iter()
            .filter(|t| t.line == error_token.line)
            .collect();

        if line_tokens.is_empty() {
            return String::new();
        }

        // Build the source line by assembling tokens
        let mut source_line = String::new();
        let mut current_column = 0;

        for token in line_tokens {
            // Add spaces to reach the token's column position
            while current_column < token.column {
                source_line.push(' ');
                current_column += 1;
            }

            // Add the token's value
            let token_value = token.get_token_value();
            source_line.push_str(&token_value);
            current_column += token_value.len();
        }

        source_line
    }
}

impl SourceLineExtensions for [Token] {
    fn get_source_line(&self, error_token: &Token, _context_size: usize) -> String {
        // Get all tokens on the same line as the error token
        let line_tokens: Vec<&Token> = self
            .iter()
            .filter(|t| t.line == error_token.line)
            .collect();

        if line_tokens.is_empty() {
            return String::new();
        }

        // Build the source line by assembling tokens
        let mut source_line = String::new();
        let mut current_column = 0;

        for token in line_tokens {
            // Add spaces to reach the token's column position
            while current_column < token.column {
                source_line.push(' ');
                current_column += 1;
            }

            // Add the token's value
            let token_value = token.get_token_value();
            source_line.push_str(&token_value);
            current_column += token_value.len();
        }

        source_line
    }
}

/// Standalone helper function for getting source line from token list
pub fn get_source_line_from_tokens(
    tokens: &[Token],
    error_token: &Token,
    _context_size: usize,
) -> String {
    tokens.get_source_line(error_token, _context_size)
}