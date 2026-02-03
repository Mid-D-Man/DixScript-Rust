//! Main orchestrator for binary serialization

use std::io::Write;
use std::time::Instant;
use crate::Compiler::AST::DixScript;
use super::{
    binary_format::{MAGIC_NUMBER, VERSION_MAJOR, VERSION_MINOR, VERSION_PATCH, SectionId, SectionFlags},
    binary_header::BinaryHeader,
    section_offset::SectionOffset,
    checksum_validator::ChecksumValidator,
    binary_serialization_context::BinarySerializationContext,
    binary_serialization_result::BinarySerializationResult,
    binary_serialization_error::BinarySerializationError,
};

/// Main binary serializer for DixScript AST
pub struct BinaryPacker {
    context: BinarySerializationContext,
}

impl BinaryPacker {
    /// Create new binary packer
    pub fn new() -> Self {
        BinaryPacker {
            context: BinarySerializationContext::new(),
        }
    }

    /// Pack AST into binary format
    pub fn pack(&mut self, ast: &DixScript) -> BinarySerializationResult {
        let start_time = Instant::now();
        
        self.context.log_info("Starting binary serialization");

        match self.pack_internal(ast) {
            Ok(binary_data) => {
                let duration = start_time.elapsed();
                self.context.log_info(&format!(
                    "Binary serialization completed in {:.2}ms",
                    duration.as_secs_f64() * 1000.0
                ));

                BinarySerializationResult::success(
                    binary_data,
                    0, // TODO: Calculate original size from AST
                    duration,
                    self.context.statistics.clone(),
                )
            }
            Err(err) => {
                let duration = start_time.elapsed();
                self.context.log_info(&format!(
                    "Binary serialization failed: {}",
                    err
                ));

                BinarySerializationResult::failure(
                    vec![err.to_string()],
                    Vec::new(),
                    duration,
                )
            }
        }
    }

    /// Internal packing implementation
    fn pack_internal(&mut self, ast: &DixScript) -> Result<Vec<u8>, BinarySerializationError> {
        // Step 1: Determine which sections are present
        let sections = self.determine_sections(ast);
        self.context.statistics.total_sections = sections.len();

        // Step 2: Create header
        let mut header = BinaryHeader::new();
        for section_id in &sections {
            header.add_section(*section_id);
        }

        // Step 3: Serialize each section to temporary buffers
        let section_data = self.serialize_sections(ast, &sections)?;

        // Step 4: Calculate section offsets
        let offsets = self.calculate_offsets(&header, &section_data)?;

        // Step 5: Set offset table position in header
        let offset_table_position = self.calculate_offset_table_position(&section_data);
        header.offset_table_position = offset_table_position as i32;

        // Step 6: Assemble final binary
        let mut binary_data = Vec::new();
        
        // Write header
        header.write_to(&mut binary_data)
            .map_err(|e| BinarySerializationError::write_error(e, "Header"))?;

        // Write section data
        for data in section_data {
            binary_data.write_all(&data)
                .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SectionData"))?;
        }

        // Write offset table
        for offset in offsets {
            offset.write_to(&mut binary_data)
                .map_err(|e| BinarySerializationError::write_error(e, "OffsetTable"))?;
        }

        // Step 7: Calculate and append checksum
        ChecksumValidator::append_checksum(&mut binary_data)
            .map_err(|e| BinarySerializationError::write_error(e, "Checksum"))?;

        // Update statistics
        self.context.statistics.total_bytes = binary_data.len();

        Ok(binary_data)
    }

    /// Determine which sections are present in the AST
    fn determine_sections(&self, ast: &DixScript) -> Vec<SectionId> {
        let mut sections = Vec::new();

        // Config section (always present if there's config data)
        if ast.config.is_some() {
            sections.push(SectionId::Config);
        }

        // Enums section
        if !ast.enums.is_empty() {
            sections.push(SectionId::Enums);
        }

        // Data section (always present - contains main script body)
        sections.push(SectionId::Data);

        // Security section
        if ast.security.is_some() {
            sections.push(SectionId::Security);
        }

        // Imports section
        if !ast.imports.is_empty() {
            sections.push(SectionId::Imports);
        }

        sections
    }

    /// Serialize each section to temporary buffers
    fn serialize_sections(
        &mut self,
        ast: &DixScript,
        sections: &[SectionId],
    ) -> Result<Vec<Vec<u8>>, BinarySerializationError> {
        let mut section_data = Vec::new();

        for section_id in sections {
            let data = match section_id {
                SectionId::Config => self.serialize_config_section(ast)?,
                SectionId::Enums => self.serialize_enums_section(ast)?,
                SectionId::Data => self.serialize_data_section(ast)?,
                SectionId::Security => self.serialize_security_section(ast)?,
                SectionId::Imports => self.serialize_imports_section(ast)?,
            };

            self.context.statistics.record_section_size(*section_id, data.len());
            section_data.push(data);
        }

        Ok(section_data)
    }

