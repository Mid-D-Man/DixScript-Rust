//! Parser for the `@ENUMS(...)` section.
//!
//! ```text
//! EnumsSection    ::= "@ENUMS(" EnumDeclaration+ ")"
//! EnumDeclaration ::= Identifier "{" EnumFieldList "}"
//! EnumFieldList   ::= EnumField (","? EnumField)*
//! EnumField       ::= Identifier ("=" Integer)?
//! ```
//!
//! Commas are optional between fields, and are NOT valid between declarations.

use crate::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField, Position};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, ParseErrorType,DebugConfig};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;
use crate::Utilities::{Keywords, estimate_enum_fields_count};

const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 500_000;
const MAX_STUCK_COUNT: usize = 3;

pub struct EnumsSectionParser<'a> {
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    position: usize,
    last_position: usize,
    stuck_count: usize,
    iteration_count: usize,
    has_encountered_errors: bool,
}

impl<'a> EnumsSectionParser<'a> {
    pub fn new(tokens: &'a [Token], operational_settings: &'a OperationalSettings) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

        if debug_config.is_enabled {
            error_manager.log_debug(&format!(
                "ENUMS parser: {} tokens, strategy: {:?}",
                tokens.len(),
                operational_settings.error_handling_strategy
            ));
        }

        EnumsSectionParser {
            tokens,
            operational_settings,
            error_manager,
            debug_config,
            position: 0,
            last_position: usize::MAX,
            stuck_count: 0,
            iteration_count: 0,
            has_encountered_errors: false,
        }
    }

    pub fn parse_section(&mut self) -> Option<EnumsSection> {
        let section_start_pos = Position::from_token(self.current());
        self.reset_parse_state();

        let mut enum_declarations = Vec::with_capacity(usize::max(2, self.tokens.len() / 20));

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.report_error(ParseErrorType::MissingToken, "Expected '(' to start ENUMS section", &current);
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos);
            }
            if !self.recover_to_symbol('(', 10) {
                return self.partial_or_none(section_start_pos);
            }
        }

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if !self.force_advance() {
                    break;
                }
                continue;
            }

            if self.is_current_symbol(',') {
                let current = self.current().clone();
                self.report_error(
                    ParseErrorType::SectionSyntaxError,
                    "Commas are not allowed between enum declarations",
                    &current,
                );
                self.advance();
                continue;
            }

            match self.parse_enum_declaration() {
                Some(decl) => {
                    if self.debug_config.is_enabled {
                        self.error_manager.log_debug(&format!(
                            "ENUMS: parsed '{}' with {} fields",
                            decl.name, decl.fields.len()
                        ));
                    }
                    enum_declarations.push(decl);
                }
                None => {
                    if self.should_halt_section() {
                        return self.partial_or_none(section_start_pos);
                    }
                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.recover_to_next_enum() {
                            self.ensure_progress();
                        }
                    } else {
                        self.ensure_progress();
                    }
                }
            }
        }

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.report_error(ParseErrorType::MissingToken, "Expected ')' to close ENUMS section", &current);
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos);
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "ENUMS section done: {} declarations, errors: {}",
                enum_declarations.len(),
                self.has_encountered_errors
            ));
        }

        Some(EnumsSection::new(enum_declarations, section_start_pos))
    }

    fn parse_enum_declaration(&mut self) -> Option<EnumDeclaration> {
        let enum_start_pos = Position::from_token(self.current());
        let enum_name = self.parse_enum_identifier("Expected enum name identifier", "Cannot use reserved keyword '{}' as enum name")?;

        if !self.match_and_consume_symbol('{') {
            let current = self.current().clone();
            let msg = format!("Expected '{{' after enum name '{}'", enum_name);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            if self.should_halt_section() {
                return None;
            }
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.recover_to_symbol('{', 10) {
                    return None;
                }
            } else {
                return None;
            }
        }

        let mut fields = Vec::with_capacity(estimate_enum_fields_count(
            self.tokens.len().saturating_sub(self.position),
        ));
        let mut expecting_field = true;

        while !self.is_at_end() && !self.is_current_symbol('}') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if !self.force_advance() {
                    break;
                }
                continue;
            }

            match self.parse_enum_field() {
                Some(field) => {
                    if self.debug_config.is_verbose {
                        self.error_manager.log_debug(&format!(
                            "  field: {}{}",
                            field.name,
                            field.value.map(|v| format!(" = {}", v)).unwrap_or_default()
                        ));
                    }
                    fields.push(field);
                    expecting_field = false;
                }
                None => {
                    if self.should_halt_section() {
                        return None;
                    }
                }
            }

            if self.is_current_symbol(',') {
                self.advance();
                expecting_field = true;
            } else if self.is_current_symbol('}') {
                break;
            } else if !self.is_at_end() && !expecting_field {
                // Detect missing `}` when the next token looks like a new enum declaration.
                if matches!(self.current().token_type, TokenType::Identifier(_)) {
                    if let Some(next) = self.peek() {
                        if matches!(next.token_type, TokenType::Symbol('{')) {
                            let current = self.current().clone();
                            let msg = format!("Missing '}}' to close enum '{}' — found start of next enum", enum_name);
                            self.report_error(ParseErrorType::MissingToken, &msg, &current);
                            break;
                        }
                    }
                }

                let current = self.current().clone();
                let msg = format!(
                    "Expected ',' or '}}' after field in '{}', found {}",
                    enum_name,
                    current.get_token_value()
                );
                self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                if self.should_halt_section() {
                    return None;
                }
                self.ensure_progress();
            }
        }

        if !self.match_and_consume_symbol('}') {
            let current = self.current().clone();
            let msg = format!("Expected '}}' to close enum '{}'", enum_name);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            if self.should_halt_section() {
                return None;
            }
        }

        Some(EnumDeclaration::new(enum_name, fields, enum_start_pos))
    }

    fn parse_enum_field(&mut self) -> Option<EnumField> {
        let field_start_pos = Position::from_token(self.current());
        let field_name = self.parse_enum_identifier(
            "Expected enum field name identifier",
            "Cannot use reserved keyword '{}' as field name",
        )?;

        if !self.is_current_symbol('=') {
            return Some(EnumField::new(field_name, None, field_start_pos));
        }
        self.advance();

        match self.parse_field_value() {
            Some(value) => Some(EnumField::new(field_name, Some(value), field_start_pos)),
            None => {
                let current = self.current().clone();
                let msg = format!("Expected integer value after '=' in field '{}'", field_name);
                self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                if self.should_halt_section() {
                    None
                } else {
                    Some(EnumField::new(field_name, None, field_start_pos))
                }
            }
        }
    }

    /// Parse an identifier or allowable keyword in the ENUMS context.
    /// `missing_msg` is shown when no identifier is present at all.
    /// `keyword_msg` is a format template with `{}` for the keyword when a reserved word is used.
    fn parse_enum_identifier(&mut self, missing_msg: &str, keyword_msg: &str) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                Some(name)
            }
            TokenType::Keyword(k) => {
                if Keywords::can_be_identifier_in_context(k, "ENUMS") {
                    let name = k.to_string();
                    self.advance();
                    Some(name)
                } else {
                    let current = self.current().clone();
                    let msg = keyword_msg.replacen("{}", k, 1);
                    self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                    None
                }
            }
            _ => {
                let current = self.current().clone();
                self.report_error(ParseErrorType::UnexpectedToken, missing_msg, &current);
                None
            }
        }
    }

    fn parse_field_value(&mut self) -> Option<i32> {
        match &self.current().token_type {
            TokenType::Integer(v) => {
                let val = *v;
                self.advance();
                Some(val)
            }
            TokenType::Identifier(id) => {
                if let Ok(val) = id.parse::<i32>() {
                    self.advance();
                    Some(val)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn report_error(&mut self, error_type: ParseErrorType, message: &str, token: &Token) {
        self.has_encountered_errors = true;
        let source_line = self.reconstruct_source_line(token);
        self.error_manager.add_parse_error(
            error_type,
            message.to_string(),
            token.line,
            token.column,
            None,
            source_line,
        );
    }

    #[inline]
    fn should_halt_section(&self) -> bool {
        self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
            && self.has_encountered_errors
    }

    fn partial_or_none(&self, start_pos: Position) -> Option<EnumsSection> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            None
        } else {
            Some(EnumsSection::new(Vec::new(), start_pos))
        }
    }

    fn recover_to_symbol(&mut self, symbol: char, max_steps: usize) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }
        for _ in 0..max_steps {
            if self.is_at_end() {
                return false;
            }
            if self.is_current_symbol(symbol) {
                self.advance();
                return true;
            }
            self.advance();
        }
        false
    }

    fn recover_to_next_enum(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }
        for _ in 0..50 {
            if self.is_at_end() || self.is_current_symbol('}') || self.is_current_symbol(')') {
                return true;
            }
            if matches!(self.current().token_type, TokenType::Identifier(_)) {
                if let Some(next) = self.peek() {
                    if matches!(next.token_type, TokenType::Symbol('{')) {
                        return true;
                    }
                }
            }
            self.advance();
        }
        false
    }

    #[inline]
    fn current(&self) -> &Token {
        static EOF: Token = Token {
            token_type: TokenType::EndOfFile,
            line: 1,
            column: 1,
            section: SectionId::None,
        };
        self.tokens.get(self.position).unwrap_or(&EOF)
    }

    #[inline]
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position + 1)
    }

    #[inline]
    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len()
            || matches!(self.current().token_type, TokenType::EndOfFile)
    }

    #[inline]
    fn advance(&mut self) {
        if self.position < self.tokens.len() {
            self.position += 1;
        }
    }

    #[inline]
    fn is_current_symbol(&self, symbol: char) -> bool {
        matches!(&self.current().token_type, TokenType::Symbol(s) if *s == symbol)
    }

    #[inline]
    fn match_and_consume_symbol(&mut self, symbol: char) -> bool {
        if self.is_current_symbol(symbol) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn reconstruct_source_line(&self, token: &Token) -> Option<String> {
        let mut source = String::new();
        let mut col = 0usize;
        for t in self.tokens.iter().filter(|t| t.line == token.line) {
            while col < t.column {
                source.push(' ');
                col += 1;
            }
            let v = t.get_token_value();
            col += v.len();
            source.push_str(&v);
        }
        if source.is_empty() { None } else { Some(source) }
    }

    fn reset_parse_state(&mut self) {
        self.last_position = usize::MAX;
        self.stuck_count = 0;
        self.iteration_count = 0;
        self.has_encountered_errors = false;
    }

    fn track_progress(&mut self) {
        self.iteration_count += 1;
        if self.position == self.last_position {
            self.stuck_count += 1;
        } else {
            self.last_position = self.position;
            self.stuck_count = 0;
        }
    }

    #[inline]
    fn is_stuck(&self) -> bool {
        self.stuck_count >= MAX_STUCK_COUNT
    }

    fn should_terminate_loop(&self) -> bool {
        let limit = (self.tokens.len() * MAX_ITERATIONS_PER_TOKEN).min(ABSOLUTE_MAX_ITERATIONS);
        if self.iteration_count >= limit {
            self.error_manager.log_error(&format!(
                "ENUMS parser exceeded {} iterations — possible infinite loop",
                limit
            ));
            return true;
        }
        false
    }

    fn force_advance(&mut self) -> bool {
        if self.is_at_end() {
            return false;
        }
        self.advance();
        self.stuck_count = 0;
        true
    }

    #[inline]
    fn ensure_progress(&mut self) {
        if !self.is_at_end() {
            self.advance();
        }
    }
}