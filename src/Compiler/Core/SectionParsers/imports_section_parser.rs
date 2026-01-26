// src/Compiler/Core/SectionParsers/imports_section_parser.rs

use crate::Compiler::AST::{ImportsSection, ImportDeclaration, Position};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy, DebugMode};
use crate::ErrorManager::{ErrorManager, ParseErrorType};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Utilities::estimate_properties_count;
use std::collections::HashSet;

/// ImportsSection Parser v1.0.0 - Dynamic Max Iterations
///
/// EBNF: @IMPORTS( ImportDeclaration ("," ImportDeclaration)* )
/// ImportDeclaration ::= Identifier ("from" | "from_cloud") StringLiteral ("verify" StringLiteral)?
///
/// Note: Commas between imports are OPTIONAL (v1.0.0)
pub struct ImportsSectionParser<'a> {
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

// Reserved names that cannot be used as import aliases
const RESERVED_ALIASES: &[&str] = &[
    // Built-in objects
    "Math", "DateTime", "Array", "Random", "Enum", "Guid", "IpAddress",
    // Special identifiers
    "config", "Dix",
    // DixScript keywords
    "if", "elif", "else", "chk", "miss", "then", "return", "log",
    "and", "or", "not", "true", "false", "null", "global",
    "from", "from_cloud", "verify",
];

