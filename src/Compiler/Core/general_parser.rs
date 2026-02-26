// src/Compiler/Core/general_parser.rs
//! GeneralParser v1.1.0
//!
//! ## Changes from v1.0.0
//!
//! ### Lifetime `'a` on `GeneralParser<'a>`
//! `OperationalSettings` is now **borrowed** (`&'a OperationalSettings`) rather
//! than cloned.  The parser must not outlive the settings object, which is the
//! natural caller-owns-settings pattern anyway.  Tokens remain **owned** because
//! the parser mutates them (comment filtering, section extraction, synthetic
//! token insertion) and hands slices down to section parsers.
//!
//! ### `DebugConfig` replaces direct `debug_mode` checks
//! A `DebugConfig` is created once from `operational_settings` in `new()` and
//! stored on the struct.  All log-gate decisions (`is_enabled`, `is_verbose`)
//! go through `self.debug_config`, matching the pattern used in the lexer and
//! section parsers.
//!
//! ### `CONCURRENT_PARSING_ENABLED` constant
//! Replaces the `concurrent_parsing_enabled: bool` struct field.  Set it to
//! `false` at the top of this file to force sequential mode for debugging /
//! flamegraph profiling.  Leave it `true` for all other builds.
//!
//! ### `CommentFilter` utility
//! Comment stripping is now delegated to `CommentFilter::filter` from
//! `src/Compiler/Utilities/comment_filter.rs` so the logic is reusable outside
//! the parser.
//!
//! ### `SectionId` in synthetic tokens
//! The old `pack_section_tokens` passed `Some(section_name.to_string())` to
//! `Token::new`, which no longer matches the updated `Token::new(…, SectionId)`
//! signature.  All synthetic token construction now uses
//! `SectionId::from_context_str(section_name)`.

use crate::Compiler::AST::*;
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::Core::Tokenizer::token::SectionId;
use crate::Compiler::Core::SectionParsers::*;
use crate::Compiler::Core::{OperationalSettings, ErrorHandlingStrategy};
use crate::Compiler::VersionControl::VersionConstraints;
use crate::Compiler::Utilities::{SecurityUtilities, CommentFilter};
use crate::ErrorManager::{ErrorManager, ParseException, DebugConfig};
use std::time::Instant;

// ─────────────────────────────────────────────────────────────────────────────
// Module-level constant — flip to `false` to force sequential parsing.
// ─────────────────────────────────────────────────────────────────────────────

/// Enable rayon-based concurrent section parsing.
///
/// Set to `false` here (not at runtime) to force sequential mode when
/// debugging or running flamegraph profiles where interleaved log output
/// would be confusing.  Leave `true` for production builds.
const CONCURRENT_PARSING_ENABLED: bool = true;

// ─────────────────────────────────────────────────────────────────────────────
// Internal types
// ─────────────────────────────────────────────────────────────────────────────

/// Token bundle for one extracted section.
struct SectionData {
    name:     String,
    tokens:   Vec<Token>,
    /// Token-stream index where this section began (for diagnostics).
    position: usize,
}

/// Timing breakdown exposed for profiling / telemetry.
#[derive(Debug, Clone, Default)]
pub struct ParseTimings {
    pub comment_filter_ms:  f64,
    pub section_extract_ms: f64,
    pub section_parse_ms:   f64,
    pub total_ms:           f64,
}

/// Parsed section result — internal routing enum.
enum ParsedSection {
    DLM(Option<DLMSection>),
    Enums(Option<EnumsSection>),
    Imports(Option<ImportsSection>),
    QuickFuncs(Option<QuickFuncsSection>),
    Data(Option<DataSection>),
    Security(Option<SecuritySection>),
}

// ─────────────────────────────────────────────────────────────────────────────
// GeneralParser<'a>
// ─────────────────────────────────────────────────────────────────────────────

/// General parser for DixScript.
///
/// **Ownership notes**
/// - `tokens`               — owned; the parser filters and slices this vec.
/// - `config_section`       — borrowed; already processed before this parser runs so just clone.
/// - `operational_settings` — borrowed for `'a`; parser must not outlive settings.
/// - Section parsers        — receive `&[Token]` slices from per-section vecs.
pub struct GeneralParser<'a> {
    tokens:               Vec<Token>,
    config_section:      &'a ConfigSection,
    operational_settings: &'a OperationalSettings,

    /// Cached at construction time from `operational_settings.debug_mode`.
    /// Use this for all log-gate decisions — never read `debug_mode` directly.
    debug_config:         DebugConfig,

    error_manager:        ErrorManager,
    position:             usize,

    // Feature gates derived from settings at construction — one read, many uses.
    has_imports_enabled:    bool,
    has_enums_enabled:      bool,
    has_dlm_enabled:        bool,
    has_quickfuncs_enabled: bool,
    is_advanced_mode:       bool,
}

