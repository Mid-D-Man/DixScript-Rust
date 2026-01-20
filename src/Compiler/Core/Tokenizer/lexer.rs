//! DixScript Lexer v1.0.0
//!
//! High-performance manual tokenization with zero-allocation optimizations.
//! This is a HOT PATH - avoid cloning at all costs.

use std::collections::HashMap;
use crate::Utilities::{Token, TokenType};
use crate::ErrorManager::{ErrorManager, LexicalErrorType};
use crate::Compiler::VersionControl::VersionManager;

// Constants for optimization
const INITIAL_TOKEN_POOL_SIZE: usize = 256;
const MAX_RECOVERY_ATTEMPTS: usize = 10;

/// Position tracking state for tokenization
///
/// This struct is kept small and stack-allocated for performance.
/// All fields are Copy types to avoid any allocation overhead.
#[derive(Debug, Clone, Copy)]
struct TokenizerState {
    position: usize,
    line: usize,
    column: usize,
    input_length: usize,
}

impl TokenizerState {
    #[inline]
    fn new(input_length: usize) -> Self {
        TokenizerState {
            position: 0,
            line: 1,
            column: 1,
            input_length,
        }
    }

    #[inline]
    fn is_at_end(&self) -> bool {
        self.position >= self.input_length
    }

    #[inline]
    fn peek(&self, input: &str) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            input.chars().nth(self.position).unwrap_or('\0')
        }
    }

    #[inline]
    fn peek_next(&self, input: &str) -> char {
        if self.position + 1 >= self.input_length {
            '\0'
        } else {
            input.chars().nth(self.position + 1).unwrap_or('\0')
        }
    }

    #[inline]
    fn advance(&mut self, input: &str) -> char {
        if self.is_at_end() {
            return '\0';
        }

        let current = input.chars().nth(self.position).unwrap_or('\0');
        self.position += 1;

        if current == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }

        current
    }

    #[inline]
    fn slice<'a>(&self, input: &'a str, start: usize, length: usize) -> &'a str {
        let bytes = input.as_bytes();
        let end = (start + length).min(bytes.len());
        std::str::from_utf8(&bytes[start..end]).unwrap_or("")
    }
}

/// Main tokenizer for DixScript
///
/// Uses a pre-allocated token buffer and borrows input heavily to minimize allocations.
pub struct Tokenizer {
    input: String,
    version_manager: &'static VersionManager,
    error_manager: ErrorManager,
    current_section: Option<String>,
    prefixed_constructors_found: Vec<PrefixedConstructorInfo>,
    static_calls_found: Vec<StaticCallInfo>,
    identifier_cache: HashMap<String, String>,
    token_pool: Vec<Token>,
}

impl Tokenizer {
    /// Create a new tokenizer for the given input
    pub fn new(input: String) -> Self {
        let estimated_tokens = input.len() / 10;
        let initial_capacity = estimated_tokens.max(INITIAL_TOKEN_POOL_SIZE);

        Tokenizer {
            input,
            version_manager: VersionManager::instance(),
            error_manager: ErrorManager::get_shared_instance(),
            current_section: None,
            prefixed_constructors_found: Vec::new(),
            static_calls_found: Vec::new(),
            identifier_cache: HashMap::new(),
            token_pool: Vec::with_capacity(initial_capacity),
        }
    }

    /// Main tokenization entry point
    pub fn tokenize(mut self) -> TokenizationResult {
        let input_len = self.input.len();
        let mut state = TokenizerState::new(input_len);
        let mut recovery_attempts = 0;
        let mut last_error_position = usize::MAX;

        loop {
            // Skip whitespace
            self.skip_whitespace(&mut state);

            if state.is_at_end() {
                break;
            }

            // Attempt to scan next token
            match self.scan_token(&mut state) {
                Ok(Some(token)) => {
                    recovery_attempts = 0;

                    // Version check
                    if !self.is_token_supported(&token) {
                        self.handle_unsupported_token(&token, &state);

                        if self.should_terminate() {
                            break;
                        }
                        continue;
                    }

                    // Add token and update section context
                    self.update_section_context(&token);
                    self.token_pool.push(token);
                }
                Ok(None) => {
                    // No token, continue
                }
                Err(err_msg) => {
                    // Handle tokenization error
                    self.handle_tokenization_error(
                        &err_msg,
                        &mut state,
                        &mut recovery_attempts,
                        &mut last_error_position,
                    );

                    if self.should_terminate() {
                        break;
                    }

                    // Attempt recovery
                    if self.supports_recovery() {
                        if recovery_attempts >= MAX_RECOVERY_ATTEMPTS {
                            self.error_manager.add_lexical_error(
                                LexicalErrorType::InvalidCharacter,
                                "Maximum recovery attempts exceeded - aborting tokenization".to_string(),
                                state.line,
                                state.column,
                                None,
                                None,
                            );
                            break;
                        }

                        if !self.attempt_recovery(&mut state) {
                            break;
                        }
                        continue;
                    }

                    // Continue mode: add error token
                    if self.should_continue() {
                        let error_token = Token::new(
                            TokenType::Error(err_msg.clone()),
                            state.line,
                            state.column,
                            self.current_section.clone(),
                        );
                        self.token_pool.push(error_token);

                        if !self.skip_to_next_valid_token(&mut state) {
                            break;
                        }
                        continue;
                    }
                }
            }
        }

        // Add EOF token
        let eof_token = Token::eof(state.line, state.column);
        self.token_pool.push(eof_token);

        // Analyze token sequences
        self.analyze_token_sequences();

        // Create metadata BEFORE moving token_pool
        let metadata = self.create_metadata();

        // Build result
        TokenizationResult {
            tokens: self.token_pool,
            metadata,
            prefixed_constructors: self.prefixed_constructors_found,
            static_calls: self.static_calls_found,
        }
    }

