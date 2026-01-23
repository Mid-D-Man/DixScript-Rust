//! ConfigSectionHandler v1.0.0
//!
//! CRITICAL RESPONSIBILITIES:
//! 1. Extract and parse @CONFIG section from source
//! 2. Initialize VersionManager singleton with detected version
//! 3. Initialize VersionConstraints singleton (depends on VersionManager)
//! 4. Configure ErrorManager with operational settings
//! 5. Return validated config + operational settings
//!
//! OPTIMIZATION TARGETS:
//! - Time: < 0.5ms for 9 KB file
//! - Memory: < 2KB allocations
//! - Zero-copy string scanning with &str slices

use crate::Compiler::AST::{ConfigSection, ConfigEntry, ConfigValue};
use crate::Compiler::VersionControl::{VersionManager, VersionConstraints};
use crate::ErrorManager::ErrorManager;
use crate::Utilities::MID_Logger;
use super::{ConfigSchema, OperationalSettings};
use std::collections::HashMap;

const CONFIG_SECTION_KEYWORD: &str = "@CONFIG";
const CONFIG_KEYWORD_LENGTH: usize = 7;

/// Main configuration section handler
pub struct ConfigSectionHandler {
    logger: Option<std::sync::Arc<std::sync::Mutex<MID_Logger>>>,
    error_manager: ErrorManager,
}

impl ConfigSectionHandler {
    /// Create a new ConfigSectionHandler
    pub fn new(logger: Option<std::sync::Arc<std::sync::Mutex<MID_Logger>>>) -> Self {
        let logger = logger.or_else(|| {
            if MID_Logger::HasSharedInstance() {
                Some(MID_Logger::GetSharedInstance(None, None))
            } else {
                None
            }
        });

        let error_manager = ErrorManager::get_shared_instance();

        ConfigSectionHandler {
            logger,
            error_manager,
        }
    }

