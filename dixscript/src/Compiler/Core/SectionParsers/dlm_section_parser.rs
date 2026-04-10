//! Parser for the `@DLM(...)` section.
//!
//! ```text
//! DLMSection  ::= "@DLM(" DLMList? ")"
//! DLMList     ::= DLMModule (","? DLMModule)*
//! DLMModule   ::= ModuleType ("." ModuleSubtype)?
//! ModuleType  ::= "DCompressor" | "DAuditor" | "DEncryptor"
//! ModuleSubtype ::= "gzip" | "bzip2" | "lzma"
//!                 | "diy" | "enhanced"
//!                 | "xor" | "aes128" | "aes256" | "chacha20"
//! ```
//!
//! Commas between modules are optional.

use crate::Compiler::AST::{DLMSection, DLMModule, Position, DLMModuleType, DLMModuleSubtype};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, ParseErrorType, DebugConfig};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;

const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 500_000;
const MAX_STUCK_COUNT: usize = 3;

pub struct DlmSectionParser<'a> {
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

impl<'a> DlmSectionParser<'a> {
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
                "DLM parser: {} tokens, strategy: {:?}",
                tokens.len(),
                operational_settings.error_handling_strategy
            ));
        }

        DlmSectionParser {
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

    pub fn parse_section(&mut self) -> Option<DLMSection> {
        let section_start_pos = Position::from_token(self.current());
        self.reset_parse_state();

        let mut modules = Vec::with_capacity(usize::max(2, self.tokens.len() / 10));

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.report_error(ParseErrorType::MissingToken, "Expected '(' to start DLM section", &current);
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos);
            }
            if !self.recover_to_symbol('(', 10) {
                return self.partial_or_none(section_start_pos);
            }
        }

        if self.is_current_symbol(')') {
            self.advance();
            return Some(DLMSection::new(modules, section_start_pos));
        }

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if !self.force_advance() {
                    break;
                }
                continue;
            }

            match self.parse_dlm_module() {
                Some(module) => {
                    if self.debug_config.is_enabled {
                        self.error_manager.log_debug(&format!("DLM: parsed module '{}'", module));
                    }
                    modules.push(module);
                }
                None => {
                    if self.should_halt_section() {
                        return self.partial_or_none(section_start_pos);
                    }
                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.recover_to_next_module() {
                            self.ensure_progress();
                        }
                    } else {
                        self.ensure_progress();
                    }
                }
            }

            // Commas between modules are optional.
            if self.is_current_symbol(',') {
                self.advance();
            } else if self.is_current_symbol(')') {
                break;
            } else if !self.is_at_end() && !self.could_be_module_type() {
                let current = self.current().clone();
                let msg = format!(
                    "Expected ',' or ')' after DLM module, found {}",
                    current.get_token_value()
                );
                self.report_error(ParseErrorType::UnexpectedToken, &msg, &current);
                if self.should_halt_section() {
                    return self.partial_or_none(section_start_pos);
                }
                if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                    if !self.recover_to_next_module() {
                        self.ensure_progress();
                    }
                } else {
                    self.ensure_progress();
                }
            }
        }

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.report_error(ParseErrorType::MissingToken, "Expected ')' to close DLM section", &current);
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos);
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "DLM section done: {} modules, errors: {}",
                modules.len(),
                self.has_encountered_errors
            ));
        }

        Some(DLMSection::new(modules, section_start_pos))
    }

    fn parse_dlm_module(&mut self) -> Option<DLMModule> {
        let module_start_pos = Position::from_token(self.current());

        let type_name = self.parse_identifier_or_keyword("Expected DLM module type (DCompressor, DAuditor, DEncryptor)")?;

        let module_type = match type_name.as_str() {
            "DCompressor" => DLMModuleType::DCompressor,
            "DAuditor"    => DLMModuleType::DAuditor,
            "DEncryptor"  => DLMModuleType::DEncryptor,
            _             => DLMModuleType::ParseError,
        };

        let mut subtype = None;

        if self.is_current_symbol('.') {
            self.advance();

            match self.parse_identifier_or_keyword("Expected DLM module subtype after '.'") {
                Some(name) => {
                    let parsed = match name.as_str() {
                        "gzip"     => DLMModuleSubtype::Gzip,
                        "bzip2"    => DLMModuleSubtype::Bzip2,
                        "lzma"     => DLMModuleSubtype::Lzma,
                        "diy"      => DLMModuleSubtype::Diy,
                        "enhanced" => DLMModuleSubtype::Enhanced,
                        "xor"      => DLMModuleSubtype::Xor,
                        "aes128"   => DLMModuleSubtype::Aes128,
                        "aes256"   => DLMModuleSubtype::Aes256,
                        "chacha20" => DLMModuleSubtype::Chacha20,
                        _          => DLMModuleSubtype::ParseError,
                    };
                    subtype = Some(parsed);
                }
                None => {
                    if self.should_halt_section() {
                        return None;
                    }
                }
            }
        }

        Some(DLMModule::new(module_type, subtype, module_start_pos))
    }

    fn parse_identifier_or_keyword(&mut self, context: &str) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                Some(name)
            }
            TokenType::Keyword(k) => {
                let name = k.to_string();
                self.advance();
                Some(name)
            }
            _ => {
                let current = self.current().clone();
                self.report_error(ParseErrorType::UnexpectedToken, context, &current);
                None
            }
        }
    }

    #[inline]
    fn could_be_module_type(&self) -> bool {
        matches!(
            &self.current().token_type,
            TokenType::Identifier(_) | TokenType::Keyword(_)
        )
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

    fn partial_or_none(&self, start_pos: Position) -> Option<DLMSection> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            None
        } else {
            Some(DLMSection::new(Vec::new(), start_pos))
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

    fn recover_to_next_module(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }
        for _ in 0..50 {
            if self.is_at_end() || self.is_current_symbol(',') || self.is_current_symbol(')') {
                return true;
            }
            if self.could_be_module_type() {
                return true;
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
        if self.iteration_count >= self.max_iterations {
            self.error_manager.log_error(&format!(
                "DLM parser exceeded {} iterations — possible infinite loop",
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