    /// Calculate section offsets
    fn calculate_offsets(
        &self,
        header: &BinaryHeader,
        section_data: &[Vec<u8>],
    ) -> Result<Vec<SectionOffset>, BinarySerializationError> {
        let mut offsets = Vec::new();
        let mut current_offset = 16; // Header is 16 bytes

        // Get section IDs from header flags
        let section_ids = self.get_section_ids_from_header(header);

        for (i, section_id) in section_ids.iter().enumerate() {
            let length = section_data[i].len();
            
            let offset = SectionOffset::new(
                *section_id,
                current_offset as i32,
                length as i32,
            );

            offsets.push(offset);
            current_offset += length;
        }

        Ok(offsets)
    }

    /// Get section IDs from header flags in order
    fn get_section_ids_from_header(&self, header: &BinaryHeader) -> Vec<SectionId> {
        let mut section_ids = Vec::new();

        if header.has_section(SectionId::Config) {
            section_ids.push(SectionId::Config);
        }
        if header.has_section(SectionId::Enums) {
            section_ids.push(SectionId::Enums);
        }
        if header.has_section(SectionId::Data) {
            section_ids.push(SectionId::Data);
        }
        if header.has_section(SectionId::Security) {
            section_ids.push(SectionId::Security);
        }
        if header.has_section(SectionId::Imports) {
            section_ids.push(SectionId::Imports);
        }

        section_ids
    }

    /// Calculate offset table position
    fn calculate_offset_table_position(&self, section_data: &[Vec<u8>]) -> usize {
        let header_size = 16;
        let total_section_size: usize = section_data.iter().map(|d| d.len()).sum();
        header_size + total_section_size
    }

    // ==================== SECTION SERIALIZERS (STUBS) ====================
    // These will be implemented with the section writers in the next chat

    fn serialize_config_section(&mut self, ast: &DixScript) -> Result<Vec<u8>, BinarySerializationError> {
        // TODO: Implement with ConfigSectionWriter
        self.context.log_verbose("Serializing config section");
        
        let mut buffer = Vec::new();
        
        // Placeholder: Write empty config for now
        if let Some(_config) = &ast.config {
            // Will use ConfigSectionWriter::write()
        }
        
        Ok(buffer)
    }

    fn serialize_enums_section(&mut self, ast: &DixScript) -> Result<Vec<u8>, BinarySerializationError> {
        // TODO: Implement with EnumsSectionWriter
        self.context.log_verbose("Serializing enums section");
        
        let mut buffer = Vec::new();
        
        // Placeholder: Write enum count
        let enum_count = ast.enums.len() as i32;
        buffer.extend_from_slice(&enum_count.to_le_bytes());
        
        // Will use EnumsSectionWriter::write()
        
        Ok(buffer)
    }

    fn serialize_data_section(&mut self, ast: &DixScript) -> Result<Vec<u8>, BinarySerializationError> {
        // TODO: Implement with DataSectionWriter
        self.context.log_verbose("Serializing data section");
        
        let mut buffer = Vec::new();
        
        // Placeholder: Write statement count
        let statement_count = ast.statements.len() as i32;
        buffer.extend_from_slice(&statement_count.to_le_bytes());
        
        // Will use DataSectionWriter::write()
        
        Ok(buffer)
    }

    fn serialize_security_section(&mut self, ast: &DixScript) -> Result<Vec<u8>, BinarySerializationError> {
        // TODO: Implement with SecuritySectionWriter
        self.context.log_verbose("Serializing security section");
        
        let mut buffer = Vec::new();
        
        // Placeholder: Write empty security for now
        if let Some(_security) = &ast.security {
            // Will use SecuritySectionWriter::write()
        }
        
        Ok(buffer)
    }

    fn serialize_imports_section(&mut self, ast: &DixScript) -> Result<Vec<u8>, BinarySerializationError> {
        // TODO: Implement with ImportsSectionWriter
        self.context.log_verbose("Serializing imports section");
        
        let mut buffer = Vec::new();
        
        // Placeholder: Write import count
        let import_count = ast.imports.len() as i32;
        buffer.extend_from_slice(&import_count.to_le_bytes());
        
        // Will use ImportsSectionWriter::write()
        
        Ok(buffer)
    }
}

impl Default for BinaryPacker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_packer() {
        let packer = BinaryPacker::new();
        assert_eq!(packer.context.nesting_depth(), 0);
    }

    #[test]
    fn test_determine_sections_minimal() {
        let packer = BinaryPacker::new();
        let ast = DixScript::default();
        
        let sections = packer.determine_sections(&ast);
        
        // At minimum, should have Data section
        assert!(sections.contains(&SectionId::Data));
    }

    #[test]
    fn test_calculate_offset_table_position() {
        let packer = BinaryPacker::new();
        
        let section_data = vec![
            vec![0u8; 100],  // 100 bytes
            vec![0u8; 200],  // 200 bytes
        ];
        
        let position = packer.calculate_offset_table_position(&section_data);
        
        // Header (16) + sections (300) = 316
        assert_eq!(position, 316);
    }
  }
