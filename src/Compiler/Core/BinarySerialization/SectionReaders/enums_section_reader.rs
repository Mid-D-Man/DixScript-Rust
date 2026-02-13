//! Reads @ENUMS section from binary format

use std::io::Read;
use crate::Compiler::AST::{EnumsSection, EnumDeclaration, EnumField, Position};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Reads @ENUMS section from binary format
/// Format: [Section ID: 4][Section Length: 4][Enum Count: 4][Enums...]
/// Each enum: [Name Length: 4][Name UTF-8][Field Count: 4][Fields...]
/// Each field: [Name Length: 4][Name UTF-8][Value: 4]
pub struct EnumsSectionReader<'a> {
    context: &'a mut BinarySerializationContext,
    error_manager: ErrorManager,
}

impl<'a> EnumsSectionReader<'a> {
    /// Create new enums section reader
    pub fn new(context: &'a mut BinarySerializationContext) -> Self {
        EnumsSectionReader {
            context,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Read @ENUMS section from binary
    pub fn read_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
    ) -> Result<EnumsSection, BinarySerializationError> {
        self.context.log_info(&format!(
            "Reading @ENUMS section from offset {}",
            offset.offset
        ));

        // Read and validate section ID
        let mut id_buf = [0u8; 4];
        reader.read_exact(&mut id_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumsSection"))?;
        let section_id = u32::from_le_bytes(id_buf);

        if section_id != SectionId::Enums as u32 {
            return Err(BinarySerializationError::invalid_section_id(
                section_id,
                "EnumsSection",
            ));
        }

        // Read section length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumsSection"))?;
        let section_length = i32::from_le_bytes(len_buf);
        self.context.log_debug(&format!("Section length: {} bytes", section_length));

        // Read enum count
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumsSection"))?;
        let enum_count = i32::from_le_bytes(count_buf);
        self.context.log_info(&format!("Reading {} enums", enum_count));

        // Read all enums
        let mut enums = Vec::with_capacity(enum_count as usize);
        for _ in 0..enum_count {
            let enum_decl = self.read_enum_declaration(reader)?;
            enums.push(enum_decl);
        }

        self.context.log_info(&format!("✅ @ENUMS section read: {} enums", enum_count));

        Ok(EnumsSection::new(enums, Position::UNKNOWN))
    }

    /// Read individual enum declaration
    /// Format: [Name Length: 4][Name UTF-8][Field Count: 4][Fields...]
    fn read_enum_declaration<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<EnumDeclaration, BinarySerializationError> {
        // Read enum name length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumDeclaration"))?;
        let name_length = i32::from_le_bytes(len_buf) as usize;

        // Read enum name
        let mut name_bytes = vec![0u8; name_length];
        reader.read_exact(&mut name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumDeclaration"))?;
        let name = String::from_utf8(name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumDeclaration"))?;

        // Read field count
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumDeclaration"))?;
        let field_count = i32::from_le_bytes(count_buf);

        self.context.log_debug(&format!("  Enum: {} ({} fields)", name, field_count));

        // Read all fields
        let mut fields = Vec::with_capacity(field_count as usize);
        for _ in 0..field_count {
            let field = self.read_enum_field(reader)?;
            fields.push(field);
        }

        Ok(EnumDeclaration::new(name, fields, Position::UNKNOWN))
    }

    /// Read individual enum field
    /// Format: [Name Length: 4][Name UTF-8][Value: 4]
    fn read_enum_field<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<EnumField, BinarySerializationError> {
        // Read field name length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumField"))?;
        let name_length = i32::from_le_bytes(len_buf) as usize;

        // Read field name
        let mut name_bytes = vec![0u8; name_length];
        reader.read_exact(&mut name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumField"))?;
        let name = String::from_utf8(name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumField"))?;

        // Read field value
        let mut value_buf = [0u8; 4];
        reader.read_exact(&mut value_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "EnumField"))?;
        let value = i32::from_le_bytes(value_buf);

        self.context.log_debug(&format!("    Field: {} = {}", name, value));

        Ok(EnumField::new(name, Some(value), Position::UNKNOWN))
    }
      }
