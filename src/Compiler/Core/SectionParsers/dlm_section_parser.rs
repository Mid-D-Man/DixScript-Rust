// src/Compiler/Core/SectionParsers/dlm_section_parser.rs

use crate::Compiler::AST::{DLMSection, DLMModule, Position, DLMModuleType, DLMModuleSubtype};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, ParseErrorType};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;

/// DLM Section Parser v1.0.0 - Section-Scoped Error Handling with Dynamic Iterations
///
/// EBNF: @DLM( DLMList? )
/// DLMList ::= DLMModule (","? DLMModule)*
/// DLMModule ::= ModuleType ("." ModuleSubtype)?
///
/// Note: Commas are OPTIONAL between DLM modules
pub struct DlmSectionParser<'a> {
    tokens: &'a [Token],
    operational_settings: &'a OperationalSettings,
    error_manager: ErrorManager,

    // Parse state
    position: usize,
    last_position: usize,
    stuck_count: usize,
    iteration_count: usize,
    has_encountered_errors: bool,
}

// Dynamic iteration limits based on token count
const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 500_000;
const MAX_STUCK_COUNT: usize = 3;

impl<'a> DlmSectionParser<'a> {
    /// Create a new DLM section parser
    pub fn new(
        tokens: &'a [Token],
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();

        error_manager.log_debug(&format!(
            "Initializing DLM section parser v1.0.0 with {} tokens",
            tokens.len()
        ));
        error_manager.log_debug(&format!(
            "Error strategy: {:?}",
            operational_settings.error_handling_strategy
        ));

        // Calculate dynamic max iterations
        let dynamic_limit = tokens.len() * MAX_ITERATIONS_PER_TOKEN;
        let max_iterations = dynamic_limit.min(ABSOLUTE_MAX_ITERATIONS);
        error_manager.log_debug(&format!(
            "Dynamic max iterations: {} (token-based: {}, absolute cap: {})",
            max_iterations, dynamic_limit, ABSOLUTE_MAX_ITERATIONS
        ));

        DlmSectionParser {
            tokens,
            operational_settings,
            error_manager,
            position: 0,
            last_position: usize::MAX,
            stuck_count: 0,
            iteration_count: 0,
            has_encountered_errors: false,
        }
    }

    /// Parse the DLM section
    pub fn parse_section(&mut self) -> Option<DLMSection> {
        self.log_debug("Starting DLM section parse");

        let section_start_token = self.current();
        let section_start_pos = Position::from_token(&section_start_token);

        // Reset parse state
        self.reset_parse_state();

        // Estimate capacity for modules (typically small - 1-5 modules)
        let estimated_modules = usize::max(2, self.tokens.len() / 10);
        let mut modules = Vec::with_capacity(estimated_modules);

        // Expect opening parenthesis
        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected '(' to start DLM section content",
                &current,
            );

            if self.should_halt_section() {
                return self.handle_section_failure(section_start_pos);
            }

