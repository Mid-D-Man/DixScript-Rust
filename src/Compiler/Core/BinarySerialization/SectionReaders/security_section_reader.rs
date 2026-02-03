//! Reads @SECURITY section from binary format

use std::io::Read;
use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Position};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_decoder::ValueDecoder;

/// Reads @SECURITY section from binary format
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
/// Each entry: [Block Key Length: 4][Block Key UTF-8][Field Count: 4][Fields...]
/// Each field: [Key Length: 4][Key UTF-8][Value]
pub struct SecuritySectionReader<'a> {
    context: &'a mut BinarySerializationContext,
    value_decoder: &'a mut ValueDecoder<'a>,
    error_manager: ErrorManager,
}

impl<'a> SecuritySectionReader<'a> {
    /// Create new security section reader
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_decoder: &'a mut ValueDecoder<'a>,
    ) -> Self {
        SecuritySectionReader {
            context,
            value_decoder,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Read @SECURITY section from binary
    pub fn read_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
    ) -> Result<SecuritySection, BinarySerializationError> {
        self.context.log_info(&format!(
            "Reading @SECURITY section from offset {}",
            offset.offset
        ));

        // Read and validate section ID
        let mut id_buf = [0u8; 4];
        reader.read_exact(&mut id_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecuritySection"))?;
        let section_id = u32::from_le_bytes(id_buf);

        if section_id != SectionId::Security as u32 {
            return Err(BinarySerializationError::invalid_section_id(
                section_id,
                "SecuritySection",
            ));
        }

        // Read section length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecuritySection"))?;
        let section_length = i32::from_le_bytes(len_buf);
        self.context.log_debug(&format!("Section length: {} bytes", section_length));

        // Read entry count
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecuritySection"))?;
        let entry_count = i32::from_le_bytes(count_buf);
        self.context.log_info(&format!("Reading {} security entries", entry_count));

        // Read all entries
        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let entry = self.read_security_entry(reader)?;
            entries.push(entry);
        }

        self.context.log_info(&format!("✅ @SECURITY section read: {} entries", entry_count));

        Ok(SecuritySection::new(entries, Position::UNKNOWN))
    }

    /// Read individual security entry
    /// Format: [Block Key Length: 4][Block Key UTF-8][Field Count: 4][Fields...]
    fn read_security_entry<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<SecurityEntry, BinarySerializationError> {
        // Read block key length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecurityEntry"))?;
        let block_key_length = i32::from_le_bytes(len_buf) as usize;

        // Read block key
        let mut block_key_bytes = vec![0u8; block_key_length];
        reader.read_exact(&mut block_key_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecurityEntry"))?;
        let block_key = String::from_utf8(block_key_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecurityEntry"))?;

        // Read field count
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecurityEntry"))?;
        let field_count = i32::from_le_bytes(count_buf);

        self.context.log_debug(&format!(
            "  Security block: {} ({} fields)",
            block_key, field_count
        ));

        // Read all fields
        let mut fields = Vec::with_capacity(field_count as usize);
        for _ in 0..field_count {
            let field = self.read_security_field(reader)?;
            fields.push(field);
        }

        Ok(SecurityEntry::new(block_key, fields, Position::UNKNOWN))
    }

    /// Read individual security field
    /// Format: [Key Length: 4][Key UTF-8][Value]
    fn read_security_field<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<SecurityField, BinarySerializationError> {
        // Read field key length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecurityField"))?;
        let key_length = i32::from_le_bytes(len_buf) as usize;

        // Read field key
        let mut key_bytes = vec![0u8; key_length];
        reader.read_exact(&mut key_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecurityField"))?;
        let key = String::from_utf8(key_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecurityField"))?;

        // Read field value
        let value = self.value_decoder.decode_value(reader)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SecurityField"))?;

        self.context.log_debug(&format!("    Field: {} = {}", key, value));

        Ok(SecurityField::new(key, value, Position::UNKNOWN))
    }
      }