    // ==================== ERROR HANDLING ====================

    #[inline]
    fn handle_tokenization_error(
        &self,
        err_msg: &str,
        state: &mut TokenizerState,
        recovery_attempts: &mut usize,
        last_error_position: &mut usize,
    ) {
        if state.position == *last_error_position {
            *recovery_attempts += 1;
        } else {
            *recovery_attempts = 1;
            *last_error_position = state.position;
        }

        self.error_manager.add_lexical_error(
            LexicalErrorType::InvalidCharacter,
            err_msg.to_string(),
            state.line,
            state.column,
            None,
            None,
        );
    }

    #[inline]
    fn handle_unsupported_token(&self, token: &Token, _state: &TokenizerState) {
        self.error_manager.add_lexical_error(
            LexicalErrorType::InvalidCharacter,
            format!(
                "Token type not supported in version {}",
                self.version_manager.get_current_version()
            ),
            token.line,
            token.column,
            None,
            None,
        );
    }

    fn attempt_recovery(&self, state: &mut TokenizerState) -> bool {
        let start_position = state.position;

        // Try to find a valid recovery point
        while !state.is_at_end() && state.position < start_position + 100 {
            let current = state.peek(&self.input);

            // Whitespace is a good recovery point
            if current.is_whitespace() {
                self.skip_whitespace(state);
                return true;
            }

            // Statement terminators
            if matches!(current, ';' | ',' | '}' | ')' | ']') {
                state.advance(&self.input);
                return true;
            }

            // Start of new token
            if current.is_alphanumeric() || matches!(current, '"' | '\'' | '@') {
                return true;
            }

            state.advance(&self.input);
        }

        !state.is_at_end()
    }

    fn skip_to_next_valid_token(&self, state: &mut TokenizerState) -> bool {
        while !state.is_at_end() {
            let current = state.peek(&self.input);

            if current.is_whitespace() {
                self.skip_whitespace(state);
                return true;
            }

            if current.is_alphanumeric() || matches!(current, '"' | '\'' | '@' | '{' | '[') {
                return true;
            }

            state.advance(&self.input);
        }

        false
    }

    // ==================== HELPER CHECKS ====================

    #[inline]
    fn should_terminate(&self) -> bool {
        self.error_manager.should_terminate_parsing()
    }

    #[inline]
    fn supports_recovery(&self) -> bool {
        // Check if strategy is Recover
        // If not terminating and has errors, we're in Continue or Recover mode
        !self.should_terminate() || self.should_continue()
    }

    #[inline]
    fn should_continue(&self) -> bool {
        // If not terminating and has errors, we're in Continue or Recover mode
        self.error_manager.has_errors() && !self.should_terminate()
    }

    #[inline]
    fn is_token_supported(&self, token: &Token) -> bool {
        self.version_manager.is_token_valid_for_version(&token.token_type)
    }

    // ==================== HELPER METHODS ====================

    #[inline]
    fn skip_whitespace(&self, state: &mut TokenizerState) {
        while !state.is_at_end() {
            let current = state.peek(&self.input);
            if current.is_whitespace() {
                state.advance(&self.input);
            } else {
                break;
            }
        }
    }

    #[inline]
    fn is_hex_digit(&self, c: char) -> bool {
        c.is_ascii_hexdigit()
    }

    fn intern_string(&mut self, s: &str) -> String {
        if let Some(interned) = self.identifier_cache.get(s) {
            interned.clone()
        } else {
            let owned = s.to_string();
            self.identifier_cache.insert(owned.clone(), owned.clone());
            owned
        }
    }

    #[inline]
    fn update_section_context(&mut self, token: &Token) {
        if let Some(section) = token.token_type.get_section_context() {
            self.current_section = Some(section.to_string());
        }
    }

    #[inline]
    fn is_advanced_section(&self) -> bool {
        if let Some(ref section) = self.current_section {
            matches!(
                section.to_uppercase().as_str(),
                "QUICKFUNCS" | "IMPORTS" | "DLM"
            )
        } else {
            false
        }
    }
}
// ==================== CORE SCANNING ====================

