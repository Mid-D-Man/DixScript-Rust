//! GeneralParser — filters comments, extracts sections, delegates to section parsers.
//!
//! ## @CONFIG handling
//!
//! `ConfigSectionHandler` now replaces the @CONFIG block with blank lines before
//! this parser ever sees the token stream.  The tokenizer produces no
//! `SectionConfig` tokens from blank lines, so there is nothing to skip here.
//! The `script.config` field is populated from the pre-parsed `ConfigSection`
//! passed in via the constructor — exactly as before.
//!
//! ## Error manager propagation
//! `propagate_error_manager = true` (LSP / `new_for_lsp`):
//!   every section parser gets `self.error_manager.clone()` so diagnostics from
//!   all sub-parsers aggregate in the same isolated per-document store.
//! `propagate_error_manager = false` (CLI / `new`):
//!   section parsers call their own `new()` and use the shared process singleton.
//!   Behaviour is functionally identical to the propagating path because the
//!   top-level error manager IS the shared singleton when constructed via `new()`.

use crate::Compiler::AST::*;
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;
use crate::Compiler::Core::SectionParsers::*;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::Compiler::VersionControl::VersionConstraints;
use crate::Compiler::Utilities::{SecurityUtilities, CommentFilter};
use crate::ErrorManager::{ErrorManager, ParseException, DebugConfig};
use std::time::Instant;

#[cfg(not(target_arch = "wasm32"))]
const CONCURRENT_PARSING_ENABLED: bool = true;
#[cfg(target_arch = "wasm32")]
const CONCURRENT_PARSING_ENABLED: bool = false;

struct SectionData {
    name:     String,
    tokens:   Vec<Token>,
    position: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ParseTimings {
    pub comment_filter_ms:  f64,
    pub section_extract_ms: f64,
    pub section_parse_ms:   f64,
    pub total_ms:           f64,
}

enum ParsedSection {
    DLM(Option<DLMSection>),
    Enums(Option<EnumsSection>),
    Imports(Option<ImportsSection>),
    QuickFuncs(Option<QuickFuncsSection>),
    Data(Option<DataSection>),
    Security(Option<SecuritySection>),
}

pub struct GeneralParser<'a> {
    tokens:               Vec<Token>,
    config_section:       &'a ConfigSection,
    operational_settings: &'a OperationalSettings,
    debug_config:         DebugConfig,
    error_manager:        ErrorManager,
    position:             usize,
    allow_concurrent:     bool,
    has_imports_enabled:    bool,
    has_enums_enabled:      bool,
    has_dlm_enabled:        bool,
    has_quickfuncs_enabled: bool,
    is_advanced_mode:       bool,
    /// See module-level doc comment.
    propagate_error_manager: bool,
}

impl<'a> GeneralParser<'a> {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// Full constructor used by both public paths.
    pub fn new_with_error_manager(
        tokens:               Vec<Token>,
        config_section:       &'a ConfigSection,
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
        allow_concurrent:     bool,
    ) -> Result<Self, ParseException> {
        Self::build(tokens, config_section, operational_settings, error_manager, allow_concurrent, true)
    }

    /// CLI path — sub-parsers use shared singleton via their own `new()`.
    pub fn new(
        tokens:               Vec<Token>,
        config_section:       &'a ConfigSection,
        operational_settings: &'a OperationalSettings,
    ) -> Result<Self, ParseException> {
        Self::build(
            tokens,
            config_section,
            operational_settings,
            ErrorManager::get_shared_instance(),
            true,
            false, // CLI: sub-parsers use new() directly
        )
    }

    /// LSP path — disables rayon + propagates the document's isolated error manager.
    pub fn new_for_lsp(
        tokens:               Vec<Token>,
        config_section:       &'a ConfigSection,
        operational_settings: &'a OperationalSettings,
        error_manager:        ErrorManager,
    ) -> Result<Self, ParseException> {
        Self::build(
            tokens,
            config_section,
            operational_settings,
            error_manager,
            false, // no rayon in LSP
            true,  // propagate isolated EM to sub-parsers
        )
    }

