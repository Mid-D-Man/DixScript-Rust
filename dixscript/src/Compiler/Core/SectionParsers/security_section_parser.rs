//! Parser for the `@SECURITY(...)` section.
//!
//! ```text
//! SecuritySection   ::= "@SECURITY(" SecurityEntry (","? SecurityEntry)* ")"
//! SecurityEntry     ::= SecurityBlockKey "->" "{" SecurityFieldList? "}"
//! SecurityBlockKey  ::= "encryption" | "validation" | "keystore" | "override" | "metadata"
//! SecurityFieldList ::= SecurityField ("," SecurityField)*
//! SecurityField     ::= Identifier "=" SecurityValue
//! SecurityValue     ::= StringLiteral | Integer | Boolean | HexLiteral | "auto"
//! ```
//!
//! Commas between SecurityEntry blocks are optional.
//! Commas between SecurityField entries within a block are required.

use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Position, Value};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, ParseErrorType, DebugConfig};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;

const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 500_000;
const MAX_STUCK_COUNT: usize = 3;

pub struct SecuritySectionParser<'a> {
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
    position: usize,
    last_position: usize,
    stuck_count: usize,
    iteration_count: usize,
    max_iterations: usize,
    has_encountered_errors: bool,
}

impl<'a> SecuritySectionParser<'a> {
    pub fn new(tokens: &'a [Token], operational_settings: &'a OperationalSettings) -> Self {
        Self::new_with_error_manager(tokens,operational_settings,ErrorManager::get_shared_instance())

    }

    pub fn new_with_error_manager(
        tokens: &'a [Token],
        operational_settings: &'a OperationalSettings,
        error_manager: ErrorManager,
    ) -> Self {

        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

        let dynamic_limit = tokens.len() * MAX_ITERATIONS_PER_TOKEN;
        let max_iterations = dynamic_limit.min(ABSOLUTE_MAX_ITERATIONS);

        if debug_config.is_enabled {
            error_manager.log_debug(&format!(
                "SECURITY parser: {} tokens, strategy: {:?}",
                tokens.len(),
                operational_settings.error_handling_strategy
            ));
        }

        SecuritySectionParser {
            tokens,
            operational_settings,
            error_manager,
            debug_config,
            position: 0,
            last_position: usize::MAX,
            stuck_count: 0,
            iteration_count: 0,
            max_iterations,
            has_encountered_errors: false,
        }
    }