impl Tokenizer {
    fn scan_token(&mut self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        if state.is_at_end() {
            return Ok(None);
        }

        let current = state.peek(&self.input);

        // Comments
        if current == '/' {
            let next = state.peek_next(&self.input);
            if next == '/' {
                return Ok(Some(self.scan_single_line_comment(state)));
            }
            if next == '*' {
                return self.scan_multi_line_comment(state);
            }
        }

        // Section keywords
        if current == '@' {
            if let Some(section_token) = self.try_scan_section_keyword(state) {
                return Ok(Some(section_token));
            }
        }

        // Strings
        if current == '"' || current == '\'' {
            return self.scan_string_literal(state);
        }

        // Interpolated strings (advanced sections only)
        if current == '$' && self.is_advanced_section() {
            let next = state.peek_next(&self.input);
            if next == '"' || next == '\'' {
                return self.scan_interpolated_string(state);
            }
        }

        // Hex literals
        if current == '0' {
            let next = state.peek_next(&self.input);
            if next == 'x' || next == 'X' {
                return self.scan_hex_literal(state);
            }
        }

        // Numbers
        if current.is_ascii_digit() || (current == '-' && state.peek_next(&self.input).is_ascii_digit()) {
            return self.scan_numeric_literal(state);
        }

        // Hex colors
        if current == '#' {
            return Ok(Some(self.scan_hex_color(state)));
        }

        // Multi-char operators
        if let Some(multi_char_op) = self.try_scan_multi_char_operator(state) {
            return Ok(Some(multi_char_op));
        }

        // Prefixed constructors (b:, t:, r:)
        if current.is_ascii_alphabetic() && state.peek_next(&self.input) == ':' && self.is_valid_prefixed_constructor(state) {
            return Ok(Some(self.scan_prefixed_constructor(state)));
        }

        // Identifiers and keywords
        if current.is_ascii_alphabetic() || current == '_' {
            return Ok(Some(self.scan_identifier_or_keyword(state)));
        }

        // Single characters
        self.scan_single_character(state)
    }

    // ==================== COMMENT SCANNING ====================

    fn scan_single_line_comment(&mut self, state: &mut TokenizerState) -> Token {
        let start_column = state.column;
        let start_line = state.line;

        // Consume //
        state.advance(&self.input);
        state.advance(&self.input);

        let comment_start = state.position;

        // Read until newline
        while !state.is_at_end() && state.peek(&self.input) != '\n' {
            state.advance(&self.input);
        }

        let comment_text = self.intern_string(
            state.slice(&self.input, comment_start, state.position - comment_start)
        );

        // Consume newline if present
        if !state.is_at_end() && state.peek(&self.input) == '\n' {
            state.advance(&self.input);
        }

        Token::new(
            TokenType::Comment(comment_text),
            start_line,
            start_column,
            self.current_section.clone(),
        )
    }

    fn scan_multi_line_comment(&mut self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_column = state.column;
        let start_line = state.line;

        // Consume /*
        state.advance(&self.input);
        state.advance(&self.input);

        let comment_start = state.position;

        // Find closing */
        while !state.is_at_end() {
            if state.peek(&self.input) == '*' && state.peek_next(&self.input) == '/' {
                let comment_text = self.intern_string(
                    state.slice(&self.input, comment_start, state.position - comment_start)
                );

                // Consume */
                state.advance(&self.input);
                state.advance(&self.input);

                return Ok(Some(Token::new(
                    TokenType::Comment(comment_text),
                    start_line,
                    start_column,
                    self.current_section.clone(),
                )));
            }

            state.advance(&self.input);
        }

        // Unterminated comment
        self.error_manager.add_lexical_error(
            LexicalErrorType::UnterminatedString,
            "Unterminated multi-line comment".to_string(),
            start_line,
            start_column,
            None,
            None,
        );

        if self.should_terminate() {
            return Err(format!(
                "Unterminated multi-line comment at line {}, col {}",
                start_line, start_column
            ));
        }

        // Return partial comment
        let comment_text = self.intern_string(
            state.slice(&self.input, comment_start, state.position - comment_start)
        );

        Ok(Some(Token::new(
            TokenType::Comment(comment_text),
            start_line,
            start_column,
            self.current_section.clone(),
        )))
    }

    // ==================== SECTION KEYWORD SCANNING ====================

    fn try_scan_section_keyword(&mut self, state: &mut TokenizerState) -> Option<Token> {
        let start_pos = state.position;
        let start_line = state.line;
        let start_column = state.column;

        // Must start with @
        if state.peek(&self.input) != '@' {
            return None;
        }
        state.advance(&self.input);

        let section_start = state.position;

        // Read section name
        while !state.is_at_end() {
            let ch = state.peek(&self.input);
            if ch.is_alphanumeric() {
                state.advance(&self.input);
            } else {
                break;
            }
        }

        let section_len = state.position - section_start;
        if section_len == 0 {
            // Restore position
            state.position = start_pos;
            state.line = start_line;
            state.column = start_column;
            return None;
        }

        let section_name = state.slice(&self.input, section_start, section_len);

        // Match section keywords
        let token_type = match section_name.to_uppercase().as_str() {
            "CONFIG" => Some(TokenType::SectionConfig),
            "DLM" => Some(TokenType::SectionDLM),
            "ENUMS" => Some(TokenType::SectionEnums),
            "IMPORTS" => Some(TokenType::SectionImports),
            "QUICKFUNCS" => Some(TokenType::SectionQuickFuncs),
            "DATA" => Some(TokenType::SectionData),
            "SECURITY" => Some(TokenType::SectionSecurity),
            _ => None,
        };

        if let Some(tt) = token_type {
            Some(Token::new(tt, start_line, start_column, self.current_section.clone()))
        } else {
            // Restore position
            state.position = start_pos;
            state.line = start_line;
            state.column = start_column;
            None
        }
    }

    // ==================== STRING LITERAL SCANNING ====================

