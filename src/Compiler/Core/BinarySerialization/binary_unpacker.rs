//! Main binary deserialization orchestrator - unpacks binary format into DixScript AST

use std::time::Instant;
use std::io::Cursor;
use crate::Compiler::AST::{DixScript, Config, Security};
use super::{
    binary_format::{HEADER_SIZE, FOOTER_SIZE, SectionId, SectionFlags},
    binary_header::BinaryHeader,
    section_offset::SectionOffset,
    checksum_validator::ChecksumValidator,
    binary_serialization_context::BinarySerializationContext,
    binary_serialization_result::BinaryDeserializationResult,
    binary_serialization_error::BinarySerializationError,
};

// TODO: Import section readers when implemented
// use super::section_readers::{
//     ConfigSectionReader,
//     EnumsSectionReader,
//     DataSectionReader,
//     SecuritySectionReader,
//     ImportsSectionReader,
// };

/// Main binary unpacker - orchestrates deserialization into DixScript AST
pub struct BinaryUnpacker {
    context: BinarySerializationContext,
}

impl BinaryUnpacker {
    /// Create new unpacker
    pub fn new() -> Self {
        BinaryUnpacker {
            context: BinarySerializationContext::new(),
        }
    }

    /// Unpack binary data into DixScript AST
    pub fn unpack(&mut self, binary_data: &[u8]) -> BinaryDeserializationResult {
        let start_time = Instant::now();
        
        self.context.log_info("Starting binary deserialization...");

        let binary_size = binary_data.len();

        // Unpack the binary data
        match self.unpack_internal(binary_data) {
            Ok(ast) => {
                let duration = start_time.elapsed();
                self.context.log_info(&format!(
                    "Binary deserialization completed: {} bytes in {:.2}ms",
                    binary_size,
                    duration.as_secs_f64() * 1000.0
                ));

                BinaryDeserializationResult::success(
                    ast,
                    binary_size,
                    duration,
                    self.context.statistics.clone().into(), // Convert statistics
                )
            }
            Err(e) => {
                let duration = start_time.elapsed();
                self.context.log_info(&format!(
                    "Binary deserialization failed: {} in {:.2}ms",
                    e,
                    duration.as_secs_f64() * 1000.0
                ));

                BinaryDeserializationResult::failure(
                    vec![e.to_string()],
                    Vec::new(),
                    duration,
                )
            }
        }
    }

    /// Internal unpacking implementation
    fn unpack_internal(&mut self, binary_data: &[u8]) -> Result<DixScript, BinarySerializationError> {
        // Validate minimum size
        if binary_data.len() < HEADER_SIZE + FOOTER_SIZE {
            return Err(BinarySerializationError::corrupted_data(
                format!("File too small: {} bytes (minimum {} required)", 
                    binary_data.len(), 
                    HEADER_SIZE + FOOTER_SIZE
                )
            ));
        }

        // Validate checksum
        let data_without_checksum = ChecksumValidator::validate_and_extract(binary_data)
            .map_err(|e| BinarySerializationError::corrupted_data(e))?;

        // Read header
        let mut cursor = Cursor::new(data_without_checksum);
        let header = BinaryHeader::read_from(&mut cursor)
            .map_err(|e| BinarySerializationError::corrupted_header(e))?;

        self.context.log_verbose(&format!("Header: {}", header));

        // Read offset table
        cursor.set_position(header.offset_table_position as u64);
        let section_offsets = self.read_offset_table(&mut cursor, header.section_count)?;

        self.context.log_verbose(&format!("Found {} sections", section_offsets.len()));

        // Read sections and build AST
        let ast = self.read_sections(data_without_checksum, &header, &section_offsets)?;

        // Update statistics
        self.context.statistics.total_bytes = binary_data.len();
        self.context.statistics.total_sections = section_offsets.len();

        Ok(ast)
    }

    /// Read offset table
    fn read_offset_table(
        &self,
        cursor: &mut Cursor<&[u8]>,
        section_count: i32,
    ) -> Result<Vec<SectionOffset>, BinarySerializationError> {
        let mut offsets = Vec::new();

        for i in 0..section_count {
            let offset = SectionOffset::read_from(cursor)
                .map_err(|e| BinarySerializationError::read_error(
                    format!("Failed to read offset {}: {}", i, e),
                    "OffsetTable"
                ))?;
            
            self.context.log_verbose(&format!("  Section {}: {}", i, offset));
            offsets.push(offset);
        }

        Ok(offsets)
    }

