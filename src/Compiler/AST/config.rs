use super::position::Position;
use super::data_types::{ErrorHandlingStrategy, CompatibilityMode, DebugMode};

/// @CONFIG Section
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigSection {
    pub entries: Vec<ConfigEntry>,
    pub position: Position,
}

impl ConfigSection {
    pub fn new(entries: Vec<ConfigEntry>, position: Position) -> Self {
        ConfigSection { entries, position }
    }
}

impl std::fmt::Display for ConfigSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "@CONFIG(")?;
        for (i, entry) in self.entries.iter().enumerate() {
            write!(f, "  {} -> {}", entry.key, entry.value)?;
            if i < self.entries.len() - 1 {
                writeln!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }
        write!(f, ")")
    }
}

/// Single config entry (key -> value)
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigEntry {
    pub key: String,
    pub value: ConfigValue,
    pub position: Position,
}

impl ConfigEntry {
    pub fn new(key: String, value: ConfigValue, position: Position) -> Self {
        ConfigEntry { key, value, position }
    }
}

impl std::fmt::Display for ConfigEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -> {}", self.key, self.value)
    }
}

/// Config value types
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    String(String),
    Integer(i32),
    Float(f32),
    Boolean(bool),
    Date(String),
    Timestamp(String),
    Features(Vec<String>),
    ErrorHandling(ErrorHandlingStrategy),
    Compatibility(CompatibilityMode),
    Debug(DebugMode),
}

impl std::fmt::Display for ConfigValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigValue::String(s) => write!(f, "\"{}\"", s),
            ConfigValue::Integer(i) => write!(f, "{}", i),
            ConfigValue::Float(fl) => write!(f, "{}", fl),
            ConfigValue::Boolean(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            ConfigValue::Date(d) => write!(f, "{}", d),
            ConfigValue::Timestamp(t) => write!(f, "{}", t),
            ConfigValue::Features(features) => write!(f, "\"{}\"", features.join(",")),
            ConfigValue::ErrorHandling(eh) => write!(f, "\"{}\"", eh),
            ConfigValue::Compatibility(cm) => write!(f, "\"{}\"", cm),
            ConfigValue::Debug(dm) => write!(f, "\"{}\"", dm),
        }
    }
}