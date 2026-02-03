//! Main binary serialization orchestrator - packs DixScript AST into binary format

use std::time::Instant;
use std::io::{Write, Cursor};
use crate::Compiler::AST::DixScript;
use super::{
    binary_format::{HEADER_SIZE, FOOTER_SIZE, SectionId, SectionFlags},
    binary_header::BinaryHeader,
    section_offset::SectionOffset,
    checksum_validator::ChecksumValidator,
    binary_serialization_context::BinarySerializationContext,
    binary_serialization_result::BinarySerializationResult,
    binary_serialization_error::BinarySerializationError,
};

// TODO: Import section writers when implemented
// use super::section_writers::{
//     ConfigSectionWriter,
//     EnumsSectionWriter,
//     DataSectionWriter,
//     SecuritySectionWriter,
//     ImportsSectionWriter,
// };

/// Main binary packer - orchestrates serialization of DixScript AST
pub struct BinaryPacker {
    context: BinarySerializationContext,
}

impl BinaryPacker {
    /// Create new packer
    pub fn new() -> Self {
        BinaryPacker {
            context: BinarySerializationContext::new(),
        }
    }

    /// Pack DixScript AST into binary format
    pub fn pack(&mut self, ast: &DixScript) -> BinarySerializationResult {
        let start_time = Instant::now();
        
        self.context.log_info("Starting binary serialization...");

        // Estimate original size (rough JSON equivalent)
        let original_size = self.estimate_original_size(ast);

        // Pack the AST
        match self.pack_internal(ast) {
            Ok(binary_data) => {
                let duration = start_time.elapsed();
                self.context.log_info(&format!(
                    "Binary serialization completed: {} bytes in {:.2}ms",
                    binary_data.len(),
                    duration.as_secs_f64() * 1000.0
                ));

                BinarySerializationResult::success(
                    binary_data,
                    original_size,
                    duration,
                    self.context.statistics.clone(),
                )
            }
            Err(e) => {
                let duration = start_time.elapsed();
                self.context.log_info(&format!(
                    "Binary serialization failed: {} in {:.2}ms",
                    e,
                    duration.as_secs_f64() * 1000.0
                ));

                BinarySerializationResult::failure(
                    vec![e.to_string()],
                    Vec::new(),
                    duration,
                )
            }
        }
    }

