// dixscript/src/Compiler/Core/Config/config_section_handler.rs
//! Extracts and processes the @CONFIG section, then initialises VersionManager
//! and ErrorManager.
//!
//! ## Two entry points
//!
//! ### Token-based (Approach B — recommended, used by loader and LSP)
//! `process_config_tokens(&[Token])` — called after the tokenizer has run on
//! the full source.  `@CONFIG` tokens arrive with accurate 1-based positions
//! already set; no source scanning or position fixup is required.
//!
//! ### Text-based (legacy, kept for benches and any callers with raw source)
//! `process_config_section(&str)` — strips the `@CONFIG` block, parses it,
//! and returns the cleaned source for a second tokenizer pass.
//!
//! ## Source-stripping strategy (text path only)
//!
//! After extracting the @CONFIG block we replace it with the same number of
//! `\n` characters that the block originally contained. This preserves every
//! source line number exactly so that all downstream token positions already
//! reflect the original file — no offset arithmetic is required anywhere in
//! the pipeline.

use crate::Compiler::AST::{ConfigSection, Position};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::VersionControl::VersionManager;
use crate::ErrorManager::{ErrorManager, DebugConfig};
use crate::Utilities::MID_Logger;
use super::{ConfigSchema, OperationalSettings};
use std::collections::HashMap;

const CONFIG_SECTION_KEYWORD: &str = "@CONFIG";
const CONFIG_KEYWORD_LENGTH: usize = 7;

pub struct ConfigSectionHandler {
    logger: Option<std::sync::Arc<std::sync::Mutex<MID_Logger>>>,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
}

impl ConfigSectionHandler {
    pub fn new_with_error_manager(
        logger: Option<std::sync::Arc<std::sync::Mutex<MID_Logger>>>,
        error_manager: ErrorManager,
    ) -> Self {
        let logger = logger.or_else(|| {
            if MID_Logger::HasSharedInstance() {
                Some(MID_Logger::GetSharedInstance(None, None))
            } else {
                None
            }
        });
        ConfigSectionHandler {
            logger,
            error_manager,
            debug_config: DebugConfig::silent(),
        }
    }

    pub fn new(logger: Option<std::sync::Arc<std::sync::Mutex<MID_Logger>>>) -> Self {
        Self::new_with_error_manager(logger, ErrorManager::get_shared_instance())
    }

    pub fn error_manager(&self) -> ErrorManager {
        self.error_manager.clone()
    }

    // =========================================================================
    // TOKEN-BASED ENTRY POINT (Approach B)
    // =========================================================================

    /// Process `@CONFIG` from a slice of tokens.
    ///
    /// Called after the tokenizer has run on the full source.  Positions on
    /// every `ConfigEntry` are read directly from the tokens — no source
    /// scanning required.
    ///
    /// `config_tokens` is the output of `split_config_tokens().config_tokens`:
    /// it runs from the `SectionConfig` token through the section's closing `)`.
    /// When the source has no `@CONFIG` section, pass an empty slice.
    ///
    /// `cleaned_input_string` in the returned `ProcessConfigResult` is always
    /// empty string; in the token path the caller already has the full token
    /// stream and does not re-run the tokenizer.
    pub fn process_config_tokens(
        &mut self,
        config_tokens: &[Token],
    ) -> ProcessConfigResult {
        self.log_info("Starting CONFIG token-based extraction");

        let mut result = ProcessConfigResult::default();

        if config_tokens.is_empty() {
            self.log_info("No CONFIG tokens — using cached minimal config");
            result.config_section = ConfigSchema::create_minimal_config();
            result.warnings.push(
                "No CONFIG tokens provided — using cached defaults".to_string(),
            );
            self.initialize_singletons(&mut result);
            return result;
        }

        // Position of the @CONFIG keyword itself (used as ConfigSection.position).
        let section_position = config_tokens
            .iter()
            .find(|t| matches!(t.token_type, TokenType::SectionConfig))
            .map(|t| Position::new(t.line, t.column))
            .unwrap_or(Position::UNKNOWN);

        // Parse key→value pairs from the token stream.
        let entries_result = self.parse_config_entries_from_tokens(config_tokens);
        result.warnings.extend(entries_result.warnings.clone());

        match ConfigSchema::validate_and_enhance_config(entries_result.entries) {
            Ok(validated) => {
                let mut config_section = ConfigSchema::create_config_section(validated);
                // Token positions are already 1-based and absolute — no source
                // scan needed.
                self.apply_positions_to_config_section(
                    &mut config_section,
                    &entries_result.positions,
                    section_position,
                );
                result.config_section = config_section;
            }
            Err(e) => {
                self.log_warning(&format!(
                    "Config token validation failed: {} — using defaults",
                    e
                ));
                result.config_section = ConfigSchema::create_minimal_config();
                result.warnings.push(format!("Config validation error: {}", e));
            }
        }

        // Not used in the token path: the caller already has the full token
        // stream.  Set to empty so any stale reader gets a safe no-op.
        result.cleaned_input_string = String::new();

        self.initialize_singletons(&mut result);
        result
    }

