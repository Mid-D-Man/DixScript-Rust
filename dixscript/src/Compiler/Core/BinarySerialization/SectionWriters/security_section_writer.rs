//! Writes @SECURITY section to binary format.

use std::io::{Write, Seek, SeekFrom};
use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField};
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_encoder::ValueEncoder;

/// Writes @SECURITY section to binary format.
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
/// Each entry: [Block Key Length: 4][Block Key UTF-8][Field Count: 4][Fields...]
/// Each field:  [Key Length: 4][Key UTF-8][Value]
pub struct SecuritySectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
    value_encoder: &'a mut ValueEncoder,
}

impl<'a> SecuritySectionWriter<'a> {
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_encoder: &'a mut ValueEncoder,
    ) -> Self {
        SecuritySectionWriter { context, value_encoder }
    }

    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        security_section: &SecuritySection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!(
                "Writing @SECURITY section ({} entries)",
                security_section.entries.len()
            ));
        }

        let start_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "SecuritySection"))?
            as i32;

        writer
            .write_all(&(SectionId::Security as u32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "SecuritySection"))?;

        let length_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "SecuritySection"))?;
        writer
            .write_all(&0i32.to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "SecuritySection"))?;

        writer
            .write_all(&(security_section.entries.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "SecuritySection"))?;

        for entry in &security_section.entries {
            self.write_security_entry(writer, entry)?;
            if self.context.error_manager.should_terminate_parsing() {
                return Err(BinarySerializationError::invalid_state(
                    "Terminating SECURITY write due to accumulated errors",
                    "SecuritySection",
                ));
            }
        }

        let end_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "SecuritySection"))?
            as i32;
        let section_length = end_pos - start_pos;

        writer
            .seek(SeekFrom::Start(length_pos))
            .map_err(|e| self.write_err(e.to_string(), "SecuritySection"))?;
        writer
            .write_all(&section_length.to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "SecuritySection"))?;
        writer
            .seek(SeekFrom::Start(end_pos as u64))
            .map_err(|e| self.write_err(e.to_string(), "SecuritySection"))?;

        if self.context.debug_config.is_enabled {
            self.context
                .log_info(&format!("@SECURITY written: {} bytes", section_length));
        }

        self.context
            .statistics
            .record_section_size(SectionId::Security, section_length as usize);

        Ok(SectionOffset::new(SectionId::Security, start_pos, end_pos - start_pos))
    }

    fn write_security_entry<W: Write>(
        &mut self,
        writer: &mut W,
        entry: &SecurityEntry,
    ) -> Result<(), BinarySerializationError> {
        self.write_string_field(writer, &entry.block_key, "SecurityEntry")?;

        writer
            .write_all(&(entry.fields.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "SecurityEntry"))?;

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  security block: {} ({} fields)",
                entry.block_key,
                entry.fields.len()
            ));
        }

        for field in &entry.fields {
            self.write_security_field(writer, field)?;
        }

        Ok(())
    }

    fn write_security_field<W: Write>(
        &mut self,
        writer: &mut W,
        field: &SecurityField,
    ) -> Result<(), BinarySerializationError> {
        self.write_string_field(writer, &field.key, "SecurityField")?;

        self.value_encoder
            .encode_value(writer, &field.value, self.context)
            .map_err(|e| self.write_err(e.to_string(), "SecurityField"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("    field: {} = {}", field.key, field.value));
        }

        Ok(())
    }

    fn write_string_field<W: Write>(
        &mut self,
        writer: &mut W,
        value: &str,
        location: &str,
    ) -> Result<(), BinarySerializationError> {
        let bytes = value.as_bytes();
        writer
            .write_all(&(bytes.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), location))?;
        writer
            .write_all(bytes)
            .map_err(|e| self.write_err(e.to_string(), location))?;
        Ok(())
    }

    fn write_err(&self, message: String, location: &str) -> BinarySerializationError {
        let e = BinarySerializationError::write_error(message, location);
        self.context
            .error_manager
            .add_binary_serialization_error(e.error_type, e.message.clone(), None, None, None, None);
        e
    }
}
