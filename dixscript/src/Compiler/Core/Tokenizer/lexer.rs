//! DixScript Lexer — tokenises a `.mdix` source string into a `Vec<Token>`.
//!
//! ## Numeric literal additions (v1.0.0)
//!
//! ### Underscore separators
//! Any integer, hex, or binary literal may contain `_` between digits as a
//! visual separator. Underscores are stripped before parsing:
//!   `1_000_000`  → `Integer(1000000)`
//!   `0xFF_FF`    → `Integer(65535)`
//!   `0b1111_0000` → `Integer(240)`
//! Underscores inside date/timestamp literals are intentionally NOT supported.
//!
//! ### Binary literals  (`0b` / `0B`)
//! Prefix `0b` or `0B` followed by `0` / `1` digits (with optional `_`).
//! Fits in i32 → `Integer`. Overflows i32 → `Long`. `L` suffix → `Long`.
//!   `0b1010`          → `Integer(10)`
//!   `0b1111_0000`     → `Integer(240)`
//!   `0b1L`            → `Long(1)`
//!
//! ### Long (64-bit) integers  (`L` / `l` suffix)
//! Appending `L` or `l` to any integer literal (decimal, hex, binary) emits
//! `Long(i64)` instead of `Integer(i32)`.
//!   `9_000_000_000L`  → `Long(9000000000)`
//!   `0xDEAD_BEEFL`    → `Long(3735928559)`
//!   `0b1111L`         → `Long(15)`
//! Plain integers that overflow i32 are also auto-promoted to `Long`:
//!   `3_000_000_000`   → `Long(3000000000)`

use phf::phf_map;
use memchr::memchr;
use super::token::{Token, TokenType, SectionId};
use super::char_tables::{DIGIT, HEX_DIGIT, IDENT_CONT, IDENT_START};
use super::platform;
use crate::ErrorManager::{ErrorManager, LexicalErrorType, DebugConfig};
use crate::Compiler::Core::Config::OperationalSettings;
use crate::Compiler::Core::Config::operational_settings::ErrorHandlingStrategy;
use crate::Compiler::VersionControl::VersionManager;

const INITIAL_TOKEN_POOL_SIZE: usize = 256;
const MAX_RECOVERY_ATTEMPTS:   usize = 10;

static KEYWORDS: phf::Map<&'static str, fn() -> TokenType> = phf_map! {
    "if"          => || TokenType::Keyword("if"),
    "or"          => || TokenType::Keyword("or"),
    "and"         => || TokenType::Keyword("and"),
    "not"         => || TokenType::Keyword("not"),
    "int"         => || TokenType::Keyword("int"),
    "hex"         => || TokenType::Keyword("hex"),
    "chk"         => || TokenType::Keyword("chk"),
    "let"         => || TokenType::Keyword("let"),
    "mut"         => || TokenType::Keyword("mut"),
    "any"         => || TokenType::Keyword("any"),
    "true"        => || TokenType::Bool(true),
    "null"        => || TokenType::Keyword("null"),
    "else"        => || TokenType::Keyword("else"),
    "elif"        => || TokenType::Keyword("elif"),
    "then"        => || TokenType::Keyword("then"),
    "enum"        => || TokenType::Keyword("enum"),
    "date"        => || TokenType::Keyword("date"),
    "bool"        => || TokenType::Keyword("bool"),
    "blob"        => || TokenType::Keyword("blob"),
    "miss"        => || TokenType::Keyword("miss"),
    "from"        => || TokenType::Keyword("from"),
    "false"       => || TokenType::Bool(false),
    "float"       => || TokenType::Keyword("float"),
    "tuple"       => || TokenType::Keyword("tuple"),
    "regex"       => || TokenType::Keyword("regex"),
    "array"       => || TokenType::Keyword("array"),
    "const"       => || TokenType::Keyword("const"),
    "long"        => || TokenType::Keyword("long"),
    "string"      => || TokenType::Keyword("string"),
    "double"      => || TokenType::Keyword("double"),
    "object"      => || TokenType::Keyword("object"),
    "return"      => || TokenType::Keyword("return"),
    "global"      => || TokenType::Keyword("global"),
    "verify"      => || TokenType::Keyword("verify"),
    "timestamp"   => || TokenType::Keyword("timestamp"),
    "from_cloud"  => || TokenType::Keyword("from_cloud"),
};

// ── Tokenizer state ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct TokenizerState {
    position:     usize,
    line:         usize,
    column:       usize,
    input_length: usize,
}

impl TokenizerState {
    #[inline] fn new(input_length: usize) -> Self {
        TokenizerState { position: 0, line: 1, column: 1, input_length }
    }
    #[inline] fn is_at_end(&self) -> bool { self.position >= self.input_length }
    #[inline] fn peek(&self, input: &str) -> char {
        if self.is_at_end() { '\0' } else { input.as_bytes()[self.position] as char }
    }
    #[inline] fn peek_next(&self, input: &str) -> char {
        if self.position + 1 >= self.input_length { '\0' }
        else { input.as_bytes()[self.position + 1] as char }
    }
    #[inline] fn peek_at(&self, input: &str, offset: usize) -> char {
        let pos = self.position + offset;
        if pos >= self.input_length { '\0' } else { input.as_bytes()[pos] as char }
    }
    #[inline] fn advance(&mut self, input: &str) -> char {
        if self.is_at_end() { return '\0'; }
        let current = input.as_bytes()[self.position] as char;
        self.position += 1;
        if current == '\n' { self.line += 1; self.column = 1; } else { self.column += 1; }
        current
    }
    #[inline] fn slice<'a>(&self, input: &'a str, start: usize, length: usize) -> &'a str {
        let bytes = input.as_bytes();
        let end   = (start + length).min(bytes.len());
        std::str::from_utf8(&bytes[start..end]).unwrap_or("")
    }
}

// ── Tokenizer struct ──────────────────────────────────────────────────────────

pub struct Tokenizer<'src> {
    input:    &'src str,
    settings: &'src OperationalSettings,

    version_allows_all_tokens: bool,
    debug_config:  DebugConfig,
    error_manager: ErrorManager,
    current_section: SectionId,

    prefixed_constructors_found: Vec<PrefixedConstructorInfo>,
    static_calls_found:          Vec<StaticCallInfo>,
    token_pool:                  Vec<Token>,
}

