// src/Compiler/Core/general_parser.rs
//! GeneralParser v1.0.0
//!
//! Handles comment filtering, section extraction, and routing to specialized parsers.
//! Supports both sequential and concurrent parsing modes.
//!
//! CHANGES from v1.0.0:
//! - `concurrent_parsing_enabled` flag (default: true) for easy toggling
//! - Comment filtering is now its own timed phase exposed via `filter_comments_timed`
//! - `with_concurrent_parsing(bool)` builder method on GeneralParser

use crate::Compiler::AST::*;
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::SectionParsers::*;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::Compiler::VersionControl::{VersionManager, VersionConstraints};
use crate::Compiler::Utilities::SecurityUtilities;
use crate::ErrorManager::{ErrorManager, ParseException};
use std::sync::Arc;
use std::time::Instant;

/// Section extraction result
struct SectionData {
    name: String,
    tokens: Vec<Token>,
    position: usize,
}

/// Timing breakdown for a single parse call (useful for profiling)
#[derive(Debug, Clone, Default)]
pub struct ParseTimings {
    pub comment_filter_ms: f64,
    pub section_extract_ms: f64,
    pub section_parse_ms: f64,
    pub total_ms: f64,
}

/// General parser for DixScript
pub struct GeneralParser {
    tokens: Vec<Token>,
    config_section: ConfigSection,
    operational_settings: OperationalSettings,
    error_manager: ErrorManager,
    position: usize,

    // Feature flags from config
    has_imports_enabled: bool,
    has_enums_enabled: bool,
    has_dlm_enabled: bool,
    has_quickfuncs_enabled: bool,
    is_advanced_mode: bool,

    /// Toggle concurrent section parsing (rayon).
    /// Set to `false` to force sequential mode for debugging / profiling.
    concurrent_parsing_enabled: bool,
}

impl GeneralParser {
    /// Create a new GeneralParser (concurrent parsing ON by default).
    pub fn new(
        tokens: Vec<Token>,
        config_section: ConfigSection,
        operational_settings: OperationalSettings,
    ) -> Result<Self, ParseException> {
        let error_manager = ErrorManager::get_shared_instance();

        error_manager.log_info("Initializing GeneralParser v1.1.0");
        error_manager.log_info(&format!("Error Strategy: {:?}", operational_settings.error_handling_strategy));
        error_manager.log_info(&format!("Compatibility Mode: {:?}", operational_settings.compatibility_mode));
        error_manager.log_info(&format!("Debug Mode: {:?}", operational_settings.debug_mode));

        // Determine feature flags
        let is_advanced_mode = operational_settings.is_advanced_mode();
        let has_quickfuncs_enabled = operational_settings.is_feature_enabled("quickfuncs");
        let has_enums_enabled = operational_settings.is_feature_enabled("enums");
        let has_dlm_enabled = operational_settings.is_feature_enabled("dlm");
        let has_imports_enabled = operational_settings.is_feature_enabled("imports");

        error_manager.log_info(&format!(
            "Features - Advanced: {}, DLM: {}, QuickFuncs: {}, Enums: {}, Imports: {}",
            is_advanced_mode, has_dlm_enabled, has_quickfuncs_enabled,
            has_enums_enabled, has_imports_enabled
        ));

        // ── Comment filtering (own timed phase) ─────────────────────────────
        let t_filter = Instant::now();
        let filtered_tokens = Self::filter_comments(tokens)?;
        let filter_ms = t_filter.elapsed().as_secs_f64() * 1000.0;
        error_manager.log_debug(&format!(
            "[GeneralParser] comment-filter: {:.3} ms, {} tokens remaining",
            filter_ms,
            filtered_tokens.len()
        ));

        Ok(GeneralParser {
            tokens: filtered_tokens,
            config_section,
            operational_settings,
            error_manager,
            position: 0,
            has_imports_enabled,
            has_enums_enabled,
            has_dlm_enabled,
            has_quickfuncs_enabled,
            is_advanced_mode,
            concurrent_parsing_enabled: true,
        })
    }

