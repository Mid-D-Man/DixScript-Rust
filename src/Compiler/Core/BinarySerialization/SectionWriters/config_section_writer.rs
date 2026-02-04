//! Writes @CONFIG section to binary format

use std::io::{Write, Seek, SeekFrom, Cursor};
use crate::Compiler::AST::{ConfigSection, ConfigEntry, ConfigValue};
use crate::Compiler::AST::data_types::{ErrorHandlingStrategy, CompatibilityMode, DebugMode};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_encoder::ValueEncoder;
use crate::Compiler::AST::Value;

/// Writes @CONFIG section to binary format
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
/// Each entry: [Key Length: 4][Key UTF-8][Value]
pub struct ConfigSectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
    value_encoder: &'a mut ValueEncoder<'a>,
    error_manager: ErrorManager,
}

impl<'a> ConfigSectionWriter<'a> {
    /// Create new config section writer
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_encoder: &'a mut ValueEncoder<'a>,
    ) -> Self {
        ConfigSectionWriter {
            context,
            value_encoder,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Write @CONFIG section to binary
    /// Returns offset information for offset table
    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        config_section: &ConfigSection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        self.context.log_info(&format!(
            "Writing @CONFIG section ({} entries)",
            config_section.entries.len()
        ));

        let start_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?
            as i32;

        // Write section header
        writer.write_all(&(SectionId::Config as u32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        // Placeholder for section length (will update later)
        let length_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;
        writer.write_all(&0i32.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        // Write entry count
        writer.write_all(&(config_section.entries.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        // Write each config entry
        for entry in &config_section.entries {
            self.write_config_entry(writer, entry)?;
        }

        // Calculate and update section length
        let end_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?
            as i32;
        let section_length = end_position - start_position - 8; // Exclude section ID and length field

        writer.seek(SeekFrom::Start(length_position))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;
        writer.write_all(&section_length.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;
        writer.seek(SeekFrom::Start(end_position as u64))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        self.context.log_info(&format!("✅ @CONFIG section written: {} bytes", section_length));
        self.context.statistics.record_section_size(SectionId::Config, section_length as usize);

        Ok(SectionOffset::new(
            SectionId::Config,
            start_position,
            end_position - start_position,
        ))
    }

    /// Write individual config entry
    /// Format: [Key Length: 4][Key UTF-8][Value]
    fn write_config_entry<W: Write>(
        &mut self,
        writer: &mut W,
        entry: &ConfigEntry,
    ) -> Result<(), BinarySerializationError> {
        // Write key
        let key_bytes = entry.key.as_bytes();
        writer.write_all(&(key_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigEntry"))?;
        writer.write_all(key_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigEntry"))?;

        // Convert ConfigValue to AST Value and write
        let ast_value = self.convert_config_value_to_ast_value(&entry.value);
        self.value_encoder.encode_value(writer, &ast_value)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigEntry"))?;

        self.context.log_debug(&format!("  Config entry: {} = {}", entry.key, entry.value));

        Ok(())
    }

    /// Convert ConfigValue to AST Value for encoding
    fn convert_config_value_to_ast_value(&self, config_value: &ConfigValue) -> Value {
        use crate::Compiler::AST::Position;

        match config_value {
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
            ConfigValue::Features(features) => Value::String {
                value: features.join(","),
                position: Position::UNKNOWN,
            },
            ConfigValue::ErrorHandling(strategy) => Value::String {
                value: match strategy {
                    ErrorHandlingStrategy::Halt => "halt",
                    ErrorHandlingStrategy::Continue => "continue",
                    ErrorHandlingStrategy::Recover => "recover",
                }.to_string(),
                position: Position::UNKNOWN,
            },
            ConfigValue::Compatibility(mode) => Value::String {
                value: match mode {
                    CompatibilityMode::Strict => "strict",
                    CompatibilityMode::BestEffort => "best_effort",
                    CompatibilityMode::Permissive => "permissive",
                }.to_string(),
                position: Position::UNKNOWN,
            },
            ConfigValue::Debug(debug_mode) => Value::String {
                value: match debug_mode {
                    DebugMode::Off => "off",
                    DebugMode::Regular => "regular",
                    DebugMode::Verbose => "verbose",
                }.to_string(),
                position: Position::UNKNOWN,
            },
        }
    }
      }