impl<'src> Tokenizer<'src> {
    pub fn new_with_error_manager(
        input:         &'src str,
        settings:      &'src OperationalSettings,
        error_manager: ErrorManager,
    ) -> Self {
        let estimated_tokens = (input.len() / 10).max(INITIAL_TOKEN_POOL_SIZE);
        let debug_config     = DebugConfig::from_debug_mode(error_manager.get_debug_mode());

        let version_allows_all_tokens = VersionManager::instance()
            .read()
            .map(|vm| {
                let v = vm.get_current_version();
                v == "1.0.0" || v.starts_with("1.")
            })
            .unwrap_or(true);

        Tokenizer {
            input,
            settings,
            version_allows_all_tokens,
            debug_config,
            error_manager,
            current_section: SectionId::None,
            prefixed_constructors_found: Vec::new(),
            static_calls_found:          Vec::new(),
            token_pool:                  Vec::with_capacity(estimated_tokens),
        }
    }

    pub fn new(input: &'src str, settings: &'src OperationalSettings) -> Self {
        Self::new_with_error_manager(input, settings, ErrorManager::get_shared_instance())
    }

    pub fn tokenize(mut self) -> TokenizationResult {
        let input_len = self.input.len();
        let mut state = TokenizerState::new(input_len);
        let mut recovery_attempts   = 0usize;
        let mut last_error_position = usize::MAX;

        loop {
            self.skip_whitespace(&mut state);
            if state.is_at_end() { break; }

            match self.scan_token(&mut state) {
                Ok(Some(token)) => {
                    recovery_attempts = 0;
                    if !self.version_allows_all_tokens
                        && !self.is_token_supported_slow(&token)
                    {
                        self.handle_unsupported_token(&token);
                        if self.should_terminate() { break; }
                        continue;
                    }
                    self.update_section_context(&token);
                    self.token_pool.push(token);
                }
                Ok(None) => {}
                Err(err_msg) => {
                    self.handle_tokenization_error(
                        &err_msg, &mut state,
                        &mut recovery_attempts, &mut last_error_position,
                    );
                    if self.should_terminate() { break; }
                    if self.supports_recovery() {
                        if recovery_attempts >= MAX_RECOVERY_ATTEMPTS {
                            self.error_manager.add_lexical_error(
                                LexicalErrorType::InvalidCharacter,
                                "Maximum recovery attempts exceeded — aborting tokenization".to_string(),
                                state.line, state.column, None, None,
                            );
                            break;
                        }
                        if !self.attempt_recovery(&mut state) { break; }
                        continue;
                    }
                    if self.should_continue() {
                        let error_token = Token::new(
                            TokenType::Error(err_msg.clone()),
                            state.line, state.column, self.current_section,
                        );
                        self.token_pool.push(error_token);
                        if !self.skip_to_next_valid_token(&mut state) { break; }
                    }
                }
            }
        }

        self.token_pool.push(Token::eof(state.line, state.column));

        if self.debug_config.is_testing {
            self.analyze_token_sequences();
        }

        let metadata = self.create_metadata();

        TokenizationResult {
            tokens:                self.token_pool,
            metadata,
            prefixed_constructors: self.prefixed_constructors_found,
            static_calls:          self.static_calls_found,
        }
    }

    #[inline]
    fn skip_whitespace(&self, state: &mut TokenizerState) {
        let bytes = self.input.as_bytes();
        let start = state.position;
        let end   = platform::find_whitespace_end(bytes, start);
        if end == start { return; }

        let ws_slice      = &bytes[start..end];
        let newline_count = memchr::memchr_iter(b'\n', ws_slice).count();

        if newline_count == 0 {
            state.column += end - start;
        } else {
            let last_nl_offset = memchr::memchr_iter(b'\n', ws_slice).last().unwrap();
            state.line  += newline_count;
            state.column = end - (start + last_nl_offset);
        }

        state.position = end;
        if state.column == 0 { state.column = 1; }
    }

    #[inline]
    fn handle_tokenization_error(
        &self, err_msg: &str, state: &mut TokenizerState,
        recovery_attempts: &mut usize, last_error_position: &mut usize,
    ) {
        if state.position == *last_error_position {
            *recovery_attempts += 1;
        } else {
            *recovery_attempts   = 1;
            *last_error_position = state.position;
        }
        self.error_manager.add_lexical_error(
            LexicalErrorType::InvalidCharacter, err_msg.to_string(),
            state.line, state.column, None, None,
        );
    }

    fn handle_unsupported_token(&self, token: &Token) {
        let msg = if self.debug_config.is_enabled {
            let v = VersionManager::instance().read()
                .map(|vm| vm.get_current_version().to_string())
                .unwrap_or_else(|_| "unknown".to_string());
            format!("Token type not supported in version {}", v)
        } else {
            "Token type not supported in current version".to_string()
        };
        self.error_manager.add_lexical_error(
            LexicalErrorType::InvalidCharacter, msg,
            token.line, token.column, None, None,
        );
    }

    fn attempt_recovery(&self, state: &mut TokenizerState) -> bool {
        let start_position = state.position;
        while !state.is_at_end() && state.position < start_position + 100 {
            let current = state.peek(self.input);
            if current.is_whitespace() { self.skip_whitespace(state); return true; }
            if matches!(current, ';' | ',' | '}' | ')' | ']') {
                state.advance(self.input); return true;
            }
            if current.is_alphanumeric() || matches!(current, '"' | '\'' | '@') { return true; }
            state.advance(self.input);
        }
        !state.is_at_end()
    }

    fn skip_to_next_valid_token(&self, state: &mut TokenizerState) -> bool {
        while !state.is_at_end() {
            let current = state.peek(self.input);
            if current.is_whitespace() { self.skip_whitespace(state); return true; }
            if current.is_alphanumeric()
                || matches!(current, '"' | '\'' | '@' | '{' | '[') { return true; }
            state.advance(self.input);
        }
        false
    }

    #[inline] fn should_terminate(&self) -> bool { self.error_manager.should_terminate_parsing() }
    #[inline] fn supports_recovery(&self) -> bool { !self.should_terminate() || self.should_continue() }
    #[inline] fn should_continue(&self) -> bool { self.error_manager.has_errors() && !self.should_terminate() }

    #[inline]
    fn is_token_supported_slow(&self, token: &Token) -> bool {
        VersionManager::instance().read()
            .map(|vm| vm.is_token_valid_for_version(&token.token_type))
            .unwrap_or(true)
    }

