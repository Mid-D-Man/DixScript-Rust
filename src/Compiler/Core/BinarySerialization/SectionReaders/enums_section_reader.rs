//! Reads @ENUMS section from binary format.

use std::io::Read;
use crate::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField, Position};
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Reads @ENUMS section from binary format.
/// Format: [Section ID: 4][Section Length: 4][Enum Count: 4][Enums...]
/// Each enum:  [Name Length: 4][Name UTF-8][Field Count: 4][Fields...]
/// Each field: [Name Length: 4][Name UTF-8][Value: 4]
pub struct EnumsSectionReader<'a> {
    context: &'a mut BinarySerializationContext,
}

impl<'a> EnumsSectionReader<'a> {
    pub fn new(context: &'a mut BinarySerializationContext) -> Self {
        EnumsSectionReader { context }
    }

    pub fn read_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
    ) -> Result<EnumsSection, BinarySerializationError> {
        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!(
                "Reading @ENUMS section from offset {}",
                offset.offset
            ));
        }

        let mut id_buf = [0u8; 4];
        reader
            .read_exact(&mut id_buf)
            .map_err(|e| self.read_err(e.to_string(), "EnumsSection"))?;
        let section_id = u32::from_le_bytes(id_buf);
        if section_id != SectionId::Enums as u32 {
            return Err(BinarySerializationError::invalid_section_id(section_id, "EnumsSection"));
        }

        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .map_err(|e| self.read_err(e.to_string(), "EnumsSection"))?;
        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  section length: {} bytes",
                i32::from_le_bytes(len_buf)
            ));
        }

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "EnumsSection"))?;
        let enum_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_enabled {
            self.context
                .log_info(&format!("  reading {} enums", enum_count));
        }

        let mut enums = Vec::with_capacity(enum_count as usize);
        for _ in 0..enum_count {
            let decl = self.read_enum_declaration(reader)?;
            enums.push(decl);
            if self.context.error_manager.should_terminate_parsing() {
                return Err(BinarySerializationError::invalid_state(
                    "Terminating ENUMS read due to accumulated errors",
                    "EnumsSection",
                ));
            }
        }

        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!("@ENUMS read: {} enums", enum_count));
        }

        Ok(EnumsSection::new(enums, Position::UNKNOWN))
    }

    fn read_enum_declaration<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<EnumDeclaration, BinarySerializationError> {
        let name = self.read_string(reader, "EnumDeclaration")?;

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "EnumDeclaration"))?;
        let field_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  enum: {} ({} fields)",
                name, field_count
            ));
        }

        let mut fields = Vec::with_capacity(field_count as usize);
        for _ in 0..field_count {
            fields.push(self.read_enum_field(reader)?);
        }

        Ok(EnumDeclaration::new(name, fields, Position::UNKNOWN))
    }

    fn read_enum_field<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<EnumField, BinarySerializationError> {
        let name = self.read_string(reader, "EnumField")?;

        let mut value_buf = [0u8; 4];
        reader
            .read_exact(&mut value_buf)
            .map_err(|e| self.read_err(e.to_string(), "EnumField"))?;
        let value = i32::from_le_bytes(value_buf);

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("    field: {} = {}", name, value));
        }

        Ok(EnumField::new(name, Some(value), Position::UNKNOWN))
    }

    fn read_string(&self, reader: &mut impl Read, location: &str) -> Result<String, BinarySerializationError> {
        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .map_err(|e| self.read_err(e.to_string(), location))?;
        let length = i32::from_le_bytes(len_buf) as usize;

        let mut bytes = vec![0u8; length];
        reader
            .read_exact(&mut bytes)
            .map_err(|e| self.read_err(e.to_string(), location))?;

        String::from_utf8(bytes).map_err(|e| self.read_err(e.to_string(), location))
    }

    fn read_err(&self, message: String, location: &str) -> BinarySerializationError {
        let e = BinarySerializationError::read_error(message, location);
        self.context
            .error_manager
            .add_binary_serialization_error(e.error_type, e.message.clone(), None, None, None, None);
        e
    }
}