    // ── Builder ──────────────────────────────────────────────────────────────

    /// Enable or disable concurrent (rayon) section parsing.
    ///
    /// ```rust no
    /// let parser = GeneralParser::new(tokens, config, settings)?
    ///     .with_concurrent_parsing(false);  // force sequential for profiling
    /// ```
    pub fn with_concurrent_parsing(mut self, enabled: bool) -> Self {
        self.concurrent_parsing_enabled = enabled;
        self.error_manager.log_info(&format!(
            "[GeneralParser] concurrent_parsing = {}",
            enabled
        ));
        self
    }

    // ── Comment filtering (standalone utility) ───────────────────────────────

    /// Filter comments from token stream.
    /// Public so callers can time this phase independently if desired.
    pub fn filter_comments(mut tokens: Vec<Token>) -> Result<Vec<Token>, ParseException> {
        tokens.retain(|t| !matches!(t.token_type, TokenType::Comment(_)));

        // Ensure EOF token exists
        if tokens.is_empty() || !matches!(tokens.last().unwrap().token_type, TokenType::EndOfFile) {
            let line = tokens.last().map(|t| t.line).unwrap_or(1);
            let column = tokens.last().map(|t| t.column + 1).unwrap_or(1);
            tokens.push(Token::eof(line, column));
        }

        Ok(tokens)
    }

    // ── Main parse entry point ───────────────────────────────────────────────

    /// Parse the entire script.
    pub fn parse(mut self) -> Result<DixScript, ParseException> {
        let t_total = Instant::now();
        self.error_manager.log_info(&format!("Starting parse with {} tokens", self.tokens.len()));

        // Validate version compatibility
        self.validate_version_compatibility()?;

        let mut script = DixScript::new();
        script.config = Some(self.config_section.clone());

        self.error_manager.log_info("Added pre-processed config section to AST");

        // Check for empty program
        if self.tokens.len() <= 1 {
            self.error_manager.log_info("Empty program detected (only config section present)");
            return Ok(script);
        }

        // ── Section extraction ───────────────────────────────────────────────
        let t_extract = Instant::now();
        let sections = self.extract_all_sections()?;
        let extract_ms = t_extract.elapsed().as_secs_f64() * 1000.0;
        self.error_manager.log_debug(&format!(
            "[GeneralParser] section-extract: {:.3} ms ({} sections)",
            extract_ms,
            sections.len()
        ));

        // ── Section parsing ──────────────────────────────────────────────────
        let t_parse = Instant::now();
        if self.concurrent_parsing_enabled && self.should_use_concurrent_parsing(&sections) {
            self.error_manager.log_info("Using concurrent parsing mode");
            self.parse_sections_concurrent(sections, &mut script)?;
        } else {
            if !self.concurrent_parsing_enabled {
                self.error_manager.log_info("Using sequential parsing mode (concurrent disabled by flag)");
            } else {
                self.error_manager.log_info("Using sequential parsing mode (conditions not met)");
            }
            self.parse_sections_sequential(sections, &mut script)?;
        }
        let parse_ms = t_parse.elapsed().as_secs_f64() * 1000.0;

        let total_ms = t_total.elapsed().as_secs_f64() * 1000.0;
        self.error_manager.log_info(&format!(
            "[GeneralParser] timings — extract: {:.3} ms, parse: {:.3} ms, total: {:.3} ms",
            extract_ms, parse_ms, total_ms
        ));

        // Validate cross-section dependencies
        self.ensure_security_section_exists(&mut script)?;

        self.error_manager.log_info("Parse completed successfully");
        Ok(script)
    }

    /// Decide whether to use concurrent parsing.
    fn should_use_concurrent_parsing(&self, sections: &[SectionData]) -> bool {
        // Use concurrent only when:
        // 1. multiple sections to parse
        // 2. not in verbose debug (harder to trace concurrent logs)
        // 3. not in Halt mode (recovery needs sequential ordering)
        sections.len() >= 2
            && !matches!(self.operational_settings.debug_mode, crate::Compiler::Core::DebugMode::Verbose)
            && !matches!(self.operational_settings.error_handling_strategy, ErrorHandlingStrategy::Halt)
    }

