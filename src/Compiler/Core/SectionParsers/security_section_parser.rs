// src/Compiler/Core/SectionParsers/security_section_parser.rs

use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Position, Value};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, ParseErrorType};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};

/// Security Section Parser v1.0.0 - Section-Scoped Error Handling with Dynamic Iterations
///
/// EBNF: @SECURITY( SecurityEntry+ )
/// SecurityEntry ::= SecurityBlockKey "->" "{" SecurityFieldList? "}"
/// SecurityFieldList ::= SecurityField ("," SecurityField)*
///
/// Note: SecurityEntry blocks are NOT comma-separated (unlike fields inside them)
pub struct SecuritySectionParser<'a> {
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
const PROGRESS_CHECK_INTERVAL: usize = 100;

// Known security block keys
const VALID_BLOCK_KEYS: &[&str] = &["encryption", "validation", "keystore", "override", "metadata"];

impl<'a> SecuritySectionParser<'a> {
    /// Create a new security section parser
    pub fn new(
        tokens: &'a [Token],
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();

        error_manager.log_debug(&format!(
            "Initializing SECURITY section parser v1.0.0 with {} tokens",
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

        SecuritySectionParser {
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

    /// Parse the SECURITY section
    pub fn parse_section(&mut self) -> Option<SecuritySection> {
        self.log_debug("Starting SECURITY section parse");

        let section_start_token = self.current();
        let section_start_pos = Position::from_token(&section_start_token);

        // Reset parse state
        self.reset_parse_state();

        // Estimate capacity for security entries (typically 3-5 entries)
        let estimated_entries = usize::max(3, self.tokens.len() / 20);
        let mut security_entries = Vec::with_capacity(estimated_entries);

        // Expect opening parenthesis
        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected '(' to start SECURITY section",
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

        // Parse security entries
        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            if self.iteration_count % PROGRESS_CHECK_INTERVAL == 0 {
                self.track_progress();

                if self.is_stuck() {
                    self.error_manager.log_warning("Parser stuck in SECURITY section");
                    if !self.recover_from_stuck() {
                        break;
                    }
                    continue;
                }
            }

            self.iteration_count += 1;

            // Check for invalid comma between entries
            if self.is_current_symbol(',') {
                self.log_debug("WARNING: Found unexpected comma between security entries - skipping");
                self.advance();
                continue;
            }

            // Parse security entry
            match self.parse_security_entry() {
                Some(entry) => {
                    self.log_verbose(&format!("Successfully parsed security entry: {}", entry.block_key));
                    security_entries.push(entry);
                }
                None => {
                    if self.should_halt_section() {
                        return self.handle_section_failure(section_start_pos);
                    }

                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.attempt_recovery_to_next_entry() {
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
                "Expected ')' to close SECURITY section",
                &current,
            );

            if self.should_halt_section() {
                return self.handle_section_failure(section_start_pos);
            }
        }

        let result = SecuritySection::new(security_entries, section_start_pos);

        if self.has_encountered_errors {
            self.error_manager.log_warning(&format!(
                "SECURITY section parsed with errors ({} entries recovered)",
                result.entries.len()
            ));
        } else {
            self.log_debug(&format!(
                "SECURITY section parsed successfully with {} security entries",
                result.entries.len()
            ));
        }

        Some(result)
    }

    // ==================== SECURITY ENTRY PARSING ====================

    fn parse_security_entry(&mut self) -> Option<SecurityEntry> {
        let entry_start_token = self.current();
        let entry_start_pos = Position::from_token(&entry_start_token);

        self.log_verbose("Parsing security entry");

        // Parse security block key
        let block_key = self.parse_security_block_key()?;
        self.log_verbose(&format!("Parsed security block key: {}", block_key));

        // Expect arrow operator (->)
        if !self.match_arrow() {
            let message = format!("Expected '->' after security block key '{}'", block_key);
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &message,
                &current,
            );

            if self.should_halt_section() {
                return None;
            }

            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.attempt_recovery_to_arrow() {
                    return None;
                }
            } else {
                return None;
            }
        }

        // Expect opening brace
        if !self.match_and_consume_symbol('{') {
            let message = format!("Expected '{{' after '->' in security entry '{}'", block_key);
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &message,
                &current,
            );

            if self.should_halt_section() {
                return None;
            }

            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.attempt_recovery_to_opening_brace() {
                    return None;
                }
            } else {
                return None;
            }
        }

        // Parse security fields
        let estimated_fields = usize::max(4, self.tokens.len() / 50);
        let mut fields = Vec::with_capacity(estimated_fields);

        while !self.is_at_end() && !self.is_current_symbol('}') && !self.should_terminate_loop() {
            if self.iteration_count % PROGRESS_CHECK_INTERVAL == 0 {
                self.track_progress();

                if self.is_stuck() {
                    self.error_manager.log_warning(&format!("Parser stuck in security block '{}' fields", block_key));
                    if !self.recover_from_stuck() {
                        break;
                    }
                    continue;
                }
            }

            self.iteration_count += 1;

            // Parse field
            match self.parse_security_field() {
                Some(field) => {
                    self.log_verbose(&format!("  Parsed field: {} = {}", field.key, field.value));
                    fields.push(field);
                }
                None => {
                    if self.should_halt_section() {
                        return None;
                    }
                }
            }

            // Handle comma separation
            if self.is_current_symbol(',') {
                self.advance();
                self.log_verbose("  Consumed comma separator between fields");
            } else if self.is_current_symbol('}') {
                self.log_verbose("  Found closing brace, ending fields");
                break;
            } else if !self.is_at_end() {
                let message = format!(
                    "Expected ',' or '}}' after field in '{}', found {}",
                    block_key,
                    self.current().get_token_value()
                );
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &message,
                    &current,
                );

                if self.should_halt_section() {
                    return None;
                }

                if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                    if !self.attempt_recovery_in_fields() {
                        self.ensure_progress();
                    }
                } else {
                    self.ensure_progress();
                }
            }
        }

