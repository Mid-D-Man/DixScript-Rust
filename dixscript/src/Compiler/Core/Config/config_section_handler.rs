//! Extracts and processes the @CONFIG section, then initialises VersionManager and ErrorManager.
//!
//! Grammar reference: `others/midx.ebnf`, @CONFIG section.

use crate::Compiler::AST::ConfigSection;
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

    pub fn process_config_section(&mut self, input_string: &str) -> ProcessConfigResult {
        self.log_info("Starting CONFIG section extraction");

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
            // Return full source so the tokeniser can see every section.
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
                            // *** Option B: return the original full source unchanged.
                            // The parser will skip @CONFIG tokens itself. ***
                            result.cleaned_input_string = input_string.to_string();
                            if self.debug_config.is_enabled {
                                self.log_info(&format!(
                                    "Parsed CONFIG with {} warnings",
                                    result.warnings.len()
                                ));
                            }
                        }
                        Err(e) => {
                            self.log_error(&format!("Error parsing CONFIG: {}", e));
                            result.config_section = ConfigSchema::create_minimal_config();
                            result.warnings.push(format!("Parsing error: {}", e));
                            result.cleaned_input_string = input_string.to_string();
                        }
                    }
                } else {
                    self.log_info("No valid CONFIG section - using cached defaults");
                    result.config_section = ConfigSchema::create_minimal_config();
                    result.warnings
                        .push("No valid CONFIG section - using cached defaults".to_string());
                    result.cleaned_input_string = input_string.to_string();
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

    #[inline]
    fn contains_config_keyword(&self, input: &str) -> bool {
        if input.len() < CONFIG_KEYWORD_LENGTH {
            return false;
        }
        input.contains(CONFIG_SECTION_KEYWORD)
            || input.to_uppercase().contains(CONFIG_SECTION_KEYWORD)
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
            self.remove_config_section_optimized(input, config_start_index, config_end_index);

        Ok(ConfigExtractionResult {
            found: true,
            start_position: config_start_index,
            config_string,
            cleaned_input_string,
        })
    }

    #[inline]
    fn index_of_config(&self, input: &str) -> Option<usize> {
        input.to_uppercase().find(CONFIG_SECTION_KEYWORD)
    }

    #[inline]
    fn find_opening_paren_optimized(&self, input: &str, start_from: usize) -> Option<usize> {
        for (i, c) in input[start_from..].char_indices() {
            if c.is_whitespace() {
                continue;
            }
            if c == '(' {
                return Some(start_from + i);
            }
            if c != '/' && c != '*' {
                return None;
            }
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
            if inside_multi_line_comment {
                continue;
            }

            if !inside_string && current == '/' && next == '/' {
                inside_comment = true;
                continue;
            }
            if inside_comment {
                if current == '\n' || current == '\r' {
                    inside_comment = false;
                }
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

    fn remove_config_section_optimized(
        &self,
        input: &str,
        start_index: usize,
        end_index: usize,
    ) -> String {
        let before = input[..start_index].trim_end();
        let after = input[end_index..].trim_start();
        if before.is_empty() {
            return after.to_string();
        }
        if after.is_empty() {
            return before.to_string();
        }
        format!("{}\n{}", before, after)
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
        let open = config_string.find('(').ok_or("Missing '(' in CONFIG")?;
        let close = config_string.rfind(')').ok_or("Missing ')' in CONFIG")?;
        if close <= open {
            return Err("Invalid CONFIG parentheses".to_string());
        }
        Ok(config_string[open + 1..close].to_string())
    }

    fn parse_config_entries_optimized(&self, content: &str) -> ConfigEntriesParseResult {
        let mut result = ConfigEntriesParseResult {
            entries: HashMap::new(),
            warnings: Vec::new(),
        };

        for entry_str in self.split_config_entries(content) {
            let entry_str = entry_str.trim();
            if entry_str.is_empty() {
                continue;
            }
            match self.parse_single_entry_optimized(entry_str) {
                Ok((key, value)) => {
                    result.entries.insert(key, value);
                }
                Err(e) => {
                    result
                        .warnings
                        .push(format!("Failed to parse '{}': {}", entry_str, e));
                }
            }
        }

        result
    }

    /// Split config content on commas OR newlines (both are valid entry separators
    /// per the grammar — commas are optional). Splits are not performed inside
    /// string literals.
    fn split_config_entries<'a>(&self, content: &'a str) -> Vec<&'a str> {
        let mut entries: Vec<&'a str> = Vec::new();
        let mut start = 0usize;
        let mut inside_string = false;
        let mut string_delimiter = '\0';
        let mut escape_next = false;

        for (byte_offset, c) in content.char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }

            if (c == '"' || c == '\'') && !inside_string {
                inside_string = true;
                string_delimiter = c;
            } else if inside_string {
                if c == '\\' {
                    escape_next = true;
                } else if c == string_delimiter {
                    inside_string = false;
                    string_delimiter = '\0';
                }
            } else if c == ',' || c == '\n' || c == '\r' {
                // Push whatever we have between start and here.
                // Use byte_offset so we can get a valid &str slice.
                entries.push(&content[start..byte_offset]);
                start = byte_offset + c.len_utf8();
            }
        }

        // Trailing segment after the last separator
        if start <= content.len() {
            entries.push(&content[start..]);
        }

        // Remove whitespace-only entries — these arise from consecutive
        // separators (e.g. \r\n produces two split points) or trailing separators.
        entries.into_iter().filter(|e| !e.trim().is_empty()).collect()
    }

    #[inline]
    fn parse_single_entry_optimized(&self, entry: &str) -> Result<(String, String), String> {
        let arrow_index = entry.find("->").ok_or("Missing '->' in config entry")?;
        let key = entry[..arrow_index].trim();
        let value = entry[arrow_index + 2..].trim();
        if key.is_empty() {
            return Err("Empty key".to_string());
        }
        if value.is_empty() {
            return Err("Empty value".to_string());
        }
        Ok((key.to_string(), self.clean_config_value_optimized(value)))
    }

    #[inline]
    fn clean_config_value_optimized(&self, value: &str) -> String {
        let value = value.trim();
        if value.len() >= 2 {
            let first = value.chars().next();
            let last = value.chars().last();
            if let (Some(f), Some(l)) = (first, last) {
                if (f == '"' && l == '"') || (f == '\'' && l == '\'') {
                    return value[1..value.len() - 1].to_string();
                }
            }
        }
        value.to_string()
    }

    fn log_info(&self, message: &str) {
        if let Some(ref logger) = self.logger {
            if let Ok(mut log) = logger.lock() {
                log.Info(message);
            }
        }
    }

    fn log_warning(&self, message: &str) {
        if let Some(ref logger) = self.logger {
            if let Ok(mut log) = logger.lock() {
                log.Warning(message);
            }
        }
    }

    fn log_error(&self, message: &str) {
        if let Some(ref logger) = self.logger {
            if let Ok(mut log) = logger.lock() {
                log.Error(message);
            }
        }
    }
}

// ── Result types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProcessConfigResult {
    pub config_section: ConfigSection,
    pub operational_settings: OperationalSettings,
    pub cleaned_input_string: String,
    pub warnings: Vec<String>,
}

impl Default for ProcessConfigResult {
    fn default() -> Self {
        ProcessConfigResult {
            config_section: ConfigSchema::create_minimal_config(),
            operational_settings: OperationalSettings::default(),
            cleaned_input_string: String::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigExtractionResult {
    pub found: bool,
    pub start_position: usize,
    pub config_string: String,
    pub cleaned_input_string: String,
}

impl Default for ConfigExtractionResult {
    fn default() -> Self {
        ConfigExtractionResult {
            found: false,
            start_position: 0,
            config_string: String::new(),
            cleaned_input_string: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigParseResult {
    pub config_section: ConfigSection,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct ConfigEntriesParseResult {
    entries: HashMap<String, String>,
    warnings: Vec<String>,
                            }