    fn scan_string_literal(&mut self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_line = state.line;
        let start_column = state.column;
        let quote = state.peek(&self.input);

        state.advance(&self.input); // Consume opening quote

        let content_start = state.position;
        let mut has_escapes = false;
        let mut scan_pos = state.position;

        // Fast scan for closing quote
        let bytes = self.input.as_bytes();
        while scan_pos < bytes.len() {
            if bytes[scan_pos] == quote as u8 {
                break;
            }
            if bytes[scan_pos] == b'\\' {
                has_escapes = true;
                scan_pos += 2; // Skip escape sequence
            } else {
                scan_pos += 1;
            }
        }

        // Check if we found the closing quote
        if scan_pos >= bytes.len() {
            self.error_manager.add_lexical_error(
                LexicalErrorType::UnterminatedString,
                "Unterminated string literal".to_string(),
                start_line,
                start_column,
                None,
                None,
            );

            if self.should_terminate() {
                return Err(format!(
                    "Unterminated string at line {}, col {}",
                    start_line, start_column
                ));
            }

            // Return partial string
            let partial = state.slice(&self.input, content_start, bytes.len() - content_start);
            state.position = bytes.len();

            let token_type = if quote == '\'' {
                TokenType::StringSingle(self.intern_string(partial))
            } else {
                TokenType::String(self.intern_string(partial))
            };

            return Ok(Some(Token::new(
                token_type,
                start_line,
                start_column,
                self.current_section.clone(),
            )));
        }

        // Process the string content
        let content = if has_escapes {
            let mut result = String::new();
            while state.position < scan_pos {
                let ch = state.peek(&self.input);
                if ch == '\\' {
                    state.position += 1;
                    if state.position < scan_pos {
                        let escaped = self.input.chars().nth(state.position).unwrap_or('\0');
                        result.push(self.process_escape_sequence(escaped));
                        state.position += 1;
                        state.column += 2;
                    }
                } else {
                    result.push(ch);
                    state.position += 1;
                    state.column += 1;
                }
            }
            self.intern_string(&result)
        } else {
            let slice = state.slice(&self.input, content_start, scan_pos - content_start);
            state.position = scan_pos;
            state.column += scan_pos - content_start;
            self.intern_string(slice)
        };

        // Consume closing quote
        state.advance(&self.input);

        let token_type = if quote == '\'' {
            TokenType::StringSingle(content)
        } else {
            TokenType::String(content)
        };

        Ok(Some(Token::new(
            token_type,
            start_line,
            start_column,
            self.current_section.clone(),
        )))
    }

    fn scan_interpolated_string(&mut self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_line = state.line;
        let start_column = state.column;

        // Consume $
        state.advance(&self.input);
        let quote = state.advance(&self.input);

        let mut content = String::new();
        let mut brace_depth = 0;

        while !state.is_at_end() {
            let current = state.peek(&self.input);

            if current == quote && brace_depth == 0 {
                state.advance(&self.input);
                break;
            } else if current == '{' {
                brace_depth += 1;
                content.push(state.advance(&self.input));
            } else if current == '}' {
                if brace_depth > 0 {
                    brace_depth -= 1;
                }
                content.push(state.advance(&self.input));
            } else if current == '\\' {
                state.advance(&self.input);
                if !state.is_at_end() {
                    let escaped = state.advance(&self.input);
                    content.push(self.process_escape_sequence(escaped));
                }
            } else {
                content.push(state.advance(&self.input));
            }
        }

        if brace_depth != 0 {
            self.error_manager.add_lexical_error(
                LexicalErrorType::UnterminatedString,
                "Unmatched braces in interpolated string".to_string(),
                start_line,
                start_column,
                None,
                None,
            );

            if self.should_terminate() {
                return Err(format!(
                    "Unmatched braces at line {}, col {}",
                    start_line, start_column
                ));
            }
        }

        Ok(Some(Token::new(
            TokenType::InterpolatedString(self.intern_string(&content)),
            start_line,
            start_column,
            self.current_section.clone(),
        )))
    }

    #[inline]
    fn process_escape_sequence(&self, escaped: char) -> char {
        match escaped {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '\\' => '\\',
            '"' => '"',
            '\'' => '\'',
            '{' => '{',
            '}' => '}',
            '0' => '\0',
            _ => escaped,
        }
    }
}

// ==================== NUMERIC LITERAL SCANNING ====================

