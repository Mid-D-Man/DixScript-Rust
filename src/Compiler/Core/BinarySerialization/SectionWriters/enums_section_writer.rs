//! Writes @ENUMS section to binary format

use std::io::{Write, Seek, SeekFrom};
use crate::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Writes @ENUMS section to binary format
/// Format: [Section ID: 4][Section Length: 4][Enum Count: 4][Enums...]
/// Each enum: [Name Length: 4][Name UTF-8][Field Count: 4][Fields...]
/// Each field: [Name Length: 4][Name UTF-8][Value: 4]
pub struct EnumsSectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
    error_manager: ErrorManager,
}

impl<'a> EnumsSectionWriter<'a> {
    /// Create new enums section writer
    pub fn new(context: &'a mut BinarySerializationContext) -> Self {
        EnumsSectionWriter {
            context,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Write @ENUMS section to binary
    /// Returns offset information for offset table
    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        enums_section: &EnumsSection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        self.context.log_info(&format!(
            "Writing @ENUMS section ({} enums)",
            enums_section.enums.len()
        ));

        let start_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?
            as i32;

        // Write section header
        writer.write_all(&(SectionId::Enums as u32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?;

        // Placeholder for section length
        let length_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?;
        writer.write_all(&0i32.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?;

        // Write enum count
        writer.write_all(&(enums_section.enums.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?;

        // Write each enum
        for enum_decl in &enums_section.enums {
            self.write_enum_declaration(writer, enum_decl)?;
        }

        // Calculate and update section length
        let end_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?
            as i32;
        let section_length = end_position - start_position - 8;

        writer.seek(SeekFrom::Start(length_position))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?;
        writer.write_all(&section_length.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?;
        writer.seek(SeekFrom::Start(end_position as u64))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?;

        self.context.log_info(&format!("✅ @ENUMS section written: {} bytes", section_length));
        self.context.statistics.record_section_size(SectionId::Enums, section_length as usize);

        Ok(SectionOffset::new(
            SectionId::Enums,
            start_position,
            end_position - start_position,
        ))
    }

    /// Write individual enum declaration
    /// Format: [Name Length: 4][Name UTF-8][Field Count: 4][Fields...]
    fn write_enum_declaration<W: Write>(
        &mut self,
        writer: &mut W,
        enum_decl: &EnumDeclaration,
    ) -> Result<(), BinarySerializationError> {
        // Write enum name
        let name_bytes = enum_decl.name.as_bytes();
        writer.write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumDeclaration"))?;
        writer.write_all(name_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumDeclaration"))?;

        // Write field count
        writer.write_all(&(enum_decl.fields.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumDeclaration"))?;

        self.context.log_debug(&format!(
            "  Enum: {} ({} fields)",
            enum_decl.name,
            enum_decl.fields.len()
        ));

        // Write each field
        for field in &enum_decl.fields {
            self.write_enum_field(writer, field)?;
        }

        Ok(())
    }

    /// Write individual enum field
    /// Format: [Name Length: 4][Name UTF-8][Value: 4]
    fn write_enum_field<W: Write>(
        &mut self,
        writer: &mut W,
        field: &EnumField,
    ) -> Result<(), BinarySerializationError> {
        // Write field name
        let name_bytes = field.name.as_bytes();
        writer.write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumField"))?;
        writer.write_all(name_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumField"))?;

        // Write field value (should always have value after semantic analysis)
        let value = field.value.unwrap_or(0);
        writer.write_all(&value.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumField"))?;

        self.context.log_debug(&format!("    Field: {} = {}", field.name, value));

        Ok(())
    }
      }
