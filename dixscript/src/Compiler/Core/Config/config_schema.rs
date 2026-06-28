//! Configuration schema validation, defaults, and extraction of operational settings.
//!
//! Grammar reference: `others/midx.ebnf`, @CONFIG section.

use crate::Compiler::AST::{ConfigSection, ConfigEntry, ConfigValue};
use crate::Compiler::AST::data_types::{ErrorHandlingStrategy, CompatibilityMode, DebugMode};
use std::collections::HashMap;
use super::OperationalSettings;
use lazy_static::lazy_static;

lazy_static! {
    static ref VERSION_REGEX: regex::Regex =
        regex::Regex::new(r"^(x_\d+\.\d+|\d+\.\d+(\.\d+)?(-[a-zA-Z0-9]+)?)$")
            .expect("VERSION_REGEX compile failed");

    static ref TIMESTAMP_REGEX: regex::Regex =
        regex::Regex::new(
            r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}:\d{2}(\.\d{3})?(Z|[+-]\d{2}:\d{2})?)?$"
        ).expect("TIMESTAMP_REGEX compile failed");

    static ref STATIC_DEFAULTS: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("version",            "1.0.0");
        m.insert("encoding",           "utf-8");
        m.insert("author",             "Unknown Author");
        m.insert("features",           "advanced");
        m.insert("debug_mode",         "off");
        m.insert("error_handling",     "halt");
        m.insert("compatibility_mode", "strict");
        m
    };
}

pub struct ConfigSchema;

