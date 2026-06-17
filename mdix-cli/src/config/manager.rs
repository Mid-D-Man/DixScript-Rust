//! Loads and saves CliConfig at ~/.dixscript/config.toml.
//!
//! The config directory is created on first write. Reads return defaults if
//! the file does not yet exist, so the CLI works out of the box without any
//! setup step.
//!
//! ## Test isolation
//!
//! `config_dir()` checks the `MDIX_CONFIG_DIR` environment variable before
//! falling back to `~/.dixscript`. This lets integration tests point each
//! test process at its own temp directory instead of mutating the real
//! user config file — without this, parallel test runs that touch the same
//! keys (e.g. `default_indent_size`) race against each other and produce
//! flaky failures such as `config_reset_single_key_exits_zero` intermittently
//! observing a value written by a concurrently-running test.

use std::path::PathBuf;
use crate::commands::CliError;
use super::cli_config::CliConfig;

pub struct ConfigManager;

impl ConfigManager {
    /// Return the path to the config directory (~/.dixscript/ by default).
    ///
    /// If the `MDIX_CONFIG_DIR` environment variable is set to a non-empty
    /// value, that path is used instead. This is intended for test isolation
    /// only — normal CLI usage never sets this variable.
    pub fn config_dir() -> PathBuf {
        if let Ok(override_dir) = std::env::var("MDIX_CONFIG_DIR") {
            if !override_dir.is_empty() {
                return PathBuf::from(override_dir);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env vars are process-global, so even though each test below uses its
    // own temp directory, concurrent tests setting/clearing MDIX_CONFIG_DIR
    // would still race with each other. Serialize just the env-var mutation
    // window within this module.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_isolated_dir<F: FnOnce()>(f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join(format!(
            "mdix_cfg_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("MDIX_CONFIG_DIR", &tmp);

        f();

        std::env::remove_var("MDIX_CONFIG_DIR");
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn config_dir_respects_env_override() {
        with_isolated_dir(|| {
            let dir = ConfigManager::config_dir();
            let expected = std::env::var("MDIX_CONFIG_DIR").unwrap();
            assert_eq!(dir, PathBuf::from(expected));
        });
    }

    #[test]
    fn load_returns_defaults_when_file_missing() {
        with_isolated_dir(|| {
            let config = ConfigManager::load();
            assert_eq!(config.default_indent_size, 2);
        });
    }

    #[test]
    fn save_then_load_roundtrips() {
        with_isolated_dir(|| {
            let mut config = CliConfig::default();
            config.default_indent_size = 8;
            ConfigManager::save(&config).unwrap();

            let loaded = ConfigManager::load();
            assert_eq!(loaded.default_indent_size, 8);
        });
    }

    #[test]
    fn reset_single_key_restores_default() {
        with_isolated_dir(|| {
            ConfigManager::set_value("default_indent_size", "8").unwrap();
            assert_eq!(ConfigManager::get_value("default_indent_size").unwrap(), "8");

            ConfigManager::reset(Some("default_indent_size")).unwrap();
            assert_eq!(ConfigManager::get_value("default_indent_size").unwrap(), "2");
        });
    }

    #[test]
    fn reset_all_restores_every_default() {
        with_isolated_dir(|| {
            ConfigManager::set_value("default_indent_size", "8").unwrap();
            ConfigManager::set_value("use_tabs", "true").unwrap();

            ConfigManager::reset(None).unwrap();

            assert_eq!(ConfigManager::get_value("default_indent_size").unwrap(), "2");
            assert_eq!(ConfigManager::get_value("use_tabs").unwrap(), "false");
        });
    }
}
