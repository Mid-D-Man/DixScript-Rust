// src/Compiler/Core/SectionParsers/imports_section_parser.rs
//! Parser for the `@IMPORTS(...)` section.
//!
//! ```text
//! ImportsSection    ::= "@IMPORTS(" ImportDeclaration (","? ImportDeclaration)* ")"
//! ImportDeclaration ::= Identifier ("from" | "from_cloud") StringLiteral ("verify" StringLiteral)?
//! ```
//!
//! Commas between declarations are optional (v1.0.0).

use crate::Compiler::AST::{ImportsSection, ImportDeclaration, Position};
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::ErrorManager::{ErrorManager, ParseErrorType, DebugConfig};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;
use crate::Utilities::Keywords;

const MAX_ITERATIONS_PER_TOKEN: usize = 3;
const ABSOLUTE_MAX_ITERATIONS: usize = 500_000;
const MAX_STUCK_COUNT: usize = 3;

static BUILTIN_OBJECT_NAMES: &[&str] = &[
    "Math", "DateTime", "Array", "Random", "Enum", "Guid", "IpAddress",
];

pub struct ImportsSectionParser<'a> {
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

impl<'a> ImportsSectionParser<'a> {
    pub fn new(tokens: &'a [Token], operational_settings: &'a OperationalSettings) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

        let dynamic_limit = tokens.len() * MAX_ITERATIONS_PER_TOKEN;
        let max_iterations = dynamic_limit.min(ABSOLUTE_MAX_ITERATIONS);

        if debug_config.is_enabled {
            error_manager.log_debug(&format!(
                "IMPORTS parser: {} tokens, strategy: {:?}",
                tokens.len(),
                operational_settings.error_handling_strategy
            ));
        }