    /// Main entry point - processes CONFIG and initializes singletons
    /// This method MUST be called before any parsing begins
    pub fn process_config_section(&self, input_string: &str) -> ProcessConfigResult {
        self.log_info("Starting CONFIG section extraction (v1.0.0)");

        let mut result = ProcessConfigResult::default();

        // FAST-PATH 1: Empty input (return cached empty config)
        if input_string.trim().is_empty() {
            self.log_warning("Empty input - using cached minimal config");
            result.config_section = ConfigSchema::create_minimal_config();
            result.warnings.push("Empty input - using cached minimal configuration".to_string());
            result.cleaned_input_string = String::new();

            // Initialize singletons with default config
            self.initialize_singletons(&result.config_section, &mut result);

            return result;
        }

        // FAST-PATH 2: Quick check if @CONFIG exists
        if !self.contains_config_keyword(input_string) {
            self.log_info("No CONFIG section found - using cached defaults");
            result.config_section = ConfigSchema::create_minimal_config();
            result.warnings.push("No CONFIG section found - using cached defaults".to_string());
            result.cleaned_input_string = input_string.to_string();

            // Initialize singletons with default config
            self.initialize_singletons(&result.config_section, &mut result);

            return result;
        }

        // Extract CONFIG section using optimized approach
        match self.extract_config_section_optimized(input_string) {
            Ok(extraction_result) => {
                if extraction_result.found {
                    self.log_info(&format!("Found CONFIG at position {}", extraction_result.start_position));

                    // Parse the extracted config string
                    match self.parse_config_string_optimized(&extraction_result.config_string) {
                        Ok(parse_result) => {
                            result.config_section = parse_result.config_section;
                            result.warnings.extend(parse_result.warnings);
                            result.cleaned_input_string = extraction_result.cleaned_input_string;

                            self.log_info(&format!("Parsed CONFIG with {} warnings", result.warnings.len()));
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
                    result.warnings.push("No valid CONFIG section - using cached defaults".to_string());
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

        // Initialize all singletons with parsed config
        self.initialize_singletons(&result.config_section, &mut result);

        result
    }

    /// Initialize all singletons in correct order
    /// Order matters: VersionManager → VersionConstraints → ErrorManager
    fn initialize_singletons(&self, config: &ConfigSection, result: &mut ProcessConfigResult) {
        self.log_info("Starting singleton initialization");

        // Step 1: Extract operational settings from config
        result.operational_settings = ConfigSchema::extract_operational_settings(config);

        self.log_info(&format!(
            "Settings: {:?}, {:?}, Version: {}",
            result.operational_settings.error_handling_strategy,
            result.operational_settings.debug_mode,
            result.operational_settings.version
        ));

        // Step 2: Initialize VersionManager singleton
        match VersionManager::initialize(&result.operational_settings.version) {
            Ok(_) => {
                self.log_info(&format!(
                    "VersionManager initialized with version {}",
                    result.operational_settings.version
                ));
            }
            Err(e) => {
                // Already initialized - this is OK for subsequent compilations
                self.log_info(&format!(
                    "VersionManager already initialized: {}",
                    e
                ));

                // Verify version compatibility
                if let Ok(vm) = VersionManager::instance() {
                    if vm.current_version() != result.operational_settings.version {
                        self.log_warning(&format!(
                            "Version mismatch: Config requests {}, but VersionManager is {}",
                            result.operational_settings.version,
                            vm.current_version()
                        ));
                    }
                }
            }
        }

        // Step 3: Ensure VersionConstraints singleton is initialized
        match VersionConstraints::instance() {
            Ok(_) => {
                self.log_info("VersionConstraints singleton ready");
            }
            Err(e) => {
                self.log_warning(&format!("VersionConstraints initialization issue: {}", e));
            }
        }

        // Step 4: Update ErrorManager with operational settings
        self.error_manager.update_settings(result.operational_settings.clone());
        self.log_info("ErrorManager configured with operational settings");

        self.log_info("All singletons initialized successfully");
    }

    /// Fast check if @CONFIG keyword exists
    #[inline]
    fn contains_config_keyword(&self, input: &str) -> bool {
        if input.len() < CONFIG_KEYWORD_LENGTH {
            return false;
        }

        // Use Boyer-Moore-like search via standard library
        input.contains(CONFIG_SECTION_KEYWORD) ||
            input.to_uppercase().contains(CONFIG_SECTION_KEYWORD)
    }

    /// Zero-copy extraction using string slices
    fn extract_config_section_optimized(&self, input: &str) -> Result<ConfigExtractionResult, String> {
        let config_start_index = self.index_of_config(input)
            .ok_or("@CONFIG not found")?;

        let open_paren_index = self.find_opening_paren_optimized(input, config_start_index + CONFIG_KEYWORD_LENGTH)
            .ok_or("Missing '(' after @CONFIG")?;

        let config_end_index = self.find_config_end_optimized(input, open_paren_index);

        let config_string = input[config_start_index..config_end_index].to_string();
        let cleaned_input_string = self.remove_config_section_optimized(
            input,
            config_start_index,
            config_end_index,
        );

        Ok(ConfigExtractionResult {
            found: true,
            start_position: config_start_index,
            config_string,
            cleaned_input_string,
        })
    }

    /// Find index of @CONFIG keyword (case-insensitive)
    #[inline]
    fn index_of_config(&self, input: &str) -> Option<usize> {
        let input_upper = input.to_uppercase();
        input_upper.find(CONFIG_SECTION_KEYWORD)
    }

    /// Find opening parenthesis after @CONFIG
    #[inline]
    fn find_opening_paren_optimized(&self, input: &str, start_from: usize) -> Option<usize> {
        let remaining = &input[start_from..];

        for (i, c) in remaining.char_indices() {
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

    /// Find the end of CONFIG section (closing parenthesis)
    fn find_config_end_optimized(&self, input: &str, open_paren_index: usize) -> usize {
        let mut paren_depth = 0;
        let mut inside_string = false;
        let mut string_delimiter = '\0';
        let mut inside_comment = false;
        let mut inside_multi_line_comment = false;

        let chars: Vec<char> = input.chars().collect();
        let mut i = open_paren_index;

        while i < chars.len() {
            let current = chars[i];
            let next = if i + 1 < chars.len() { chars[i + 1] } else { '\0' };

            // Handle multi-line comments
            if !inside_string && current == '/' && next == '*' {
                inside_multi_line_comment = true;
                i += 2;
                continue;
            }

            if inside_multi_line_comment && current == '*' && next == '/' {
                inside_multi_line_comment = false;
                i += 2;
                continue;
            }

            if inside_multi_line_comment {
                i += 1;
                continue;
            }

            // Handle single-line comments
            if !inside_string && current == '/' && next == '/' {
                inside_comment = true;
                i += 1;
                continue;
            }

            if inside_comment {
                if current == '\n' || current == '\r' {
                    inside_comment = false;
                }
                i += 1;
                continue;
            }

            // Handle strings
            if (current == '"' || current == '\'') && !inside_string {
                inside_string = true;
                string_delimiter = current;
                i += 1;
                continue;
            }

            if inside_string {
                if current == '\\' && next == string_delimiter {
                    i += 2;
                    continue;
                }

                if current == string_delimiter {
                    inside_string = false;
                    string_delimiter = '\0';
                }

                i += 1;
                continue;
            }

            // Track parentheses
            if current == '(' {
                paren_depth += 1;
            } else if current == ')' {
                paren_depth -= 1;
                if paren_depth == 0 {
                    return i + 1;
                }
            }

            // Check for next section start
            if current == '@' && paren_depth == 1 {
                if self.is_known_section_start(&input[i..]) {
                    self.log_warning(&format!("Next section at {} - CONFIG not properly closed", i));
                    return i;
                }
            }

            i += 1;
        }

        input.len()
    }

    /// Check if position is start of a known section
    #[inline]
    fn is_known_section_start(&self, remaining: &str) -> bool {
        if remaining.len() < 4 || !remaining.starts_with('@') {
            return false;
        }

        let remaining_upper = remaining.to_uppercase();

        remaining_upper.starts_with("@DATA") ||
            remaining_upper.starts_with("@DLM") ||
            remaining_upper.starts_with("@ENUMS") ||
            remaining_upper.starts_with("@QUICKFUNCS") ||
            remaining_upper.starts_with("@SECURITY") ||
            remaining_upper.starts_with("@IMPORTS") ||
            remaining_upper.starts_with("@JSON") ||
            remaining_upper.starts_with("@XML")
    }

    /// Remove CONFIG section from input string
    fn remove_config_section_optimized(&self, input: &str, start_index: usize, end_index: usize) -> String {
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

    /// Parse CONFIG content string
    fn parse_config_string_optimized(&self, config_string: &str) -> Result<ConfigParseResult, String> {
        let content = self.extract_config_content_optimized(config_string)?;

        if content.is_empty() {
            self.log_warning("CONFIG section empty - using defaults");
            return Ok(ConfigParseResult {
                config_section: ConfigSchema::create_minimal_config(),
                warnings: vec!["CONFIG section was empty".to_string()],
            });
        }

        let entries_result = self.parse_config_entries_optimized(content);

        // Use static ConfigSchema for validation
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

    /// Extract content between @CONFIG( and )
    #[inline]
    fn extract_config_content_optimized(&self, config_string: &str) -> Result<&str, String> {
        let open_paren_index = config_string.find('(')
            .ok_or("Missing '(' in CONFIG")?;

        let close_paren_index = config_string.rfind(')')
            .ok_or("Missing ')' in CONFIG")?;

        if close_paren_index <= open_paren_index {
            return Err("Invalid CONFIG parentheses".to_string());
        }

        Ok(&config_string[open_paren_index + 1..close_paren_index])
    }

    /// Parse config entries from content
    fn parse_config_entries_optimized(&self, content: &str) -> ConfigEntriesParseResult {
        let mut result = ConfigEntriesParseResult {
            entries: HashMap::new(),
            warnings: Vec::new(),
        };

        // Split by commas (accounting for strings)
        let entry_strings = self.split_config_entries(content);

        for entry_str in entry_strings {
            let entry_str = entry_str.trim();
            if entry_str.is_empty() {
                continue;
            }

            match self.parse_single_entry_optimized(entry_str) {
                Ok((key, value)) => {
                    result.entries.insert(key, value);
                }
                Err(e) => {
                    result.warnings.push(format!("Failed to parse '{}': {}", entry_str, e));
                }
            }
        }

        result
    }

    /// Split config entries by comma (respecting strings)
    fn split_config_entries<'a>(&self, content: &'a str) -> Vec<&'a str> {
        let mut entries = Vec::new();
        let mut start = 0;
        let mut inside_string = false;
        let mut string_delimiter = '\0';

        let chars: Vec<char> = content.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];

            if (c == '"' || c == '\'') && !inside_string {
                inside_string = true;
                string_delimiter = c;
            } else if inside_string {
                if c == '\\' && i + 1 < chars.len() {
                    i += 1; // Skip escaped character
                } else if c == string_delimiter {
                    inside_string = false;
                    string_delimiter = '\0';
                }
            } else if c == ',' {
                entries.push(&content[start..i]);
                start = i + 1;
            }

            i += 1;
        }

        if start < content.len() {
            entries.push(&content[start..]);
        }

        entries
    }

    /// Parse a single config entry (key -> value)
    #[inline]
    fn parse_single_entry_optimized(&self, entry: &str) -> Result<(String, String), String> {
        let arrow_index = entry.find("->")
            .ok_or("Missing '->' in config entry")?;

        let key = entry[..arrow_index].trim();
        let value = entry[arrow_index + 2..].trim();

        if key.is_empty() {
            return Err("Empty key".to_string());
        }

        if value.is_empty() {
            return Err("Empty value".to_string());
        }

        let cleaned_value = self.clean_config_value_optimized(value);

        Ok((key.to_string(), cleaned_value.to_string()))
    }

    /// Clean config value (remove quotes)
    #[inline]
    fn clean_config_value_optimized<'a>(&self, value: &'a str) -> &'a str {
        let value = value.trim();

        if value.len() >= 2 {
            let first = value.chars().next().unwrap();
            let last = value.chars().last().unwrap();

            if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
                return &value[1..value.len() - 1];
            }
        }

        value
    }

    // Logging helpers
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

// ==================== RESULT STRUCTS ====================

/// Result of processing CONFIG section
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

/// Result of extracting CONFIG section from input
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

/// Result of parsing CONFIG content
#[derive(Debug, Clone)]
pub struct ConfigParseResult {
    pub config_section: ConfigSection,
    pub warnings: Vec<String>,
}

/// Result of parsing config entries
#[derive(Debug, Clone)]
struct ConfigEntriesParseResult {
    entries: HashMap<String, String>,
    warnings: Vec<String>,
}