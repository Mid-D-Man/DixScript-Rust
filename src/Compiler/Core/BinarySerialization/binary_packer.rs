//! Main binary serialization orchestrator - packs DixScript AST into binary format

use std::time::Instant;
use std::io::{Write, Cursor};
use crate::Compiler::AST::DixScript;
use super::{
    binary_format::{HEADER_SIZE, SectionFlags},
    binary_header::BinaryHeader,
    section_offset::SectionOffset,
    checksum_validator::ChecksumValidator,
    binary_serialization_context::BinarySerializationContext,
    binary_serialization_result::BinarySerializationResult,
    binary_serialization_error::BinarySerializationError,
    ConfigSectionWriter,
    EnumsSectionWriter,
    DataSectionWriter,
    SecuritySectionWriter,
    ImportsSectionWriter,
    ValueEncoder,
};

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
        header.section_count = section_offsets.len() as i32;

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
        if ast.config.is_some() {
            let offset = self.write_config_section(ast, buffer)?;
            header.add_section(SectionFlags::CONFIG);
            section_offsets.push(offset);
        }

        // Write Imports section if present
        if ast.imports.is_some() {
            let offset = self.write_imports_section(ast, buffer)?;
            header.add_section(SectionFlags::IMPORTS);
            section_offsets.push(offset);
        }

        // Write Enums section if present
        if ast.enums.is_some() {
            let offset = self.write_enums_section(ast, buffer)?;
            header.add_section(SectionFlags::ENUMS);
            section_offsets.push(offset);
        }

        // Write Data section if present
        if ast.data.is_some() {
            let offset = self.write_data_section(ast, buffer)?;
            header.add_section(SectionFlags::DATA);
            section_offsets.push(offset);
        }

        // Write Security section if present
        if ast.security.is_some() {
            let offset = self.write_security_section(ast, buffer)?;
            header.add_section(SectionFlags::SECURITY);
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
        if let Some(ref config_section) = ast.config {
            // Create encoder and writer - encoder no longer holds context
            let mut value_encoder = ValueEncoder::new();
            let mut writer = ConfigSectionWriter::new(&mut self.context, &mut value_encoder);
            writer.write_section(buffer, config_section)
        } else {
            Err(BinarySerializationError::invalid_state(
                "Config section is None",
                "ConfigSection"
            ))
        }
    }

    /// Write Enums section
    fn write_enums_section(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<SectionOffset, BinarySerializationError> {
        if let Some(ref enums_section) = ast.enums {
            let mut writer = EnumsSectionWriter::new(&mut self.context);
            writer.write_section(buffer, enums_section)
        } else {
            Err(BinarySerializationError::invalid_state(
                "Enums section is None",
                "EnumsSection"
            ))
        }
    }

    /// Write Data section
    fn write_data_section(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<SectionOffset, BinarySerializationError> {
        if let Some(ref data_section) = ast.data {
            // Create encoder and writer - encoder no longer holds context
            let mut value_encoder = ValueEncoder::new();
            let mut writer = DataSectionWriter::new(&mut self.context, &mut value_encoder);
            writer.write_section(buffer, data_section)
        } else {
            Err(BinarySerializationError::invalid_state(
                "Data section is None",
                "DataSection"
            ))
        }
    }

    /// Write Security section
    fn write_security_section(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<SectionOffset, BinarySerializationError> {
        if let Some(ref security_section) = ast.security {
            // Create encoder and writer - encoder no longer holds context
            let mut value_encoder = ValueEncoder::new();
            let mut writer = SecuritySectionWriter::new(&mut self.context, &mut value_encoder);
            writer.write_section(buffer, security_section)
        } else {
            Err(BinarySerializationError::invalid_state(
                "Security section is None",
                "SecuritySection"
            ))
        }
    }

    /// Write Imports section
    fn write_imports_section(
        &mut self,
        ast: &DixScript,
        buffer: &mut Cursor<Vec<u8>>,
    ) -> Result<SectionOffset, BinarySerializationError> {
        if let Some(ref imports_section) = ast.imports {
            let mut writer = ImportsSectionWriter::new(&mut self.context);
            writer.write_section(buffer, imports_section)
        } else {
            Err(BinarySerializationError::invalid_state(
                "Imports section is None",
                "ImportsSection"
            ))
        }
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
        let mut size = 100; // Base overhead

        if ast.config.is_some() {
            size += 200;
        }

        if let Some(ref enums) = ast.enums {
            size += enums.enums.len() * 100;
        }

        if let Some(ref data) = ast.data {
            size += data.entries.len() * 150;
        }

        if ast.security.is_some() {
            size += 100;
        }

        if let Some(ref imports) = ast.imports {
            size += imports.imports.len() * 80;
        }

        size
    }
}

impl Default for BinaryPacker {
    fn default() -> Self {
        Self::new()
    }
}