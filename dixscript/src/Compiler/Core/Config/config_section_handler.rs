//! Extracts and processes the @CONFIG section, then initialises VersionManager
//! and ErrorManager.
//!
//! Token-based pipeline
//!
//!   Tokenizer (full source)
//!       ↓
//!   split_config_tokens
//!       ├─ config_tokens → ConfigSectionHandler::process_config_tokens
//!       └─ rest_tokens   → GeneralParser
//!
//! Because the tokenizer runs on the FULL source, all token positions are
//! accurate relative to the original file with no offset arithmetic.

use crate::Compiler::AST::{ConfigSection, Position};
use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::VersionControl::VersionManager;
use crate::ErrorManager::{ErrorManager, DebugConfig};
use crate::Utilities::MID_Logger;
use super::{ConfigSchema, OperationalSettings};
use std::collections::HashMap;

pub struct ConfigSectionHandler {
    logger:        Option<std::sync::Arc<std::sync::Mutex<MID_Logger>>>,
    error_manager: ErrorManager,
    debug_config:  DebugConfig,
}

impl ConfigSectionHandler {
    pub fn new_with_error_manager(
        logger:        Option<std::sync::Arc<std::sync::Mutex<MID_Logger>>>,
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
    // TOKEN-BASED ENTRY POINT
    // =========================================================================

    /// Process `@CONFIG` from a slice of tokens produced by `split_config_tokens`.
    ///
    /// `config_tokens` runs from the `SectionConfig` token through the section's
    /// closing `)`, inclusive.  Pass an empty slice when the source has no
    /// `@CONFIG` section.
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

        // Section position comes directly from the @CONFIG keyword token.
        let section_position = config_tokens
            .iter()
            .find(|t| matches!(t.token_type, TokenType::SectionConfig))
            .map(|t| Position::new(t.line, t.column))
            .unwrap_or(Position::UNKNOWN);

        let entries_result = self.parse_config_entries_from_tokens(config_tokens);
        result.warnings.extend(entries_result.warnings.clone());

        match ConfigSchema::validate_and_enhance_config(entries_result.entries) {
            Ok(validated) => {
                let mut config_section = ConfigSchema::create_config_section(validated);
                // Positions are already 1-based and absolute from the tokenizer —
                // no source-scan or offset arithmetic needed.
                self.apply_positions_to_config_section(
                    &mut config_section,
                    &entries_result.positions,
                    section_position,
                );
                result.config_section = config_section;
            }
            Err(e) => {
                self.log_warning(&format!(
                    "Config token validation failed: {} — using defaults", e
                ));
                result.config_section = ConfigSchema::create_minimal_config();
                result.warnings.push(format!("Config validation error: {}", e));
            }
        }

        self.initialize_singletons(&mut result);
        result
    }

    // =========================================================================
    // PRIVATE HELPERS
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
                TokenType::EndOfFile    => break,
                _ => {}
            }

            // ── Key ───────────────────────────────────────────────────────────
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

            // ── Arrow (->) ────────────────────────────────────────────────────
            if i >= tokens.len() { break; }
            match &tokens[i].token_type {
                TokenType::SwitchCase => { i += 1; }
                _ => {
                    warnings.push(format!(
                        "Expected '->' after config key '{}' at {}:{}",
                        key, tokens[i].line, tokens[i].column
                    ));
                    // Recovery: skip until next identifier or EOF.
                    while i < tokens.len() {
                        match &tokens[i].token_type {
                            TokenType::Identifier(_) | TokenType::EndOfFile => break,
                            _ => { i += 1; }
                        }
                    }
                    continue;
                }
            }

            // Skip comments between arrow and value.
            while i < tokens.len()
                && matches!(tokens[i].token_type, TokenType::Comment(_))
            {
                i += 1;
            }

            // ── Value ─────────────────────────────────────────────────────────
            if i >= tokens.len() { break; }
            let value_str = match &tokens[i].token_type {
                TokenType::String(s) | TokenType::StringSingle(s) => s.clone(),
                TokenType::Integer(n)            => n.to_string(),
                TokenType::Long(l)               => l.to_string(),
                TokenType::Float(f)              => f.to_string(),
                TokenType::Double(d)             => d.to_string(),
                TokenType::ScientificNotation(d) => d.to_string(),
                TokenType::Bool(b)               => b.to_string(),
                TokenType::Date(d)               => d.clone(),
                TokenType::Timestamp(t)          => t.clone(),
                TokenType::Identifier(s)         => s.clone(),
                TokenType::Keyword(k)            => k.to_string(),
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
}

// =============================================================================
// PUBLIC RESULT TYPE
// =============================================================================

#[derive(Debug, Clone)]
pub struct ProcessConfigResult {
    pub config_section:       ConfigSection,
    pub operational_settings: OperationalSettings,
    pub warnings:             Vec<String>,
}

impl Default for ProcessConfigResult {
    fn default() -> Self {
        ProcessConfigResult {
            config_section:       ConfigSchema::create_minimal_config(),
            operational_settings: OperationalSettings::default(),
            warnings:             Vec::new(),
        }
    }
}

// =============================================================================
// PRIVATE HELPER TYPE
// =============================================================================

struct TokenConfigEntriesResult {
    entries:   HashMap<String, String>,
    positions: HashMap<String, Position>,
    warnings:  Vec<String>,
            }
