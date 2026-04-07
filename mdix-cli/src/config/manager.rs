
//! Loads and saves CliConfig at ~/.dixscript/config.toml.
//!
//! The config directory is created on first write. Reads return defaults if
//! the file does not yet exist, so the CLI works out of the box without any
//! setup step.

use std::path::PathBuf;
use crate::commands::CliError;
use super::cli_config::CliConfig;

pub struct ConfigManager;

impl ConfigManager {
    /// Return the path to the config directory (~/.dixscript/).
    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dixscript")
    }

    /// Return the path to the config file (~/.dixscript/config.toml).
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    /// Load config from disk, returning defaults if the file does not exist.
    pub fn load() -> CliConfig {
        let path = Self::config_path();
        if !path.exists() {
            return CliConfig::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|_| CliConfig::default()),
            Err(_)      => CliConfig::default(),
        }
    }

    /// Persist `config` to disk, creating the directory if needed.
    pub fn save(config: &CliConfig) -> Result<(), CliError> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir).map_err(CliError::IoError)?;

        let content = toml::to_string_pretty(config)
            .map_err(|e| CliError::ConfigError(e.to_string()))?;

        std::fs::write(Self::config_path(), content).map_err(CliError::IoError)
    }

    /// Read a single key from the persisted config.
    pub fn get_value(key: &str) -> Result<String, CliError> {
        Self::load()
            .get_value(key)
            .map_err(CliError::ConfigError)
    }

    /// Update a single key and persist the config.
    pub fn set_value(key: &str, value: &str) -> Result<(), CliError> {
        let mut config = Self::load();
        config.set_value(key, value).map_err(CliError::ConfigError)?;
        Self::save(&config)
    }

    /// Reset one key (or all keys if `key` is `None`) to defaults and persist.
    pub fn reset(key: Option<&str>) -> Result<(), CliError> {
        let mut config = Self::load();
        match key {
            Some(k) => config.reset_key(k).map_err(CliError::ConfigError)?,
            None    => config = CliConfig::default(),
        }
        Self::save(&config)
    }

    /// Return all key-value-isdefault triples from the persisted config.
    pub fn list_all() -> Vec<(String, String, bool)> {
        Self::load().list_all()
    }
}