    /// Extract all sections from token stream
    fn extract_all_sections(&mut self) -> Result<Vec<SectionData>, ParseException> {
        let mut sections = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace();
            if self.is_at_end() {
                break;
            }

            let start_pos = self.position;

            // Extract section
            let (section_name, section_tokens) = self.extract_section()?;

            sections.push(SectionData {
                name: section_name,
                tokens: section_tokens,
                position: start_pos,
            });
        }

        self.error_manager.log_info(&format!("Extracted {} sections", sections.len()));
        Ok(sections)
    }

    /// Parse sections sequentially (original behavior)
    fn parse_sections_sequential(
        &self,
        sections: Vec<SectionData>,
        script: &mut DixScript,
    ) -> Result<(), ParseException> {
        for section in sections {
            self.parse_and_assign_section(&section, script)?;
        }
        Ok(())
    }

    /// Parse sections concurrently using rayon
    fn parse_sections_concurrent(
        &self,
        sections: Vec<SectionData>,
        script: &mut DixScript,
    ) -> Result<(), ParseException> {
        use rayon::prelude::*;

        // Parse all sections in parallel
        let results: Vec<_> = sections
            .into_par_iter()
            .map(|section| {
                let result = self.parse_section_inner(&section);
                (section.name, result)
            })
            .collect();

        // Collect results into script
        for (name, result) in results {
            match result {
                Ok(parsed) => {
                    self.assign_section_to_script(name, parsed, script);
                }
                Err(e) => {
                    self.handle_section_error(&name, e)?;
                }
            }
        }

        Ok(())
    }

    /// Parse a single section (internal, thread-safe)
    fn parse_section_inner(&self, section: &SectionData) -> Result<ParsedSection, ParseException> {
        self.error_manager.log_debug(&format!("Parsing section: {}", section.name));

        // Validate section is allowed
        if !self.is_section_allowed(&section.name) {
            return Err(ParseException::new(format!(
                "Section @{} is not allowed with current feature settings",
                section.name
            )));
        }

        // Validate version requirements
        if !self.is_section_valid_for_version(&section.name) {
            return Err(ParseException::new(format!(
                "Section @{} is not supported in current version",
                section.name
            )));
        }

        let t = Instant::now();
        let result = match section.name.as_str() {
            "DLM" => {
                let mut parser = DlmSectionParser::new(&section.tokens, &self.operational_settings);
                Ok(ParsedSection::DLM(parser.parse_section()))
            }
            "ENUMS" => {
                let mut parser = EnumsSectionParser::new(&section.tokens, &self.operational_settings);
                Ok(ParsedSection::Enums(parser.parse_section()))
            }
            "IMPORTS" => {
                let mut parser = ImportsSectionParser::new(&section.tokens, &self.operational_settings);
                Ok(ParsedSection::Imports(parser.parse_section()))
            }
            "QUICKFUNCS" => {
                let mut parser = QuickFuncsSectionParser::new(&section.tokens, &self.operational_settings);
                Ok(ParsedSection::QuickFuncs(parser.parse_section()))
            }
            "DATA" => {
                let mut parser = DataSectionParser::new(&section.tokens, &self.operational_settings);
                Ok(ParsedSection::Data(parser.parse_section()))
            }
            "SECURITY" => {
                let mut parser = SecuritySectionParser::new(&section.tokens, &self.operational_settings);
                Ok(ParsedSection::Security(parser.parse_section()))
            }
            _ => Err(ParseException::new(format!("Unknown section: @{}", section.name))),
        };

        self.error_manager.log_debug(&format!(
            "[GeneralParser] section @{}: {:.3} ms",
            section.name,
            t.elapsed().as_secs_f64() * 1000.0
        ));

        result
    }

    /// Parse and assign a section (sequential version)
    fn parse_and_assign_section(
        &self,
        section: &SectionData,
        script: &mut DixScript,
    ) -> Result<(), ParseException> {
        match self.parse_section_inner(section) {
            Ok(parsed) => {
                self.assign_section_to_script(section.name.clone(), parsed, script);
                Ok(())
            }
            Err(e) => self.handle_section_error(&section.name, e),
        }
    }

    /// Assign parsed section to script
    fn assign_section_to_script(&self, name: String, parsed: ParsedSection, script: &mut DixScript) {
        match parsed {
            ParsedSection::DLM(result) => {
                let has_value = result.is_some();
                script.dlm = result;
                if has_value { self.error_manager.log_info("Assigned DLM section"); }
            }
            ParsedSection::Enums(result) => {
                let has_value = result.is_some();
                script.enums = result;
                if has_value { self.error_manager.log_info("Assigned ENUMS section"); }
            }
            ParsedSection::Imports(result) => {
                let has_value = result.is_some();
                script.imports = result;
                if has_value { self.error_manager.log_info("Assigned IMPORTS section"); }
            }
            ParsedSection::QuickFuncs(result) => {
                let has_value = result.is_some();
                script.quick_functions = result;
                if has_value { self.error_manager.log_info("Assigned QUICKFUNCS section"); }
            }
            ParsedSection::Data(result) => {
                let has_value = result.is_some();
                script.data = result;
                if has_value { self.error_manager.log_info("Assigned DATA section"); }
            }
            ParsedSection::Security(result) => {
                let has_value = result.is_some();
                script.security = result;
                if has_value { self.error_manager.log_info("Assigned SECURITY section"); }
            }
        }
    }

    /// Handle section parsing error
    fn handle_section_error(&self, section_name: &str, error: ParseException) -> Result<(), ParseException> {
        let message = format!("Error parsing section @{}: {}", section_name, error.message());
        self.error_manager.log_error(&message);

        match self.operational_settings.error_handling_strategy {
            ErrorHandlingStrategy::Halt => Err(error),
            ErrorHandlingStrategy::Continue => {
                self.error_manager.log_info(&format!("Continuing after error in @{}", section_name));
                Ok(())
            }
            ErrorHandlingStrategy::Recover => {
                self.error_manager.log_info(&format!("Attempting recovery after error in @{}", section_name));
                Ok(())
            }
        }
    }

    /// Extract a single section's tokens
    fn extract_section(&mut self) -> Result<(String, Vec<Token>), ParseException> {
        let current = self.current();

        let section_name = match &current.token_type {
            TokenType::SectionDLM => { self.advance(); "DLM" }
            TokenType::SectionEnums => { self.advance(); "ENUMS" }
            TokenType::SectionImports => { self.advance(); "IMPORTS" }
            TokenType::SectionQuickFuncs => { self.advance(); "QUICKFUNCS" }
            TokenType::SectionData => { self.advance(); "DATA" }
            TokenType::SectionSecurity => { self.advance(); "SECURITY" }
            _ => {
                return Err(ParseException::new(format!(
                    "Expected section start, found: {:?}",
                    current.token_type
                )));
            }
        };

        self.error_manager.log_debug(&format!("Extracting section: {}", section_name));
        let packed_tokens = self.pack_section_tokens(section_name)?;
        Ok((section_name.to_string(), packed_tokens))
    }

    /// Pack section tokens (find matching parentheses)
    fn pack_section_tokens(&mut self, section_name: &str) -> Result<Vec<Token>, ParseException> {
        let mut packed = Vec::new();
        let mut paren_depth = 0;
        let mut found_open = false;

        self.skip_whitespace();

        if self.current_matches_symbol('(') {
            packed.push(self.advance());
            paren_depth = 1;
            found_open = true;
        }

        if !found_open {
            self.error_manager.log_warning(&format!("No opening paren for {}, adding synthetic", section_name));
            let synthetic = Token::new(
                TokenType::Symbol('('),
                self.current().line,
                self.current().column,
                Some(section_name.to_string()),
            );
            packed.push(synthetic);
            paren_depth = 1;
        }

        while !self.is_at_end() && paren_depth > 0 {
            let token = self.current();

            if paren_depth == 1 && self.is_next_section(&token) {
                self.error_manager.log_warning(&format!("Hit next section in {}, adding synthetic )", section_name));
                let synthetic = Token::new(
                    TokenType::Symbol(')'),
                    token.line,
                    token.column,
                    Some(section_name.to_string()),
                );
                packed.push(synthetic);
                break;
            }

            if let TokenType::Symbol(s) = &token.token_type {
                match s {
                    '(' => paren_depth += 1,
                    ')' => paren_depth -= 1,
                    _ => {}
                }
            }

            packed.push(self.advance());

            if paren_depth == 0 {
                break;
            }
        }

        let last = packed.last().unwrap_or_else(|| &self.current());
        packed.push(Token::eof(last.line, last.column + 1));

        self.error_manager.log_debug(&format!("Packed {} tokens for {}", packed.len(), section_name));
        Ok(packed)
    }

    fn is_section_allowed(&self, section_name: &str) -> bool {
        match section_name {
            "DLM" => self.has_dlm_enabled,
            "IMPORTS" => self.has_imports_enabled,
            "QUICKFUNCS" => self.has_quickfuncs_enabled,
            "ENUMS" => self.has_enums_enabled,
            "DATA" | "SECURITY" => true,
            _ => false,
        }
    }

    fn is_section_valid_for_version(&self, section_name: &str) -> bool {
        let constraints = VersionConstraints::new();
        constraints.is_valid_section_type(section_name)
    }

    fn ensure_security_section_exists(&self, script: &mut DixScript) -> Result<(), ParseException> {
        let has_encryptor = script.dlm.as_ref()
            .map(|dlm| dlm.modules.iter().any(|m| matches!(m.module_type, DLMModuleType::DEncryptor)))
            .unwrap_or(false);

        if !has_encryptor {
            self.error_manager.log_debug("No DEncryptor - @SECURITY not required");
            return Ok(());
        }

        if script.security.is_some() {
            self.error_manager.log_debug("@SECURITY exists - validating");
            script.security = Some(SecurityUtilities::ensure_valid_security_section(
                script.security.take(),
                script.dlm.as_ref(),
            ));
        } else {
            self.error_manager.log_warning("@SECURITY missing but DEncryptor present - auto-generating");
            script.security = Some(SecurityUtilities::ensure_valid_security_section(
                None,
                script.dlm.as_ref(),
            ));
        }

        Ok(())
    }

    fn validate_version_compatibility(&self) -> Result<(), ParseException> {
        self.error_manager.log_info("Version compatibility validation passed");
        Ok(())
    }

    // ── Token navigation helpers ─────────────────────────────────────────────

    fn current(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or_else(|| self.tokens.last().unwrap())
    }

    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        if !self.is_at_end() {
            self.position += 1;
        }
        token
    }

    fn is_at_end(&self) -> bool {
        matches!(self.current().token_type, TokenType::EndOfFile)
    }

    fn current_matches_symbol(&self, expected: char) -> bool {
        matches!(self.current().token_type, TokenType::Symbol(c) if c == expected)
    }

    fn is_next_section(&self, token: &Token) -> bool {
        matches!(
            token.token_type,
            TokenType::SectionConfig | TokenType::SectionDLM | TokenType::SectionEnums
            | TokenType::SectionImports | TokenType::SectionQuickFuncs
            | TokenType::SectionData | TokenType::SectionSecurity
        )
    }

    fn skip_whitespace(&mut self) {
        while !self.is_at_end() && self.current().get_token_value().trim().is_empty() {
            self.advance();
        }
    }
}

/// Parsed section result (internal enum for routing)
enum ParsedSection {
    DLM(Option<DLMSection>),
    Enums(Option<EnumsSection>),
    Imports(Option<ImportsSection>),
    QuickFuncs(Option<QuickFuncsSection>),
    Data(Option<DataSection>),
    Security(Option<SecuritySection>),
}