        ImportsSectionParser {
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

    pub fn parse_section(&mut self) -> Option<ImportsSection> {
        let section_start_pos = Position::from_token(self.current());
        self.reset_parse_state();

        let mut imports: Vec<ImportDeclaration> = Vec::with_capacity(4);
        let mut seen_count = 0usize;

        if !self.match_and_consume_symbol('(') {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::MissingToken,
                "Expected '(' to start IMPORTS section",
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
            return Some(ImportsSection::new(imports, section_start_pos));
        }

        while !self.is_at_end() && !self.is_current_symbol(')') && !self.should_terminate_loop() {
            self.track_progress();

            if self.is_stuck() {
                if !self.force_advance() {
                    break;
                }
                continue;
            }

            match self.parse_import_declaration() {
                Some(import) => {
                    let is_dup = imports.iter().any(|i| i.alias == import.alias);
                    if is_dup {
                        let current = self.current().clone();
                        self.report_error(
                            ParseErrorType::DuplicateDefinition,
                            &format!(
                                "Duplicate import alias '{}' — each alias must be unique",
                                import.alias
                            ),
                            &current,
                        );
                        if self.should_halt_section() {
                            return self.partial_or_none(section_start_pos);
                        }
                    } else {
                        if self.debug_config.is_enabled {
                            self.error_manager.log_debug(&format!(
                                "IMPORTS: parsed '{}'",
                                import.alias
                            ));
                        }
                        imports.push(import);
                        seen_count += 1;
                    }
                }
                None => {
                    if self.should_halt_section() {
                        return self.partial_or_none(section_start_pos);
                    }
                }
            }

            if self.is_current_symbol(',') {
                self.advance();
            } else if self.is_current_symbol(')') {
                break;
            } else if !self.is_at_end() {
                if matches!(self.current().token_type, TokenType::Identifier(_)) {
                    continue;
                }
                let current = self.current().clone();
                self.report_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Expected ',' or ')' after import declaration, found {}",
                        current.get_token_value()
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    return self.partial_or_none(section_start_pos);
                }
                if self.operational_settings.error_handling_strategy
                    == ErrorHandlingStrategy::Recover
                {
                    if !self.recover_to_next_import() {
                        self.ensure_progress();
                    }
                } else {
                    self.ensure_progress();
                }
            }
        }

        if !self.match_and_consume_symbol(')') {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::MissingToken,
                "Expected ')' to close IMPORTS section",
                &current,
            );
            if self.should_halt_section() {
                return self.partial_or_none(section_start_pos);
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "IMPORTS section done: {} declarations, errors: {}",
                seen_count, self.has_encountered_errors
            ));
        }

        Some(ImportsSection::new(imports, section_start_pos))
    }

    fn parse_import_declaration(&mut self) -> Option<ImportDeclaration> {
        let import_start_pos = Position::from_token(self.current());

        let alias = self.parse_identifier("Expected import alias identifier")?;

        if !self.validate_alias(&alias) {
            if self.should_halt_section() {
                return None;
            }
        }

        let is_cloud = if self.check_keyword("from_cloud") {
            self.advance();
            true
        } else if self.check_keyword("from") {
            self.advance();
            false
        } else {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::MissingToken,
                &format!(
                    "Expected 'from' or 'from_cloud' after import alias '{}', found {}",
                    alias,
                    current.get_token_value()
                ),
                &current,
            );
            if self.should_halt_section() {
                return None;
            }
            if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Recover
                && self.advance_to_from_keyword()
            {
                let cloud = self.check_keyword("from_cloud");
                self.advance();
                cloud
            } else {
                return None;
            }
        };

        let path = match self.parse_string_literal() {
            Some(p) => p,
            None => {
                let current = self.current().clone();
                self.report_error(
                    ParseErrorType::UnexpectedToken,
                    &format!(
                        "Expected file path after '{}', found {}",
                        if is_cloud { "from_cloud" } else { "from" },
                        current.get_token_value()
                    ),
                    &current,
                );
                if self.should_halt_section() {
                    return None;
                }
                if is_cloud {
                    "cloud://invalid/path.dixscript".to_string()
                } else {
                    "invalid_path.dixscript".to_string()
                }
            }
        };

        if !self.validate_path(&path, is_cloud) && self.should_halt_section() {
            return None;
        }

        let verify_hash = if self.check_keyword("verify") {
            self.advance();
            match self.parse_string_literal() {
                Some(h) => Some(h),
                None => {
                    let current = self.current().clone();
                    self.report_error(
                        ParseErrorType::UnexpectedToken,
                        &format!(
                            "Expected hash string after 'verify', found {}",
                            current.get_token_value()
                        ),
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

        Some(ImportDeclaration::new(alias, path, is_cloud, verify_hash, import_start_pos))
    }

    fn parse_identifier(&mut self, missing_msg: &str) -> Option<String> {
        match &self.current().token_type {
            TokenType::Identifier(id) => {
                let name = id.clone();
                self.advance();
                Some(name)
            }
            _ => {
                let current = self.current().clone();
                self.report_error(ParseErrorType::UnexpectedToken, missing_msg, &current);
                None
            }
        }
    }

    fn parse_string_literal(&mut self) -> Option<String> {
        match &self.current().token_type {
            TokenType::String(s) => {
                let v = s.clone();
                self.advance();
                Some(v)
            }
            TokenType::StringSingle(s) => {
                let v = s.clone();
                self.advance();
                Some(v)
            }
            _ => None,
        }
    }

    fn validate_alias(&mut self, alias: &str) -> bool {
        if Keywords::is_reserved_in_context(alias, "QUICKFUNCS") {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::InvalidIdentifier,
                &format!("'{}' is a reserved keyword and cannot be used as an import alias", alias),
                &current,
            );
            return false;
        }

        if Keywords::is_contextual_identifier(alias) {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::InvalidIdentifier,
                &format!("'{}' is a reserved contextual identifier and cannot be used as an import alias", alias),
                &current,
            );
            return false;
        }

        if BUILTIN_OBJECT_NAMES.iter().any(|&n| n.eq_ignore_ascii_case(alias)) {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::InvalidIdentifier,
                &format!("'{}' is a built-in object name and cannot be used as an import alias", alias),
                &current,
            );
            return false;
        }

        if !is_valid_identifier(alias) {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::InvalidIdentifier,
                &format!("'{}' is not a valid identifier", alias),
                &current,
            );
            return false;
        }

        true
    }

    fn validate_path(&mut self, path: &str, is_cloud: bool) -> bool {
        if path.trim().is_empty() {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::InvalidLiteral,
                "Import path cannot be empty",
                &current,
            );
            return false;
        }

        let path_without_query = path.find('?').map_or(path, |i| &path[..i]);

        if is_cloud {
            if path.starts_with("https://") || path.starts_with("http://") {
                if path.starts_with("http://")
                    && !path.contains("localhost")
                    && !path.contains("127.0.0.1")
                {
                    self.error_manager.log_warning(&format!(
                        "Insecure HTTP used for cloud import — prefer HTTPS: {}",
                        path
                    ));
                }
                if !path_without_query.ends_with(".dixscript") {
                    let current = self.current().clone();
                    self.report_error(
                        ParseErrorType::InvalidLiteral,
                        &format!(
                            "Cloud import path must end with '.mdix', got: {}",
                            path_without_query
                        ),
                        &current,
                    );
                    return false;
                }
                return true;
            }

            if path.starts_with("s3://")
                || path.starts_with("azure://")
                || path.starts_with("gs://")
            {
                let current = self.current().clone();
                self.report_error(
                    ParseErrorType::InvalidLiteral,
                    &format!(
                        "Cloud storage schemes (s3://, azure://, gs://) are not supported in v1.0.0. \
                         Use a direct HTTPS URL: {}",
                        path
                    ),
                    &current,
                );
                return false;
            }

            let current = self.current().clone();
            self.report_error(
                ParseErrorType::InvalidLiteral,
                &format!(
                    "Cloud import must be a valid HTTPS/HTTP URL ending in .mdix, got: {}",
                    path
                ),
                &current,
            );
            return false;
        }

        if !path_without_query.ends_with(".dixscript") {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::InvalidLiteral,
                &format!("Import path must end with '.mdix', got: {}", path_without_query),
                &current,
            );
            return false;
        }

        if path_without_query.contains('\\') {
            let current = self.current().clone();
            self.report_error(
                ParseErrorType::InvalidLiteral,
                &format!(
                    "Import path must use forward slashes, not backslashes: {}",
                    path_without_query
                ),
                &current,
            );
            return false;
        }

        if path_without_query.starts_with('/')
            || (path_without_query.len() > 1
                && path_without_query.chars().nth(1) == Some(':'))
        {
            self.error_manager.log_warning(&format!(
                "Import path '{}' looks absolute — relative paths are recommended",
                path_without_query
            ));
        }

        true
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

    fn partial_or_none(&self, start_pos: Position) -> Option<ImportsSection> {
        if self.operational_settings.error_handling_strategy == ErrorHandlingStrategy::Halt {
            None
        } else {
            Some(ImportsSection::new(Vec::new(), start_pos))
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

    fn recover_to_next_import(&mut self) -> bool {
        if self.operational_settings.error_handling_strategy != ErrorHandlingStrategy::Recover {
            return false;
        }
        for _ in 0..50 {
            if self.is_at_end() || self.is_current_symbol(')') {
                return true;
            }
            if self.is_current_symbol(',') {
                return true;
            }
            if matches!(self.current().token_type, TokenType::Identifier(_)) {
                return true;
            }
            self.advance();
        }
        false
    }

    fn advance_to_from_keyword(&mut self) -> bool {
        for _ in 0..20 {
            if self.is_at_end() {
                return false;
            }
            if self.check_keyword("from") || self.check_keyword("from_cloud") {
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
                "IMPORTS parser exceeded {} iterations — possible infinite loop",
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

#[inline]
fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        None => false,
        Some(c) => {
            (c.is_alphabetic() || c == '_')
                && chars.all(|c| c.is_alphanumeric() || c == '_')
        }
    }
}
