//! Configuration schema validation and defaults

use crate::Compiler::AST::{ConfigSection, ConfigEntry, ConfigValue};
use crate::Compiler::AST::data_types::{ErrorHandlingStrategy, CompatibilityMode, DebugMode};
use std::collections::HashMap;
use regex::Regex;
use super::OperationalSettings;

/// Static configuration schema validator and builder
pub struct ConfigSchema;

impl ConfigSchema {
    /// Required configuration keys
    const REQUIRED_KEYS: &'static [&'static str] = &["version", "encoding"];

    /// Get default configuration values
    fn default_values() -> HashMap<String, String> {
        let mut defaults = HashMap::new();
        defaults.insert("version".to_string(), "1.0.0".to_string());
        defaults.insert("encoding".to_string(), "utf-8".to_string());
        defaults.insert("author".to_string(), "Unknown Author".to_string());
        defaults.insert(
            "created".to_string(),
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        );
        defaults.insert("features".to_string(), "advanced".to_string());
        defaults.insert("debug_mode".to_string(), "off".to_string());
        defaults.insert("error_handling".to_string(), "halt".to_string());
        defaults.insert("compatibility_mode".to_string(), "strict".to_string());
        defaults
    }

    /// Validate and enhance configuration with defaults
    pub fn validate_and_enhance_config(
        config: HashMap<String, String>,
    ) -> Result<HashMap<String, String>, String> {
        let mut validated = Self::default_values();
        let mut warnings = Vec::new();

        // Merge user config over defaults
        for (key, value) in config {
            let key_lower = key.to_lowercase();
            match Self::validate_entry(&key_lower, &value) {
                Ok(_) => {
                    validated.insert(key, value);
                }
                Err(e) => {
                    warnings.push(format!("Invalid {}: '{}' - using default. Error: {}", key, value, e));
                }
            }
        }

        // Ensure required keys exist
        for &required in Self::REQUIRED_KEYS {
            if !validated.contains_key(required) {
                validated.insert(required.to_string(), Self::default_values()[required].clone());
                warnings.push(format!("Missing '{}' - added default", required));
            }
        }

        // Apply version-specific defaults
        Self::apply_version_specific_defaults(&mut validated, &mut warnings);

        // Log warnings if any (would use logger in production)
        if !warnings.is_empty() {
            eprintln!("Config validation warnings: {:#?}", warnings);
        }

        Ok(validated)
    }

    /// Validate a single configuration entry
    fn validate_entry(key: &str, value: &str) -> Result<(), String> {
        match key {
            "version" => Self::validate_version(value),
            "encoding" => Self::validate_encoding(value),
            "features" => Self::validate_features(value),
            "created" => Self::validate_timestamp(value),
            "author" => Ok(()), // any string is valid
            "debug_mode" => Self::validate_debug_mode(value),
            "error_handling" => Self::validate_error_handling(value),
            "compatibility_mode" => Self::validate_compatibility(value),
            _ => Ok(()), // Unknown keys are allowed
        }
    }

    /// Validate version format
    fn validate_version(value: &str) -> Result<(), String> {
        if value == "1.0.0" || value == "1.0" || value.starts_with("x_1.") {
            return Ok(());
        }

        let version_regex = Regex::new(r"^(x_\d+\.\d+|\d+\.\d+(\.\d+)?(-[a-zA-Z0-9]+)?)$")
            .map_err(|e| format!("Regex error: {}", e))?;

        if version_regex.is_match(value) {
            Ok(())
        } else {
            Err(format!("Invalid version format: {}", value))
        }
    }

    /// Validate encoding
    fn validate_encoding(value: &str) -> Result<(), String> {
        let lower = value.to_lowercase();
        match lower.as_str() {
            "utf-8" | "utf-16" | "utf-16le" | "utf-16be" | "ascii" | "iso-8859-1" => Ok(()),
            _ => Err(format!("Invalid encoding: {}", value)),
        }
    }

    /// Validate features
    fn validate_features(value: &str) -> Result<(), String> {
        let lower = value.to_lowercase();
        if lower == "basic" || lower == "advanced" {
            return Ok(());
        }

        // Check if it's a comma-separated list of valid features
        let valid_features = ["quickfuncs", "enums", "dlm", "data", "security", "imports"];
        let features: Vec<&str> = value.split(',').map(|s| s.trim()).collect();

        for feature in features {
            if !valid_features.contains(&feature.to_lowercase().as_str()) {
                return Err(format!("Invalid feature: {}", feature));
            }
        }

        Ok(())
    }

    /// Validate debug mode
    fn validate_debug_mode(value: &str) -> Result<(), String> {
        match value.to_lowercase().as_str() {
            "off" | "regular" | "verbose" => Ok(()),
            _ => Err(format!("Invalid debug_mode: {}", value)),
        }
    }

    /// Validate error handling strategy
    fn validate_error_handling(value: &str) -> Result<(), String> {
        match value.to_lowercase().as_str() {
            "halt" | "continue" | "recover" => Ok(()),
            _ => Err(format!("Invalid error_handling: {}", value)),
        }
    }

    /// Validate compatibility mode
    fn validate_compatibility(value: &str) -> Result<(), String> {
        match value.to_lowercase().as_str() {
            "strict" | "best_effort" | "permissive" => Ok(()),
            _ => Err(format!("Invalid compatibility_mode: {}", value)),
        }
    }

    /// Validate timestamp format
    fn validate_timestamp(value: &str) -> Result<(), String> {
        // Quick format check
        if value.len() >= 19 && value.chars().nth(4) == Some('-') &&
            value.chars().nth(7) == Some('-') && value.chars().nth(10) == Some('T') {
            return Ok(());
        }

        // Full regex validation
        let timestamp_regex = Regex::new(
            r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2}(\.\d{3})?(Z|[+-]\d{2}:\d{2})?)?$"
        ).map_err(|e| format!("Regex error: {}", e))?;

        if timestamp_regex.is_match(value) {
            Ok(())
        } else {
            Err(format!("Invalid timestamp format: {}", value))
        }
    }

    /// Apply version-specific defaults
    fn apply_version_specific_defaults(config: &mut HashMap<String, String>, warnings: &mut Vec<String>) {
        let version = config.get("version").map(|s| s.as_str()).unwrap_or("1.0.0");

        if version.starts_with("1.0") {
            if let Some(features) = config.get("features") {
                // FIX: Use .as_str() to compare &String with &str
                if features.as_str() == "legacy" || features.as_str() == "minimal" {
                    config.insert("features".to_string(), "advanced".to_string());
                    warnings.push("Updated legacy features to 'advanced' for v1.0.0".to_string());
                }
            }
        }
    }

    /// Create ConfigSection from validated configuration
    pub fn create_config_section(validated_config: HashMap<String, String>) -> ConfigSection {
        let mut entries = Vec::with_capacity(validated_config.len());

        for (key, value) in validated_config {
            let config_value = Self::create_enhanced_config_value(&key, &value);
            entries.push(ConfigEntry::new(
                key,
                config_value,
                Default::default(), // Position unknown during config creation
            ));
        }

        ConfigSection::new(entries, Default::default())
    }

    /// Create enhanced ConfigValue based on key type
    fn create_enhanced_config_value(key: &str, value: &str) -> ConfigValue {
        match key {
            "error_handling" => Self::create_error_handling_value(value),
            "compatibility_mode" => Self::create_compatibility_value(value),
            "debug_mode" => Self::create_debug_value(value),
            "features" => Self::create_feature_value(value),
            "created" if Self::is_timestamp(value) => ConfigValue::Timestamp(value.to_string()),
            "created" => ConfigValue::Date(value.to_string()),
            _ => ConfigValue::String(value.to_string()),
        }
    }

    /// Create error handling ConfigValue
    fn create_error_handling_value(value: &str) -> ConfigValue {
        let strategy = match value.to_lowercase().as_str() {
            "halt" => ErrorHandlingStrategy::Halt,
            "continue" => ErrorHandlingStrategy::Continue,
            "recover" => ErrorHandlingStrategy::Recover,
            _ => ErrorHandlingStrategy::Halt,
        };
        ConfigValue::ErrorHandling(strategy)
    }

    /// Create compatibility ConfigValue
    fn create_compatibility_value(value: &str) -> ConfigValue {
        let mode = match value.to_lowercase().as_str() {
            "strict" => CompatibilityMode::Strict,
            "best_effort" => CompatibilityMode::BestEffort,
            "permissive" => CompatibilityMode::Permissive,
            _ => CompatibilityMode::Strict,
        };
        ConfigValue::Compatibility(mode)
    }

    /// Create debug ConfigValue
    fn create_debug_value(value: &str) -> ConfigValue {
        let mode = match value.to_lowercase().as_str() {
            "off" => DebugMode::Off,
            "regular" => DebugMode::Regular,
            "verbose" => DebugMode::Verbose,
            _ => DebugMode::Off,
        };
        ConfigValue::Debug(mode)
    }

    /// Create feature ConfigValue
    fn create_feature_value(value: &str) -> ConfigValue {
        if value.eq_ignore_ascii_case("basic") {
            return ConfigValue::Features(vec!["basic".to_string()]);
        }

        if value.eq_ignore_ascii_case("advanced") {
            return ConfigValue::Features(vec!["advanced".to_string()]);
        }

        // Parse comma-separated list
        let features: Vec<String> = value
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        ConfigValue::Features(features)
    }

    /// Check if value is a timestamp
    fn is_timestamp(value: &str) -> bool {
        value.contains('T') && (value.contains('Z') || value.contains('+') || value.contains('-'))
    }

    /// Create minimal default configuration (cached)
    pub fn create_minimal_config() -> ConfigSection {
        use std::sync::OnceLock;
        static CACHED_CONFIG: OnceLock<ConfigSection> = OnceLock::new();

        CACHED_CONFIG.get_or_init(|| {
            let mut entries = Vec::with_capacity(8);

            entries.push(ConfigEntry::new(
                "version".to_string(),
                ConfigValue::String("1.0.0".to_string()),
                Default::default(),
            ));

            entries.push(ConfigEntry::new(
                "encoding".to_string(),
                ConfigValue::String("utf-8".to_string()),
                Default::default(),
            ));

            entries.push(ConfigEntry::new(
                "author".to_string(),
                ConfigValue::String("DixScript Compiler".to_string()),
                Default::default(),
            ));

            entries.push(ConfigEntry::new(
                "created".to_string(),
                ConfigValue::Timestamp(
                    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
                ),
                Default::default(),
            ));

            entries.push(ConfigEntry::new(
                "features".to_string(),
                ConfigValue::Features(vec!["advanced".to_string()]),
                Default::default(),
            ));

            entries.push(ConfigEntry::new(
                "debug_mode".to_string(),
                ConfigValue::Debug(DebugMode::Off),
                Default::default(),
            ));

            entries.push(ConfigEntry::new(
                "error_handling".to_string(),
                ConfigValue::ErrorHandling(ErrorHandlingStrategy::Halt),
                Default::default(),
            ));

            entries.push(ConfigEntry::new(
                "compatibility_mode".to_string(),
                ConfigValue::Compatibility(CompatibilityMode::Strict),
                Default::default(),
            ));

            ConfigSection::new(entries, Default::default())
        }).clone()
    }

    /// Extract operational settings from ConfigSection
    pub fn extract_operational_settings(config: &ConfigSection) -> OperationalSettings {
        let mut settings = OperationalSettings::default();

        for entry in &config.entries {
            match entry.key.to_lowercase().as_str() {
                "error_handling" => {
                    if let ConfigValue::ErrorHandling(ref strategy) = entry.value {
                        settings.error_handling_strategy = *strategy;
                    }
                }
                "compatibility_mode" => {
                    if let ConfigValue::Compatibility(ref mode) = entry.value {
                        settings.compatibility_mode = *mode;
                    }
                }
                "debug_mode" => {
                    if let ConfigValue::Debug(ref mode) = entry.value {
                        settings.debug_mode = *mode;
                    }
                }
                "features" => {
                    if let ConfigValue::Features(ref features) = entry.value {
                        settings.enabled_features = features.clone();
                    } else if let ConfigValue::String(ref s) = entry.value {
                        // Handle string value for features
                        if s.eq_ignore_ascii_case("advanced") {
                            settings.enabled_features = vec!["advanced".to_string()];
                        } else if s.eq_ignore_ascii_case("basic") {
                            settings.enabled_features = vec!["basic".to_string()];
                        } else {
                            settings.enabled_features = s
                                .split(',')
                                .map(|f| f.trim().to_string())
                                .collect();
                        }
                    }
                }
                "version" => {
                    if let ConfigValue::String(ref v) = entry.value {
                        settings.version = v.clone();
                    }
                }
                _ => {}
            }
        }

        settings
    }
}