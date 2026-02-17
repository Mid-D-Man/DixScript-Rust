//! Writes @CONFIG section to binary format

use std::io::{Write, Seek, SeekFrom};
use crate::Compiler::AST::{ConfigSection, ConfigEntry, ConfigValue};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_encoder::ValueEncoder;
use crate::Compiler::AST::Value;
use crate::Compiler::AST::Position;

/// Writes @CONFIG section to binary format
pub struct ConfigSectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
    value_encoder: &'a mut ValueEncoder,
    error_manager: ErrorManager,
}

impl<'a> ConfigSectionWriter<'a> {
    /// Create new config section writer
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_encoder: &'a mut ValueEncoder,
    ) -> Self {
        ConfigSectionWriter {
            context,
            value_encoder,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Write @CONFIG section to binary
    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        config: &ConfigSection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        self.context.log_info("Writing @CONFIG section...");

        // Record start position
        let start_pos = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        // Write section ID
        writer.write_all(&(SectionId::Config as u32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        // Placeholder for section length
        let length_pos = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;
        writer.write_all(&0u32.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        // Write entry count
        let entry_count = config.entries.len() as i32;
        writer.write_all(&entry_count.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        self.context.log_info(&format!("Writing {} config entries", entry_count));

        // Write all entries
        for entry in &config.entries {
            self.write_config_entry(writer, entry)?;
        }

        // Calculate and write section length
        let end_pos = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;
        let section_length = (end_pos - start_pos) as i32;

        writer.seek(SeekFrom::Start(length_pos))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;
        writer.write_all(&section_length.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;
        writer.seek(SeekFrom::Start(end_pos))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        self.context.log_info(&format!(" @CONFIG section written: {} bytes", section_length));

        Ok(SectionOffset {
            section_id: SectionId::Config,
            offset: start_pos as i32,
            length: section_length,
        })
    }

    /// Write individual config entry
    fn write_config_entry<W: Write>(
        &mut self,
        writer: &mut W,
        entry: &ConfigEntry,
    ) -> Result<(), BinarySerializationError> {
        // Write key length and key
        let key_bytes = entry.key.as_bytes();
        writer.write_all(&(key_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigEntry"))?;
        writer.write_all(key_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigEntry"))?;

        // Convert ConfigValue to AST Value
        let ast_value = self.convert_config_value_to_ast_value(&entry.value)?;

        // Write value using value encoder - PASS CONTEXT HERE
        self.value_encoder.encode_value(writer, &ast_value, self.context)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigEntry"))?;

        self.context.log_debug(&format!("  Config entry: {} = {}", entry.key, entry.value));

        Ok(())
    }

    /// Convert ConfigValue to AST Value
    fn convert_config_value_to_ast_value(
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
                    crate::Compiler::AST::data_types::ErrorHandlingStrategy::Halt => "halt",
                    crate::Compiler::AST::data_types::ErrorHandlingStrategy::Continue => "continue",
                    crate::Compiler::AST::data_types::ErrorHandlingStrategy::Recover => "recover",
                };
                Value::String {
                    value: s.to_string(),
                    position: Position::UNKNOWN,
                }
            }
            ConfigValue::Compatibility(mode) => {
                let s = match mode {
                    crate::Compiler::AST::data_types::CompatibilityMode::Strict => "strict",
                    crate::Compiler::AST::data_types::CompatibilityMode::BestEffort => "best_effort",
                    crate::Compiler::AST::data_types::CompatibilityMode::Permissive => "permissive",
                };
                Value::String {
                    value: s.to_string(),
                    position: Position::UNKNOWN,
                }
            }
            ConfigValue::Debug(mode) => {
                let s = match mode {
                    crate::Compiler::AST::data_types::DebugMode::Off => "off",
                    crate::Compiler::AST::data_types::DebugMode::Regular => "regular",
                    crate::Compiler::AST::data_types::DebugMode::Verbose => "verbose",
                };
                Value::String {
                    value: s.to_string(),
                    position: Position::UNKNOWN,
                }
            }
            ConfigValue::Features(features) => Value::String {
                value: features.join(","),
                position: Position::UNKNOWN,
            },
        };

        Ok(value)
    }
}