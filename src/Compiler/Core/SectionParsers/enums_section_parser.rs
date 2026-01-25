// src/Compiler/Core/SectionParsers/enums_section_parser.rs

use crate::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField, Position};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, ParseErrorType};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Utilities::{Keywords, estimate_enum_fields_count};

/// Enums Section Parser v1.0.0 - Section-Scoped Error Handling
///
/// EBNF: @ENUMS( EnumDeclaration+ )
/// EnumDeclaration ::= Identifier "{" EnumFieldList "}"
/// EnumFieldList ::= EnumField ("," EnumField)*
/// EnumField ::= Identifier ("=" Integer)?
///
/// Note: NO commas between enum declarations, only between fields
pub struct EnumsSectionParser<'a> {
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

// Constants for safety limits
const MAX_ITERATIONS: usize = 1000;
const MAX_STUCK_COUNT: usize = 3;

impl<'a> EnumsSectionParser<'a> {
    /// Create a new enums section parser
    pub fn new(
        tokens: &'a [Token],
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();

        error_manager.log_debug(&format!(
            "Initializing ENUMS section parser v1.0.0 with {} tokens",
            tokens.len()
        ));
        error_manager.log_debug(&format!(
            "Error strategy: {:?}",
            operational_settings.error_handling_strategy
        ));

        EnumsSectionParser {
            tokens,
            operational_settings,
            error_manager,
            position: 0,
            last_position: usize::MAX, // Rust equivalent of -1 for unsigned
            stuck_count: 0,
            iteration_count: 0,
            has_encountered_errors: false,
        }
    }

    /// Parse the ENUMS section
    pub fn parse_section(&mut self) -> Option<EnumsSection> {
        self.log_debug("Starting ENUMS section parse");

        let section_start_token = self.current();
        let section_start_pos = Position::from_token(&section_start_token);

        // Reset parse state
        self.reset_parse_state();

        // Estimate capacity for enum declarations
        let estimated_enums = usize::max(2, self.tokens.len() / 20);
        let mut enum_declarations = Vec::with_capacity(estimated_enums);

        // Expect opening parenthesis
        if !self.match_and_consume_symbol('(') {
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected '(' to start ENUMS section",
                self.current(),
            );

            if self.should_halt_section() {
                return self.handle_section_failure(section_start_pos);
            }

            if !self.attempt_recovery_to_opening_paren() {
                self.error_manager.log_error("Could not recover - opening parenthesis not found");
                return self.handle_section_failure(section_start_pos);
            }
        }

        // Parse enum declarations
        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                self.error_manager.log_Warning("Parser stuck in ENUMS section");
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            // Check for invalid comma between enum declarations
            if self.is_current_symbol(',') {
                self.handle_parse_error(
                    ParseErrorType::SectionSyntaxError,
                    "Commas are not allowed between enum declarations",
                    self.current(),
                );
                self.advance();
                continue;
            }

            // Parse enum declaration
            match self.parse_enum_declaration() {
                Some(enum_decl) => {
                    self.log_debug(&format!(
                        "Parsed enum: {} with {} fields",
                        enum_decl.name,
                        enum_decl.fields.len()
                    ));
                    enum_declarations.push(enum_decl);
                }
                None => {
                    if self.should_halt_section() {
                        return self.handle_section_failure(section_start_pos);
                    }

                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.attempt_recovery_to_next_enum() {
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
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected ')' to close ENUMS section",
                self.current(),
            );

            if self.should_halt_section() {
                return self.handle_section_failure(section_start_pos);
            }
        }

        let result = EnumsSection::new(enum_declarations, section_start_pos);

        if self.has_encountered_errors {
            self.error_manager.log_Warning(&format!(
                "ENUMS section parsed with errors ({} enums recovered)",
                result.enums.len()
            ));
        } else {
            self.log_debug(&format!(
                "ENUMS section parsed successfully with {} enum declarations",
                result.enums.len()
            ));
        }

        Some(result)
    }

    // ==================== ENUM DECLARATION PARSING ====================