impl<'a> GeneralParser<'a> {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Create a new `GeneralParser`.
    ///
    /// - `tokens` are **consumed** and comment-filtered internally.
    /// - `operational_settings` is **borrowed** for `'a`.
    pub fn new(
        tokens:               Vec<Token>,
        config_section:      &'a ConfigSection,
        operational_settings: &'a OperationalSettings,
    ) -> Result<Self, ParseException> {
        let error_manager = ErrorManager::get_shared_instance();

        // Build DebugConfig once.  All log gates flow through this.
        let debug_config = DebugConfig::from_debug_mode(operational_settings.debug_mode);

        if debug_config.is_enabled {
            error_manager.log_info("Initializing GeneralParser v1.1.0");
            error_manager.log_info(&format!(
                "Error strategy: {:?} | Compat: {:?} | Debug: {:?}",
                operational_settings.error_handling_strategy,
                operational_settings.compatibility_mode,
                operational_settings.debug_mode,
            ));
        }

        // Derive feature gates once so each section check is O(1).
        let is_advanced_mode       = operational_settings.is_advanced_mode();
        let has_quickfuncs_enabled = operational_settings.is_feature_enabled("quickfuncs");
        let has_enums_enabled      = operational_settings.is_feature_enabled("enums");
        let has_dlm_enabled        = operational_settings.is_feature_enabled("dlm");
        let has_imports_enabled    = operational_settings.is_feature_enabled("imports");

        if debug_config.is_enabled {
            error_manager.log_info(&format!(
                "Features — Advanced: {}, DLM: {}, QuickFuncs: {}, Enums: {}, Imports: {}",
                is_advanced_mode,
                has_dlm_enabled,
                has_quickfuncs_enabled,
                has_enums_enabled,
                has_imports_enabled,
            ));
        }

        // ── Comment filtering (delegated to utility) ──────────────────────
        let t_filter   = Instant::now();
        let filtered   = CommentFilter::filter(tokens)?;
        let filter_ms  = t_filter.elapsed().as_secs_f64() * 1000.0;

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
            has_imports_enabled,
            has_enums_enabled,
            has_dlm_enabled,
            has_quickfuncs_enabled,
            is_advanced_mode,
        })
    }

    // ── Main entry point ──────────────────────────────────────────────────────

    /// Parse the complete token stream into a [`DixScript`] AST.
    ///
    /// Consumes the parser.
    pub fn parse(mut self) -> Result<DixScript, ParseException> {
        let t_total = Instant::now();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Starting parse with {} tokens",
                self.tokens.len()
            ));
        }

        self.validate_version_compatibility()?;

        let mut script = DixScript::new();
        script.config  = Some(self.config_section.clone());

        if self.debug_config.is_enabled {
            self.error_manager.log_info("Pre-processed @CONFIG added to AST");
        }

        // Nothing else to do for a config-only program.
        if self.tokens.len() <= 1 {
            if self.debug_config.is_enabled {
                self.error_manager.log_info("Empty program (only @CONFIG present)");
            }
            return Ok(script);
        }

        // ── Section extraction ────────────────────────────────────────────
        let t_extract  = Instant::now();
        let sections   = self.extract_all_sections()?;
        let extract_ms = t_extract.elapsed().as_secs_f64() * 1000.0;

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[GeneralParser] section-extract: {:.3} ms ({} sections)",
                extract_ms,
                sections.len(),
            ));
        }

        // ── Section parsing ───────────────────────────────────────────────
        let t_parse = Instant::now();

        if CONCURRENT_PARSING_ENABLED && self.should_use_concurrent_parsing(&sections) {
            if self.debug_config.is_enabled {
                self.error_manager.log_info("Using concurrent parsing mode (rayon)");
            }
            self.parse_sections_concurrent(sections, &mut script)?;
        } else {
            if self.debug_config.is_enabled {
                let reason = if !CONCURRENT_PARSING_ENABLED {
                    "(CONCURRENT_PARSING_ENABLED = false)"
                } else {
                    "(conditions not met for concurrent)"
                };
                self.error_manager
                    .log_info(&format!("Using sequential parsing mode {}", reason));
            }
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

        if self.debug_config.is_enabled {
            self.error_manager.log_info("Parse completed successfully");
        }

        Ok(script)
    }

    // ── Concurrent vs sequential decision ────────────────────────────────────

    /// Use rayon when there are enough sections to justify the overhead, we are
    /// not in verbose debug mode (ordered output is valuable there), and the
    /// error strategy is not Halt (order-dependent recovery is simpler
    /// sequentially).
    fn should_use_concurrent_parsing(&self, sections: &[SectionData]) -> bool {
        sections.len() >= 2
            && !self.debug_config.is_verbose
            && !matches!(
                self.operational_settings.error_handling_strategy,
                ErrorHandlingStrategy::Halt
            )
    }

    // ── Section extraction ────────────────────────────────────────────────────

    fn extract_all_sections(&mut self) -> Result<Vec<SectionData>, ParseException> {
        let mut sections = Vec::new();

        while !self.is_at_end() {
            // The lexer never emits whitespace tokens, so this is a no-op in
            // practice.  Kept as a forward-compatibility safety net.
            self.skip_non_meaningful_tokens();

            if self.is_at_end() {
                break;
            }

            let start_pos               = self.position;
            let (name, tokens)          = self.extract_section()?;
            sections.push(SectionData { name, tokens, position: start_pos });
        }

        if self.debug_config.is_enabled {
            self.error_manager
                .log_info(&format!("Extracted {} sections", sections.len()));
        }

        Ok(sections)
    }

    fn extract_section(&mut self) -> Result<(String, Vec<Token>), ParseException> {
        // Advance past the section-keyword token and capture its name.
        let section_name = match &self.current().token_type {
            TokenType::SectionDLM        => { self.advance(); "DLM"        }
            TokenType::SectionEnums      => { self.advance(); "ENUMS"      }
            TokenType::SectionImports    => { self.advance(); "IMPORTS"    }
            TokenType::SectionQuickFuncs => { self.advance(); "QUICKFUNCS" }
            TokenType::SectionData       => { self.advance(); "DATA"       }
            TokenType::SectionSecurity   => { self.advance(); "SECURITY"   }
            other => {
                return Err(ParseException::new(format!(
                    "Expected section keyword, found: {}",
                    other
                )));
            }
        };

        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug(&format!("Extracting @{}", section_name));
        }

        let packed = self.pack_section_tokens(section_name)?;
        Ok((section_name.to_string(), packed))
    }

    /// Collect all tokens for this section up to (and including) the matching
    /// closing `)`.  Inserts synthetic `(` / `)` tokens when the source is
    /// malformed so that downstream section parsers always receive a
    /// syntactically framed token stream and can emit proper diagnostics.
    fn pack_section_tokens(
        &mut self,
        section_name: &str,
    ) -> Result<Vec<Token>, ParseException> {
        let section_id = SectionId::from_context_str(section_name);
        let mut packed = Vec::new();
        let mut depth  = 0i32;

        self.skip_non_meaningful_tokens();

        // ── Opening '(' ───────────────────────────────────────────────────
        if self.current_matches_symbol('(') {
            packed.push(self.advance());
            depth = 1;
        } else {
            // Malformed source: synthesise an opener so the section parser has
            // something balanced to work with.
            if self.debug_config.is_enabled {
                self.error_manager.log_warning(&format!(
                    "No opening '(' for @{} — inserting synthetic token",
                    section_name
                ));
            }
            packed.push(Token::new(
                TokenType::Symbol('('),
                self.current().line,
                self.current().column,
                section_id,
            ));
            depth = 1;
        }

        // ── Body ──────────────────────────────────────────────────────────
        while !self.is_at_end() && depth > 0 {
            let tok = self.current();

            // Guard: stop before the next top-level section keyword so a
            // missing `)` doesn't swallow the rest of the file.
            if depth == 1 && self.is_section_keyword_token(tok) {
                if self.debug_config.is_enabled {
                    self.error_manager.log_warning(&format!(
                        "Hit next section inside @{} — inserting synthetic ')'",
                        section_name
                    ));
                }
                packed.push(Token::new(
                    TokenType::Symbol(')'),
                    tok.line,
                    tok.column,
                    section_id,
                ));
                break; // do NOT advance — leave the section keyword for the outer loop
            }

            // Track paren depth.
            match &tok.token_type {
                TokenType::Symbol('(') => depth += 1,
                TokenType::Symbol(')') => depth -= 1,
                _ => {}
            }

            packed.push(self.advance());

            if depth == 0 {
                break;
            }
        }

        // EOF sentinel required by all section parsers.
        let last_line   = packed.last().map(|t| t.line).unwrap_or(1);
        let last_column = packed.last().map(|t| t.column + 1).unwrap_or(1);
        packed.push(Token::eof(last_line, last_column));

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Packed {} tokens for @{}",
                packed.len(),
                section_name
            ));
        }

        Ok(packed)
    }

    // ── Sequential parsing ────────────────────────────────────────────────────

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

    // ── Concurrent parsing ────────────────────────────────────────────────────

    /// Phase 1 — parse all sections in parallel (`&self` is read-only during
    /// this phase, which is safe because `ErrorManager` is `Arc<Mutex<…>>`).
    ///
    /// Phase 2 — assign results to `script` sequentially (requires `&mut`).
    fn parse_sections_concurrent(
        &self,
        sections: Vec<SectionData>,
        script:   &mut DixScript,
    ) -> Result<(), ParseException> {
        use rayon::prelude::*;

        // Phase 1: parallel parse.
        let results: Vec<(String, Result<ParsedSection, ParseException>)> = sections
            .into_par_iter()
            .map(|section| {
                let result = self.parse_section_inner(&section);
                (section.name, result)
            })
            .collect();

        // Phase 2: sequential assignment.
        for (name, result) in results {
            match result {
                Ok(parsed) => self.assign_section_to_script(parsed, script),
                Err(e)     => self.handle_section_error(&name, e)?,
            }
        }

        Ok(())
    }

    // ── Inner section parse (thread-safe, reads `&self` only) ────────────────

    fn parse_section_inner(
        &self,
        section: &SectionData,
    ) -> Result<ParsedSection, ParseException> {
        if self.debug_config.is_enabled {
            self.error_manager
                .log_debug(&format!("Parsing @{}", section.name));
        }

        if !self.is_section_allowed(&section.name) {
            return Err(ParseException::new(format!(
                "@{} is not allowed with current feature settings",
                section.name
            )));
        }

        if !self.is_section_valid_for_version(&section.name) {
            return Err(ParseException::new(format!(
                "@{} is not supported in the current version",
                section.name
            )));
        }

        let t = Instant::now();

        let result = match section.name.as_str() {
            "DLM" => {
                let mut p = DlmSectionParser::new(&section.tokens, self.operational_settings);
                Ok(ParsedSection::DLM(p.parse_section()))
            }
            "ENUMS" => {
                let mut p = EnumsSectionParser::new(&section.tokens, self.operational_settings);
                Ok(ParsedSection::Enums(p.parse_section()))
            }
            "IMPORTS" => {
                let mut p = ImportsSectionParser::new(&section.tokens, self.operational_settings);
                Ok(ParsedSection::Imports(p.parse_section()))
            }
            "QUICKFUNCS" => {
                let mut p = QuickFuncsSectionParser::new(&section.tokens, self.operational_settings);
                Ok(ParsedSection::QuickFuncs(p.parse_section()))
            }
            "DATA" => {
                let mut p = DataSectionParser::new(&section.tokens, self.operational_settings);
                Ok(ParsedSection::Data(p.parse_section()))
            }
            "SECURITY" => {
                let mut p = SecuritySectionParser::new(&section.tokens, self.operational_settings);
                Ok(ParsedSection::Security(p.parse_section()))
            }
            _ => Err(ParseException::new(format!(
                "Unknown section: @{}",
                section.name
            ))),
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
            Ok(parsed) => {
                self.assign_section_to_script(parsed, script);
                Ok(())
            }
            Err(e) => self.handle_section_error(&section.name, e),
        }
    }

    fn assign_section_to_script(&self, parsed: ParsedSection, script: &mut DixScript) {
        match parsed {
            ParsedSection::DLM(result) => {
                if result.is_some() && self.debug_config.is_enabled {
                    self.error_manager.log_info("Assigned @DLM section");
                }
                script.dlm = result;
            }
            ParsedSection::Enums(result) => {
                if result.is_some() && self.debug_config.is_enabled {
                    self.error_manager.log_info("Assigned @ENUMS section");
                }
                script.enums = result;
            }
            ParsedSection::Imports(result) => {
                if result.is_some() && self.debug_config.is_enabled {
                    self.error_manager.log_info("Assigned @IMPORTS section");
                }
                script.imports = result;
            }
            ParsedSection::QuickFuncs(result) => {
                if result.is_some() && self.debug_config.is_enabled {
                    self.error_manager.log_info("Assigned @QUICKFUNCS section");
                }
                script.quick_functions = result;
            }
            ParsedSection::Data(result) => {
                if result.is_some() && self.debug_config.is_enabled {
                    self.error_manager.log_info("Assigned @DATA section");
                }
                script.data = result;
            }
            ParsedSection::Security(result) => {
                if result.is_some() && self.debug_config.is_enabled {
                    self.error_manager.log_info("Assigned @SECURITY section");
                }
                script.security = result;
            }
        }
    }

    fn handle_section_error(
        &self,
        section_name: &str,
        error: ParseException,
    ) -> Result<(), ParseException> {
        self.error_manager.log_error(&format!(
            "Error parsing @{}: {}",
            section_name,
            error.message()
        ));

        match self.operational_settings.error_handling_strategy {
            ErrorHandlingStrategy::Halt => Err(error),
            ErrorHandlingStrategy::Continue => {
                if self.debug_config.is_enabled {
                    self.error_manager
                        .log_info(&format!("Continuing after error in @{}", section_name));
                }
                Ok(())
            }
            ErrorHandlingStrategy::Recover => {
                if self.debug_config.is_enabled {
                    self.error_manager
                        .log_info(&format!("Recovering after error in @{}", section_name));
                }
                Ok(())
            }
        }
    }

    // ── Feature / version guards ──────────────────────────────────────────────

    #[inline]
    fn is_section_allowed(&self, section_name: &str) -> bool {
        match section_name {
            "DLM"        => self.has_dlm_enabled,
            "IMPORTS"    => self.has_imports_enabled,
            "QUICKFUNCS" => self.has_quickfuncs_enabled,
            "ENUMS"      => self.has_enums_enabled,
            "DATA" | "SECURITY" => true,
            _ => false,
        }
    }

    #[inline]
    fn is_section_valid_for_version(&self, section_name: &str) -> bool {
        VersionConstraints::new().is_valid_section_type(section_name)
    }

    fn ensure_security_section_exists(
        &self,
        script: &mut DixScript,
    ) -> Result<(), ParseException> {
        let has_encryptor = script
            .dlm
            .as_ref()
            .map(|dlm| {
                dlm.modules
                    .iter()
                    .any(|m| matches!(m.module_type, DLMModuleType::DEncryptor))
            })
            .unwrap_or(false);

        if !has_encryptor {
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_debug("No DEncryptor — @SECURITY not required");
            }
            return Ok(());
        }

        if script.security.is_some() {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug("@SECURITY present — validating");
            }
            script.security = Some(SecurityUtilities::ensure_valid_security_section(
                script.security.take(),
                script.dlm.as_ref(),
            ));
        } else {
            self.error_manager
                .log_warning("@SECURITY missing but DEncryptor present — auto-generating");
            script.security = Some(SecurityUtilities::ensure_valid_security_section(
                None,
                script.dlm.as_ref(),
            ));
        }

        Ok(())
    }

    fn validate_version_compatibility(&self) -> Result<(), ParseException> {
        if self.debug_config.is_enabled {
            self.error_manager
                .log_info("Version compatibility check passed");
        }
        Ok(())
    }

    // ── Token navigation ──────────────────────────────────────────────────────

    #[inline]
    fn current(&self) -> &Token {
        self.tokens
            .get(self.position)
            .unwrap_or_else(|| self.tokens.last().unwrap())
    }

    /// Consume the current token and return a clone of it.
    ///
    /// The parser **owns** `self.tokens`, so the clone is necessary here.
    /// Section parsers receive `&[Token]` slices and also clone on advance,
    /// keeping the same pattern throughout the pipeline.
    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        if !self.is_at_end() {
            self.position += 1;
        }
        token
    }

    #[inline]
    fn is_at_end(&self) -> bool {
        matches!(self.current().token_type, TokenType::EndOfFile)
    }

    #[inline]
    fn current_matches_symbol(&self, expected: char) -> bool {
        matches!(self.current().token_type, TokenType::Symbol(c) if c == expected)
    }

    /// Returns `true` if `token` opens a top-level section — used as a
    /// guard inside `pack_section_tokens` to catch unclosed sections.
    #[inline]
    fn is_section_keyword_token(&self, token: &Token) -> bool {
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

    /// No-op after comment filtering — the lexer never emits whitespace tokens.
    /// Kept as a forward-compatibility hook in case the token stream ever
    /// includes ignorable tokens in the future.
    #[inline]
    fn skip_non_meaningful_tokens(&mut self) {
        // Intentionally empty.
    }
                                   }
