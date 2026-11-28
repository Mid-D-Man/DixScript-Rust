// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/BinarySerialization/SectionReaders/security_section_reader.rs"
// ============================================================================
//! Reads @SECURITY section from binary format.

use std::io::Read;
use crate::Compiler::AST::{SecuritySection, SecurityEntry, SecurityField, Position};
use crate::Compiler::AST::arena::{Arena, DocSecurity, DocSecurityEntry, DocSecurityField};
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_decoder::ValueDecoder;

/// Reads @SECURITY section from binary format.
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
/// Each entry: [Block Key Length: 4][Block Key UTF-8][Field Count: 4][Fields...]
/// Each field:  [Key Length: 4][Key UTF-8][Value]
pub struct SecuritySectionReader<'a> {
    context: &'a mut BinarySerializationContext,
    value_decoder: &'a mut ValueDecoder,
}

impl<'a> SecuritySectionReader<'a> {
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_decoder: &'a mut ValueDecoder,
    ) -> Self {
        SecuritySectionReader { context, value_decoder }
    }

    pub fn read_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
    ) -> Result<SecuritySection, BinarySerializationError> {
        let entries = self.read_entries(reader, offset, |this, reader| this.read_security_entry(reader))?;
        Ok(SecuritySection::new(entries, Position::UNKNOWN))
    }

    /// Reads the `@SECURITY` section into `arena` and returns it as a [`DocSecurity`].
    ///
    /// Reads the same bytes and fails with the same errors as [`read_section`](Self::read_section);
    /// exporting the result gives the section `read_section` returns. The nodes are appended
    /// to `arena`.
    pub fn read_document_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
        arena: &mut Arena,
    ) -> Result<DocSecurity, BinarySerializationError> {
        let entries = self.read_entries(reader, offset, |this, reader| this.read_doc_security_entry(reader, arena))?;
        Ok(DocSecurity { entries, position: Position::UNKNOWN })
    }

    /// The frame both entry points share: the section id, the length, the entry count, then
    /// `entry_count` entries read by `read_entry`.
    fn read_entries<R: Read, T>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
        mut read_entry: impl FnMut(&mut Self, &mut R) -> Result<T, BinarySerializationError>,
    ) -> Result<Vec<T>, BinarySerializationError> {
        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!(
                "Reading @SECURITY section from offset {}",
                offset.offset
            ));
        }

        let mut id_buf = [0u8; 4];
        reader
            .read_exact(&mut id_buf)
            .map_err(|e| self.read_err(e.to_string(), "SecuritySection"))?;
        let section_id = u32::from_le_bytes(id_buf);
        if section_id != SectionId::Security as u32 {
            return Err(BinarySerializationError::invalid_section_id(section_id, "SecuritySection"));
        }

        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .map_err(|e| self.read_err(e.to_string(), "SecuritySection"))?;
        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  section length: {} bytes",
                i32::from_le_bytes(len_buf)
            ));
        }

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "SecuritySection"))?;
        let entry_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_enabled {
            self.context
                .log_info(&format!("  reading {} security entries", entry_count));
        }

        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let entry = read_entry(self, reader)?;
            entries.push(entry);
            if self.context.error_manager.should_terminate_parsing() {
                return Err(BinarySerializationError::invalid_state(
                    "Terminating SECURITY read due to accumulated errors",
                    "SecuritySection",
                ));
            }
        }

        if self.context.debug_config.is_enabled {
            self.context
                .log_info(&format!("@SECURITY read: {} entries", entry_count));
        }

        Ok(entries)
    }

    fn read_doc_security_entry<R: Read>(
        &mut self,
        reader: &mut R,
        arena: &mut Arena,
    ) -> Result<DocSecurityEntry, BinarySerializationError> {
        let block_key = self.read_string(reader, "SecurityEntry")?;

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "SecurityEntry"))?;
        let field_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  security block: {} ({} fields)",
                block_key, field_count
            ));
        }

        let mut fields = Vec::with_capacity(field_count as usize);
        for _ in 0..field_count {
            let key = self.read_string(reader, "SecurityField")?;
            let value = self
                .value_decoder
                .decode_node(reader, arena, self.context)
                .map_err(|e| self.read_err(e.to_string(), "SecurityField"))?;

            if self.context.debug_config.is_verbose {
                self.context
                    .log_verbose(&format!("    field: {} = {}", key, arena.value_ref(value)));
            }

            fields.push(DocSecurityField { key, value, position: Position::UNKNOWN });
        }

        Ok(DocSecurityEntry { block_key, fields, position: Position::UNKNOWN })
    }

    fn read_security_entry<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<SecurityEntry, BinarySerializationError> {
        let block_key = self.read_string(reader, "SecurityEntry")?;

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "SecurityEntry"))?;
        let field_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  security block: {} ({} fields)",
                block_key, field_count
            ));
        }

        let mut fields = Vec::with_capacity(field_count as usize);
        for _ in 0..field_count {
            fields.push(self.read_security_field(reader)?);
        }

        Ok(SecurityEntry::new(block_key, fields, Position::UNKNOWN))
    }

    fn read_security_field<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<SecurityField, BinarySerializationError> {
        let key = self.read_string(reader, "SecurityField")?;
        let value = self
            .value_decoder
            .decode_value(reader, self.context)
            .map_err(|e| self.read_err(e.to_string(), "SecurityField"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("    field: {} = {}", key, value));
        }

        Ok(SecurityField::new(key, value, Position::UNKNOWN))
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