    // =========================================================================
    // TEXT-BASED ENTRY POINT (legacy)
    // =========================================================================

    pub fn process_config_section(&mut self, input_string: &str) -> ProcessConfigResult {
        self.log_info("Starting CONFIG section extraction (text path)");

        let mut result = ProcessConfigResult::default();

        if input_string.trim().is_empty() {
            self.log_warning("Empty input - using cached minimal config");
            result.config_section = ConfigSchema::create_minimal_config();
            result.warnings.push("Empty input - using cached minimal configuration".to_string());
            result.cleaned_input_string = String::new();
            self.initialize_singletons(&mut result);
            return result;
        }

        if !self.contains_config_keyword(input_string) {
            self.log_info("No CONFIG section found - using cached defaults");
            result.config_section = ConfigSchema::create_minimal_config();
            result.warnings.push("No CONFIG section found - using cached defaults".to_string());
            result.cleaned_input_string = input_string.to_string();
            self.initialize_singletons(&mut result);
            return result;
        }

        match self.extract_config_section_optimized(input_string) {
            Ok(extraction_result) => {
                if extraction_result.found {
                    if self.debug_config.is_enabled {
                        self.log_info(&format!(
                            "Found CONFIG at position {}",
                            extraction_result.start_position
                        ));
                    }
                    match self.parse_config_string_optimized(&extraction_result.config_string) {
                        Ok(parse_result) => {
                            result.config_section = parse_result.config_section;
                            result.warnings.extend(parse_result.warnings);
                            result.cleaned_input_string =
                                extraction_result.cleaned_input_string.clone();

                            let config_end = extraction_result.start_position
                                + extraction_result.config_string.len();
                            let key_positions = self.scan_key_positions_in_source(
                                input_string,
                                extraction_result.start_position,
                                config_end,
                            );
                            let section_pos = Self::byte_offset_to_position(
                                input_string,
                                extraction_result.start_position,
                            );
                            self.apply_positions_to_config_section(
                                &mut result.config_section,
                                &key_positions,
                                section_pos,
                            );

                            if self.debug_config.is_enabled {
                                self.log_info(&format!(
                                    "Config positions fixed: {} keys located, section @L{}:C{}",
                                    key_positions.len(),
                                    section_pos.line,
                                    section_pos.column,
                                ));
                            }
                        }
                        Err(e) => {
                            self.log_error(&format!("Error parsing CONFIG: {}", e));
                            result.config_section = ConfigSchema::create_minimal_config();
                            result.warnings.push(format!("Parsing error: {}", e));
                            result.cleaned_input_string =
                                extraction_result.cleaned_input_string.clone();
                        }
                    }
                } else {
                    self.log_info("No valid CONFIG section - using cached defaults");
                    result.config_section = ConfigSchema::create_minimal_config();
                    result.warnings.push(
                        "No valid CONFIG section - using cached defaults".to_string(),
                    );
                    result.cleaned_input_string =
                        extraction_result.cleaned_input_string.clone();
                }
            }
            Err(e) => {
                self.log_error(&format!("CONFIG extraction error: {}", e));
                result.config_section = ConfigSchema::create_minimal_config();
                result.warnings.push(format!("Extraction error: {}", e));
                result.cleaned_input_string = input_string.to_string();
            }
        }

        self.initialize_singletons(&mut result);
        result
    }

    // =========================================================================
    // SHARED SINGLETON INITIALISATION
    // =========================================================================