        // Expect closing brace
        if !self.match_and_consume_symbol('}') {
            let message = format!("Expected '}}' to close security block '{}'", block_key);
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &message,
                &current,
            );

            if self.should_halt_section() {
                return None;
            }
        }

        let security_entry = SecurityEntry::new(block_key, fields, entry_start_pos);
        self.log_verbose(&format!(
            "Created security entry AST node: {} with {} fields",
            security_entry.block_key,
            security_entry.fields.len()
        ));

        Some(security_entry)
    }

    fn parse_security_field(&mut self) -> Option<SecurityField> {
        let field_start_token = self.current();
        let field_start_pos = Position::from_token(&field_start_token);

        self.log_verbose("Parsing security field");

        // Parse field key
        let key = self.parse_field_key()?;
        self.log_verbose(&format!("    Field key: {}", key));

        // Expect equals sign
        if !self.match_and_consume_symbol('=') {
            let message = format!("Expected '=' after security field key '{}'", key);
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &message,
                &current,
            );

            if self.should_halt_section() {
                return None;
            }

            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.attempt_recovery_to_equals() {
                    return None;
                }
            } else {
                return None;
            }
        }

        // Parse field value
        let value = match self.parse_security_value() {
            Some(v) => v,
            None => {
                let message = format!("Expected value after '=' in security field '{}'", key);
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &message,
                    &current,
                );

                if self.should_halt_section() {
                    return None;
                }

                // Create error value as placeholder
                Value::Error {
                    message: format!("Missing value for key '{}'", key),
                    position: Position::from_token(&current),
                }
            }
        };

        let field = SecurityField::new(key, value, field_start_pos);
        self.log_verbose(&format!("Created security field AST node: {} = {}", field.key, field.value));

        Some(field)
    }

    // ==================== IDENTIFIER AND VALUE PARSING ====================

    fn parse_security_block_key(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let key = id.clone();
                self.advance();
                Some(key)
            }
            TokenType::Keyword(keyword) => {
                let key = keyword.clone();
                self.advance();
                Some(key)
            }
            _ => {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected security block key (encryption, validation, keystore, override, metadata)",
                    &current,
                );
                None
            }
        }
    }

    fn parse_field_key(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let key = id.clone();
                self.advance();
                Some(key)
            }
            TokenType::Keyword(keyword) => {
                let key = keyword.clone();
                self.advance();
                Some(key)
            }
            _ => {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected security field key identifier",
                    &current,
                );
                None
            }
        }
    }

    fn parse_security_value(&mut self) -> Option<Value> {
        let token = self.current();
        let value_position = Position::from_token(&token);

        let value = match &token.token_type {
            TokenType::Integer(i) => Some(Value::Integer {
                value: *i,
                position: value_position,
            }),
            TokenType::String(s) => Some(Value::String {
                value: s.clone(),
                position: value_position,
            }),
            TokenType::StringSingle(ss) => Some(Value::String {
                value: ss.clone(),
                position: value_position,
            }),
            TokenType::Bool(b) => Some(Value::Boolean {
                value: *b,
                position: value_position,
            }),
            TokenType::HexLiteral(hl) => Some(Value::Integer {
                value: *hl,
                position: value_position,
            }),
            // FIX: Use .as_str() for String comparisons
            TokenType::Keyword(k) if k.as_str() == "true" => Some(Value::Boolean {
                value: true,
                position: value_position,
            }),
            TokenType::Keyword(k) if k.as_str() == "false" => Some(Value::Boolean {
                value: false,
                position: value_position,
            }),
            TokenType::Keyword(k) if k.as_str() == "auto" => Some(Value::String {
                value: "auto".to_string(),
                position: value_position,
            }),
            TokenType::Identifier(id) if id.as_str() == "true" => Some(Value::Boolean {
                value: true,
                position: value_position,
            }),
            TokenType::Identifier(id) if id.as_str() == "false" => Some(Value::Boolean {
                value: false,
                position: value_position,
            }),
            TokenType::Identifier(id) if id.as_str() == "auto" => Some(Value::String {
                value: "auto".to_string(),
                position: value_position,
            }),
            _ => None,
        };

        if value.is_some() {
            self.advance();
            if let Some(ref v) = value {
                self.log_verbose(&format!("Parsed security value: {}", v));
            }
        }

        value
    }

    fn match_arrow(&mut self) -> bool {
        // Try MultiCharSymbol "->"
        if let TokenType::MultiCharSymbol(ms) = &self.current().token_type {
            if ms.as_str() == "->" {  // FIX: Use .as_str()
                self.advance();
                self.log_verbose("Consumed arrow operator '->'");
                return true;
            }
        }

        // Try SwitchCase token (alternative representation)
        if matches!(self.current().token_type, TokenType::SwitchCase) {
            self.advance();
            self.log_verbose("Consumed arrow operator '->' (SwitchCase token)");
            return true;
        }

        // Try two separate symbols: '-' and '>'
        if let TokenType::Symbol('-') = self.current().token_type {
            if self.position + 1 < self.tokens.len() {
                if let TokenType::Symbol('>') = self.tokens[self.position + 1].token_type {
                    self.advance();
                    self.advance();
                    self.log_verbose("Consumed arrow operator '->' (two symbols)");
                    return true;
                }
            }
        }

        false
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

    fn handle_section_failure(&self, start_pos: Position) -> Option<SecuritySection> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            self.error_manager.log_error("SECURITY section parsing halted due to errors");
            None
        } else {
            self.error_manager.log_warning("SECURITY section parsing completed with errors - returning empty section");
            Some(SecuritySection::new(Vec::new(), start_pos))
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

    fn attempt_recovery_to_opening_brace(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }

        self.log_debug("RECOVER: Attempting to find opening brace");

        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 10;

        while !self.is_at_end() && attempts < MAX_ATTEMPTS {
            if self.is_current_symbol('{') {
                self.advance();
                self.log_debug("RECOVER: Found opening brace");
                return true;
            }
            self.advance();
            attempts += 1;
        }

        false
    }

    fn attempt_recovery_to_arrow(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }

        self.log_debug("RECOVER: Attempting to find arrow operator");

        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 10;

        while !self.is_at_end() && attempts < MAX_ATTEMPTS {
            if self.match_arrow() {
                self.log_debug("RECOVER: Found arrow operator");
                return true;
            }
            self.advance();
            attempts += 1;
        }

        false
    }

    fn attempt_recovery_to_equals(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }

        self.log_debug("RECOVER: Attempting to find equals sign");

        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 10;

        while !self.is_at_end() && attempts < MAX_ATTEMPTS {
            if self.is_current_symbol('=') {
                self.advance();
                self.log_debug("RECOVER: Found equals sign");
                return true;
            }
            self.advance();
            attempts += 1;
        }

        false
    }

    fn attempt_recovery_to_next_entry(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }

        self.log_debug("RECOVER: Attempting to find next security entry");

        let mut recovery_attempts = 0;
        const MAX_RECOVERY_ATTEMPTS: usize = 50;

        while !self.is_at_end() && recovery_attempts < MAX_RECOVERY_ATTEMPTS {
            if self.is_current_symbol('}') || self.is_current_symbol(')') {
                self.log_debug(&format!("RECOVER: Found recovery point at {}", self.current().get_token_value()));
                return true;
            }

            self.advance();
            recovery_attempts += 1;
        }

        false
    }

    fn attempt_recovery_in_fields(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }

        self.log_debug("RECOVER: Attempting to find next field or end of block");

        let mut recovery_attempts = 0;
        const MAX_RECOVERY_ATTEMPTS: usize = 50;

        while !self.is_at_end() && recovery_attempts < MAX_RECOVERY_ATTEMPTS {
            if self.is_current_symbol(',') || self.is_current_symbol('}') {
                self.log_debug(&format!("RECOVER: Found recovery point at {}", self.current().get_token_value()));
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
                section: None,
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