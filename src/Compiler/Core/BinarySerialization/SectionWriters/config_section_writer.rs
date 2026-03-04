//! Writes @CONFIG section to binary format.

use std::io::{Write, Seek, SeekFrom};
use crate::Compiler::AST::{ConfigSection, ConfigEntry, ConfigValue, Value, Position};
use crate::Compiler::AST::data_types::{ErrorHandlingStrategy, CompatibilityMode, DebugMode};
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_encoder::ValueEncoder;

/// Writes @CONFIG section to binary format.
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
/// Each entry: [Key Length: 4][Key UTF-8][Value Type: 1][Value Data]
pub struct ConfigSectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
    value_encoder: &'a mut ValueEncoder,
}

impl<'a> ConfigSectionWriter<'a> {
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_encoder: &'a mut ValueEncoder,
    ) -> Self {
        ConfigSectionWriter { context, value_encoder }
    }

    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        config: &ConfigSection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!(
                "Writing @CONFIG section ({} entries)",
                config.entries.len()
            ));
        }

        let start_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "ConfigSection"))?
            as i32;

        writer
            .write_all(&(SectionId::Config as u32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "ConfigSection"))?;

        let length_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "ConfigSection"))?;
        writer
            .write_all(&0i32.to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "ConfigSection"))?;

        writer
            .write_all(&(config.entries.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "ConfigSection"))?;

        for entry in &config.entries {
            self.write_config_entry(writer, entry)?;
            if self.context.error_manager.should_terminate_parsing() {
                return Err(BinarySerializationError::invalid_state(
                    "Terminating CONFIG write due to accumulated errors",
                    "ConfigSection",
                ));
            }
        }

        let end_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "ConfigSection"))?
            as i32;
        let section_length = end_pos - start_pos;

        writer
            .seek(SeekFrom::Start(length_pos))
            .map_err(|e| self.write_err(e.to_string(), "ConfigSection"))?;
        writer
            .write_all(&section_length.to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "ConfigSection"))?;
        writer
            .seek(SeekFrom::Start(end_pos as u64))
            .map_err(|e| self.write_err(e.to_string(), "ConfigSection"))?;

        if self.context.debug_config.is_enabled {
            self.context
                .log_info(&format!("@CONFIG written: {} bytes", section_length));
        }

        self.context
            .statistics
            .record_section_size(SectionId::Config, section_length as usize);

        Ok(SectionOffset::new(SectionId::Config, start_pos, end_pos - start_pos))
    }

    fn write_config_entry<W: Write>(
        &mut self,
        writer: &mut W,
        entry: &ConfigEntry,
    ) -> Result<(), BinarySerializationError> {
        let key_bytes = entry.key.as_bytes();
        writer
            .write_all(&(key_bytes.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "ConfigEntry"))?;
        writer
            .write_all(key_bytes)
            .map_err(|e| self.write_err(e.to_string(), "ConfigEntry"))?;

        let ast_value = self.config_value_to_ast(&entry.value)?;
        self.value_encoder
            .encode_value(writer, &ast_value, self.context)
            .map_err(|e| self.write_err(e.to_string(), "ConfigEntry"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  config entry: {} = {}", entry.key, entry.value));
        }

        Ok(())
    }

    fn config_value_to_ast(
        &self,
        config_value: &ConfigValue,
    ) -> Result<Value, BinarySerializationError> {
        let value = match config_value {
            ConfigValue::String(s) => Value::String {
                value: s.clone(),
                position: Position::UNKNOWN,
            },
            ConfigValue::Integer(i) => Value::Integer {
                value: *i,
                position: Position::UNKNOWN,
            },
            ConfigValue::Float(f) => Value::Float {
                value: *f,
                position: Position::UNKNOWN,
            },
            ConfigValue::Boolean(b) => Value::Boolean {
                value: *b,
                position: Position::UNKNOWN,
            },
            ConfigValue::Date(d) => Value::Date {
                value: d.clone(),
                position: Position::UNKNOWN,
            },
            ConfigValue::Timestamp(t) => Value::Timestamp {
                value: t.clone(),
                position: Position::UNKNOWN,
            },
            ConfigValue::ErrorHandling(strategy) => {
                let s = match strategy {
                    ErrorHandlingStrategy::Halt => "halt",
                    ErrorHandlingStrategy::Continue => "continue",
                    ErrorHandlingStrategy::Recover => "recover",
                };
                Value::String { value: s.to_string(), position: Position::UNKNOWN }
            }
            ConfigValue::Compatibility(mode) => {
                let s = match mode {
                    CompatibilityMode::Strict => "strict",
                    CompatibilityMode::BestEffort => "best_effort",
                    CompatibilityMode::Permissive => "permissive",
                };
                Value::String { value: s.to_string(), position: Position::UNKNOWN }
            }
            ConfigValue::Debug(mode) => {
                let s = match mode {
                    DebugMode::Off => "off",
                    DebugMode::Regular => "regular",
                    DebugMode::Verbose => "verbose",
                };
                Value::String { value: s.to_string(), position: Position::UNKNOWN }
            }
            ConfigValue::Features(features) => Value::String {
                value: features.join(","),
                position: Position::UNKNOWN,
            },
        };
        Ok(value)
    }

    fn write_err(&self, message: String, location: &str) -> BinarySerializationError {
        let e = BinarySerializationError::write_error(message, location);
        self.context
            .error_manager
            .add_binary_serialization_error(e.error_type, e.message.clone(), None, None, None, None);
        e
    }
}