    /// Read all sections and construct AST
    fn read_sections(
        &mut self,
        data: &[u8],
        header: &BinaryHeader,
        section_offsets: &[SectionOffset],
    ) -> Result<DixScript, BinarySerializationError> {
        let mut config = None;
        let mut enums = Vec::new();
        let mut data_items = Vec::new();
        let mut security = None;
        let mut imports = Vec::new();

        // Read each section
        for offset in section_offsets {
            match offset.section_id {
                SectionId::Config => {
                    if header.has_section(SectionFlags::CONFIG) {
                        config = Some(self.read_config_section(data, offset)?);
                    }
                }
                SectionId::Enums => {
                    if header.has_section(SectionFlags::ENUMS) {
                        enums = self.read_enums_section(data, offset)?;
                    }
                }
                SectionId::Data => {
                    if header.has_section(SectionFlags::DATA) {
                        data_items = self.read_data_section(data, offset)?;
                    }
                }
                SectionId::Security => {
                    if header.has_section(SectionFlags::SECURITY) {
                        security = Some(self.read_security_section(data, offset)?);
                    }
                }
                SectionId::Imports => {
                    if header.has_section(SectionFlags::IMPORTS) {
                        imports = self.read_imports_section(data, offset)?;
                    }
                }
            }
        }

        // Construct DixScript AST
        Ok(DixScript {
            config,
            enums,
            data: data_items,
            security,
            imports,
        })
    }

    /// Read Config section
    fn read_config_section(
        &mut self,
        data: &[u8],
        offset: &SectionOffset,
    ) -> Result<Config, BinarySerializationError> {
        self.context.log_verbose("Reading Config section (placeholder)");

        // Get section data
        let section_data = self.get_section_data(data, offset)?;

        // TODO: Use ConfigSectionReader when implemented
        // let reader = ConfigSectionReader::new(&mut self.context);
        // reader.read(&section_data)

        // PLACEHOLDER: Return default config
        Ok(Config::default())
    }

    /// Read Enums section
    fn read_enums_section(
        &mut self,
        data: &[u8],
        offset: &SectionOffset,
    ) -> Result<Vec<crate::Compiler::AST::EnumDeclaration>, BinarySerializationError> {
        self.context.log_verbose("Reading Enums section (placeholder)");

        // Get section data
        let _section_data = self.get_section_data(data, offset)?;

        // TODO: Use EnumsSectionReader when implemented
        // let reader = EnumsSectionReader::new(&mut self.context);
        // reader.read(&section_data)

        // PLACEHOLDER: Return empty vec
        Ok(Vec::new())
    }

    /// Read Data section
    fn read_data_section(
        &mut self,
        data: &[u8],
        offset: &SectionOffset,
    ) -> Result<Vec<crate::Compiler::AST::DataItem>, BinarySerializationError> {
        self.context.log_verbose("Reading Data section (placeholder)");

        // Get section data
        let _section_data = self.get_section_data(data, offset)?;

        // TODO: Use DataSectionReader when implemented
        // let reader = DataSectionReader::new(&mut self.context);
        // reader.read(&section_data)

        // PLACEHOLDER: Return empty vec
        Ok(Vec::new())
    }

    /// Read Security section
    fn read_security_section(
        &mut self,
        data: &[u8],
        offset: &SectionOffset,
    ) -> Result<Security, BinarySerializationError> {
        self.context.log_verbose("Reading Security section (placeholder)");

        // Get section data
        let _section_data = self.get_section_data(data, offset)?;

        // TODO: Use SecuritySectionReader when implemented
        // let reader = SecuritySectionReader::new(&mut self.context);
        // reader.read(&section_data)

        // PLACEHOLDER: Return default security
        Ok(Security::default())
    }

    /// Read Imports section
    fn read_imports_section(
        &mut self,
        data: &[u8],
        offset: &SectionOffset,
    ) -> Result<Vec<crate::Compiler::AST::Import>, BinarySerializationError> {
        self.context.log_verbose("Reading Imports section (placeholder)");

        // Get section data
        let _section_data = self.get_section_data(data, offset)?;

        // TODO: Use ImportsSectionReader when implemented
        // let reader = ImportsSectionReader::new(&mut self.context);
        // reader.read(&section_data)

        // PLACEHOLDER: Return empty vec
        Ok(Vec::new())
    }

    /// Extract section data from binary
    fn get_section_data(
        &self,
        data: &[u8],
        offset: &SectionOffset,
    ) -> Result<&[u8], BinarySerializationError> {
        let start = offset.offset as usize;
        let end = start + offset.length as usize;

        if end > data.len() {
            return Err(BinarySerializationError::corrupted_data(
                format!("Section extends beyond file: offset={}, length={}, file_size={}",
                    offset.offset, offset.length, data.len()
                )
            ));
        }

        Ok(&data[start..end])
    }
}

impl Default for BinaryUnpacker {
    fn default() -> Self {
        Self::new()
    }
}

// Helper to convert statistics types
impl From<crate::Compiler::Core::BinarySerialization::binary_serialization_context::BinarySerializationStatistics> 
    for crate::Compiler::Core::BinarySerialization::binary_serialization_context::BinaryDeserializationStatistics 
{
    fn from(stats: crate::Compiler::Core::BinarySerialization::binary_serialization_context::BinarySerializationStatistics) -> Self {
        let mut deser_stats = crate::Compiler::Core::BinarySerialization::binary_serialization_context::BinaryDeserializationStatistics::new();
        deser_stats.total_sections = stats.total_sections;
        deser_stats.total_values = stats.total_values;
        deser_stats.total_bytes = stats.total_bytes;
        deser_stats.value_counts = stats.value_counts;
        deser_stats
    }
          }