    fn initialize_singletons(&mut self, result: &mut ProcessConfigResult) {
        let settings = ConfigSchema::extract_operational_settings(&result.config_section);
        self.debug_config = DebugConfig::from_debug_mode(settings.debug_mode);

        if self.debug_config.is_enabled {
            self.log_info(&format!(
                "Settings: {:?}, {:?}, Version: {}",
                settings.error_handling_strategy, settings.debug_mode, settings.version
            ));
        }

        VersionManager::initialize(&settings.version);

        if self.debug_config.is_enabled {
            self.log_info(&format!(
                "VersionManager initialized with version {}",
                settings.version
            ));
        }

        if let Ok(vm) = VersionManager::instance().read() {
            if vm.current_version() != settings.version {
                self.log_warning(&format!(
                    "Version mismatch: Config requests {}, but VersionManager is {}",
                    settings.version,
                    vm.current_version()
                ));
            }
        }

        self.error_manager.update_settings(settings.clone());
        result.operational_settings = settings;

        self.log_info("ErrorManager configured with operational settings");
    }

    // =========================================================================
    // TOKEN-PATH PRIVATE HELPERS
    // =========================================================================

    /// Parse `key -> value` entries from a `@CONFIG` token slice.
    ///
    /// Handles optional commas and comments between entries.
    fn parse_config_entries_from_tokens(
        &self,
        tokens: &[Token],
    ) -> TokenConfigEntriesResult {
        let mut entries:   HashMap<String, String>   = HashMap::new();
        let mut positions: HashMap<String, Position> = HashMap::new();
        let mut warnings:  Vec<String>               = Vec::new();

        let mut i = 0usize;

        // Skip the section keyword and opening paren.
        while i < tokens.len() {
            match &tokens[i].token_type {
                TokenType::SectionConfig
                | TokenType::Symbol('(')
                | TokenType::Comment(_) => { i += 1; }
                _ => break,
            }
        }

        // Parse entries: Identifier SwitchCase value [,]
        while i < tokens.len() {
            // Skip entry separators and structural tokens.
            match &tokens[i].token_type {
                TokenType::Symbol(',')
                | TokenType::Symbol(')')
                | TokenType::Comment(_) => { i += 1; continue; }
                TokenType::EndOfFile => break,
                _ => {}
            }

            // ── Key ───────────────────────────────────────────────────────
            let (key, key_pos) = match &tokens[i].token_type {
                TokenType::Identifier(k) => {
                    (k.clone(), Position::new(tokens[i].line, tokens[i].column))
                }
                other => {
                    warnings.push(format!(
                        "Unexpected token in @CONFIG at {}:{} — {:?}",
                        tokens[i].line, tokens[i].column, other
                    ));
                    i += 1;
                    continue;
                }
            };
            i += 1;

            // ── Arrow (->) ────────────────────────────────────────────────
            if i >= tokens.len() { break; }
            match &tokens[i].token_type {
                TokenType::SwitchCase => { i += 1; }
                _ => {
                    warnings.push(format!(
                        "Expected '->' after config key '{}' at {}:{}",
                        key, tokens[i].line, tokens[i].column
                    ));
                    // Attempt recovery: keep consuming until next Identifier or EOF.
                    while i < tokens.len() {
                        match &tokens[i].token_type {
                            TokenType::Identifier(_) | TokenType::EndOfFile => break,
                            _ => { i += 1; }
                        }
                    }
                    continue;
                }
            }

            // Skip any comments between arrow and value.
            while i < tokens.len()
                && matches!(tokens[i].token_type, TokenType::Comment(_))
            {
                i += 1;
            }

            // ── Value ─────────────────────────────────────────────────────
            if i >= tokens.len() { break; }
            let value_str = match &tokens[i].token_type {
                TokenType::String(s) | TokenType::StringSingle(s) => s.clone(),
                TokenType::Integer(n)              => n.to_string(),
                TokenType::Float(f)                => f.to_string(),
                TokenType::Double(d)               => d.to_string(),
                TokenType::ScientificNotation(d)   => d.to_string(),
                TokenType::Bool(b)                 => b.to_string(),
                TokenType::Date(d)                 => d.clone(),
                TokenType::Timestamp(t)            => t.clone(),
                TokenType::Identifier(s)           => s.clone(),
                TokenType::Keyword(k)              => k.to_string(),
                other => {
                    warnings.push(format!(
                        "Unexpected value token for config key '{}' at {}:{} — {:?}",
                        key, tokens[i].line, tokens[i].column, other
                    ));
                    i += 1;
                    continue;
                }
            };
            i += 1;

            entries.insert(key.clone(), value_str);
            positions.insert(key, key_pos);
        }

        TokenConfigEntriesResult { entries, positions, warnings }
    }

    // =========================================================================
    // TEXT-PATH PRIVATE HELPERS (position fixup, extraction, parsing)
    // =========================================================================