impl<'a> ImportsSectionParser<'a> {
    /// Create a new imports section parser
    pub fn new(
        tokens: &'a [Token],
        operational_settings: &'a OperationalSettings,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();

        error_manager.log_debug(&format!(
            "Initializing IMPORTS section parser v1.0.0 with {} tokens",
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

        ImportsSectionParser {
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

    /// Parse the IMPORTS section
    pub fn parse_section(&mut self) -> Option<ImportsSection> {
        self.log_debug("Starting IMPORTS section parse");

        let section_start_token = self.current().clone();
        let section_start_pos = Position::from_token(&section_start_token);

        // Reset parse state
        self.reset_parse_state();

        // Estimate capacity for import declarations
        let estimated_imports = usize::max(2, self.tokens.len() / 15);
        let mut imports = Vec::with_capacity(estimated_imports);

        // Track aliases to detect duplicates
        let mut seen_aliases = HashSet::with_capacity(estimated_imports);

        // Expect opening parenthesis
        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                "Expected '(' to start IMPORTS section",
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

        // Check for empty IMPORTS section
        if self.is_current_symbol(')') {
            self.log_debug("Empty IMPORTS section detected");
            self.advance();
            return Some(ImportsSection::new(imports, section_start_pos));
        }

        // Parse import declarations
        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                self.error_manager.log_Warning(&format!("Parser stuck at position {}", self.position));
                if !self.recover_from_stuck() {
                    break;
                }
                continue;
            }

            // Parse import declaration
            match self.parse_import_declaration() {
                Some(import) => {
                    // Check for duplicate alias
                    if seen_aliases.contains(&import.alias) {
                        let current = self.current().clone();
                        self.handle_parse_error(
                            ParseErrorType::DuplicateDefinition,
                            &format!("Duplicate import alias '{}' - each alias must be unique", import.alias),
                            &current,
                        );

                        if self.should_halt_section() {
                            return self.handle_section_failure(section_start_pos);
                        }
                    } else {
                        seen_aliases.insert(import.alias.clone());
                        self.log_debug(&format!("Successfully parsed import: {}", import));
                        imports.push(import);
                    }
                }
                None => {
                    if self.should_halt_section() {
                        return self.handle_section_failure(section_start_pos);
                    }
                }
            }

            // Handle comma separator (OPTIONAL in v1.0.1)
            if self.is_current_symbol(',') {
                self.advance();
                self.log_verbose("Consumed comma separator");
            } else if self.is_current_symbol(')') {
                self.log_verbose("Found closing parenthesis");
                break;
            } else if !self.is_at_end() {
                // Check if next token looks like another import (identifier)
                if self.current().token_type.is_identifier() {
                    self.log_verbose("Found next import without comma (allowed in v1.0.1)");
                    continue;
                } else {
                    let current = self.current().clone();
                    self.handle_parse_error(
                        ParseErrorType::UnexpectedToken,
                        &format!("Expected ',' or ')' after import declaration, found {}", current.get_token_value()),
                        &current,
                    );

                    if self.should_halt_section() {
                        return self.handle_section_failure(section_start_pos);
                    }

                    if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                        if !self.attempt_recovery_to_next_import() {
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
                &format!("Expected ')' to close IMPORTS section, found {}", current.get_token_value()),
                &current,
            );

            if self.should_halt_section() {
                return self.handle_section_failure(section_start_pos);
            }
        }

        let result = ImportsSection::new(imports, section_start_pos);

        if self.has_encountered_errors {
            self.error_manager.log_Warning(&format!(
                "IMPORTS section parsed with errors ({} imports recovered)",
                result.imports.len()
            ));
        } else {
            self.log_debug(&format!(
                "IMPORTS section parsed successfully with {} imports",
                result.imports.len()
            ));
        }

        Some(result)
    }

    // ==================== IMPORT DECLARATION PARSING ====================

    fn parse_import_declaration(&mut self) -> Option<ImportDeclaration> {
        let import_start_token = self.current().clone();
        let import_start_pos = Position::from_token(&import_start_token);

        self.log_verbose("Parsing import declaration");

        // Parse alias (identifier)
        let alias = self.parse_alias()?;
        self.log_verbose(&format!("Parsed import alias: {}", alias));

        // Validate alias
        if !self.validate_alias(&alias) {
            if self.should_halt_section() {
                return None;
            }
        }

        // Expect 'from' OR 'from_cloud' keyword
        let is_cloud_import = if self.check_keyword("from_cloud") {
            self.advance();
            self.log_verbose("Found 'from_cloud' keyword");
            true
        } else if self.check_keyword("from") {
            self.advance();
            self.log_verbose("Found 'from' keyword");
            false
        } else {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::MissingToken,
                &format!("Expected 'from' or 'from_cloud' keyword after import alias '{}', found {}",
                         alias, current.get_token_value()),
                &current,
            );

            if self.should_halt_section() {
                return None;
            }

            // RECOVER: Try to find 'from' or 'from_cloud' keyword
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover {
                if !self.advance_to_keywords(&["from", "from_cloud"]) {
                    return None;
                }
                let is_cloud = self.check_keyword("from_cloud");
                self.advance();
                is_cloud
            } else {
                return None;
            }
        };

        // Parse path (string literal)
        let path = match self.parse_path() {
            Some(p) => p,
            None => {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    &format!("Expected file path string after '{}', found {}",
                             if is_cloud_import { "from_cloud" } else { "from" },
                             current.get_token_value()),
                    &current,
                );

                if self.should_halt_section() {
                    return None;
                }

                // Use placeholder
                if is_cloud_import {
                    "cloud://invalid/path.mdix".to_string()
                } else {
                    "invalid_path.mdix".to_string()
                }
            }
        };

        self.log_verbose(&format!("Parsed import path: {} (cloud: {})", path, is_cloud_import));

        // Validate path
        if !self.validate_path(&path, is_cloud_import) {
            if self.should_halt_section() {
                return None;
            }
        }

        // Optional: Parse verify clause
        let verify_hash = if self.check_keyword("verify") {
            self.advance();
            self.log_verbose("Found 'verify' keyword");

            match self.parse_verify_hash() {
                Some(hash) => {
                    self.log_verbose(&format!("Parsed verify hash: {}", hash));
                    Some(hash)
                }
                None => {
                    let current = self.current().clone();
                    self.handle_parse_error(
                        ParseErrorType::UnexpectedToken,
                        &format!("Expected hash string after 'verify', found {}", current.get_token_value()),
                        &current,
                    );

                    if self.should_halt_section() {
                        return None;
                    }
                    None
                }
            }
        } else {
            None
        };

        let import = ImportDeclaration::new(alias, path, is_cloud_import, verify_hash, import_start_pos);
        self.log_verbose(&format!("Created import declaration: {}", import));
        Some(import)
    }

    fn parse_alias(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let alias = id.clone();
                self.advance();
                Some(alias)
            }
            _ => {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::UnexpectedToken,
                    "Expected import alias identifier",
                    &current,
                );
                None
            }
        }
    }

    fn parse_path(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::String(s) => {
                let path = s.clone();
                self.advance();
                Some(path)
            }
            TokenType::StringSingle(s) => {
                let path = s.clone();
                self.advance();
                Some(path)
            }
            _ => None,
        }
    }

    fn parse_verify_hash(&mut self) -> Option<String> {
        self.parse_path() // Same parsing logic as path
    }

    fn validate_alias(&mut self, alias: &str) -> bool {
        // Check if reserved
        if RESERVED_ALIASES.iter().any(|&reserved| reserved.eq_ignore_ascii_case(alias)) {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::InvalidIdentifier,
                &format!("Import alias '{}' is reserved (built-in object or keyword)", alias),
                &current,
            );
            return false;
        }

        // Check if valid identifier pattern
        if !self.is_valid_identifier(alias) {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::InvalidIdentifier,
                &format!("Import alias '{}' is not a valid identifier", alias),
                &current,
            );
            return false;
        }

        true
    }

    fn validate_path(&mut self, path: &str, is_cloud_import: bool) -> bool {
        // Check not empty
        if path.trim().is_empty() {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::InvalidLiteral,
                "Import path cannot be empty",
                &current,
            );
            return false;
        }

        // Strip query parameters before validation
        let path_without_query = if let Some(query_index) = path.find('?') {
            self.log_verbose(&format!("Stripped query parameters: {} → {}", path, &path[..query_index]));
            &path[..query_index]
        } else {
            path
        };

        // Cloud import validation (Phase 1: HTTP/HTTPS only)
        if is_cloud_import {
            // Phase 1: Support direct HTTPS/HTTP URLs
            if path.starts_with("https://") || path.starts_with("http://") {
                // Warn if using HTTP (not HTTPS)
                if path.starts_with("http://")
                    && !path.contains("localhost")
                    && !path.contains("127.0.0.1")
                {
                    self.error_manager.log_Warning(&format!(
                        "⚠️ SECURITY WARNING: Using insecure HTTP for cloud import. \
                         Use HTTPS for production: {}",
                        path
                    ));
                }

                // Check .mdix extension (use path WITHOUT query parameters)
                if !path_without_query.ends_with(".mdix") {
                    let current = self.current().clone();
                    self.handle_parse_error(
                        ParseErrorType::InvalidLiteral,
                        &format!("Cloud import path must end with '.mdix' extension, got: {}", path_without_query),
                        &current,
                    );
                    return false;
                }

                return true;
            }

            // Phase 2+: Future cloud schemes (not yet supported)
            if path.starts_with("s3://")
                || path.starts_with("azure://")
                || path.starts_with("gs://")
            {
                let current = self.current().clone();
                self.handle_parse_error(
                    ParseErrorType::InvalidLiteral,
                    &format!(
                        "Cloud storage schemes (s3://, azure://, gs://) are not yet supported in v1.0.0. \
                         Use direct HTTPS URLs for now: {}",
                        path
                    ),
                    &current,
                );
                return false;
            }

            // Invalid cloud import format
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::InvalidLiteral,
                &format!(
                    "Cloud import path must be a valid HTTPS or HTTP URL. \
                     Expected format: https://example.com/path/to/file.mdix, got: {}",
                    path
                ),
                &current,
            );
            return false;
        }

        // Local import validation
        // Check .mdix extension (use path WITHOUT query parameters)
        if !path_without_query.ends_with(".mdix") {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::InvalidLiteral,
                &format!("Import path must end with '.mdix' extension, got: {}", path_without_query),
                &current,
            );
            return false;
        }

        // Check no backslashes (use forward slashes)
        if path_without_query.contains('\\') {
            let current = self.current().clone();
            self.handle_parse_error(
                ParseErrorType::InvalidLiteral,
                &format!("Import path must use forward slashes (/), not backslashes (\\): {}", path_without_query),
                &current,
            );
            return false;
        }

        // Warn if absolute path (should be relative)
        if path_without_query.starts_with('/') || (path_without_query.len() > 1 && path_without_query.chars().nth(1) == Some(':')) {
            self.error_manager.log_Warning(&format!(
                "Import path '{}' appears to be absolute - relative paths are recommended",
                path_without_query
            ));
        }

        true
    }

    fn is_valid_identifier(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        // First char must be letter or underscore
        let first = name.chars().next().unwrap();
        if !first.is_alphabetic() && first != '_' {
            return false;
        }

        // Rest must be letter, digit, or underscore
        name.chars().skip(1).all(|c| c.is_alphanumeric() || c == '_')
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

    fn handle_section_failure(&self, start_pos: Position) -> Option<ImportsSection> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            self.error_manager.log_error("IMPORTS section parsing halted due to errors");
            None
        } else {
            self.error_manager.log_Warning("IMPORTS section parsing completed with errors - returning empty section");
            Some(ImportsSection::new(Vec::new(), start_pos))
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

    fn attempt_recovery_to_next_import(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }

        self.log_debug("RECOVER: Attempting to find next import");

        let mut recovery_attempts = 0;
        const MAX_RECOVERY_ATTEMPTS: usize = 50;

        while !self.is_at_end() && recovery_attempts < MAX_RECOVERY_ATTEMPTS {
            if self.is_current_symbol(',') || self.is_current_symbol(')') {
                self.log_debug(&format!("RECOVER: Found recovery point at {}", self.current().get_token_value()));
                return true;
            }

            // Also look for 'from' or 'from_cloud' keyword (might be start of next import)
            if self.check_keyword("from") || self.check_keyword("from_cloud") {
                self.log_debug("RECOVER: Found import keyword - likely next import");
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
    fn check_keyword(&self, keyword: &str) -> bool {
        matches!(&self.current().token_type, TokenType::Keyword(k) if k.eq_ignore_ascii_case(keyword))
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

    fn advance_to_keywords(&mut self, keywords: &[&str]) -> bool {
        let mut attempts = 0;
        const MAX_ATTEMPTS: usize = 20;

        while !self.is_at_end() && attempts < MAX_ATTEMPTS {
            for &keyword in keywords {
                if self.check_keyword(keyword) {
                    return true;
                }
            }
            self.advance();
            attempts += 1;
        }

        false
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

// Helper trait extension for TokenType
trait TokenTypeExt {
    fn is_identifier(&self) -> bool;
}

impl TokenTypeExt for TokenType {
    fn is_identifier(&self) -> bool {
        matches!(self, TokenType::Identifier(_))
    }
}