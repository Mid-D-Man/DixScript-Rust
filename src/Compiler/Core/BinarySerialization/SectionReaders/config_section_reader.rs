//! Reads @CONFIG section from binary format.

use std::io::Read;
use crate::Compiler::AST::{ConfigSection, ConfigEntry, ConfigValue, Position, Value};
use crate::Compiler::AST::data_types::{ErrorHandlingStrategy, CompatibilityMode, DebugMode};
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_decoder::ValueDecoder;

/// Reads @CONFIG section from binary format.
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
/// Each entry: [Key Length: 4][Key UTF-8][Value Type: 1][Value Data]
pub struct ConfigSectionReader<'a> {
    context: &'a mut BinarySerializationContext,
    value_decoder: &'a mut ValueDecoder,
}

impl<'a> ConfigSectionReader<'a> {
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_decoder: &'a mut ValueDecoder,
    ) -> Self {
        ConfigSectionReader { context, value_decoder }
    }

    pub fn read_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
    ) -> Result<ConfigSection, BinarySerializationError> {
        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!(
                "Reading @CONFIG section from offset {}",
                offset.offset
            ));
        }

        let mut id_buf = [0u8; 4];
        reader
            .read_exact(&mut id_buf)
            .map_err(|e| self.read_err(e.to_string(), "ConfigSection"))?;
        let section_id = u32::from_le_bytes(id_buf);
        if section_id != SectionId::Config as u32 {
            return Err(BinarySerializationError::invalid_section_id(section_id, "ConfigSection"));
        }

        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .map_err(|e| self.read_err(e.to_string(), "ConfigSection"))?;
        let section_length = i32::from_le_bytes(len_buf);
        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  section length: {} bytes", section_length));
        }

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "ConfigSection"))?;
        let entry_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_enabled {
            self.context
                .log_info(&format!("  reading {} config entries", entry_count));
        }

        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let entry = self.read_config_entry(reader)?;
            entries.push(entry);
            if self.context.error_manager.should_terminate_parsing() {
                return Err(BinarySerializationError::invalid_state(
                    "Terminating CONFIG read due to accumulated errors",
                    "ConfigSection",
                ));
            }
        }

        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!("@CONFIG read: {} entries", entry_count));
        }

        Ok(ConfigSection::new(entries, Position::UNKNOWN))
    }

    fn read_config_entry<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<ConfigEntry, BinarySerializationError> {
        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .map_err(|e| self.read_err(e.to_string(), "ConfigEntry"))?;
        let key_length = i32::from_le_bytes(len_buf) as usize;

        let mut key_bytes = vec![0u8; key_length];
        reader
            .read_exact(&mut key_bytes)
            .map_err(|e| self.read_err(e.to_string(), "ConfigEntry"))?;
        let key = String::from_utf8(key_bytes)
            .map_err(|e| self.read_err(e.to_string(), "ConfigEntry"))?;

        let ast_value = self
            .value_decoder
            .decode_value(reader, self.context)
            .map_err(|e| self.read_err(e.to_string(), "ConfigEntry"))?;

        let config_value = self.ast_to_config_value(&key, &ast_value)?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  config entry: {} = {}", key, config_value));
        }

        Ok(ConfigEntry::new(key, config_value, Position::UNKNOWN))
    }

    fn ast_to_config_value(
        &mut self,
        key: &str,
        ast_value: &Value,
    ) -> Result<ConfigValue, BinarySerializationError> {
        match key.to_lowercase().as_str() {
            "error_handling" => self.parse_error_handling(ast_value),
            "compatibility_mode" => self.parse_compatibility(ast_value),
            "debug_mode" => self.parse_debug_mode(ast_value),
            "features" => self.parse_features(ast_value),
            "created" => match ast_value {
                Value::Timestamp { value, .. } => Ok(ConfigValue::Timestamp(value.clone())),
                Value::Date { value, .. } => Ok(ConfigValue::Date(value.clone())),
                _ => self.generic_convert(ast_value),
            },
            _ => self.generic_convert(ast_value),
        }
    }

    fn parse_error_handling(&self, v: &Value) -> Result<ConfigValue, BinarySerializationError> {
        if let Value::String { value, .. } = v {
            let strategy = match value.to_lowercase().as_str() {
                "continue" => ErrorHandlingStrategy::Continue,
                "recover" => ErrorHandlingStrategy::Recover,
                _ => ErrorHandlingStrategy::Halt,
            };
            Ok(ConfigValue::ErrorHandling(strategy))
        } else {
            Err(self.format_err("error_handling must be a string"))
        }
    }

    fn parse_compatibility(&self, v: &Value) -> Result<ConfigValue, BinarySerializationError> {
        if let Value::String { value, .. } = v {
            let mode = match value.to_lowercase().as_str() {
                "best_effort" => CompatibilityMode::BestEffort,
                "permissive" => CompatibilityMode::Permissive,
                _ => CompatibilityMode::Strict,
            };
            Ok(ConfigValue::Compatibility(mode))
        } else {
            Err(self.format_err("compatibility_mode must be a string"))
        }
    }

    fn parse_debug_mode(&self, v: &Value) -> Result<ConfigValue, BinarySerializationError> {
        if let Value::String { value, .. } = v {
            let mode = match value.to_lowercase().as_str() {
                "regular" => DebugMode::Regular,
                "verbose" => DebugMode::Verbose,
                _ => DebugMode::Off,
            };
            Ok(ConfigValue::Debug(mode))
        } else {
            Err(self.format_err("debug_mode must be a string"))
        }
    }

    fn parse_features(&self, v: &Value) -> Result<ConfigValue, BinarySerializationError> {
        if let Value::String { value, .. } = v {
            let features = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            Ok(ConfigValue::Features(features))
        } else {
            Err(self.format_err("features must be a string"))
        }
    }

    fn generic_convert(&self, v: &Value) -> Result<ConfigValue, BinarySerializationError> {
        match v {
            Value::String { value, .. } => Ok(ConfigValue::String(value.clone())),
            Value::Integer { value, .. } => Ok(ConfigValue::Integer(*value)),
            Value::Float { value, .. } => Ok(ConfigValue::Float(*value)),
            Value::Boolean { value, .. } => Ok(ConfigValue::Boolean(*value)),
            Value::Date { value, .. } => Ok(ConfigValue::Date(value.clone())),
            Value::Timestamp { value, .. } => Ok(ConfigValue::Timestamp(value.clone())),
            _ => Err(self.format_err(&format!("Unsupported config value type: {:?}", v))),
        }
    }

    fn read_err(&self, message: String, location: &str) -> BinarySerializationError {
        let e = BinarySerializationError::read_error(message, location);
        self.context
            .error_manager
            .add_binary_serialization_error(e.error_type, e.message.clone(), None, None, None, None);
        e
    }

    fn format_err(&self, message: &str) -> BinarySerializationError {
        let e = BinarySerializationError::new(
            BinarySerializationErrorType::InvalidFormat,
            message,
            self.context.get_current_scope(),
        );
        self.context
            .error_manager
            .add_binary_serialization_error(e.error_type, e.message.clone(), None, None, None, None);
        e
    }
}