impl Tokenizer {
    fn scan_numeric_literal(&mut self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_column = state.column;
        let start_line = state.line;
        let start_pos = state.position;

        let mut has_dot = false;
        let mut has_exponent = false;
        let mut is_negative = false;
        let mut dash_count = 0;
        let mut colon_count = 0;
        let mut is_date = false;
        let mut is_timestamp = false;
        let mut in_timezone_offset = false;

        // Check for negative
        if state.peek(&self.input) == '-' {
            state.advance(&self.input);
            is_negative = true;
        }

        // Scan the number
        while !state.is_at_end() {
            let current = state.peek(&self.input);

            if current.is_ascii_digit() {
                state.advance(&self.input);
            } else if current == '.' && !has_dot && !has_exponent && !is_date {
                let next_char = state.peek_next(&self.input);
                if next_char.is_ascii_digit() {
                    has_dot = true;
                    state.advance(&self.input);
                } else {
                    break;
                }
            } else if (current == 'e' || current == 'E') && !has_exponent && !is_date {
                has_exponent = true;
                state.advance(&self.input);

                if !state.is_at_end() && matches!(state.peek(&self.input), '+' | '-') {
                    state.advance(&self.input);
                }
            } else if current == '-' && dash_count < 2 && !is_negative {
                dash_count += 1;
                is_date = true;
                state.advance(&self.input);
            } else if current == 'T' && is_date {
                is_timestamp = true;
                state.advance(&self.input);
            } else if current == ':' && is_timestamp {
                if in_timezone_offset && colon_count >= 2 {
                    state.advance(&self.input);
                } else if colon_count < 2 {
                    colon_count += 1;
                    state.advance(&self.input);
                } else {
                    break;
                }
            } else if current == '.' && is_timestamp {
                state.advance(&self.input);
            } else if current == 'Z' && is_timestamp {
                state.advance(&self.input);
                break;
            } else if (current == '+' || current == '-') && is_timestamp {
                let scanned = state.slice(&self.input, start_pos, state.position - start_pos);
                if scanned.contains('T') {
                    in_timezone_offset = true;
                    state.advance(&self.input);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let number_string = state.slice(&self.input, start_pos, state.position - start_pos);

        // Return appropriate token type
        if is_timestamp {
            return Ok(Some(Token::new(
                TokenType::Timestamp(self.intern_string(number_string)),
                start_line,
                start_column,
                self.current_section.clone(),
            )));
        }

        if is_date {
            return Ok(Some(Token::new(
                TokenType::Date(self.intern_string(number_string)),
                start_line,
                start_column,
                self.current_section.clone(),
            )));
        }

        self.create_numeric_token(number_string, has_dot, has_exponent, state, start_line, start_column)
    }

    fn create_numeric_token(
        &mut self,
        number_string: &str,
        has_dot: bool,
        has_exponent: bool,
        state: &mut TokenizerState,
        start_line: usize,
        start_column: usize,
    ) -> Result<Option<Token>, String> {
        let mut has_float_suffix = false;
        let number_to_parse = number_string;

        // Check for float suffix
        if !state.is_at_end() && matches!(state.peek(&self.input), 'f' | 'F') {
            has_float_suffix = true;
            state.advance(&self.input);
        }

        let token_type = if has_exponent {
            if has_float_suffix {
                match number_to_parse.parse::<f32>() {
                    Ok(val) => TokenType::Float(val),
                    Err(_) => {
                        return self.handle_invalid_number(number_string, start_line, start_column);
                    }
                }
            } else {
                match number_to_parse.parse::<f64>() {
                    Ok(val) => TokenType::ScientificNotation(val),
                    Err(_) => {
                        return self.handle_invalid_number(number_string, start_line, start_column);
                    }
                }
            }
        } else if has_dot {
            if has_float_suffix {
                match number_to_parse.parse::<f32>() {
                    Ok(val) => TokenType::Float(val),
                    Err(_) => {
                        return self.handle_invalid_number(number_string, start_line, start_column);
                    }
                }
            } else {
                match number_to_parse.parse::<f64>() {
                    Ok(val) => TokenType::Double(val),
                    Err(_) => {
                        return self.handle_invalid_number(number_string, start_line, start_column);
                    }
                }
            }
        } else {
            if has_float_suffix {
                match number_to_parse.parse::<f32>() {
                    Ok(val) => TokenType::Float(val),
                    Err(_) => {
                        return self.handle_invalid_number(number_string, start_line, start_column);
                    }
                }
            } else {
                match number_to_parse.parse::<i32>() {
                    Ok(val) => TokenType::Integer(val),
                    Err(_) => {
                        return self.handle_invalid_number(number_string, start_line, start_column);
                    }
                }
            }
        };

        Ok(Some(Token::new(
            token_type,
            start_line,
            start_column,
            self.current_section.clone(),
        )))
    }

    fn handle_invalid_number(
        &self,
        number_string: &str,
        start_line: usize,
        start_column: usize,
    ) -> Result<Option<Token>, String> {
        self.error_manager.add_lexical_error(
            LexicalErrorType::InvalidNumericFormat,
            format!("Invalid numeric format: {}", number_string),
            start_line,
            start_column,
            None,
            None,
        );

        if self.should_terminate() {
            return Err(format!("Invalid number format: {}", number_string));
        }

        Ok(Some(Token::new(
            TokenType::Error(format!("Invalid number: {}", number_string)),
            start_line,
            start_column,
            self.current_section.clone(),
        )))
    }

    // ==================== HEX SCANNING ====================

    fn scan_hex_color(&mut self, state: &mut TokenizerState) -> Token {
        let start_column = state.column;
        let start_line = state.line;
        let start_pos = state.position;

        state.advance(&self.input); // Consume #

        while !state.is_at_end()
            && self.is_hex_digit(state.peek(&self.input))
            && (state.position - start_pos) < 9
        {
            state.advance(&self.input);
        }

        let hex_value = state.slice(&self.input, start_pos, state.position - start_pos);

        Token::new(
            TokenType::HexColor(self.intern_string(hex_value)),
            start_line,
            start_column,
            self.current_section.clone(),
        )
    }

    fn scan_hex_literal(&mut self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_column = state.column;
        let start_line = state.line;
        let start_pos = state.position;

        // Consume 0x
        state.advance(&self.input);
        state.advance(&self.input);

        while !state.is_at_end() && self.is_hex_digit(state.peek(&self.input)) {
            state.advance(&self.input);
        }

        let hex_part = state.slice(&self.input, start_pos + 2, state.position - start_pos - 2);

        match i32::from_str_radix(hex_part, 16) {
            Ok(value) => Ok(Some(Token::new(
                TokenType::Integer(value),
                start_line,
                start_column,
                self.current_section.clone(),
            ))),
            Err(_) => {
                self.error_manager.add_lexical_error(
                    LexicalErrorType::InvalidNumericFormat,
                    format!("Invalid hex literal: {}", hex_part),
                    start_line,
                    start_column,
                    None,
                    None,
                );

                if self.should_terminate() {
                    return Err(format!("Invalid hex literal: {}", hex_part));
                }

                Ok(Some(Token::new(
                    TokenType::Error(format!("Invalid hex: {}", hex_part)),
                    start_line,
                    start_column,
                    self.current_section.clone(),
                )))
            }
        }
    }

    // ==================== MULTI-CHAR OPERATOR SCANNING ====================

    fn try_scan_multi_char_operator(&mut self, state: &mut TokenizerState) -> Option<Token> {
        let start_column = state.column;
        let start_line = state.line;
        let current = state.peek(&self.input);

        if state.is_at_end() {
            return None;
        }

        let next = state.peek_next(&self.input);

        // Check for three-character operators
        if state.position + 2 < state.input_length {
            let third = self.input.chars().nth(state.position + 2).unwrap_or('\0');

            let three_char = match (current, next, third) {
                ('*', '*', '=') => Some(TokenType::ArithmeticAssignOp("**=".to_string())),
                ('<', '<', '=') => Some(TokenType::BitwiseOp("<<=".to_string())),
                ('>', '>', '=') => Some(TokenType::BitwiseOp(">>=".to_string())),
                ('>', '_', '<') => Some(TokenType::BitwiseOp(">_<".to_string())),
                _ => None,
            };

            if let Some(tt) = three_char {
                state.advance(&self.input);
                state.advance(&self.input);
                state.advance(&self.input);
                return Some(Token::new(tt, start_line, start_column, self.current_section.clone()));
            }
        }

        // Check for two-character operators
        let two_char = match (current, next) {
            ('=', '>') => Some(TokenType::Arrow),
            (':', ':') => Some(TokenType::DoubleColon),
            ('-', '>') => Some(TokenType::SwitchCase),
            ('*', '*') => Some(TokenType::ArithmeticOp("**".to_string())),
            ('<', '<') => Some(TokenType::BitwiseOp("<<".to_string())),
            ('>', '>') => Some(TokenType::BitwiseOp(">>".to_string())),
            ('~', '?') => Some(TokenType::BitwiseOp("~?".to_string())),
            ('%', '%') => Some(TokenType::ArithmeticOp("%%".to_string())),
            ('%', '&') => Some(TokenType::ArithmeticOp("%&".to_string())),
            ('&', '%') => Some(TokenType::ArithmeticOp("&%".to_string())),
            ('=', '=') => Some(TokenType::ComparisonOp("==".to_string())),
            ('!', '=') => Some(TokenType::ComparisonOp("!=".to_string())),
            ('<', '=') => Some(TokenType::ComparisonOp("<=".to_string())),
            ('>', '=') => Some(TokenType::ComparisonOp(">=".to_string())),
            ('&', '&') => Some(TokenType::LogicalOp("&&".to_string())),
            ('|', '|') => Some(TokenType::LogicalOp("||".to_string())),
            ('+', '+') => Some(TokenType::ArithmeticOp("++".to_string())),
            ('-', '-') => Some(TokenType::ArithmeticOp("--".to_string())),
            ('+', '=') => Some(TokenType::ArithmeticAssignOp("+=".to_string())),
            ('-', '=') => Some(TokenType::ArithmeticAssignOp("-=".to_string())),
            ('*', '=') => Some(TokenType::ArithmeticAssignOp("*=".to_string())),
            ('/', '=') => Some(TokenType::ArithmeticAssignOp("/=".to_string())),
            ('%', '=') => Some(TokenType::ArithmeticAssignOp("%=".to_string())),
            ('&', '=') => Some(TokenType::BitwiseOp("&=".to_string())),
            ('|', '=') => Some(TokenType::BitwiseOp("|=".to_string())),
            ('^', '=') => Some(TokenType::BitwiseOp("^=".to_string())),
            _ => None,
        };

        if let Some(tt) = two_char {
            state.advance(&self.input);
            state.advance(&self.input);
            return Some(Token::new(tt, start_line, start_column, self.current_section.clone()));
        }

        None
    }
}

impl Tokenizer {
    // ==================== IDENTIFIER AND KEYWORD SCANNING ====================

    fn scan_identifier_or_keyword(&mut self, state: &mut TokenizerState) -> Token {
        let start_column = state.column;
        let start_line = state.line;
        let start_pos = state.position;

        while !state.is_at_end() {
            let ch = state.peek(&self.input);
            if ch.is_alphanumeric() || ch == '_' {
                state.advance(&self.input);
            } else {
                break;
            }
        }

        let identifier = state.slice(&self.input, start_pos, state.position - start_pos);

        // Fast keyword lookup
        if let Some(keyword_type) = self.try_get_keyword_fast(identifier) {
            return Token::new(
                keyword_type,
                start_line,
                start_column,
                self.current_section.clone(),
            );
        }

        Token::new(
            TokenType::Identifier(self.intern_string(identifier)),
            start_line,
            start_column,
            self.current_section.clone(),
        )
    }

    #[inline]
    fn try_get_keyword_fast(&self, word: &str) -> Option<TokenType> {
        match word.len() {
            2 => match word {
                "if" => Some(TokenType::Keyword("if".to_string())),
                "or" => Some(TokenType::Keyword("or".to_string())),
                _ => None,
            },
            3 => match word {
                "and" => Some(TokenType::Keyword("and".to_string())),
                "not" => Some(TokenType::Keyword("not".to_string())),
                "int" => Some(TokenType::Keyword("int".to_string())),
                "hex" => Some(TokenType::Keyword("hex".to_string())),
                "chk" => Some(TokenType::Keyword("chk".to_string())),
                "let" => Some(TokenType::Keyword("let".to_string())),
                "mut" => Some(TokenType::Keyword("mut".to_string())),
                "any" => Some(TokenType::Keyword("any".to_string())),
                _ => None,
            },
            4 => match word {
                "true" => Some(TokenType::Bool(true)),
                "null" => Some(TokenType::Keyword("null".to_string())),
                "else" => Some(TokenType::Keyword("else".to_string())),
                "elif" => Some(TokenType::Keyword("elif".to_string())),
                "then" => Some(TokenType::Keyword("then".to_string())),
                "enum" => Some(TokenType::Keyword("enum".to_string())),
                "date" => Some(TokenType::Keyword("date".to_string())),
                "bool" => Some(TokenType::Keyword("bool".to_string())),
                "blob" => Some(TokenType::Keyword("blob".to_string())),
                "miss" => Some(TokenType::Keyword("miss".to_string())),
                "from" => Some(TokenType::Keyword("from".to_string())),
                _ => None,
            },
            5 => match word {
                "false" => Some(TokenType::Bool(false)),
                "float" => Some(TokenType::Keyword("float".to_string())),
                "tuple" => Some(TokenType::Keyword("tuple".to_string())),
                "regex" => Some(TokenType::Keyword("regex".to_string())),
                "array" => Some(TokenType::Keyword("array".to_string())),
                "const" => Some(TokenType::Keyword("const".to_string())),
                _ => None,
            },
            6 => match word {
                "string" => Some(TokenType::Keyword("string".to_string())),
                "double" => Some(TokenType::Keyword("double".to_string())),
                "object" => Some(TokenType::Keyword("object".to_string())),
                "return" => Some(TokenType::Keyword("return".to_string())),
                "global" => Some(TokenType::Keyword("global".to_string())),
                "verify" => Some(TokenType::Keyword("verify".to_string())),
                _ => None,
            },
            9 => match word {
                "timestamp" => Some(TokenType::Keyword("timestamp".to_string())),
                _ => None,
            },
            10 => match word {
                "from_cloud" => Some(TokenType::Keyword("from_cloud".to_string())),
                _ => None,
            },
            _ => None,
        }
    }

    // ==================== PREFIXED CONSTRUCTOR SCANNING ====================

    fn is_valid_prefixed_constructor(&self, state: &TokenizerState) -> bool {
        if state.position + 1 >= state.input_length {
            return false;
        }
        let prefix = state.peek(&self.input);
        matches!(prefix, 'b' | 't' | 'r')
    }

    fn scan_prefixed_constructor(&mut self, state: &mut TokenizerState) -> Token {
        let start_column = state.column;
        let start_line = state.line;
        let prefix = state.advance(&self.input);
        state.advance(&self.input); // Consume :

        let prefix_string = prefix.to_string();
        let constructor_type = match prefix {
            'b' => "BLOB_CONSTRUCTOR",
            't' => "TUPLE_CONSTRUCTOR",
            'r' => "REGEX_CONSTRUCTOR",
            _ => "UNKNOWN_CONSTRUCTOR",
        };

        self.prefixed_constructors_found.push(PrefixedConstructorInfo {
            constructor_type: constructor_type.to_string(),
            prefix: prefix_string.clone(),
            line: start_line,
            column: start_column,
            section: self.current_section.clone(),
        });

        let token_type = match prefix {
            'b' => TokenType::BlobConstructor("".to_string()),
            't' => TokenType::TupleConstructor("".to_string()),
            'r' => TokenType::RegexConstructor("".to_string()),
            _ => TokenType::Error(format!("Unknown constructor: {}", prefix_string)),
        };

        Token::new(
            token_type,
            start_line,
            start_column,
            self.current_section.clone(),
        )
    }

    // ==================== SINGLE CHARACTER SCANNING ====================

    fn scan_single_character(&mut self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_column = state.column;
        let start_line = state.line;
        let symbol = state.advance(&self.input);

        // Special arithmetic operators
        let token_type = match symbol {
            '+' => TokenType::ArithmeticOp("+".to_string()),
            '*' => TokenType::ArithmeticOp("*".to_string()),
            '%' => TokenType::ArithmeticOp("%".to_string()),
            '^' => TokenType::BitwiseOp("^".to_string()),
            '&' => TokenType::BitwiseOp("&".to_string()),
            '|' => TokenType::BitwiseOp("|".to_string()),
            _ if !symbol.is_control() && !symbol.is_whitespace() => TokenType::Symbol(symbol),
            _ => {
                self.error_manager.add_lexical_error(
                    LexicalErrorType::InvalidCharacter,
                    format!("Unexpected character: '{}' (0x{:X})", symbol, symbol as u32),
                    start_line,
                    start_column,
                    None,
                    None,
                );

                if self.should_terminate() {
                    return Err(format!(
                        "Invalid character at line {}, col {}: '{}'",
                        start_line, start_column, symbol
                    ));
                }

                TokenType::Error(format!("Invalid character: '{}'", symbol))
            }
        };

        Ok(Some(Token::new(
            token_type,
            start_line,
            start_column,
            self.current_section.clone(),
        )))
    }

    // ==================== TOKEN SEQUENCE ANALYSIS ====================

    fn analyze_token_sequences(&mut self) {
        let len = self.token_pool.len();

        for i in 0..len.saturating_sub(2) {
            let token1 = &self.token_pool[i];
            let token2 = &self.token_pool[i + 1];
            let token3 = &self.token_pool[i + 2];

            // Check for static calls: Identifier . Identifier
            if let TokenType::Identifier(obj_name) = &token1.token_type {
                if let TokenType::Symbol('.') = &token2.token_type {
                    if let TokenType::Identifier(method_name) = &token3.token_type {
                        if self.could_be_static_object(obj_name) {
                            self.static_calls_found.push(StaticCallInfo {
                                object_name: obj_name.clone(),
                                method_name: method_name.clone(),
                                line: token1.line,
                                column: token1.column,
                                section: self.current_section.clone(),
                                token_index: i,
                            });
                        }
                    }
                }
            }
        }
    }

    #[inline]
    fn could_be_static_object(&self, identifier: &str) -> bool {
        !identifier.is_empty()
            && identifier.chars().next().unwrap().is_uppercase()
            && identifier != "Dix"
    }

    fn create_metadata(&self) -> TokenizationMetadata {
        let sections_detected = self.get_sections_from_tokens();
        let potential_builtin_calls = self.analyze_potential_builtin_calls();

        TokenizationMetadata {
            version: "1.0.0".to_string(),
            total_lines: self.token_pool.last().map(|t| t.line).unwrap_or(1),
            total_tokens: self.token_pool.len().saturating_sub(1), // Exclude EOF
            sections_detected,
            prefixed_constructors_found: self.prefixed_constructors_found.len(),
            blob_constructors: self.prefixed_constructors_found.iter()
                .filter(|p| p.constructor_type == "BLOB_CONSTRUCTOR")
                .count(),
            tuple_constructors: self.prefixed_constructors_found.iter()
                .filter(|p| p.constructor_type == "TUPLE_CONSTRUCTOR")
                .count(),
            regex_constructors: self.prefixed_constructors_found.iter()
                .filter(|p| p.constructor_type == "REGEX_CONSTRUCTOR")
                .count(),
            static_calls_found: self.static_calls_found.len(),
            potential_builtin_calls,
        }
    }

    fn get_sections_from_tokens(&self) -> Vec<String> {
        let mut sections = Vec::new();

        for token in &self.token_pool {
            if let Some(section) = token.token_type.get_section_context() {
                if !sections.contains(&section.to_string()) {
                    sections.push(section.to_string());
                }
            }
        }

        sections
    }

    fn analyze_potential_builtin_calls(&self) -> usize {
        let len = self.token_pool.len();
        let mut count = 0;

        for i in 0..len.saturating_sub(3) {
            let token2 = &self.token_pool[i + 1];
            let token3 = &self.token_pool[i + 2];
            let token4 = &self.token_pool[i + 3];

            // Pattern: . Identifier (
            if let TokenType::Symbol('.') = &token2.token_type {
                if let TokenType::Identifier(_) = &token3.token_type {
                    if let TokenType::Symbol('(') = &token4.token_type {
                        count += 1;
                    }
                }
            }
        }

        count
    }

}

/// Tokenization result structure
#[derive(Debug, Clone)]
pub struct TokenizationResult {
    pub tokens: Vec<Token>,
    pub metadata: TokenizationMetadata,
    pub prefixed_constructors: Vec<PrefixedConstructorInfo>,
    pub static_calls: Vec<StaticCallInfo>,
}

/// Metadata about the tokenization process
#[derive(Debug, Clone)]
pub struct TokenizationMetadata {
    pub version: String,
    pub total_lines: usize,
    pub total_tokens: usize,
    pub sections_detected: Vec<String>,
    pub prefixed_constructors_found: usize,
    pub blob_constructors: usize,
    pub tuple_constructors: usize,
    pub regex_constructors: usize,
    pub static_calls_found: usize,
    pub potential_builtin_calls: usize,
}

/// Information about a prefixed constructor found during tokenization
#[derive(Debug, Clone)]
pub struct PrefixedConstructorInfo {
    pub constructor_type: String,
    pub prefix: String,
    pub line: usize,
    pub column: usize,
    pub section: Option<String>,
}

/// Information about a static call found during tokenization
#[derive(Debug, Clone)]
pub struct StaticCallInfo {
    pub object_name: String,
    pub method_name: String,
    pub line: usize,
    pub column: usize,
    pub section: Option<String>,
    pub token_index: usize,
}