    fn build(
        tokens:                  Vec<Token>,
        config_section:          &'a ConfigSection,
        operational_settings:    &'a OperationalSettings,
        error_manager:           ErrorManager,
        allow_concurrent:        bool,
        propagate_error_manager: bool,
    ) -> Result<Self, ParseException> {
        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

        if debug_config.is_enabled {
            error_manager.log_info("Initializing GeneralParser v1.0.0");
            error_manager.log_info(&format!(
                "Error strategy: {:?} | Compat: {:?} | Debug: {:?} | Concurrent: {} | Propagate EM: {}",
                operational_settings.error_handling_strategy,
                operational_settings.compatibility_mode,
                operational_settings.debug_mode,
                allow_concurrent,
                propagate_error_manager,
            ));
        }

        let is_advanced_mode       = operational_settings.is_advanced_mode();
        let has_quickfuncs_enabled = operational_settings.is_feature_enabled("quickfuncs");
        let has_enums_enabled      = operational_settings.is_feature_enabled("enums");
        let has_dlm_enabled        = operational_settings.is_feature_enabled("dlm");
        let has_imports_enabled    = operational_settings.is_feature_enabled("imports");

        let t_filter  = Instant::now();
        let filtered  = CommentFilter::filter(tokens)?;
        let filter_ms = t_filter.elapsed().as_secs_f64() * 1000.0;

        if debug_config.is_enabled {
            error_manager.log_debug(&format!(
                "[GeneralParser] comment-filter: {:.3} ms — {} tokens remaining",
                filter_ms,
                filtered.len(),
            ));
        }

        Ok(GeneralParser {
            tokens: filtered,
            config_section,
            operational_settings,
            debug_config,
            error_manager,
            position: 0,
            allow_concurrent,
            has_imports_enabled,
            has_enums_enabled,
            has_dlm_enabled,
            has_quickfuncs_enabled,
            is_advanced_mode,
            propagate_error_manager,
        })
    }

    // ── Main parse ────────────────────────────────────────────────────────────