    fn byte_offset_to_position(source: &str, offset: usize) -> Position {
        let clamped = offset.min(source.len());
        let before  = &source[..clamped];
        let line = before.chars().filter(|&c| c == '\n').count() + 1;
        let col = match before.rfind('\n') {
            Some(nl_pos) => clamped - nl_pos,
            None         => clamped + 1,
        };
        Position::new(line, col)
    }

    fn scan_key_positions_in_source(
        &self,
        source:       &str,
        start_offset: usize,
        end_offset:   usize,
    ) -> HashMap<String, Position> {
        let mut positions = HashMap::new();
        let end   = end_offset.min(source.len());
        let block = &source[start_offset..end];

        let base_line = source[..start_offset]
            .chars()
            .filter(|&c| c == '\n')
            .count() + 1;

        for (rel_idx, line) in block.lines().enumerate() {
            let abs_line = base_line + rel_idx;
            let trimmed  = line.trim_start();

            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("/*")
                || trimmed.starts_with(')')
                || trimmed.to_uppercase().starts_with("@CONFIG")
            {
                continue;
            }

            if let Some(arrow_pos) = trimmed.find("->") {
                let key_raw = trimmed[..arrow_pos].trim();

                let valid = !key_raw.is_empty()
                    && key_raw.chars().next()
                        .map(|c| c.is_ascii_alphabetic() || c == '_')
                        .unwrap_or(false)
                    && key_raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');

                if valid {
                    let leading_bytes = line.len() - line.trim_start().len();
                    let col = leading_bytes + 1;
                    positions.insert(key_raw.to_string(), Position::new(abs_line, col));
                }
            }
        }

        positions
    }

    fn apply_positions_to_config_section(
        &self,
        section:          &mut ConfigSection,
        positions:        &HashMap<String, Position>,
        section_position: Position,
    ) {
        section.position = section_position;
        for entry in &mut section.entries {
            if let Some(&pos) = positions.get(&entry.key) {
                entry.position = pos;
            }
        }
    }

    fn extract_config_section_optimized(
        &self,
        input: &str,
    ) -> Result<ConfigExtractionResult, String> {
        let config_start_index = self
            .index_of_config(input)
            .ok_or("@CONFIG not found")?;

        let open_paren_index = self
            .find_opening_paren_optimized(input, config_start_index + CONFIG_KEYWORD_LENGTH)
            .ok_or("Missing '(' after @CONFIG")?;

        let config_end_index = self.find_config_end_optimized(input, open_paren_index);

        let config_string = input[config_start_index..config_end_index].to_string();

        let cleaned_input_string =
            self.replace_config_with_blank_lines(input, config_start_index, config_end_index);

        Ok(ConfigExtractionResult {
            found: true,
            start_position: config_start_index,
            config_string,
            cleaned_input_string,
        })
    }

    #[inline]
    fn index_of_config(&self, input: &str) -> Option<usize> {
        let bytes = input.as_bytes();
        let kw    = b"@config";
        let n     = kw.len();
        if bytes.len() < n { return None; }
        if let Some(pos) = memchr::memmem::find(bytes, b"@CONFIG") { return Some(pos); }
        if let Some(pos) = memchr::memmem::find(bytes, b"@config") { return Some(pos); }
        'outer: for i in 0..=(bytes.len() - n) {
            for j in 0..n {
                if bytes[i + j].to_ascii_lowercase() != kw[j] { continue 'outer; }
            }
            return Some(i);
        }
        None
    }

    #[inline]
    fn contains_config_keyword(&self, input: &str) -> bool {
        if input.len() < CONFIG_KEYWORD_LENGTH { return false; }
        self.index_of_config(input).is_some()
    }

    #[inline]
    fn find_opening_paren_optimized(&self, input: &str, start_from: usize) -> Option<usize> {
        for (i, c) in input[start_from..].char_indices() {
            if c.is_whitespace() { continue; }
            if c == '(' { return Some(start_from + i); }
            if c != '/' && c != '*' { return None; }
        }
        None
    }