    #[inline(always)]
    fn is_hex_digit(&self, c: char) -> bool {
        (c as u32) < 256 && HEX_DIGIT[c as usize]
    }

    #[inline]
    fn update_section_context(&mut self, token: &Token) {
        if let Some(ctx) = token.token_type.get_section_context() {
            self.current_section = SectionId::from_context_str(ctx);
        }
    }

    #[inline]
    fn is_advanced_section(&self) -> bool {
        matches!(self.current_section, SectionId::QuickFuncs | SectionId::Imports | SectionId::Dlm)
    }

    /// Returns true if `bytes[pos]` is an identifier continuation character.
    /// Used by suffix checks to avoid consuming a letter that starts a following identifier.
    #[inline]
    fn is_ident_cont_at(&self, pos: usize) -> bool {
        if pos >= self.input.len() { return false; }
        let b = self.input.as_bytes()[pos];
        (b as u32) < 256 && IDENT_CONT[b as usize]
    }
}

// ── Token scanning ────────────────────────────────────────────────────────────

impl<'src> Tokenizer<'src> {
    fn scan_token(&mut self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        if state.is_at_end() { return Ok(None); }

        let current = state.peek(self.input);

        // Comments
        if current == '/' {
            let next = state.peek_next(self.input);
            if next == '/' { return Ok(Some(self.scan_single_line_comment(state))); }
            if next == '*' { return self.scan_multi_line_comment(state); }
        }

        // Section keywords
        if current == '@' {
            if let Some(t) = self.try_scan_section_keyword(state) { return Ok(Some(t)); }
        }

        // String literals
        if current == '"' || current == '\'' {
            return self.scan_string_literal(state);
        }

        // Interpolated strings
        if current == '$' && self.is_advanced_section() {
            let next = state.peek_next(self.input);
            if next == '"' || next == '\'' {
                return self.scan_interpolated_string(state);
            }
        }

        // Prefixed numeric literals: 0x (hex) and 0b (binary)
        if current == '0' {
            let next = state.peek_next(self.input);
            if next == 'x' || next == 'X' { return self.scan_hex_literal(state); }
            if next == 'b' || next == 'B' { return self.scan_binary_literal(state); }
        }

        // Numeric literals (decimal, float, double, date, timestamp)
        if current.is_ascii_digit()
            || (current == '-' && state.peek_next(self.input).is_ascii_digit())
        {
            return self.scan_numeric_literal(state);
        }

        // Hex colors
        if current == '#' { return Ok(Some(self.scan_hex_color(state))); }

        // Multi-char operators
        if let Some(t) = self.try_scan_multi_char_operator(state) { return Ok(Some(t)); }

        // Prefixed constructors (b:, t:, r:)
        if current.is_ascii_alphabetic()
            && state.peek_next(self.input) == ':'
            && self.is_valid_prefixed_constructor(state)
        {
            return Ok(Some(self.scan_prefixed_constructor(state)));
        }

        // Identifiers / keywords
        if (current as u32) < 256 && IDENT_START[current as usize] {
            return Ok(Some(self.scan_identifier_or_keyword(state)));
        }

        self.scan_single_character(state)
    }

    fn scan_single_line_comment(&self, state: &mut TokenizerState) -> Token {
        let start_column = state.column;
        let start_line   = state.line;
        state.advance(self.input);
        state.advance(self.input);
        let comment_start = state.position;
        let bytes = self.input.as_bytes();
        if let Some(offset) = memchr(b'\n', &bytes[state.position..]) {
            let content = state.slice(self.input, comment_start, offset).to_string();
            state.position += offset + 1;
            state.line += 1;
            state.column = 1;
            return Token::new(TokenType::Comment(content), start_line, start_column, self.current_section);
        }
        let content = state.slice(self.input, comment_start, bytes.len() - comment_start).to_string();
        state.position = bytes.len();
        Token::new(TokenType::Comment(content), start_line, start_column, self.current_section)
    }

    fn scan_multi_line_comment(&self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_column = state.column;
        let start_line   = state.line;
        state.advance(self.input);
        state.advance(self.input);
        let comment_start = state.position;
        let bytes = self.input.as_bytes();

        if let Some(offset) = memchr::memmem::find(&bytes[state.position..], b"*/") {
            let end_abs       = state.position + offset;
            let comment_bytes = &bytes[comment_start..end_abs];
            let newline_count = memchr::memchr_iter(b'\n', comment_bytes).count();
            let content       = std::str::from_utf8(comment_bytes).unwrap_or("").to_string();

            if newline_count > 0 {
                state.line += newline_count;
                let last_nl = memchr::memchr_iter(b'\n', comment_bytes).last().unwrap_or(0);
                state.column = (end_abs - (comment_start + last_nl)) + 2;
            } else {
                state.column += (end_abs - comment_start) + 2;
            }
            state.position = end_abs + 2;

            return Ok(Some(Token::new(
                TokenType::Comment(content), start_line, start_column, self.current_section,
            )));
        }

        self.error_manager.add_lexical_error(
            LexicalErrorType::UnterminatedString,
            "Unterminated multi-line comment".to_string(),
            start_line, start_column, None, None,
        );
        if self.should_terminate() {
            return Err(format!("Unterminated multi-line comment at line {}, col {}", start_line, start_column));
        }
        let content = state.slice(self.input, comment_start, bytes.len() - comment_start).to_string();
        Ok(Some(Token::new(TokenType::Comment(content), start_line, start_column, self.current_section)))
    }

    fn try_scan_section_keyword(&self, state: &mut TokenizerState) -> Option<Token> {
        let start_pos    = state.position;
        let start_line   = state.line;
        let start_column = state.column;

        if state.peek(self.input) != '@' { return None; }
        state.advance(self.input);

        let section_start = state.position;
        while !state.is_at_end() {
            let b = self.input.as_bytes()[state.position];
            if b.is_ascii_alphabetic() { state.advance(self.input); } else { break; }
        }
        let section_len = state.position - section_start;
        if section_len == 0 {
            state.position = start_pos; state.line = start_line; state.column = start_column;
            return None;
        }
        let section_name = state.slice(self.input, section_start, section_len);

        let token_type = match section_name.to_uppercase().as_str() {
            "CONFIG"     => Some(TokenType::SectionConfig),
            "DLM"        => Some(TokenType::SectionDLM),
            "ENUMS"      => Some(TokenType::SectionEnums),
            "IMPORTS"    => Some(TokenType::SectionImports),
            "QUICKFUNCS" => Some(TokenType::SectionQuickFuncs),
            "DATA"       => Some(TokenType::SectionData),
            "SECURITY"   => Some(TokenType::SectionSecurity),
            _ => None,
        };

        if let Some(tt) = token_type {
            let own_section = tt.get_section_context()
                .map(SectionId::from_context_str)
                .unwrap_or(SectionId::None);
            Some(Token::new(tt, start_line, start_column, own_section))
        } else {
            state.position = start_pos; state.line = start_line; state.column = start_column;
            None
        }
    }