    pub fn parse_section(&mut self) -> Option<SecuritySection> {
        let section_start_pos = Position::from_token(self.current());
        self.reset_parse_state();

        let mut security_entries = Vec::with_capacity(4);

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::MissingToken,
                "Expected '(' to start SECURITY section",
                &current,
            );
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos);
            }
            if !self.recover_to_symbol('(', 10) {
                return self.partial_or_none(section_start_pos);
            }
        }

        if self.is_current_symbol(')') {
            self.advance();
            return Some(SecuritySection::new(security_entries, section_start_pos));
        }

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if !self.force_advance() {
                    break;
                }
                continue;
            }

            // Skip optional commas between entries at the outer level.
            if self.is_current_symbol(',') {
                self.advance();
                continue;
            }

            match self.parse_security_entry() {
                Some(entry) => {
                    if self.debug_config.is_enabled {
                        self.error_manager.log_debug(&format!(
                            "SECURITY: parsed entry '{}'",
                            entry.block_key
                        ));
                    }
                    security_entries.push(entry);
                }
                None => {
                    if self.should_halt_section() {
                        return self.partial_or_none(section_start_pos);
                    }
                    if self.operational_settings.error_handling_strategy
                        == ErrorHandlingStrategy::Recover
                    {
                        if !self.recover_to_entry_boundary() {
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
            self.report_error(
                ParseErrorType::MissingToken,
                "Expected ')' to close SECURITY section",
                &current,
            );
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos);
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "SECURITY section done: {} entries, errors: {}",
                security_entries.len(),
                self.has_encountered_errors
            ));
        }

        Some(SecuritySection::new(security_entries, section_start_pos))
    }

    fn parse_security_entry(&mut self) -> Option<SecurityEntry> {
        let entry_start_pos = Position::from_token(self.current());

        let block_key = self.parse_identifier_or_keyword(
            "Expected security block key (encryption, validation, keystore, override, metadata)",
        )?;

        if !self.match_arrow() {
            let current = self.current().clone();
            let msg = format!(
                "Expected '->' after security block key '{}', found {}",
                block_key,
                current.get_token_value()
            );
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            if self.should_halt_section() {
                return None;
            }
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.recover_to_arrow() {
                    return None;
                }
            } else {
                return None;
            }
        }

        if !self.match_and_consume_symbol('{') {
            let current = self.current().clone();
            let msg = format!("Expected '{{' in security entry '{}'", block_key);
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

        let mut fields = Vec::with_capacity(4);
        let mut need_comma = false;

        while !self.is_at_end() && !self.is_current_symbol('}') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if !self.force_advance() {
                    break;
                }
                continue;
            }

            // After the first field, a comma is required before the next field.
            if need_comma {
                if self.is_current_symbol(',') {
                    self.advance();
                    // Allow trailing comma before '}'.
                    if self.is_current_symbol('}') {
                        break;
                    }
                } else if !self.is_current_symbol('}') {
                    // Missing comma between fields — report but recover if possible.
                    let current = self.current().clone();
                    let msg = format!(
                        "Expected ',' between fields in security block '{}', found {}",
                        block_key,
                        current.get_token_value()
                    );
                    self.report_error(ParseErrorType::MissingToken, &msg, &current);
                    if self.should_halt_section() {
                        return None;
                    }
                    // Recover: if the current token looks like a field key, continue anyway.
                    if !matches!(
                        self.current().token_type,
                        TokenType::Identifier(_) | TokenType::Keyword(_)
                    ) {
                        if self.operational_settings.error_handling_strategy
                            == ErrorHandlingStrategy::Recover
                        {
                            if !self.recover_in_fields() {
                                self.ensure_progress();
                            }
                        } else {
                            self.ensure_progress();
                        }
                        continue;
                    }
                }
            }

            match self.parse_security_field() {
                Some(field) => {
                    if self.debug_config.is_verbose {
                        self.error_manager
                            .log_debug(&format!("  SECURITY field: {}", field.key));
                    }
                    fields.push(field);
                    need_comma = true;
                }
                None => {
                    if self.should_halt_section() {
                        return None;
                    }
                    if self.operational_settings.error_handling_strategy
                        == ErrorHandlingStrategy::Recover
                    {
                        if !self.recover_in_fields() {
                            self.ensure_progress();
                        }
                    } else {
                        self.ensure_progress();
                    }
                }
            }
        }

        if !self.match_and_consume_symbol('}') {
            let current = self.current().clone();
            let msg = format!("Expected '}}' to close security block '{}'", block_key);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            if self.should_halt_section() {
                return None;
            }
        }

        Some(SecurityEntry::new(block_key, fields, entry_start_pos))
    }

    fn parse_security_field(&mut self) -> Option<SecurityField> {
        let field_start_pos = Position::from_token(self.current());

        let key =
            self.parse_identifier_or_keyword("Expected field key identifier in security block")?;

        if !self.match_and_consume_symbol('=') {
            let current = self.current().clone();
            let msg = format!("Expected '=' after field key '{}'", key);
            self.report_error(ParseErrorType::MissingToken, &msg, &current);
            if self.should_halt_section() {
                return None;
            }
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.recover_to_symbol('=', 10) {
                    return None;
                }
            } else {
                return None;
            }
        }

        let value = match self.parse_security_value() {
            Some(v) => v,
            None => {
                let current = self.current().clone();
                let msg = format!(
                    "Expected value for field '{}', found {}",
                    key,
                    current.get_token_value()
                );
                self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                if self.should_halt_section() {
                    return None;
                }
                Value::Error {
                    message: format!("Missing value for key '{}'", key),
                    position: field_start_pos,
                }
            }
        };

        Some(SecurityField::new(key, value, field_start_pos))
    }

    fn parse_identifier_or_keyword(&mut self, context: &str) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let key = id.clone();
                self.advance();
                Some(key)
            }
            TokenType::Keyword(k) => {
                let key = k.to_string();
                self.advance();
                Some(key)
            }
            _ => {
                let current = self.current().clone();
                self.report_error(ParseErrorType::UnexpectedToken, context, &current);
                None
            }
        }
    }

    fn parse_security_value(&mut self) -> Option<Value> {
        let pos = Position::from_token(self.current());

        let value = match &self.current().token_type {
            TokenType::String(s) => Some(Value::String {
                value: s.clone(),
                position: pos,
            }),
            TokenType::StringSingle(s) => Some(Value::String {
                value: s.clone(),
                position: pos,
            }),
            TokenType::Integer(i) => Some(Value::Integer {
                value: *i,
                position: pos,
            }),
            TokenType::Long(i) => Some(Value::Long {
                value: *i,
                position: pos,
            }),
            TokenType::Float(fl) => Some(Value::Float {
                value: *fl,
                position: pos,
            }),
            TokenType::Double(d) => Some(Value::Double {
                value: *d,
                position: pos,
            }),
            TokenType::Bool(b) => Some(Value::Boolean {
                value: *b,
                position: pos,
            }),
            TokenType::HexLiteral(h) => Some(Value::Integer {
                value: *h,
                position: pos,
            }),
            TokenType::HexColor(h) => Some(Value::HexColor {
                value: h.clone(),
                position: pos,
            }),
            TokenType::Identifier(id) if id.eq_ignore_ascii_case("auto") => {
                Some(Value::String {
                    value: "auto".to_string(),
                    position: pos,
                })
            }
            TokenType::Keyword(k) if *k == "true" => Some(Value::Boolean {
                value: true,
                position: pos,
            }),
            TokenType::Keyword(k) if *k == "false" => Some(Value::Boolean {
                value: false,
                position: pos,
            }),
            TokenType::Keyword(k) if *k == "null" => Some(Value::Null { position: pos }),
            _ => None,
        };

        if value.is_some() {
            self.advance();
        }

        value
    }

    /// Consume a `->` token (`TokenType::SwitchCase`), with a two-symbol fallback.
    #[inline]
    fn match_arrow(&mut self) -> bool {
        if matches!(self.current().token_type, TokenType::SwitchCase) {
            self.advance();
            return true;
        }
        if matches!(self.current().token_type, TokenType::Symbol('-'))
            && self.position + 1 < self.tokens.len()
            && matches!(self.tokens[self.position + 1].token_type, TokenType::Symbol('>'))
        {
            self.advance();
            self.advance();
            return true;
        }
        false
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

    fn partial_or_none(&self, start_pos: Position) -> Option<SecuritySection> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            None
        } else {
            Some(SecuritySection::new(Vec::new(), start_pos))
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

    fn recover_to_arrow(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }
        for _ in 0..10 {
            if self.is_at_end() {
                return false;
            }
            if self.match_arrow() {
                return true;
            }
            self.advance();
        }
        false
    }

    fn recover_to_entry_boundary(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }
        for _ in 0..50 {
            if self.is_at_end() || self.is_current_symbol('}') || self.is_current_symbol(')') {
                return true;
            }
            self.advance();
        }
        false
    }

    fn recover_in_fields(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }
        for _ in 0..50 {
            if self.is_at_end() || self.is_current_symbol(',') || self.is_current_symbol('}') {
                return true;
            }
            self.advance();
        }
        false
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
        if self.iteration_count >= self.max_iterations {
            self.error_manager.log_error(&format!(
                "SECURITY parser exceeded {} iterations — possible infinite loop",
                self.max_iterations
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