            if !self.attempt_recovery_to_opening_paren() {
                self.error_manager.log_error("Could not recover - opening parenthesis not found");
                return self.handle_section_failure(section_start_pos);
            }
        }

        self.log_debug("Found opening parenthesis");

        // Check for empty DLM section
        if self.is_current_symbol(')') {
            self.log_debug("Empty DLM section detected");
            self.advance();
            return Some(DLMSection::new(modules, section_start_pos));
        }

        // Parse DLM modules
        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                self.error_manager.log_warning(&format!("Parser stuck at position {}", self.position));
                if !self.recover_from_stuck() {
                    self.error_manager.log_error("Could not recover from stuck state");
                    break;
                }
                continue;
            }

            self.log_verbose(&format!("Attempting to parse DLM module at position {}", self.position));

            // Parse DLM module
            match self.parse_dlm_module() {
                Some(module) => {
                    self.log_debug(&format!("Successfully parsed DLM module: {}", module));
                    modules.push(module);
                }
                None => {
                    if self.should_halt_section() {
                        return self.handle_section_failure(section_start_pos);
                    }

                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.attempt_recovery_to_next_module() {
                            self.ensure_progress();
                        }
                    } else {
                        self.ensure_progress();
                    }
                }
            }

            // Handle comma separation (optional in EBNF)
            if self.is_current_symbol(',') {
                self.advance();
                self.log_verbose("Consumed comma separator");
            } else if self.is_current_symbol(')') {
                self.log_verbose("Found closing parenthesis");
                break;
            } else if !self.is_at_end() {
                // Check if next token looks like another module
                if self.could_be_module_type() {
                    // Continue - it's another module without comma (allowed by EBNF)
                    self.log_verbose("No comma found, but next token looks like module type (allowed)");
                    continue;
                } else {
                    let message = format!(
                        "Expected ',' or ')' after DLM module, found {}",
                        self.current().get_token_value()
                    );
                    let current = self.current().clone();
                    self.handle_parse_error(
                        ParseErrorType::UnexpectedToken,
                        &message,
                        &current,
                    );

                    if self.should_halt_section() {
                        return self.handle_section_failure(section_start_pos);
                    }

                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.attempt_recovery_to_next_module() {
                            self.ensure_progress();
                        }
                    } else {
                        self.ensure_progress();
                    }
                }
            }
        }

        // Expect closing parenthesis
        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected ')' to close DLM section",
                &current,
            );

            if self.should_halt_section() {
                return self.handle_section_failure(section_start_pos);
            }
        }

        let result = DLMSection::new(modules, section_start_pos);

        if self.has_encountered_errors {
            self.error_manager.log_warning(&format!(
                "DLM section parsed with errors ({} modules recovered)",
                result.modules.len()
            ));
        } else {
            self.log_debug(&format!(
                "DLM section parsed successfully with {} modules",
                result.modules.len()
            ));
        }

        Some(result)
    }

    // ==================== DLM MODULE PARSING ====================

    fn parse_dlm_module(&mut self) -> Option<DLMModule> {
        let module_start_token = self.current();
        let module_start_pos = Position::from_token(&module_start_token);

        self.log_verbose(&format!(
            "Parsing DLM module at position {}, current token: {}",
            self.position,
            self.current().get_token_value()
        ));

        // Parse module type
        let module_type_name = self.parse_module_type()?;
        self.log_verbose(&format!("Parsed module type name: {}", module_type_name));

        // Convert string to enum
        let module_type = match module_type_name.as_str() {
            "DCompressor" => DLMModuleType::DCompressor,
            "DAuditor" => DLMModuleType::DAuditor,
            "DEncryptor" => DLMModuleType::DEncryptor,
            _ => {
                self.log_debug(&format!("Unknown module type '{}' - creating ParseError placeholder", module_type_name));
                DLMModuleType::ParseError
            }
        };

        let mut subtype = None;

        // Check for optional subtype
        if self.is_current_symbol('.') {
            self.advance();
            self.log_verbose("Found module subtype separator '.'");

            let subtype_name = self.parse_module_subtype();
            if let Some(name) = subtype_name {
                self.log_verbose(&format!("Parsed subtype name: {}", name));

                let parsed_subtype = match name.as_str() {
                    "gzip" => DLMModuleSubtype::Gzip,
                    "bzip2" => DLMModuleSubtype::Bzip2,
                    "lzma" => DLMModuleSubtype::Lzma,
                    "diy" => DLMModuleSubtype::Diy,
                    "enhanced" => DLMModuleSubtype::Enhanced,
                    "xor" => DLMModuleSubtype::Xor,
                    "aes128" => DLMModuleSubtype::Aes128,
                    "aes256" => DLMModuleSubtype::Aes256,
                    "chacha20" => DLMModuleSubtype::Chacha20,
                    _ => {
                        self.log_debug(&format!("Unknown subtype '{}' - creating ParseError placeholder", name));
                        DLMModuleSubtype::ParseError
                    }
                };

                subtype = Some(parsed_subtype);
                self.log_verbose(&format!("Successfully parsed subtype enum: {:?}", parsed_subtype));
            } else {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected module subtype after '.'",
                    &current,
                );

                if self.should_halt_section() {
                    return None;
                }
            }
        }

        let module = DLMModule::new(module_type, subtype, module_start_pos);
        self.log_verbose(&format!("Created DLM module AST node: {}", module));

        Some(module)
    }

    fn parse_module_type(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let type_name = id.clone();
                self.advance();
                Some(type_name)
            }
            TokenType::Keyword(keyword) => {
                // Some keywords might be valid module type names
                let type_name = keyword.clone();
                self.advance();
                Some(type_name)
            }
            _ => {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected module type identifier",
                    &current,
                );
                None
            }
        }
    }

    fn parse_module_subtype(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let subtype_name = id.clone();
                self.advance();
                Some(subtype_name)
            }
            TokenType::Keyword(keyword) => {
                let subtype_name = keyword.clone();
                self.advance();
                Some(subtype_name)
            }
            _ => None,
        }
    }

    fn could_be_module_type(&self) -> bool {
        matches!(
            &self.current().token_type,
            TokenType::Identifier(_) | TokenType::Keyword(_)
        )
    }

    // ==================== ERROR HANDLING ====================

    fn handle_parse_error(&mut self, error_type: ParseErrorType, message: &str, token: &Token) {
        self.has_encountered_errors = true;

        let source_line = self.get_source_line(token);

        self.error_manager.add_parse_error(
            error_type,
            message.to_string(),
            token.line,
            token.column,
            None,
            source_line,
        );

        self.log_debug(&format!(
            "Error strategy: {:?} - {}",
            self.operational_settings.error_handling_strategy,
            message
        ));
    }

    fn should_halt_section(&self) -> bool {
        self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt
            && self.has_encountered_errors
    }

    fn handle_section_failure(&self, start_pos: Position) -> Option<DLMSection> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            self.error_manager.log_error("DLM section parsing halted due to errors");
            None
        } else {
            self.error_manager.log_warning("DLM section parsing completed with errors - returning empty section");
            Some(DLMSection::new(Vec::new(), start_pos))
        }
    }

    // ==================== ERROR RECOVERY ====================

    fn attempt_recovery_to_opening_paren(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }

        self.log_debug("RECOVER: Attempting to find opening parenthesis");

        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 10;

        while !self.is_at_end() && attempts < MAX_ATTEMPTS {
            if self.is_current_symbol('(') {
                self.advance();
                self.log_debug("RECOVER: Found opening parenthesis");
                return true;
            }
            self.advance();
            attempts += 1;
        }

        false
    }

    fn attempt_recovery_to_next_module(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }

        self.log_debug("RECOVER: Attempting to find next module");

        let mut recovery_attempts = 0;
        const MAX_RECOVERY_ATTEMPTS: usize = 50;

        while !self.is_at_end() && recovery_attempts < MAX_RECOVERY_ATTEMPTS {
            if self.is_current_symbol(',') || self.is_current_symbol(')') {
                self.log_debug(&format!("RECOVER: Found recovery point at {}", self.current().get_token_value()));
                return true;
            }

            // Check if current token could be start of next module
            if self.could_be_module_type() {
                self.log_debug("RECOVER: Found potential next module");
                return true;
            }

            self.advance();
            recovery_attempts += 1;
        }

        false
    }

    // ==================== TOKEN NAVIGATION ====================

    #[inline]
    fn current(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or_else(|| {
            static EOF_TOKEN: Token = Token {
                token_type: TokenType::EndOfFile,
                line: 1,
                column: 1,
                section: SectionId::None,
            };
            &EOF_TOKEN
        })
    }

    #[inline]
    fn is_at_end(&self) -> bool {
        self.position >= self.tokens.len() || matches!(self.current().token_type, TokenType::EndOfFile)
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

    fn get_source_line(&self, token: &Token) -> Option<String> {
        let line_tokens: Vec<&Token> = self.tokens
            .iter()
            .filter(|t| t.line == token.line)
            .collect();

        if line_tokens.is_empty() {
            return None;
        }

        let mut source_line = String::new();
        let mut current_column = 0;

        for t in line_tokens {
            while current_column < t.column {
                source_line.push(' ');
                current_column += 1;
            }

            let token_value = t.get_token_value();
            source_line.push_str(&token_value);
            current_column += token_value.len();
        }

        Some(source_line)
    }

    // ==================== STATE MANAGEMENT ====================

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
            self.stuck_count = 0;
        }

        self.last_position = self.position;
    }

    fn is_stuck(&self) -> bool {
        self.stuck_count >= MAX_STUCK_COUNT
    }

    fn should_terminate_loop(&self) -> bool {
        let dynamic_limit = self.tokens.len() * MAX_ITERATIONS_PER_TOKEN;
        let max_iterations = dynamic_limit.min(ABSOLUTE_MAX_ITERATIONS);

        if self.iteration_count >= max_iterations {
            self.error_manager.log_error(&format!(
                "Maximum iterations ({}) exceeded - possible infinite loop detected (token-based: {}, absolute cap: {})",
                max_iterations, dynamic_limit, ABSOLUTE_MAX_ITERATIONS
            ));
            return true;
        }

        false
    }

    fn recover_from_stuck(&mut self) -> bool {
        if self.is_at_end() {
            return false;
        }

        self.error_manager.log_debug(&format!("Forcing advancement from stuck position {}", self.position));
        self.advance();
        self.stuck_count = 0;
        true
    }

    fn ensure_progress(&mut self) {
        if !self.is_at_end() {
            self.advance();
        }
    }

    // ==================== LOGGING ====================

    fn log_debug(&self, message: &str) {
        if self.operational_settings.debug_mode != DebugMode::Off {
            self.error_manager.log_debug(message);
        }
    }

    fn log_verbose(&self, message: &str) {
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.error_manager.log_info(message);
        }
    }
}