    fn scan_string_literal(&self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_line   = state.line;
        let start_column = state.column;
        let quote        = state.peek(self.input) as u8;
        state.advance(self.input);

        let bytes        = self.input.as_bytes();
        let search_start = state.position;
        let mut pos      = search_start;
        let mut content  = String::new();
        let mut has_escapes = false;

        loop {
            let remaining = &bytes[pos..];
            if let Some(offset) = memchr::memchr3(quote, b'\\', b'\n', remaining) {
                let found_pos  = pos + offset;
                let found_char = bytes[found_pos];

                if found_char == quote {
                    if !has_escapes {
                        content = state.slice(self.input, search_start, found_pos - search_start).to_string();
                    } else {
                        content.push_str(state.slice(self.input, pos, found_pos - pos));
                    }
                    state.position = found_pos + 1;
                    state.column  += found_pos + 1 - search_start;

                    let token_type = if quote == b'\'' {
                        TokenType::StringSingle(content)
                    } else {
                        TokenType::String(content)
                    };
                    return Ok(Some(Token::new(token_type, start_line, start_column, self.current_section)));

                } else if found_char == b'\\' {
                    has_escapes = true;
                    content.push_str(state.slice(self.input, pos, found_pos - pos));
                    if found_pos + 1 < bytes.len() {
                        let escaped = bytes[found_pos + 1] as char;
                        content.push(self.process_escape_sequence(escaped));
                        pos = found_pos + 2;
                    } else {
                        pos = found_pos + 1;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        self.error_manager.add_lexical_error(
            LexicalErrorType::UnterminatedString,
            "Unterminated string literal".to_string(),
            start_line, start_column, None, None,
        );
        if self.should_terminate() {
            return Err(format!("Unterminated string at line {}, col {}", start_line, start_column));
        }
        let partial    = state.slice(self.input, search_start, bytes.len() - search_start).to_string();
        state.position = bytes.len();
        let token_type = if quote == b'\'' { TokenType::StringSingle(partial) } else { TokenType::String(partial) };
        Ok(Some(Token::new(token_type, start_line, start_column, self.current_section)))
    }

    fn scan_interpolated_string(&self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_line   = state.line;
        let start_column = state.column;
        state.advance(self.input);
        let quote       = state.advance(self.input);
        let mut content     = String::new();
        let mut brace_depth = 0i32;

        while !state.is_at_end() {
            let current = state.peek(self.input);
            if current == quote && brace_depth == 0 {
                state.advance(self.input); break;
            } else if current == '{' {
                brace_depth += 1; content.push(state.advance(self.input));
            } else if current == '}' {
                if brace_depth > 0 { brace_depth -= 1; }
                content.push(state.advance(self.input));
            } else if current == '\\' {
                state.advance(self.input);
                if !state.is_at_end() {
                    let escaped = state.advance(self.input);
                    content.push(self.process_escape_sequence(escaped));
                }
            } else {
                content.push(state.advance(self.input));
            }
        }
        if brace_depth != 0 {
            self.error_manager.add_lexical_error(
                LexicalErrorType::UnterminatedString,
                "Unmatched braces in interpolated string".to_string(),
                start_line, start_column, None, None,
            );
            if self.should_terminate() {
                return Err(format!("Unmatched braces at line {}, col {}", start_line, start_column));
            }
        }
        Ok(Some(Token::new(
            TokenType::InterpolatedString(content), start_line, start_column, self.current_section,
        )))
    }

    #[inline]
    fn process_escape_sequence(&self, escaped: char) -> char {
        match escaped {
            'n'  => '\n', 't' => '\t', 'r' => '\r',
            '\\' => '\\', '"' => '"',  '\'' => '\'',
            '{'  => '{',  '}' => '}',  '0'  => '\0',
            _    => escaped,
        }
    }
}

// ── Numeric scanning ──────────────────────────────────────────────────────────

impl<'src> Tokenizer<'src> {

    // ── Decimal / float / date / timestamp ───────────────────────────────────

    fn scan_numeric_literal(&self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_column = state.column;
        let start_line   = state.line;
        let start_pos    = state.position;
        let bytes        = self.input.as_bytes();

        let mut has_dot            = false;
        let mut has_exponent       = false;
        let mut is_negative        = false;
        let mut dash_count         = 0u8;
        let mut colon_count        = 0u8;
        let mut is_date            = false;
        let mut is_timestamp       = false;
        let mut in_timezone_offset = false;
        let mut has_underscore     = false;

        if state.peek(self.input) == '-' { state.advance(self.input); is_negative = true; }

        while !state.is_at_end() {
            let b = bytes[state.position];

            if DIGIT[b as usize] {
                state.advance(self.input);
            }
            // ── Underscore separator — only in pure numeric (not date/timestamp)
            else if b == b'_' && !is_date && !is_timestamp {
                has_underscore = true;
                state.advance(self.input);
            }
            else if b == b'.' && !has_dot && !has_exponent && !is_date {
                let next_pos = state.position + 1;
                if next_pos < bytes.len() && DIGIT[bytes[next_pos] as usize] {
                    has_dot = true; state.advance(self.input);
                } else { break; }
            }
            else if (b == b'e' || b == b'E') && !has_exponent && !is_date {
                has_exponent = true;
                state.advance(self.input);
                if !state.is_at_end() {
                    let nb = bytes[state.position];
                    if nb == b'+' || nb == b'-' { state.advance(self.input); }
                }
            }
            else if b == b'-' && dash_count < 2 && !is_negative {
                dash_count += 1; is_date = true; state.advance(self.input);
            }
            else if b == b'T' && is_date {
                is_timestamp = true; state.advance(self.input);
            }
            else if b == b':' && is_timestamp {
                if in_timezone_offset && colon_count >= 2 { state.advance(self.input); }
                else if colon_count < 2 { colon_count += 1; state.advance(self.input); }
                else { break; }
            }
            else if b == b'.' && is_timestamp {
                state.advance(self.input);
            }
            else if b == b'Z' && is_timestamp {
                state.advance(self.input); break;
            }
            else if (b == b'+' || b == b'-') && is_timestamp {
                let scanned = state.slice(self.input, start_pos, state.position - start_pos);
                if scanned.contains('T') {
                    in_timezone_offset = true; state.advance(self.input);
                } else { break; }
            }
            else { break; }
        }

        // Extract raw slice, then strip underscores if any were present
        let raw = state.slice(self.input, start_pos, state.position - start_pos);
        let number_string: String = if has_underscore {
            raw.chars().filter(|&c| c != '_').collect()
        } else {
            raw.to_string()
        };

        if is_timestamp {
            return Ok(Some(Token::new(
                TokenType::Timestamp(number_string), start_line, start_column, self.current_section,
            )));
        }
        if is_date {
            return Ok(Some(Token::new(
                TokenType::Date(number_string), start_line, start_column, self.current_section,
            )));
        }

        self.create_numeric_token(&number_string, has_dot, has_exponent, state, start_line, start_column)
    }

    fn create_numeric_token(
        &self,
        number_string: &str,
        has_dot:       bool,
        has_exponent:  bool,
        state:         &mut TokenizerState,
        start_line:    usize,
        start_column:  usize,
    ) -> Result<Option<Token>, String> {
        // ── Type suffix — check before consuming anything irreversibly ─────────
        // L / l  → Long (only valid on integer literals, not float/double)
        // f / F  → Float
        // Mutually exclusive; L on a float literal is a lex error.
        //
        // LOOKAHEAD GUARD: only treat L/l as Long suffix when the character
        // immediately after it is NOT an identifier continuation character.
        // This prevents e.g. `789Level` being tokenised as Long(789) +
        // Identifier("evel"). If L/l is followed by [A-Za-z0-9_], the L is
        // the start of a following identifier and we leave it alone.
        let mut has_float_suffix = false;
        let mut has_long_suffix  = false;

        if !state.is_at_end() {
            let nb = self.input.as_bytes()[state.position];
            match nb {
                b'L' | b'l' if !has_dot && !has_exponent => {
                    // Only consume as Long suffix if not followed by ident continuation
                    if !self.is_ident_cont_at(state.position + 1) {
                        state.advance(self.input);
                        has_long_suffix = true;
                    }
                }
                b'f' | b'F' => {
                    state.advance(self.input);
                    has_float_suffix = true;
                }
                _ => {}
            }
        }

        // ── Long suffix → parse as i64 ────────────────────────────────────────
        if has_long_suffix {
            return match number_string.parse::<i64>() {
                Ok(v)  => Ok(Some(Token::new(
                    TokenType::Long(v), start_line, start_column, self.current_section,
                ))),
                Err(_) => self.handle_invalid_number(
                    &format!("{}L", number_string), start_line, start_column,
                ),
            };
        }

        // ── Float / Double / Scientific / Integer ─────────────────────────────
        let token_type = if has_exponent {
            if has_float_suffix {
                match number_string.parse::<f32>() {
                    Ok(v)  => TokenType::Float(v),
                    Err(_) => return self.handle_invalid_number(number_string, start_line, start_column),
                }
            } else {
                match number_string.parse::<f64>() {
                    Ok(v)  => TokenType::ScientificNotation(v),
                    Err(_) => return self.handle_invalid_number(number_string, start_line, start_column),
                }
            }
        } else if has_dot {
            if has_float_suffix {
                match number_string.parse::<f32>() {
                    Ok(v)  => TokenType::Float(v),
                    Err(_) => return self.handle_invalid_number(number_string, start_line, start_column),
                }
            } else {
                match number_string.parse::<f64>() {
                    Ok(v)  => TokenType::Double(v),
                    Err(_) => return self.handle_invalid_number(number_string, start_line, start_column),
                }
            }
        } else if has_float_suffix {
            match number_string.parse::<f32>() {
                Ok(v)  => TokenType::Float(v),
                Err(_) => return self.handle_invalid_number(number_string, start_line, start_column),
            }
        } else {
            // Plain integer — try i32 first; auto-promote to i64 on overflow
            match number_string.parse::<i32>() {
                Ok(v) => TokenType::Integer(v),
                Err(_) => match number_string.parse::<i64>() {
                    Ok(v)  => TokenType::Long(v),
                    Err(_) => return self.handle_invalid_number(number_string, start_line, start_column),
                },
            }
        };

        Ok(Some(Token::new(token_type, start_line, start_column, self.current_section)))
    }

    fn handle_invalid_number(
        &self, number_string: &str, start_line: usize, start_column: usize,
    ) -> Result<Option<Token>, String> {
        self.error_manager.add_lexical_error(
            LexicalErrorType::InvalidNumericFormat,
            format!("Invalid numeric format: {}", number_string),
            start_line, start_column, None, None,
        );
        if self.should_terminate() {
            return Err(format!("Invalid number format: {}", number_string));
        }
        Ok(Some(Token::new(
            TokenType::Error(format!("Invalid number: {}", number_string)),
            start_line, start_column, self.current_section,
        )))
    }

    // ── Hex literals: 0x … ───────────────────────────────────────────────────
    // Accepts underscore separators: 0xFF_FF_FF_FF
    // Accepts L suffix (with lookahead guard): 0xDEAD_BEEFL → Long
    // Produces Integer for i32-range values, Long for overflow or explicit L.
    // Note: TokenType::HexLiteral exists in the enum but is intentionally not
    // emitted here — hex integer notation is semantically an integer value.
    // Use #RRGGBB notation for hex color values (produces TokenType::HexColor).

    fn scan_hex_literal(&self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_column = state.column;
        let start_line   = state.line;
        state.advance(self.input); // '0'
        state.advance(self.input); // 'x' / 'X'

        let hex_start = state.position;
        let bytes     = self.input.as_bytes();
        let mut has_underscore = false;

        while !state.is_at_end() {
            let b = bytes[state.position];
            if (b as u32) < 256 && HEX_DIGIT[b as usize] {
                state.advance(self.input);
            } else if b == b'_' {
                has_underscore = true;
                state.advance(self.input);
            } else {
                break;
            }
        }

        // Check L / l suffix with lookahead guard: only consume if not
        // followed by an identifier continuation character.
        let has_long_suffix = !state.is_at_end() && {
            let nb = bytes[state.position];
            if (nb == b'L' || nb == b'l') && !self.is_ident_cont_at(state.position + 1) {
                state.advance(self.input);
                true
            } else {
                false
            }
        };

        // Length of the hex digit run (L already consumed and excluded).
        let hex_digit_len = state.position - hex_start - if has_long_suffix { 1 } else { 0 };
        let raw = state.slice(self.input, hex_start, hex_digit_len);

        // Strip underscores (L/l were already excluded by the length calculation,
        // but we filter them defensively in case of any edge case).
        let hex_part: String = if has_underscore {
            raw.chars().filter(|&c| c != '_' && c != 'L' && c != 'l').collect()
        } else {
            raw.chars().filter(|&c| c != 'L' && c != 'l').collect()
        };

        if hex_part.is_empty() {
            self.error_manager.add_lexical_error(
                LexicalErrorType::InvalidNumericFormat,
                "Hex literal '0x' has no digits".to_string(),
                start_line, start_column, None, None,
            );
            if self.should_terminate() {
                return Err(format!("Empty hex literal at line {}, col {}", start_line, start_column));
            }
            return Ok(Some(Token::new(
                TokenType::Error("Empty hex literal".to_string()),
                start_line, start_column, self.current_section,
            )));
        }

        if has_long_suffix {
            return match i64::from_str_radix(&hex_part, 16) {
                Ok(v) => Ok(Some(Token::new(
                    TokenType::Long(v), start_line, start_column, self.current_section,
                ))),
                Err(_) => {
                    self.error_manager.add_lexical_error(
                        LexicalErrorType::InvalidNumericFormat,
                        format!("Invalid hex literal: 0x{}", hex_part),
                        start_line, start_column, None, None,
                    );
                    if self.should_terminate() {
                        return Err(format!("Invalid hex literal: 0x{}", hex_part));
                    }
                    Ok(Some(Token::new(
                        TokenType::Error(format!("Invalid hex: 0x{}", hex_part)),
                        start_line, start_column, self.current_section,
                    )))
                }
            };
        }

        // Try i32 first, auto-promote to i64 on overflow
        if let Ok(v) = i32::from_str_radix(&hex_part, 16) {
            return Ok(Some(Token::new(
                TokenType::Integer(v), start_line, start_column, self.current_section,
            )));
        }
        if let Ok(v) = i64::from_str_radix(&hex_part, 16) {
            return Ok(Some(Token::new(
                TokenType::Long(v), start_line, start_column, self.current_section,
            )));
        }

        self.error_manager.add_lexical_error(
            LexicalErrorType::InvalidNumericFormat,
            format!("Invalid hex literal: 0x{}", hex_part),
            start_line, start_column, None, None,
        );
        if self.should_terminate() {
            return Err(format!("Invalid hex literal: 0x{}", hex_part));
        }
        Ok(Some(Token::new(
            TokenType::Error(format!("Invalid hex: 0x{}", hex_part)),
            start_line, start_column, self.current_section,
        )))
    }

    // ── Binary literals: 0b … ────────────────────────────────────────────────
    // Digits: 0 and 1 only. Accepts underscore separators. Accepts L suffix
    // with lookahead guard. Fits i32 → Integer. Overflows i32 → Long.
    //   0b1010           → Integer(10)
    //   0b1111_0000      → Integer(240)
    //   0b1_0000_0000L   → Long(256)
    //   0B101            → Integer(5)

    fn scan_binary_literal(&self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_column = state.column;
        let start_line   = state.line;
        state.advance(self.input); // '0'
        state.advance(self.input); // 'b' / 'B'

        let bin_start      = state.position;
        let bytes          = self.input.as_bytes();
        let mut has_digits = false;

        while !state.is_at_end() {
            let b = bytes[state.position];
            match b {
                b'0' | b'1' => { has_digits = true; state.advance(self.input); }
                b'_'        => { state.advance(self.input); } // separator — skip
                _           => break,
            }
        }

        if !has_digits {
            self.error_manager.add_lexical_error(
                LexicalErrorType::InvalidNumericFormat,
                "Binary literal '0b' has no digits".to_string(),
                start_line, start_column, None, None,
            );
            if self.should_terminate() {
                return Err(format!("Empty binary literal at line {}, col {}", start_line, start_column));
            }
            return Ok(Some(Token::new(
                TokenType::Error("Empty binary literal".to_string()),
                start_line, start_column, self.current_section,
            )));
        }

        // Check L / l suffix with lookahead guard
        let has_long_suffix = !state.is_at_end() && {
            let nb = bytes[state.position];
            if (nb == b'L' || nb == b'l') && !self.is_ident_cont_at(state.position + 1) {
                state.advance(self.input);
                true
            } else {
                false
            }
        };

        // Extract raw and strip underscores
        let bin_digit_len = state.position - bin_start - if has_long_suffix { 1 } else { 0 };
        let raw      = state.slice(self.input, bin_start, bin_digit_len);
        let clean: String = raw.chars().filter(|&c| c != '_' && c != 'L' && c != 'l').collect();

        if has_long_suffix {
            return match i64::from_str_radix(&clean, 2) {
                Ok(v) => Ok(Some(Token::new(
                    TokenType::Long(v), start_line, start_column, self.current_section,
                ))),
                Err(_) => {
                    self.error_manager.add_lexical_error(
                        LexicalErrorType::InvalidNumericFormat,
                        format!("Invalid binary literal: 0b{}", clean),
                        start_line, start_column, None, None,
                    );
                    if self.should_terminate() {
                        return Err(format!("Invalid binary literal: 0b{}", clean));
                    }
                    Ok(Some(Token::new(
                        TokenType::Error(format!("Invalid binary: 0b{}", clean)),
                        start_line, start_column, self.current_section,
                    )))
                }
            };
        }

        // No suffix — try i32, auto-promote to i64 on overflow
        match i64::from_str_radix(&clean, 2) {
            Ok(v) if v <= i32::MAX as i64 && v >= i32::MIN as i64 => Ok(Some(Token::new(
                TokenType::Integer(v as i32), start_line, start_column, self.current_section,
            ))),
            Ok(v) => Ok(Some(Token::new(
                TokenType::Long(v), start_line, start_column, self.current_section,
            ))),
            Err(_) => {
                self.error_manager.add_lexical_error(
                    LexicalErrorType::InvalidNumericFormat,
                    format!("Invalid binary literal: 0b{}", clean),
                    start_line, start_column, None, None,
                );
                if self.should_terminate() {
                    return Err(format!("Invalid binary literal: 0b{}", clean));
                }
                Ok(Some(Token::new(
                    TokenType::Error(format!("Invalid binary: 0b{}", clean)),
                    start_line, start_column, self.current_section,
                )))
            }
        }
    }

    // ── Hex color ─────────────────────────────────────────────────────────────

    fn scan_hex_color(&self, state: &mut TokenizerState) -> Token {
        let start_column = state.column;
        let start_line   = state.line;
        let start_pos    = state.position;
        state.advance(self.input); // '#'
        let bytes = self.input.as_bytes();
        while !state.is_at_end()
            && (bytes[state.position] as u32) < 256
            && HEX_DIGIT[bytes[state.position] as usize]
            && (state.position - start_pos) < 9
        {
            state.advance(self.input);
        }
        let hex_value = state.slice(self.input, start_pos, state.position - start_pos).to_string();
        Token::new(TokenType::HexColor(hex_value), start_line, start_column, self.current_section)
    }

    // ── Multi-char operators ──────────────────────────────────────────────────

    fn try_scan_multi_char_operator(&self, state: &mut TokenizerState) -> Option<Token> {
        if state.is_at_end() { return None; }
        let start_column = state.column;
        let start_line   = state.line;
        let current      = state.peek(self.input);
        let next         = state.peek_next(self.input);

        if state.position + 2 < state.input_length {
            let third = state.peek_at(self.input, 2);
            let three_char: Option<TokenType> = match (current, next, third) {
                ('*', '*', '=') => Some(TokenType::ArithmeticAssignOp("**=")),
                ('<', '<', '=') => Some(TokenType::BitwiseOp("<<=")),
                ('>', '>', '=') => Some(TokenType::BitwiseOp(">>=")),
                ('>', '_', '<') => Some(TokenType::BitwiseOp(">_<")),
                _ => None,
            };
            if let Some(tt) = three_char {
                state.advance(self.input); state.advance(self.input); state.advance(self.input);
                return Some(Token::new(tt, start_line, start_column, self.current_section));
            }
        }

        let two_char: Option<TokenType> = match (current, next) {
            ('=', '>') => Some(TokenType::Arrow),
            (':', ':') => Some(TokenType::DoubleColon),
            ('-', '>') => Some(TokenType::SwitchCase),
            ('*', '*') => Some(TokenType::ArithmeticOp("**")),
            ('%', '%') => Some(TokenType::ArithmeticOp("%%")),
            ('%', '&') => Some(TokenType::ArithmeticOp("%&")),
            ('&', '%') => Some(TokenType::ArithmeticOp("&%")),
            ('+', '+') => Some(TokenType::ArithmeticOp("++")),
            ('-', '-') => Some(TokenType::ArithmeticOp("--")),
            ('+', '=') => Some(TokenType::ArithmeticAssignOp("+=")),
            ('-', '=') => Some(TokenType::ArithmeticAssignOp("-=")),
            ('*', '=') => Some(TokenType::ArithmeticAssignOp("*=")),
            ('/', '=') => Some(TokenType::ArithmeticAssignOp("/=")),
            ('%', '=') => Some(TokenType::ArithmeticAssignOp("%=")),
            ('=', '=') => Some(TokenType::ComparisonOp("==")),
            ('!', '=') => Some(TokenType::ComparisonOp("!=")),
            ('<', '=') => Some(TokenType::ComparisonOp("<=")),
            ('>', '=') => Some(TokenType::ComparisonOp(">=")),
            ('&', '&') => Some(TokenType::LogicalOp("&&")),
            ('|', '|') => Some(TokenType::LogicalOp("||")),
            ('<', '<') => Some(TokenType::BitwiseOp("<<")),
            ('>', '>') => Some(TokenType::BitwiseOp(">>")),
            ('~', '?') => Some(TokenType::BitwiseOp("~?")),
            ('&', '=') => Some(TokenType::BitwiseOp("&=")),
            ('|', '=') => Some(TokenType::BitwiseOp("|=")),
            ('^', '=') => Some(TokenType::BitwiseOp("^=")),
            _ => None,
        };
        if let Some(tt) = two_char {
            state.advance(self.input); state.advance(self.input);
            return Some(Token::new(tt, start_line, start_column, self.current_section));
        }
        None
    }
}

// ── Identifier scanning ───────────────────────────────────────────────────────

impl<'src> Tokenizer<'src> {
    fn scan_identifier_or_keyword(&self, state: &mut TokenizerState) -> Token {
        let start_column = state.column;
        let start_line   = state.line;
        let start_pos    = state.position;
        let bytes        = self.input.as_bytes();

        loop {
            if state.is_at_end() { break; }
            let b = bytes[state.position];

            if (b as u32) < 256 && IDENT_CONT[b as usize] {
                state.advance(self.input);
            } else if b == b'-' {
                let next_pos           = state.position + 1;
                let in_expression_ctx  = matches!(self.current_section, SectionId::QuickFuncs);
                let valid_continuation = next_pos < bytes.len()
                    && (bytes[next_pos] as u32) < 256
                    && super::char_tables::ALPHA_UNDERSCORE[bytes[next_pos] as usize];

                if !in_expression_ctx && valid_continuation { state.advance(self.input); }
                else { break; }
            } else {
                break;
            }
        }

        let identifier = state.slice(self.input, start_pos, state.position - start_pos);

        if let Some(ctor) = KEYWORDS.get(identifier) {
            return Token::new(ctor(), start_line, start_column, self.current_section);
        }
        Token::new(
            TokenType::Identifier(identifier.to_string()),
            start_line, start_column, self.current_section,
        )
    }

    #[inline]
    fn is_valid_prefixed_constructor(&self, state: &TokenizerState) -> bool {
        if state.position + 1 >= state.input_length { return false; }
        matches!(state.peek(self.input), 'b' | 't' | 'r')
    }

    fn scan_prefixed_constructor(&mut self, state: &mut TokenizerState) -> Token {
        let start_column = state.column;
        let start_line   = state.line;
        let prefix       = state.advance(self.input);
        state.advance(self.input); // ':'

        let constructor_type = match prefix {
            'b' => "BLOB_CONSTRUCTOR",
            't' => "TUPLE_CONSTRUCTOR",
            'r' => "REGEX_CONSTRUCTOR",
            _   => "UNKNOWN_CONSTRUCTOR",
        };

        self.prefixed_constructors_found.push(PrefixedConstructorInfo {
            constructor_type: constructor_type.to_string(),
            prefix:           prefix.to_string(),
            line:             start_line,
            column:           start_column,
            section:          self.current_section,
        });

        let token_type = match prefix {
            'b' => TokenType::BlobConstructor("".to_string()),
            't' => TokenType::TupleConstructor("".to_string()),
            'r' => TokenType::RegexConstructor("".to_string()),
            _   => TokenType::Error(format!("Unknown constructor: {}", prefix)),
        };
        Token::new(token_type, start_line, start_column, self.current_section)
    }

    fn scan_single_character(&self, state: &mut TokenizerState) -> Result<Option<Token>, String> {
        let start_column = state.column;
        let start_line   = state.line;
        let symbol       = state.advance(self.input);

        let token_type = match symbol {
            '+' => TokenType::ArithmeticOp("+"),
            '-' => TokenType::ArithmeticOp("-"),
            '*' => TokenType::ArithmeticOp("*"),
            '/' => TokenType::ArithmeticOp("/"),
            '%' => TokenType::ArithmeticOp("%"),
            '^' => TokenType::BitwiseOp("^"),
            '&' => TokenType::BitwiseOp("&"),
            '|' => TokenType::BitwiseOp("|"),
            '<' | '>' | '=' | '!' => TokenType::Symbol(symbol),
            _ if !symbol.is_control() && !symbol.is_whitespace() => TokenType::Symbol(symbol),
            _ => {
                let msg = if self.debug_config.is_enabled {
                    format!("Unexpected character: '{}' (0x{:X})", symbol, symbol as u32)
                } else {
                    "Unexpected character".to_string()
                };
                self.error_manager.add_lexical_error(
                    LexicalErrorType::InvalidCharacter, msg.clone(),
                    start_line, start_column, None, None,
                );
                if self.should_terminate() {
                    return Err(format!("Invalid character at line {}, col {}: '{}'", start_line, start_column, symbol));
                }
                TokenType::Error(format!("Invalid character: '{}'", symbol))
            }
        };
        Ok(Some(Token::new(token_type, start_line, start_column, self.current_section)))
    }

    // ── Debug analysis ────────────────────────────────────────────────────────

    fn analyze_token_sequences(&mut self) {
        let len = self.token_pool.len();
        for i in 0..len.saturating_sub(2) {
            let (t1_section, t1_type_is_upper_ident) = {
                let t1 = &self.token_pool[i];
                let is_candidate = if let TokenType::Identifier(id) = &t1.token_type {
                    self.could_be_static_object(id)
                } else { false };
                (t1.section, is_candidate)
            };
            if !t1_type_is_upper_ident { continue; }
            let is_dot = matches!(self.token_pool[i + 1].token_type, TokenType::Symbol('.'));
            if !is_dot { continue; }
            if let TokenType::Identifier(method_name) = &self.token_pool[i + 2].token_type {
                let object_name = if let TokenType::Identifier(n) = &self.token_pool[i].token_type {
                    n.clone()
                } else { continue };
                self.static_calls_found.push(StaticCallInfo {
                    object_name, method_name: method_name.clone(),
                    line: self.token_pool[i].line, column: self.token_pool[i].column,
                    section: t1_section, token_index: i,
                });
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
        let potential_builtin_calls = if self.debug_config.is_testing {
            self.analyze_potential_builtin_calls()
        } else { 0 };

        TokenizationMetadata {
            version:                     "1.0.0".to_string(),
            total_lines:                 self.token_pool.last().map(|t| t.line).unwrap_or(1),
            total_tokens:                self.token_pool.len().saturating_sub(1),
            sections_detected,
            prefixed_constructors_found: self.prefixed_constructors_found.len(),
            blob_constructors:           self.prefixed_constructors_found.iter()
                                             .filter(|p| p.constructor_type == "BLOB_CONSTRUCTOR").count(),
            tuple_constructors:          self.prefixed_constructors_found.iter()
                                             .filter(|p| p.constructor_type == "TUPLE_CONSTRUCTOR").count(),
            regex_constructors:          self.prefixed_constructors_found.iter()
                                             .filter(|p| p.constructor_type == "REGEX_CONSTRUCTOR").count(),
            static_calls_found:          self.static_calls_found.len(),
            potential_builtin_calls,
        }
    }

    fn get_sections_from_tokens(&self) -> Vec<String> {
        let mut sections: Vec<String> = Vec::new();
        for token in &self.token_pool {
            if let Some(s) = token.token_type.get_section_context() {
                let owned = s.to_string();
                if !sections.contains(&owned) { sections.push(owned); }
            }
        }
        sections
    }

    fn analyze_potential_builtin_calls(&self) -> usize {
        let len   = self.token_pool.len();
        let mut n = 0usize;
        for i in 0..len.saturating_sub(3) {
            if let (
                TokenType::Symbol('.'),
                TokenType::Identifier(_),
                TokenType::Symbol('('),
            ) = (
                &self.token_pool[i + 1].token_type,
                &self.token_pool[i + 2].token_type,
                &self.token_pool[i + 3].token_type,
            ) { n += 1; }
        }
        n
    }
}

// ── Public result types ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TokenizationResult {
    pub tokens:                Vec<Token>,
    pub metadata:              TokenizationMetadata,
    pub prefixed_constructors: Vec<PrefixedConstructorInfo>,
    pub static_calls:          Vec<StaticCallInfo>,
}

#[derive(Debug, Clone)]
pub struct TokenizationMetadata {
    pub version:                     String,
    pub total_lines:                 usize,
    pub total_tokens:                usize,
    pub sections_detected:           Vec<String>,
    pub prefixed_constructors_found: usize,
    pub blob_constructors:           usize,
    pub tuple_constructors:          usize,
    pub regex_constructors:          usize,
    pub static_calls_found:          usize,
    pub potential_builtin_calls:     usize,
}

#[derive(Debug, Clone)]
pub struct PrefixedConstructorInfo {
    pub constructor_type: String,
    pub prefix:           String,
    pub line:             usize,
    pub column:           usize,
    pub section:          SectionId,
}

#[derive(Debug, Clone)]
pub struct StaticCallInfo {
    pub object_name:  String,
    pub method_name:  String,
    pub line:         usize,
    pub column:       usize,
    pub section:      SectionId,
    pub token_index:  usize,
    }
