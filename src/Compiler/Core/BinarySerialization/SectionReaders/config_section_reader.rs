//! Reads @CONFIG section from binary format

use std::io::Read;
use crate::Compiler::AST::{ConfigSection, ConfigEntry, ConfigValue, Position};
use crate::Compiler::AST::data_types::{ErrorHandlingStrategy, CompatibilityMode, DebugMode};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_decoder::ValueDecoder;
use crate::Compiler::AST::Value;

/// Reads @CONFIG section from binary format
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
/// Each entry: [Key Length: 4][Key UTF-8][Value Type: 1][Value Data]
pub struct ConfigSectionReader<'a> {
    context: &'a mut BinarySerializationContext,
    value_decoder: &'a mut ValueDecoder<'a>,
    error_manager: ErrorManager,
}

impl<'a> ConfigSectionReader<'a> {
    /// Create new config section reader
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_decoder: &'a mut ValueDecoder<'a>,
    ) -> Self {
        ConfigSectionReader {
            context,
            value_decoder,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Read @CONFIG section from binary
    pub fn read_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
    ) -> Result<ConfigSection, BinarySerializationError> {
        self.context.log_info(&format!(
            "Reading @CONFIG section from offset {}",
            offset.offset
        ));

        // Read and validate section ID
        let mut id_buf = [0u8; 4];
        reader.read_exact(&mut id_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ConfigSection"))?;
        let section_id = u32::from_le_bytes(id_buf);

        if section_id != SectionId::Config as u32 {
            return Err(BinarySerializationError::invalid_section_id(
                section_id,
                "ConfigSection",
            ));
        }

        // Read section length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ConfigSection"))?;
        let section_length = i32::from_le_bytes(len_buf);
        self.context.log_debug(&format!("Section length: {} bytes", section_length));

        // Read entry count
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ConfigSection"))?;
        let entry_count = i32::from_le_bytes(count_buf);
        self.context.log_info(&format!("Reading {} config entries", entry_count));

        // Read all entries
        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let entry = self.read_config_entry(reader)?;
            entries.push(entry);
        }

        self.context.log_info(&format!("✅ @CONFIG section read: {} entries", entry_count));

        Ok(ConfigSection::new(entries, Position::UNKNOWN))
    }

    /// Read individual config entry
    /// Format: [Key Length: 4][Key UTF-8][Value]
    fn read_config_entry<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<ConfigEntry, BinarySerializationError> {
        // Read key length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ConfigEntry"))?;
        let key_length = i32::from_le_bytes(len_buf) as usize;

        // Read key
        let mut key_bytes = vec![0u8; key_length];
        reader.read_exact(&mut key_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ConfigEntry"))?;
        let key = String::from_utf8(key_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ConfigEntry"))?;

        // Read value
        let ast_value = self.value_decoder.decode_value(reader)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ConfigEntry"))?;

        // Convert AST Value to ConfigValue
        let config_value = self.convert_ast_value_to_config_value(&key, &ast_value)?;

        self.context.log_debug(&format!("  Config entry: {} = {}", key, config_value));

        Ok(ConfigEntry::new(key, config_value, Position::UNKNOWN))
    }

    /// Convert AST Value to ConfigValue based on key name
    fn convert_ast_value_to_config_value(
        &self,
        key: &str,
        ast_value: &Value,
    ) -> Result<ConfigValue, BinarySerializationError> {
        // Special handling for known config keys with specific types
        match key.to_lowercase().as_str() {
            "error_handling" => self.parse_error_handling_value(ast_value),
            "compatibility_mode" => self.parse_compatibility_value(ast_value),
            "debug_mode" => self.parse_debug_value(ast_value),
            "features" => self.parse_feature_value(ast_value),
            "created" => match ast_value {
                Value::Timestamp { value, .. } => Ok(ConfigValue::Timestamp(value.clone())),
                Value::Date { value, .. } => Ok(ConfigValue::Date(value.clone())),
                _ => self.convert_generic_value(ast_value),
            },
            _ => self.convert_generic_value(ast_value),
        }
    }

    fn parse_error_handling_value(
        &self,
        ast_value: &Value,
    ) -> Result<ConfigValue, BinarySerializationError> {
        if let Value::String { value, .. } = ast_value {
            let strategy = match value.to_lowercase().as_str() {
                "halt" => ErrorHandlingStrategy::Halt,
                "continue" => ErrorHandlingStrategy::Continue,
                "recover" => ErrorHandlingStrategy::Recover,
                _ => ErrorHandlingStrategy::Halt,
            };
            Ok(ConfigValue::ErrorHandling(strategy))
        } else {
            Err(BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                "error_handling must be a string",
                self.context.get_current_scope(),
            ))
        }
    }

    fn parse_compatibility_value(
        &self,
        ast_value: &Value,
    ) -> Result<ConfigValue, BinarySerializationError> {
        if let Value::String { value, .. } = ast_value {
            let mode = match value.to_lowercase().as_str() {
                "strict" => CompatibilityMode::Strict,
                "best_effort" => CompatibilityMode::BestEffort,
                "permissive" => CompatibilityMode::Permissive,
                _ => CompatibilityMode::Strict,
            };
            Ok(ConfigValue::Compatibility(mode))
        } else {
            Err(BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                "compatibility_mode must be a string",
                self.context.get_current_scope(),
            ))
        }
    }

    fn parse_debug_value(
        &self,
        ast_value: &Value,
    ) -> Result<ConfigValue, BinarySerializationError> {
        if let Value::String { value, .. } = ast_value {
            let mode = match value.to_lowercase().as_str() {
                "off" => DebugMode::Off,
                "regular" => DebugMode::Regular,
                "verbose" => DebugMode::Verbose,
                _ => DebugMode::Off,
            };
            Ok(ConfigValue::Debug(mode))
        } else {
            Err(BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                "debug_mode must be a string",
                self.context.get_current_scope(),
            ))
        }
    }

    fn parse_feature_value(
        &self,
        ast_value: &Value,
    ) -> Result<ConfigValue, BinarySerializationError> {
        if let Value::String { value, .. } = ast_value {
            let features: Vec<String> = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            Ok(ConfigValue::Features(features))
        } else {
            Err(BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                "features must be a string",
                self.context.get_current_scope(),
            ))
        }
    }

    fn convert_generic_value(
        &self,
        ast_value: &Value,
    ) -> Result<ConfigValue, BinarySerializationError> {
        match ast_value {
            Value::String { value, .. } => Ok(ConfigValue::String(value.clone())),
            Value::Integer { value, .. } => Ok(ConfigValue::Integer(*value)),
            Value::Float { value, .. } => Ok(ConfigValue::Float(*value)),
            Value::Boolean { value, .. } => Ok(ConfigValue::Boolean(*value)),
            Value::Date { value, .. } => Ok(ConfigValue::Date(value.clone())),
            Value::Timestamp { value, .. } => Ok(ConfigValue::Timestamp(value.clone())),
            _ => Err(BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                format!("Unsupported config value type: {:?}", ast_value),
                self.context.get_current_scope(),
            )),
        }
    }
      }
