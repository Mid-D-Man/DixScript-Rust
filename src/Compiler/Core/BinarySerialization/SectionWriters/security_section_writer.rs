//! Writes @SECURITY section to binary format

use std::io::{Write, Seek, SeekFrom};
use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_encoder::ValueEncoder;

/// Writes @SECURITY section to binary format
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
/// Each entry: [Block Key Length: 4][Block Key UTF-8][Field Count: 4][Fields...]
/// Each field: [Key Length: 4][Key UTF-8][Value]
pub struct SecuritySectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
    value_encoder: &'a mut ValueEncoder,
    error_manager: ErrorManager,
}

impl<'a> SecuritySectionWriter<'a> {
    /// Create new security section writer
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_encoder: &'a mut ValueEncoder,
    ) -> Self {
        SecuritySectionWriter {
            context,
            value_encoder,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Write @SECURITY section to binary
    /// Returns offset information for offset table
    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        security_section: &SecuritySection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        self.context.log_info(&format!(
            "Writing @SECURITY section ({} entries)",
            security_section.entries.len()
        ));

        let start_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?
            as i32;

        // Write section header
        writer.write_all(&(SectionId::Security as u32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?;

        // Placeholder for section length
        let length_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?;
        writer.write_all(&0i32.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?;

        // Write entry count
        writer.write_all(&(security_section.entries.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?;

        // Write each security entry
        for entry in &security_section.entries {
            self.write_security_entry(writer, entry)?;
        }

        // Calculate and update section length
        let end_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?
            as i32;
        let section_length = end_position - start_position - 8;

        writer.seek(SeekFrom::Start(length_position))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?;
        writer.write_all(&section_length.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?;
        writer.seek(SeekFrom::Start(end_position as u64))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?;

        self.context.log_info(&format!("✅ @SECURITY section written: {} bytes", section_length));
        self.context.statistics.record_section_size(SectionId::Security, section_length as usize);

        Ok(SectionOffset::new(
            SectionId::Security,
            start_position,
            end_position - start_position,
        ))
    }

    /// Write individual security entry
    /// Format: [Block Key Length: 4][Block Key UTF-8][Field Count: 4][Fields...]
    /// Example: encryption -> { mode = "password", algorithm = "aes256-gcm" }
    fn write_security_entry<W: Write>(
        &mut self,
        writer: &mut W,
        entry: &SecurityEntry,
    ) -> Result<(), BinarySerializationError> {
        // Write block key
        let block_key_bytes = entry.block_key.as_bytes();
        writer.write_all(&(block_key_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecurityEntry"))?;
        writer.write_all(block_key_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecurityEntry"))?;

        // Write field count
        writer.write_all(&(entry.fields.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecurityEntry"))?;

        self.context.log_debug(&format!(
            "  Security block: {} ({} fields)",
            entry.block_key,
            entry.fields.len()
        ));

        // Write each field
        for field in &entry.fields {
            self.write_security_field(writer, field)?;
        }

        Ok(())
    }

    /// Write individual security field
    /// Format: [Key Length: 4][Key UTF-8][Value]
    fn write_security_field<W: Write>(
        &mut self,
        writer: &mut W,
        field: &SecurityField,
    ) -> Result<(), BinarySerializationError> {
        // Write field key
        let key_bytes = field.key.as_bytes();
        writer.write_all(&(key_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecurityField"))?;
        writer.write_all(key_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecurityField"))?;

        // Write field value
        self.value_encoder.encode_value(writer, &field.value,self.context)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecurityField"))?;

        self.context.log_debug(&format!("    Field: {} = {}", field.key, field.value));

        Ok(())
    }
  }