impl ConfigSchema {
    const REQUIRED_KEYS: &'static [&'static str] = &["version", "encoding"];

    fn default_values() -> HashMap<String, String> {
        let mut defaults: HashMap<String, String> = STATIC_DEFAULTS
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        // `created` must be dynamic — runtime timestamp cannot be a static default.
        defaults.insert(
            "created".to_string(),
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        );
        defaults
    }

    pub fn validate_and_enhance_config(
        config: HashMap<String, String>,
    ) -> Result<HashMap<String, String>, String> {
        let mut validated = Self::default_values();
        let mut warnings = Vec::new();

        for (key, value) in config {
            let key_lower = key.to_lowercase();
            match Self::validate_entry(&key_lower, &value) {
                Ok(_) => { validated.insert(key, value); }
                Err(e) => {
                    warnings.push(format!(
                        "Invalid {}: '{}' - using default. Error: {}",
                        key, value, e
                    ));
                }
            }
        }

        for &required in Self::REQUIRED_KEYS {
            if !validated.contains_key(required) {
                if let Some(&default) = STATIC_DEFAULTS.get(required) {
                    validated.insert(required.to_string(), default.to_string());
                }
                warnings.push(format!("Missing '{}' - added default", required));
            }
        }

        Self::apply_version_specific_defaults(&mut validated, &mut warnings);

        if !warnings.is_empty() {
            eprintln!("Config validation warnings: {:#?}", warnings);
        }

        Ok(validated)
    }

    fn validate_entry(key: &str, value: &str) -> Result<(), String> {
        match key {
            "version"            => Self::validate_version(value),
            "encoding"           => Self::validate_encoding(value),
            "features"           => Self::validate_features(value),
            "created"            => Self::validate_timestamp(value),
            "author"             => Ok(()),
            "debug_mode"         => Self::validate_debug_mode(value),
            "error_handling"     => Self::validate_error_handling(value),
            "compatibility_mode" => Self::validate_compatibility(value),
            _                    => Ok(()),
        }
    }

    fn validate_version(value: &str) -> Result<(), String> {
        if value == "1.0.0" || value == "1.0" || value.starts_with("x_1.") {
            return Ok(());
        }
        if VERSION_REGEX.is_match(value) {
            Ok(())
        } else {
            Err(format!("Invalid version format: {}", value))
        }
    }

    fn validate_encoding(value: &str) -> Result<(), String> {
        match value.to_lowercase().as_str() {
            "utf-8" | "utf-16" | "utf-16le" | "utf-16be" | "ascii" | "iso-8859-1" => Ok(()),
            _ => Err(format!("Invalid encoding: {}", value)),
        }
    }

    fn validate_features(value: &str) -> Result<(), String> {
        let lower = value.to_lowercase();
        if lower == "basic" || lower == "advanced" {
            return Ok(());
        }
        const VALID: &[&str] = &["quickfuncs", "enums", "dlm", "data", "security", "imports"];
        for feature in value.split(',').map(|s| s.trim()) {
            let feature_lower = feature.to_lowercase();
            if !VALID.contains(&feature_lower.as_str()) {
                return Err(format!("Invalid feature: {}", feature));
            }
        }
        Ok(())
    }

    fn validate_debug_mode(value: &str) -> Result<(), String> {
        match value.to_lowercase().as_str() {
            "off" | "regular" | "verbose" => Ok(()),
            _ => Err(format!("Invalid debug_mode: {}", value)),
        }
    }

    fn validate_error_handling(value: &str) -> Result<(), String> {
        match value.to_lowercase().as_str() {
            "halt" | "continue" | "recover" => Ok(()),
            _ => Err(format!("Invalid error_handling: {}", value)),
        }
    }

    fn validate_compatibility(value: &str) -> Result<(), String> {
        match value.to_lowercase().as_str() {
            "strict" | "best_effort" | "permissive" => Ok(()),
            _ => Err(format!("Invalid compatibility_mode: {}", value)),
        }
    }

    fn validate_timestamp(value: &str) -> Result<(), String> {
        // Fast path for the common well-formed case before touching the regex.
        if value.len() >= 19
            && value.chars().nth(4) == Some('-')
            && value.chars().nth(7) == Some('-')
            && value.chars().nth(10) == Some('T')
        {
            return Ok(());
        }
        if TIMESTAMP_REGEX.is_match(value) {
            Ok(())
        } else {
            Err(format!("Invalid timestamp format: {}", value))
        }
    }

    fn apply_version_specific_defaults(
        config: &mut HashMap<String, String>,
        warnings: &mut Vec<String>,
    ) {
        let is_v1 = config
            .get("version")
            .map(|v| v.starts_with("1.0"))
            .unwrap_or(false);

        if is_v1 {
            if let Some(features) = config.get("features") {
                if *features == "legacy" || *features == "minimal" {
                    config.insert("features".to_string(), "advanced".to_string());
                    warnings.push("Updated legacy features to 'advanced' for v1.0.0".to_string());
                }
            }
        }
    }

    pub fn create_config_section(validated_config: HashMap<String, String>) -> ConfigSection {
        let mut entries = Vec::with_capacity(validated_config.len());
        for (key, value) in validated_config {
            let config_value = Self::create_enhanced_config_value(&key, &value);
            entries.push(ConfigEntry::new(key, config_value, Default::default()));
        }
        ConfigSection::new(entries, Default::default())
    }

    fn create_enhanced_config_value(key: &str, value: &str) -> ConfigValue {
        match key {
            "error_handling"     => Self::create_error_handling_value(value),
            "compatibility_mode" => Self::create_compatibility_value(value),
            "debug_mode"         => Self::create_debug_value(value),
            "features"           => Self::create_feature_value(value),
            "created" if Self::is_timestamp(value) => ConfigValue::Timestamp(value.to_string()),
            "created"            => ConfigValue::Date(value.to_string()),
            _                    => ConfigValue::String(value.to_string()),
        }
    }

    fn create_error_handling_value(value: &str) -> ConfigValue {
        let strategy = match value.to_lowercase().as_str() {
            "halt"     => ErrorHandlingStrategy::Halt,
            "continue" => ErrorHandlingStrategy::Continue,
            "recover"  => ErrorHandlingStrategy::Recover,
            _          => ErrorHandlingStrategy::Halt,
        };
        ConfigValue::ErrorHandling(strategy)
    }

    fn create_compatibility_value(value: &str) -> ConfigValue {
        let mode = match value.to_lowercase().as_str() {
            "strict"      => CompatibilityMode::Strict,
            "best_effort" => CompatibilityMode::BestEffort,
            "permissive"  => CompatibilityMode::Permissive,
            _             => CompatibilityMode::Strict,
        };
        ConfigValue::Compatibility(mode)
    }

    fn create_debug_value(value: &str) -> ConfigValue {
        let mode = match value.to_lowercase().as_str() {
            "off"     => DebugMode::Off,
            "regular" => DebugMode::Regular,
            "verbose" => DebugMode::Verbose,
            _         => DebugMode::Off,
        };
        ConfigValue::Debug(mode)
    }

    fn create_feature_value(value: &str) -> ConfigValue {
        // Shorthand keywords are stored normalised as lowercase singletons.
        if value.eq_ignore_ascii_case("basic") {
            return ConfigValue::Features(vec!["basic".to_string()]);
        }
        if value.eq_ignore_ascii_case("advanced") {
            return ConfigValue::Features(vec!["advanced".to_string()]);
        }

        // Comma-separated specific-feature list.
        // Normalise to lowercase so downstream comparisons are unambiguous
        // regardless of what case the author used (e.g. "QuickFuncs" → "quickfuncs").
        let features: Vec<String> = value
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        ConfigValue::Features(features)
    }

    fn is_timestamp(value: &str) -> bool {
        value.contains('T')
            && (value.contains('Z') || value.contains('+') || value.contains('-'))
    }

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
                    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
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
                        // Features are already normalised to lowercase by create_feature_value.
                        settings.enabled_features = features.clone();
                    } else if let ConfigValue::String(ref s) = entry.value {
                        // Fallback: raw string (shouldn't happen after schema processing,
                        // but guard against bypass paths).
                        if s.eq_ignore_ascii_case("advanced") {
                            settings.enabled_features = vec!["advanced".to_string()];
                        } else if s.eq_ignore_ascii_case("basic") {
                            settings.enabled_features = vec!["basic".to_string()];
                        } else {
                            settings.enabled_features = s
                                .split(',')
                                .map(|f| f.trim().to_lowercase())
                                .filter(|f| !f.is_empty())
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
