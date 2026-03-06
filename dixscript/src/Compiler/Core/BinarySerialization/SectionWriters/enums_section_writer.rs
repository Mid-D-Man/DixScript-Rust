//! Writes @ENUMS section to binary format.

use std::io::{Write, Seek, SeekFrom};
use crate::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField};
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Writes @ENUMS section to binary format.
/// Format: [Section ID: 4][Section Length: 4][Enum Count: 4][Enums...]
/// Each enum: [Name Length: 4][Name UTF-8][Field Count: 4][Fields...]
/// Each field: [Name Length: 4][Name UTF-8][Value: 4]
pub struct EnumsSectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
}

impl<'a> EnumsSectionWriter<'a> {
    pub fn new(context: &'a mut BinarySerializationContext) -> Self {
        EnumsSectionWriter { context }
    }

    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        enums_section: &EnumsSection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!(
                "Writing @ENUMS section ({} enums)",
                enums_section.enums.len()
            ));
        }

        let start_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "EnumsSection"))?
            as i32;

        writer
            .write_all(&(SectionId::Enums as u32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "EnumsSection"))?;

        let length_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "EnumsSection"))?;
        writer
            .write_all(&0i32.to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "EnumsSection"))?;

        writer
            .write_all(&(enums_section.enums.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "EnumsSection"))?;

        for enum_decl in &enums_section.enums {
            self.write_enum_declaration(writer, enum_decl)?;
            if self.context.error_manager.should_terminate_parsing() {
                return Err(BinarySerializationError::invalid_state(
                    "Terminating ENUMS write due to accumulated errors",
                    "EnumsSection",
                ));
            }
        }

        let end_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "EnumsSection"))?
            as i32;
        let section_length = end_pos - start_pos;

        writer
            .seek(SeekFrom::Start(length_pos))
            .map_err(|e| self.write_err(e.to_string(), "EnumsSection"))?;
        writer
            .write_all(&section_length.to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "EnumsSection"))?;
        writer
            .seek(SeekFrom::Start(end_pos as u64))
            .map_err(|e| self.write_err(e.to_string(), "EnumsSection"))?;

        if self.context.debug_config.is_enabled {
            self.context
                .log_info(&format!("@ENUMS written: {} bytes", section_length));
        }

        self.context
            .statistics
            .record_section_size(SectionId::Enums, section_length as usize);

        Ok(SectionOffset::new(SectionId::Enums, start_pos, end_pos - start_pos))
    }

    fn write_enum_declaration<W: Write>(
        &mut self,
        writer: &mut W,
        enum_decl: &EnumDeclaration,
    ) -> Result<(), BinarySerializationError> {
        let name_bytes = enum_decl.name.as_bytes();
        writer
            .write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "EnumDeclaration"))?;
        writer
            .write_all(name_bytes)
            .map_err(|e| self.write_err(e.to_string(), "EnumDeclaration"))?;

        writer
            .write_all(&(enum_decl.fields.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "EnumDeclaration"))?;

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  enum: {} ({} fields)",
                enum_decl.name,
                enum_decl.fields.len()
            ));
        }

        for field in &enum_decl.fields {
            self.write_enum_field(writer, field)?;
        }

        Ok(())
    }

    fn write_enum_field<W: Write>(
        &mut self,
        writer: &mut W,
        field: &EnumField,
    ) -> Result<(), BinarySerializationError> {
        let name_bytes = field.name.as_bytes();
        writer
            .write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "EnumField"))?;
        writer
            .write_all(name_bytes)
            .map_err(|e| self.write_err(e.to_string(), "EnumField"))?;

        let value = field.value.unwrap_or(0);
        writer
            .write_all(&value.to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "EnumField"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("    field: {} = {}", field.name, value));
        }

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
