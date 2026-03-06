// dixscript-cli/src/config/mod.rs
//! CLI configuration stored at ~/.dixscript/config.toml

pub mod cli_config;
pub mod manager;

pub use cli_config::CliConfig;
pub use manager::ConfigManager;
