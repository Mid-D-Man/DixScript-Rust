
//! CliConfig struct — all user-adjustable preferences with serde defaults.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CliConfig {
    /// Default output directory when -o is not provided
    pub default_output_directory: String,

    /// Spaces per indent level used by format and convert
    pub default_indent_size: usize,

    /// Use tabs instead of spaces for indentation
    pub use_tabs: bool,

    /// Enable colored terminal output
    pub color_output: bool,

    /// Automatically search for .dixscript.key files next to encrypted files
    pub auto_find_key_files: bool,

    /// Additional directories to search for key files
    pub key_search_paths: Vec<String>,

    /// Pretty-print JSON output from --json flag
    pub pretty_print_json: bool,

    /// Show warnings in command output
    pub show_warnings: bool,

    /// Maximum number of errors to display before truncating
    pub max_error_display: usize,
}

impl Default for CliConfig {
    fn default() -> Self {
        CliConfig {
            default_output_directory: "./output".to_string(),
            default_indent_size:      2,
            use_tabs:                 false,
            color_output:             true,
            auto_find_key_files:      true,
            key_search_paths:         Vec::new(),
            pretty_print_json:        true,
            show_warnings:            true,
            max_error_display:        50,
        }
    }
}

impl CliConfig {
    /// Return the string value of a named field, or an error if the key is
    /// unrecognised.
    pub fn get_value(&self, key: &str) -> Result<String, String> {
        match key {
            "default_output_directory" => Ok(self.default_output_directory.clone()),
            "default_indent_size"      => Ok(self.default_indent_size.to_string()),
            "use_tabs"                 => Ok(self.use_tabs.to_string()),
            "color_output"             => Ok(self.color_output.to_string()),
            "auto_find_key_files"      => Ok(self.auto_find_key_files.to_string()),
            "key_search_paths"         => Ok(self.key_search_paths.join(",")),
            "pretty_print_json"        => Ok(self.pretty_print_json.to_string()),
            "show_warnings"            => Ok(self.show_warnings.to_string()),
            "max_error_display"        => Ok(self.max_error_display.to_string()),
            other => Err(format!("Unknown config key: '{}'", other)),
        }
    }

    /// Set a named field from a string value, returning an error if the key is
    /// unrecognised or the value cannot be parsed.
    pub fn set_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "default_output_directory" => {
                self.default_output_directory = value.to_string();
            }
            "default_indent_size" => {
                self.default_indent_size = value
                    .parse()
                    .map_err(|_| format!("'{}' is not a valid integer", value))?;
            }
            "use_tabs" => {
                self.use_tabs = parse_bool(value)?;
            }
            "color_output" => {
                self.color_output = parse_bool(value)?;
            }
            "auto_find_key_files" => {
                self.auto_find_key_files = parse_bool(value)?;
            }
            "key_search_paths" => {
                self.key_search_paths = if value.is_empty() {
                    Vec::new()
                } else {
                    value.split(',').map(|s| s.trim().to_string()).collect()
                };
            }
            "pretty_print_json" => {
                self.pretty_print_json = parse_bool(value)?;
            }
            "show_warnings" => {
                self.show_warnings = parse_bool(value)?;
            }
            "max_error_display" => {
                self.max_error_display = value
                    .parse()
                    .map_err(|_| format!("'{}' is not a valid integer", value))?;
            }
            other => return Err(format!("Unknown config key: '{}'", other)),
        }
        Ok(())
    }

    /// Reset a single key to its default value.
    pub fn reset_key(&mut self, key: &str) -> Result<(), String> {
        let defaults = CliConfig::default();
        self.set_value(key, &defaults.get_value(key)?)
    }

    /// Return all key-value pairs with a flag indicating whether the value
    /// matches the default.
    pub fn list_all(&self) -> Vec<(String, String, bool)> {
        let defaults = CliConfig::default();
        let keys = [
            "default_output_directory",
            "default_indent_size",
            "use_tabs",
            "color_output",
            "auto_find_key_files",
            "key_search_paths",
            "pretty_print_json",
            "show_warnings",
            "max_error_display",
        ];

        keys.iter()
            .map(|k| {
                let current = self.get_value(k).unwrap_or_default();
                let default = defaults.get_value(k).unwrap_or_default();
                let is_default = current == default;
                (k.to_string(), current, is_default)
            })
            .collect()
    }
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on"  => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(format!("'{}' is not a valid boolean (use true/false)", other)),
    }
  }