    fn find_config_end_optimized(&self, input: &str, open_paren_index: usize) -> usize {
        let relevant = &input[open_paren_index..];
        let mut chars = relevant.char_indices().peekable();

        let mut paren_depth: i32 = 0;
        let mut inside_string = false;
        let mut string_delimiter = '\0';
        let mut inside_comment = false;
        let mut inside_multi_line_comment = false;

        while let Some((byte_offset, current)) = chars.next() {
            let next = chars.peek().map(|(_, c)| *c).unwrap_or('\0');

            if !inside_string && current == '/' && next == '*' {
                inside_multi_line_comment = true;
                chars.next();
                continue;
            }
            if inside_multi_line_comment && current == '*' && next == '/' {
                inside_multi_line_comment = false;
                chars.next();
                continue;
            }
            if inside_multi_line_comment { continue; }

            if !inside_string && current == '/' && next == '/' {
                inside_comment = true;
                continue;
            }
            if inside_comment {
                if current == '\n' || current == '\r' { inside_comment = false; }
                continue;
            }

            if (current == '"' || current == '\'') && !inside_string {
                inside_string = true;
                string_delimiter = current;
                continue;
            }
            if inside_string {
                if current == '\\' && next == string_delimiter {
                    chars.next();
                    continue;
                }
                if current == string_delimiter {
                    inside_string = false;
                    string_delimiter = '\0';
                }
                continue;
            }

            if current == '(' {
                paren_depth += 1;
            } else if current == ')' {
                paren_depth -= 1;
                if paren_depth == 0 {
                    return open_paren_index + byte_offset + current.len_utf8();
                }
            }

            if current == '@' && paren_depth == 1 {
                let abs = open_paren_index + byte_offset;
                if self.is_known_section_start(&input[abs..]) {
                    self.log_warning(&format!(
                        "Next section at {} - CONFIG not properly closed",
                        abs
                    ));
                    return abs;
                }
            }
        }

        input.len()
    }

    #[inline]
    fn is_known_section_start(&self, remaining: &str) -> bool {
        if remaining.len() < 4 || !remaining.starts_with('@') {
            return false;
        }
        let upper = remaining.to_uppercase();
        upper.starts_with("@DATA")
            || upper.starts_with("@DLM")
            || upper.starts_with("@ENUMS")
            || upper.starts_with("@QUICKFUNCS")
            || upper.starts_with("@SECURITY")
            || upper.starts_with("@IMPORTS")
            || upper.starts_with("@JSON")
            || upper.starts_with("@XML")
    }

    fn replace_config_with_blank_lines(
        &self,
        input:       &str,
        start_index: usize,
        end_index:   usize,
    ) -> String {
        let config_chunk   = &input[start_index..end_index];
        let newline_count  = config_chunk.chars().filter(|&c| c == '\n').count();
        let replacement    = "\n".repeat(newline_count);
        format!("{}{}{}", &input[..start_index], replacement, &input[end_index..])
    }

    fn parse_config_string_optimized(
        &self,
        config_string: &str,
    ) -> Result<ConfigParseResult, String> {
        let content = self.extract_config_content_optimized(config_string)?;

        if content.trim().is_empty() {
            self.log_warning("CONFIG section empty - using defaults");
            return Ok(ConfigParseResult {
                config_section: ConfigSchema::create_minimal_config(),
                warnings: vec!["CONFIG section was empty".to_string()],
            });
        }

        let entries_result = self.parse_config_entries_optimized(&content);

        match ConfigSchema::validate_and_enhance_config(entries_result.entries) {
            Ok(validated) => {
                let config_section = ConfigSchema::create_config_section(validated);
                Ok(ConfigParseResult {
                    config_section,
                    warnings: entries_result.warnings,
                })
            }
            Err(e) => {
                self.log_warning(&format!("Config validation failed: {}", e));
                Ok(ConfigParseResult {
                    config_section: ConfigSchema::create_minimal_config(),
                    warnings: vec![format!("Validation error: {}", e)],
                })
            }
        }
    }

    fn extract_config_content_optimized(&self, config_string: &str) -> Result<String, String> {
        let open  = config_string.find('(').ok_or("Missing '(' in CONFIG")?;
        let close = config_string.rfind(')').ok_or("Missing ')' in CONFIG")?;
        if close <= open {
            return Err("Invalid CONFIG parentheses".to_string());
        }
        Ok(config_string[open + 1..close].to_string())
    }

    fn parse_config_entries_optimized(&self, content: &str) -> ConfigEntriesParseResult {
        let mut result = ConfigEntriesParseResult {
            entries:  HashMap::new(),
            warnings: Vec::new(),
        };

        for entry_str in self.split_config_entries(content) {
            let entry_str = entry_str.trim();
            if entry_str.is_empty() { continue; }
            match self.parse_single_entry_optimized(entry_str) {
                Ok((key, value)) => { result.entries.insert(key, value); }
                Err(e)           => {
                    result.warnings.push(format!("Failed to parse '{}': {}", entry_str, e));
                }
            }
        }

        result
    }