    /// Internal packing implementation
    fn pack_internal(&mut self, ast: &DixScript) -> Result<Vec<u8>, BinarySerializationError> {
        // Create buffer for binary data
        let mut buffer = Cursor::new(Vec::new());

        // Reserve space for header (will write later)
        buffer.write_all(&vec![0u8; HEADER_SIZE])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "Header"))?;

        // Create header
        let mut header = BinaryHeader::new();
        let mut section_offsets = Vec::new();

        // Write sections and track offsets
        self.write_sections(ast, &mut buffer, &mut header, &mut section_offsets)?;

        // Get position for offset table
        let offset_table_position = buffer.position() as i32;
        header.offset_table_position = offset_table_position;

        // Write offset table
        self.write_offset_table(&section_offsets, &mut buffer)?;

        // Get final data without checksum
        let mut binary_data = buffer.into_inner();

        // Write header at the beginning
        let header_bytes = self.write_header_bytes(&header)?;
        binary_data[0..HEADER_SIZE].copy_from_slice(&header_bytes);

        // Append checksum
        binary_data = ChecksumValidator::append_checksum(&binary_data);

        // Update statistics
        self.context.statistics.total_bytes = binary_data.len();
        self.context.statistics.total_sections = section_offsets.len();

        Ok(binary_data)
    }

    /// Write all sections
    fn write_sections(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
        header: &mut BinaryHeader,
        section_offsets: &mut Vec<SectionOffset>,
    ) -> Result<(), BinarySerializationError> {
        // Write Config section if present
        if let Some(_config) = &ast.config {
            let offset = self.write_config_section(ast, buffer)?;
            header.add_section(SectionFlags::CONFIG);
            section_offsets.push(offset);
        }

        // Write Enums section if present
        if !ast.enums.is_empty() {
            let offset = self.write_enums_section(ast, buffer)?;
            header.add_section(SectionFlags::ENUMS);
            section_offsets.push(offset);
        }

        // Write Data section if present
        if !ast.data.is_empty() {
            let offset = self.write_data_section(ast, buffer)?;
            header.add_section(SectionFlags::DATA);
            section_offsets.push(offset);
        }

        // Write Security section if present
        if let Some(_security) = &ast.security {
            let offset = self.write_security_section(ast, buffer)?;
            header.add_section(SectionFlags::SECURITY);
            section_offsets.push(offset);
        }

        // Write Imports section if present
        if !ast.imports.is_empty() {
            let offset = self.write_imports_section(ast, buffer)?;
            header.add_section(SectionFlags::IMPORTS);
            section_offsets.push(offset);
        }

        Ok(())
    }

    /// Write Config section
    fn write_config_section(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<SectionOffset, BinarySerializationError> {
        let start_offset = buffer.position() as i32;

        // TODO: Use ConfigSectionWriter when implemented
        // let writer = ConfigSectionWriter::new(&mut self.context);
        // writer.write(buffer, &ast.config)?;

        // PLACEHOLDER: Write minimal config section
        self.context.log_verbose("Writing Config section (placeholder)");
        buffer.write_all(&[0u8; 4])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ConfigSection"))?;

        let end_offset = buffer.position() as i32;
        let length = end_offset - start_offset;

        self.context.statistics.record_section_size(SectionId::Config, length as usize);

        Ok(SectionOffset::new(SectionId::Config, start_offset, length))
    }

    /// Write Enums section
    fn write_enums_section(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<SectionOffset, BinarySerializationError> {
        let start_offset = buffer.position() as i32;

        // TODO: Use EnumsSectionWriter when implemented
        // let writer = EnumsSectionWriter::new(&mut self.context);
        // writer.write(buffer, &ast.enums)?;

        // PLACEHOLDER: Write minimal enums section
        self.context.log_verbose(&format!("Writing Enums section (placeholder): {} enums", ast.enums.len()));
        let count = ast.enums.len() as i32;
        buffer.write_all(&count.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "EnumsSection"))?;

        let end_offset = buffer.position() as i32;
        let length = end_offset - start_offset;

        self.context.statistics.record_section_size(SectionId::Enums, length as usize);

        Ok(SectionOffset::new(SectionId::Enums, start_offset, length))
    }

    /// Write Data section
    fn write_data_section(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<SectionOffset, BinarySerializationError> {
        let start_offset = buffer.position() as i32;

        // TODO: Use DataSectionWriter when implemented
        // let writer = DataSectionWriter::new(&mut self.context);
        // writer.write(buffer, &ast.data)?;

        // PLACEHOLDER: Write minimal data section
        self.context.log_verbose(&format!("Writing Data section (placeholder): {} items", ast.data.len()));
        let count = ast.data.len() as i32;
        buffer.write_all(&count.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;

        let end_offset = buffer.position() as i32;
        let length = end_offset - start_offset;

        self.context.statistics.record_section_size(SectionId::Data, length as usize);

        Ok(SectionOffset::new(SectionId::Data, start_offset, length))
    }

    /// Write Security section
    fn write_security_section(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<SectionOffset, BinarySerializationError> {
        let start_offset = buffer.position() as i32;

        // TODO: Use SecuritySectionWriter when implemented
        // let writer = SecuritySectionWriter::new(&mut self.context);
        // writer.write(buffer, &ast.security)?;

        // PLACEHOLDER: Write minimal security section
        self.context.log_verbose("Writing Security section (placeholder)");
        buffer.write_all(&[0u8; 4])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SecuritySection"))?;

        let end_offset = buffer.position() as i32;
        let length = end_offset - start_offset;

        self.context.statistics.record_section_size(SectionId::Security, length as usize);

        Ok(SectionOffset::new(SectionId::Security, start_offset, length))
    }

    /// Write Imports section
    fn write_imports_section(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<SectionOffset, BinarySerializationError> {
        let start_offset = buffer.position() as i32;

        // TODO: Use ImportsSectionWriter when implemented
        // let writer = ImportsSectionWriter::new(&mut self.context);
        // writer.write(buffer, &ast.imports)?;

        // PLACEHOLDER: Write minimal imports section
        self.context.log_verbose(&format!("Writing Imports section (placeholder): {} imports", ast.imports.len()));
        let count = ast.imports.len() as i32;
        buffer.write_all(&count.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?;

        let end_offset = buffer.position() as i32;
        let length = end_offset - start_offset;

        self.context.statistics.record_section_size(SectionId::Imports, length as usize);

        Ok(SectionOffset::new(SectionId::Imports, start_offset, length))
    }

    /// Write offset table
    fn write_offset_table(
        &self,
        section_offsets: &[SectionOffset],
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<(), BinarySerializationError> {
        for offset in section_offsets {
            offset.write_to(buffer)
                .map_err(|e| BinarySerializationError::write_error(e.to_string(), "OffsetTable"))?;
        }
        Ok(())
    }

    /// Write header to bytes
    fn write_header_bytes(&self, header: &BinaryHeader) -> Result<Vec<u8>, BinarySerializationError> {
        let mut buffer = Cursor::new(Vec::new());
        header.write_to(&mut buffer)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "Header"))?;
        Ok(buffer.into_inner())
    }

    /// Estimate original size (rough JSON equivalent)
    fn estimate_original_size(&self, ast: &DixScript) -> usize {
        // Rough estimate: 
        // - Config: ~200 bytes
        // - Each enum: ~100 bytes
        // - Each data item: ~150 bytes
        // - Security: ~100 bytes
        // - Each import: ~80 bytes
        
        let mut size = 100; // Base overhead

        if ast.config.is_some() {
            size += 200;
        }

        size += ast.enums.len() * 100;
        size += ast.data.len() * 150;

        if ast.security.is_some() {
            size += 100;
        }

        size += ast.imports.len() * 80;

        size
    }
}

impl Default for BinaryPacker {
    fn default() -> Self {
        Self::new()
    }
                     }