    fn parse_enum_declaration(&mut self) -> Option<EnumDeclaration> {
        let enum_start_token = self.current();
        let enum_start_pos = Position::from_token(&enum_start_token);

        self.log_verbose("Parsing enum declaration");

        // Parse enum name
        let enum_name = self.parse_enum_name()?;
        self.log_verbose(&format!("Parsed enum name: {}", enum_name));

        // Expect opening brace
        if !self.match_and_consume_symbol('{') {
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!("Expected '{{' after enum name '{}'", enum_name),
                self.current(),
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

        // Parse enum fields
        let estimated_fields = estimate_enum_fields_count(self.tokens.len() - self.position);
        let mut enum_fields = Vec::with_capacity(estimated_fields);

        let mut expecting_field = true;

        while !self.is_at_end() && !self.is_current_symbol('}') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                self.error_manager.log_Warning(&format!("Parser stuck in enum '{}' fields", enum_name));
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            // Parse field
            match self.parse_enum_field() {
                Some(field) => {
                    self.log_verbose(&format!(
                        "  Parsed field: {}{}",
                        field.name,
                        field.value.map(|v| format!(" = {}", v)).unwrap_or_default()
                    ));
                    enum_fields.push(field);
                    expecting_field = false;
                }
                None => {
                    if self.should_halt_section() {
                        return None;
                    }
                }
            }

            // Handle comma separator
            if self.is_current_symbol(',') {
                self.advance();
                self.log_verbose("  Consumed comma separator between fields");
                expecting_field = true;
            } else if self.is_current_symbol('}') {
                self.log_verbose("  Found closing brace, ending fields");
                break;
            } else if !self.is_at_end() && !expecting_field {
                // Check if next token looks like another enum declaration
                if self.current().token_type.is_identifier() {
                    if let Some(next_token) = self.peek() {
                        if matches!(next_token.token_type, TokenType::Symbol('{')) {
                            self.handle_parse_error(
                                ParseErrorType::MissingToken,
                                &format!("Missing '}}' to close enum '{}' - found start of next enum", enum_name),
                                self.current(),
                            );
                            break;
                        }
                    }
                }

                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Expected ',' or '}}' after field in '{}', found {}",
                        enum_name,
                        self.current().get_token_value()
                    ),
                    self.current(),
                );

                if self.should_halt_section() {
                    return None;
                }

                self.ensure_progress();
            }
        }

        // Expect closing brace
        if !self.match_and_consume_symbol('}') {
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!("Expected '}}' to close enum '{}'", enum_name),
                self.current(),
            );

            if self.should_halt_section() {
                return None;
            }
        }

        let enum_declaration = EnumDeclaration::new(enum_name, enum_fields, enum_start_pos);
        self.log_verbose(&format!(
            "Created enum AST node: {} with {} fields",
            enum_declaration.name,
            enum_declaration.fields.len()
        ));

        Some(enum_declaration)
    }

    fn parse_enum_field(&mut self) -> Option<EnumField> {
        let field_start_token = self.current();
        let field_start_pos = Position::from_token(&field_start_token);

        self.log_verbose("Parsing enum field");

        // Parse field name
        let field_name = self.parse_field_name()?;
        self.log_verbose(&format!("    Field name: {}", field_name));

        // Check for value assignment
        if !self.is_current_symbol('=') {
            return Some(EnumField::new(field_name, None, field_start_pos));
        }

        self.advance();
        self.log_verbose("    Found '=' for value assignment");

        // Parse field value
        match self.parse_field_value() {
            Some(value) => {
                self.log_verbose(&format!("    Assigned value: {}", value));
                Some(EnumField::new(field_name, Some(value), field_start_pos))
            }
            None => {
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!("Expected integer value after '=' in enum field '{}'", field_name),
                    self.current(),
                );

                if self.should_halt_section() {
                    None
                } else {
                    Some(EnumField::new(field_name, None, field_start_pos))
                }
            }
        }
    }

    // ==================== IDENTIFIER AND VALUE PARSING ====================

    fn parse_enum_name(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                Some(name)
            }
            TokenType::Keyword(keyword) => {
                // Check if keyword can be used as identifier in ENUMS context
                if Keywords::can_be_identifier_in_context(keyword, "ENUMS") {
                    let name = keyword.clone();
                    self.advance();
                    self.log_verbose(&format!("Accepted keyword '{}' as enum name", name));
                    Some(name)
                } else {
                    self.handle_parse_error(
                        ParseErrorType::UnexpectedToken,
                        &format!("Cannot use language keyword '{}' as enum name", keyword),
                        self.current(),
                    );
                    None
                }
            }
            _ => {
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected enum name identifier",
                    self.current(),
                );
                None
            }
        }
    }

    fn parse_field_name(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                Some(name)
            }
            TokenType::Keyword(keyword) => {
                if Keywords::can_be_identifier_in_context(keyword, "ENUMS") {
                    let name = keyword.clone();
                    self.advance();
                    self.log_verbose(&format!("Accepted keyword '{}' as field name", name));
                    Some(name)
                } else {
                    self.handle_parse_error(
                        ParseErrorType::UnexpectedToken,
                        &format!("Cannot use language keyword '{}' as field name", keyword),
                        self.current(),
                    );
                    None
                }
            }
            _ => {
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected enum field name identifier",
                    self.current(),
                );
                None
            }
        }
    }

    fn parse_field_value(&mut self) -> Option<i32> {
        match &self.current().token_type {
            TokenType::Integer(value) => {
                let val = *value;
                self.advance();
                Some(val)
            }
            TokenType::Identifier(id) => {
                // Try to parse as integer
                if let Ok(value) = id.parse::<i32>() {
                    self.advance();
                    Some(value)
                } else {
                    None
                }
            }
            _ => None,
        }
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
            Some(ParseErrorType::generate_suggestion(error_type, token, None)),
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

    fn handle_section_failure(&self, start_pos: Position) -> Option<EnumsSection> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            self.error_manager.log_error("ENUMS section parsing halted due to errors");
            None
        } else {
            self.error_manager.log_Warning("ENUMS section parsing completed with errors - returning empty section");
            Some(EnumsSection::new(Vec::new(), start_pos))
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

    fn attempt_recovery_to_next_enum(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }

        self.log_debug("RECOVER: Attempting to find next enum declaration");

        let mut recovery_attempts = 0;
        const MAX_RECOVERY_ATTEMPTS: usize = 50;

        while !self.is_at_end() && recovery_attempts < MAX_RECOVERY_ATTEMPTS {
            if self.is_current_symbol('}') || self.is_current_symbol(')') {
                self.log_debug(&format!("RECOVER: Found recovery point at {}", self.current().get_token_value()));
                return true;
            }

            // Check if this looks like next enum declaration
            if self.current().token_type.is_identifier() {
                if let Some(next) = self.peek() {
                    if matches!(next.token_type, TokenType::Symbol('{')) {
                        self.log_debug("RECOVER: Found next enum declaration");
                        return true;
                    }
                }
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
            // Return a dummy EOF token if we're past the end
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
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position + 1)
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
        // Get all tokens on same line
        let line_tokens: Vec<&Token> = self.tokens
            .iter()
            .filter(|t| t.line == token.line)
            .collect();

        if line_tokens.is_empty() {
            return None;
        }

        // Build source line
        let mut source_line = String::new();
        let mut current_column = 0;

        for t in line_tokens {
            // Add spaces to reach token column
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
        if self.iteration_count >= MAX_ITERATIONS {
            self.error_manager.log_error(&format!("Maximum iterations ({}) exceeded", MAX_ITERATIONS));
            true
        } else {
            false
        }
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
        if self.operational_settings.debug_mode >= DebugMode::Regular {
            self.error_manager.log_debug(message);
        }
    }

    fn log_verbose(&self, message: &str) {
        if self.operational_settings.debug_mode == DebugMode::Verbose {
            self.error_manager.log_info(message);
        }
    }
}

// Helper trait extension for TokenType
trait TokenTypeExt {
    fn is_identifier(&self) -> bool;
}

impl TokenTypeExt for TokenType {
    fn is_identifier(&self) -> bool {
        matches!(self, TokenType::Identifier(_))
    }
}