    pub fn parse(mut self) -> Result<DixScript, ParseException> {
        let t_total = Instant::now();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Starting parse with {} tokens", self.tokens.len()
            ));
        }

        self.validate_version_compatibility()?;

        let mut script = DixScript::new();
        // Config section was pre-parsed by ConfigSectionHandler; populate directly.
        script.config = Some(self.config_section.clone());

        if self.tokens.len() <= 1 {
            if self.debug_config.is_enabled {
                self.error_manager.log_info("Empty program (only @CONFIG present)");
            }
            return Ok(script);
        }

        let t_extract  = Instant::now();
        let sections   = self.extract_all_sections()?;
        let extract_ms = t_extract.elapsed().as_secs_f64() * 1000.0;

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[GeneralParser] section-extract: {:.3} ms ({} sections)",
                extract_ms, sections.len(),
            ));
        }

        let t_parse   = Instant::now();
        let use_rayon = self.should_use_concurrent_parsing(&sections);

        if use_rayon {
            self.parse_sections_concurrent(sections, &mut script)?;
        } else {
            self.parse_sections_sequential(sections, &mut script)?;
        }

        let parse_ms = t_parse.elapsed().as_secs_f64() * 1000.0;
        let total_ms = t_total.elapsed().as_secs_f64() * 1000.0;

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "[GeneralParser] timings — extract: {:.3} ms | parse: {:.3} ms | total: {:.3} ms",
                extract_ms, parse_ms, total_ms,
            ));
        }

        self.ensure_security_section_exists(&mut script)?;
        Ok(script)
    }

    // ── Section parsing internals ─────────────────────────────────────────────

    fn should_use_concurrent_parsing(&self, sections: &[SectionData]) -> bool {
        if cfg!(target_arch = "wasm32") { return false; }
        CONCURRENT_PARSING_ENABLED
            && self.allow_concurrent
            && sections.len() >= 2
            && !self.debug_config.is_verbose
            && !matches!(
                self.operational_settings.error_handling_strategy,
                ErrorHandlingStrategy::Halt
            )
    }

    fn extract_all_sections(&mut self) -> Result<Vec<SectionData>, ParseException> {
        let mut sections = Vec::new();
        while !self.is_at_end() {
            self.skip_non_meaningful_tokens();
            if self.is_at_end() { break; }

            // @CONFIG was replaced with blank lines by ConfigSectionHandler
            // before tokenisation.  No SectionConfig token will ever appear
            // here, so there is no skip-logic needed.

            let start_pos      = self.position;
            let (name, tokens) = self.extract_section()?;
            sections.push(SectionData { name, tokens, position: start_pos });
        }
        Ok(sections)
    }

    fn extract_section(&mut self) -> Result<(String, Vec<Token>), ParseException> {
        let section_name = match &self.current().token_type {
            TokenType::SectionDLM        => { self.advance(); "DLM"        }
            TokenType::SectionEnums      => { self.advance(); "ENUMS"      }
            TokenType::SectionImports    => { self.advance(); "IMPORTS"    }
            TokenType::SectionQuickFuncs => { self.advance(); "QUICKFUNCS" }
            TokenType::SectionData       => { self.advance(); "DATA"       }
            TokenType::SectionSecurity   => { self.advance(); "SECURITY"   }
            other => return Err(ParseException::new(format!(
                "Expected section keyword, found: {}", other
            ))),
        };
        let packed = self.pack_section_tokens(section_name)?;
        Ok((section_name.to_string(), packed))
    }

    fn pack_section_tokens(&mut self, section_name: &str) -> Result<Vec<Token>, ParseException> {
        let section_id = SectionId::from_context_str(section_name);
        let mut packed = Vec::new();
        let mut depth  = 0i32;

        self.skip_non_meaningful_tokens();

        if self.current_matches_symbol('(') {
            packed.push(self.advance());
            depth = 1;
        } else {
            packed.push(Token::new(
                TokenType::Symbol('('),
                self.current().line,
                self.current().column,
                section_id,
            ));
            depth = 1;
        }

        while !self.is_at_end() && depth > 0 {
            let tok = self.current();
            if depth == 1 && self.is_section_keyword_token(tok) {
                packed.push(Token::new(
                    TokenType::Symbol(')'),
                    tok.line, tok.column, section_id,
                ));
                break;
            }
            match &tok.token_type {
                TokenType::Symbol('(') => depth += 1,
                TokenType::Symbol(')') => depth -= 1,
                _ => {}
            }
            packed.push(self.advance());
            if depth == 0 { break; }
        }

        let last_line   = packed.last().map(|t| t.line).unwrap_or(1);
        let last_column = packed.last().map(|t| t.column + 1).unwrap_or(1);
        packed.push(Token::eof(last_line, last_column));
        Ok(packed)
    }

    fn parse_sections_sequential(
        &self,
        sections: Vec<SectionData>,
        script:   &mut DixScript,
    ) -> Result<(), ParseException> {
        for section in sections {
            self.parse_and_assign_section(&section, script)?;
        }
        Ok(())
    }

    fn parse_sections_concurrent(
        &self,
        sections: Vec<SectionData>,
        script:   &mut DixScript,
    ) -> Result<(), ParseException> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rayon::prelude::*;
            let results: Vec<(String, Result<ParsedSection, ParseException>)> = sections
                .into_par_iter()
                .map(|section| {
                    let result = self.parse_section_inner(&section);
                    (section.name, result)
                })
                .collect();
            for (name, result) in results {
                match result {
                    Ok(parsed) => self.assign_section_to_script(parsed, script),
                    Err(e)     => self.handle_section_error(&name, e)?,
                }
            }
            return Ok(());
        }
        #[cfg(target_arch = "wasm32")]
        self.parse_sections_sequential(sections, script)
    }

    fn parse_section_inner(
        &self,
        section: &SectionData,
    ) -> Result<ParsedSection, ParseException> {
        if !self.is_section_allowed(&section.name) {
            return Err(ParseException::new(format!(
                "@{} is not allowed with current feature settings", section.name
            )));
        }
        if !self.is_section_valid_for_version(&section.name) {
            return Err(ParseException::new(format!(
                "@{} is not supported in the current version", section.name
            )));
        }

        let t = Instant::now();

        // Helper: either propagate our isolated EM or let each sub-parser
        // acquire the shared singleton via its own new().
        macro_rules! make_section_parser {
            ($Parser:ty) => {
                if self.propagate_error_manager {
                    <$Parser>::new_with_error_manager(
                        &section.tokens,
                        self.operational_settings,
                        self.error_manager.clone(),
                    )
                } else {
                    <$Parser>::new(&section.tokens, self.operational_settings)
                }
            };
        }

        let result = match section.name.as_str() {
            "DLM"        => Ok(ParsedSection::DLM(make_section_parser!(DlmSectionParser).parse_section())),
            "ENUMS"      => Ok(ParsedSection::Enums(make_section_parser!(EnumsSectionParser).parse_section())),
            "IMPORTS"    => Ok(ParsedSection::Imports(make_section_parser!(ImportsSectionParser).parse_section())),
            "QUICKFUNCS" => Ok(ParsedSection::QuickFuncs(make_section_parser!(QuickFuncsSectionParser).parse_section())),
            "DATA"       => Ok(ParsedSection::Data(make_section_parser!(DataSectionParser).parse_section())),
            "SECURITY"   => Ok(ParsedSection::Security(make_section_parser!(SecuritySectionParser).parse_section())),
            _ => Err(ParseException::new(format!("Unknown section: @{}", section.name))),
        };

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[GeneralParser] @{}: {:.3} ms",
                section.name,
                t.elapsed().as_secs_f64() * 1000.0,
            ));
        }
        result
    }

    fn parse_and_assign_section(
        &self,
        section: &SectionData,
        script:  &mut DixScript,
    ) -> Result<(), ParseException> {
        match self.parse_section_inner(section) {
            Ok(parsed) => { self.assign_section_to_script(parsed, script); Ok(()) }
            Err(e)     => self.handle_section_error(&section.name, e),
        }
    }

    fn assign_section_to_script(&self, parsed: ParsedSection, script: &mut DixScript) {
        match parsed {
            ParsedSection::DLM(r)        => script.dlm             = r,
            ParsedSection::Enums(r)      => script.enums           = r,
            ParsedSection::Imports(r)    => script.imports         = r,
            ParsedSection::QuickFuncs(r) => script.quick_functions = r,
            ParsedSection::Data(r)       => script.data            = r,
            ParsedSection::Security(r)   => script.security        = r,
        }
    }

    fn handle_section_error(
        &self,
        section_name: &str,
        error: ParseException,
    ) -> Result<(), ParseException> {
        self.error_manager.log_error(&format!(
            "Error parsing @{}: {}", section_name, error.message()
        ));
        match self.operational_settings.error_handling_strategy {
            ErrorHandlingStrategy::Halt => Err(error),
            ErrorHandlingStrategy::Continue | ErrorHandlingStrategy::Recover => Ok(()),
        }
    }

    #[inline] fn is_section_allowed(&self, name: &str) -> bool {
        match name {
            "DLM"               => self.has_dlm_enabled,
            "IMPORTS"           => self.has_imports_enabled,
            "QUICKFUNCS"        => self.has_quickfuncs_enabled,
            "ENUMS"             => self.has_enums_enabled,
            "DATA" | "SECURITY" => true,
            _                   => false,
        }
    }

    #[inline] fn is_section_valid_for_version(&self, name: &str) -> bool {
        VersionConstraints::new().is_valid_section_type(name)
    }

    fn ensure_security_section_exists(
        &self,
        script: &mut DixScript,
    ) -> Result<(), ParseException> {
        let has_encryptor = script
            .dlm
            .as_ref()
            .map(|d| d.modules.iter().any(|m| matches!(m.module_type, DLMModuleType::DEncryptor)))
            .unwrap_or(false);

        if !has_encryptor { return Ok(()); }

        script.security = Some(SecurityUtilities::ensure_valid_security_section(
            script.security.take(),
            script.dlm.as_ref(),
        ));
        Ok(())
    }

    fn validate_version_compatibility(&self) -> Result<(), ParseException> { Ok(()) }

    #[inline] fn current(&self) -> &Token {
        self.tokens.get(self.position).unwrap_or_else(|| self.tokens.last().unwrap())
    }
    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        if !self.is_at_end() { self.position += 1; }
        token
    }
    #[inline] fn is_at_end(&self) -> bool {
        matches!(self.current().token_type, TokenType::EndOfFile)
    }
    #[inline] fn current_matches_symbol(&self, expected: char) -> bool {
        matches!(self.current().token_type, TokenType::Symbol(c) if c == expected)
    }
    #[inline] fn is_section_keyword_token(&self, token: &Token) -> bool {
        matches!(
            token.token_type,
            TokenType::SectionConfig
                | TokenType::SectionDLM
                | TokenType::SectionEnums
                | TokenType::SectionImports
                | TokenType::SectionQuickFuncs
                | TokenType::SectionData
                | TokenType::SectionSecurity
        )
    }
    #[inline] fn skip_non_meaningful_tokens(&mut self) {}
}