    fn split_config_entries<'a>(&self, content: &'a str) -> Vec<&'a str> {
        let mut entries: Vec<&'a str> = Vec::new();
        let mut start = 0usize;
        let mut inside_string = false;
        let mut string_delimiter = '\0';
        let mut escape_next = false;

        for (byte_offset, c) in content.char_indices() {
            if escape_next { escape_next = false; continue; }

            if (c == '"' || c == '\'') && !inside_string {
                inside_string    = true;
                string_delimiter = c;
            } else if inside_string {
                if c == '\\' { escape_next = true; }
                else if c == string_delimiter {
                    inside_string    = false;
                    string_delimiter = '\0';
                }
            } else if c == ',' || c == '\n' || c == '\r' {
                entries.push(&content[start..byte_offset]);
                start = byte_offset + c.len_utf8();
            }
        }

        if start <= content.len() {
            entries.push(&content[start..]);
        }

        entries.into_iter().filter(|e| !e.trim().is_empty()).collect()
    }

    #[inline]
    fn parse_single_entry_optimized(&self, entry: &str) -> Result<(String, String), String> {
        let arrow_index = entry.find("->").ok_or("Missing '->' in config entry")?;
        let key   = entry[..arrow_index].trim();
        let value = entry[arrow_index + 2..].trim();
        if key.is_empty()   { return Err("Empty key".to_string()); }
        if value.is_empty() { return Err("Empty value".to_string()); }
        Ok((key.to_string(), self.clean_config_value_optimized(value)))
    }

    #[inline]
    fn clean_config_value_optimized(&self, value: &str) -> String {
        let value = value.trim();
        if value.len() >= 2 {
            let first = value.chars().next();
            let last  = value.chars().last();
            if let (Some(f), Some(l)) = (first, last) {
                if (f == '"' && l == '"') || (f == '\'' && l == '\'') {
                    return value[1..value.len() - 1].to_string();
                }
            }
        }
        value.to_string()
    }

    // =========================================================================
    // LOGGING
    // =========================================================================

    fn log_info(&self, message: &str) {
        if let Some(ref logger) = self.logger {
            if let Ok(mut log) = logger.lock() { log.Info(message); }
        }
    }

    fn log_warning(&self, message: &str) {
        if let Some(ref logger) = self.logger {
            if let Ok(mut log) = logger.lock() { log.Warning(message); }
        }
    }

    fn log_error(&self, message: &str) {
        if let Some(ref logger) = self.logger {
            if let Ok(mut log) = logger.lock() { log.Error(message); }
        }
    }
}

// =============================================================================
// PUBLIC RESULT TYPES
// =============================================================================

#[derive(Debug, Clone)]
pub struct ProcessConfigResult {
    pub config_section:       ConfigSection,
    pub operational_settings: OperationalSettings,
    /// Populated by the text path only.  In the token path this is always
    /// an empty string — the caller retains the full token stream.
    pub cleaned_input_string: String,
    pub warnings:             Vec<String>,
}

impl Default for ProcessConfigResult {
    fn default() -> Self {
        ProcessConfigResult {
            config_section:       ConfigSchema::create_minimal_config(),
            operational_settings: OperationalSettings::default(),
            cleaned_input_string: String::new(),
            warnings:             Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigExtractionResult {
    pub found:                bool,
    pub start_position:       usize,
    pub config_string:        String,
    pub cleaned_input_string: String,
}

impl Default for ConfigExtractionResult {
    fn default() -> Self {
        ConfigExtractionResult {
            found:                false,
            start_position:       0,
            config_string:        String::new(),
            cleaned_input_string: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigParseResult {
    pub config_section: ConfigSection,
    pub warnings:       Vec<String>,
}

// =============================================================================
// PRIVATE HELPER TYPES
// =============================================================================

#[derive(Debug)]
struct ConfigEntriesParseResult {
    entries:  HashMap<String, String>,
    warnings: Vec<String>,
}

/// Return type of `parse_config_entries_from_tokens`.
struct TokenConfigEntriesResult {
    /// Raw key→value strings ready for `ConfigSchema::validate_and_enhance_config`.
    entries:   HashMap<String, String>,
    /// Per-key source positions (1-based) read directly from the token stream.
    positions: HashMap<String, Position>,
    warnings:  Vec<String>,
}
