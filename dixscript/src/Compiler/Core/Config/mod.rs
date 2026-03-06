//! Configuration handling for DixScript
//!
//! This module contains:
//! - ConfigSectionHandler: Extracts and processes @CONFIG section
//! - ConfigSchema: Validates and enhances configuration
//! - OperationalSettings: Runtime settings extracted from config

pub mod config_section_handler;
pub mod config_schema;
pub mod operational_settings;

// Re-exports for convenience
pub use config_section_handler::{
    ConfigSectionHandler,
    ProcessConfigResult,
    ConfigExtractionResult,
    ConfigParseResult,
};
pub use config_schema::ConfigSchema;
pub use operational_settings::{
    OperationalSettings,
    ErrorHandlingStrategy,
    CompatibilityMode,
    DebugMode,
};