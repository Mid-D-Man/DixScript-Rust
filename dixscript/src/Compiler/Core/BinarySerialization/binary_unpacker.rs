//! Main binary deserialization orchestrator — unpacks binary format into DixScript AST.

use web_time::Instant;
use std::io::Cursor;
use crate::Compiler::AST::DixScript;
use super::{
    binary_format::{HEADER_SIZE, FOOTER_SIZE, SectionFlags, SectionId},
    binary_header::BinaryHeader,
    section_offset::SectionOffset,
    checksum_validator::ChecksumValidator,
    binary_serialization_context::{BinarySerializationContext, BinaryDeserializationStatistics},
    binary_serialization_result::BinaryDeserializationResult,
    binary_serialization_error::BinarySerializationError,
    SectionReaders::{ConfigSectionReader, EnumsSectionReader, DataSectionReader, SecuritySectionReader},
    ValueDecoder,
};
use crate::Compiler::AST::{ConfigSection, EnumsSection, DataSection, SecuritySection};

#[cfg(not(target_arch = "wasm32"))]
const CONCURRENT_DESERIALIZATION_ENABLED: bool = true;
#[cfg(target_arch = "wasm32")]
const CONCURRENT_DESERIALIZATION_ENABLED: bool = false;

enum DecodedSection {
    Config(ConfigSection),
    Enums(EnumsSection),
    Data(DataSection),
    Security(SecuritySection),
}

type SectionDecodeResult = Result<Option<DecodedSection>, BinarySerializationError>;

pub struct BinaryUnpacker {
    context: BinarySerializationContext,
}

impl BinaryUnpacker {
    pub fn new() -> Self {
        BinaryUnpacker {
            context: BinarySerializationContext::new(),
        }
    }

    pub fn unpack(&mut self, binary_data: &[u8]) -> BinaryDeserializationResult {
        let start       = Instant::now();
        let binary_size = binary_data.len();

        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!(
                "Starting deserialization: {} bytes", binary_size
            ));
        }

        match self.unpack_internal(binary_data) {
            Ok(ast) => {
                let duration = start.elapsed();
                if self.context.debug_config.is_enabled {
                    self.context.log_info(&format!(
                        "Deserialization complete: {} bytes in {:.2}ms",
                        binary_size,
                        duration.as_secs_f64() * 1000.0
                    ));
                }

                let mut deser_stats = BinaryDeserializationStatistics::new();
                deser_stats.total_sections = self.context.statistics.total_sections;
                deser_stats.total_values   = self.context.statistics.total_values;
                deser_stats.total_bytes    = binary_size;
                deser_stats.value_counts   = self.context.statistics.value_counts.clone();

                BinaryDeserializationResult::success(ast, binary_size, duration, deser_stats)
            }
            Err(e) => {
                let duration = start.elapsed();
                self.context.error_manager.log_error(&format!(
                    "[BinaryUnpacker] Deserialization failed: {}", e
                ));
                BinaryDeserializationResult::failure(
                    vec![e.to_string()], Vec::new(), duration
                )
            }
        }
    }

    fn unpack_internal(
        &mut self,
        binary_data: &[u8],
    ) -> Result<DixScript, BinarySerializationError> {
        if binary_data.len() < HEADER_SIZE + FOOTER_SIZE {
            let e = BinarySerializationError::corrupted_data(format!(
                "File too small: {} bytes (minimum {} required)",
                binary_data.len(),
                HEADER_SIZE + FOOTER_SIZE
            ));
            self.context.add_error(e.error_type, e.message.clone());
            return Err(e);
        }

        let data = ChecksumValidator::validate_and_extract(binary_data)
            .map_err(|msg| {
                let e = BinarySerializationError::checksum_mismatch();
                self.context.add_error(e.error_type, msg.clone());
                BinarySerializationError::corrupted_data(msg)
            })?;

        let mut cursor = Cursor::new(data.as_slice());
        let header     = BinaryHeader::read_from(&mut cursor).map_err(|e| {
            let err = BinarySerializationError::corrupted_header(e.to_string());
            self.context.add_error(err.error_type, err.message.clone());
            err
        })?;

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!("Header: {}", header));
        }

        cursor.set_position(header.offset_table_position as u64);
        let section_offsets = self.read_offset_table(&mut cursor, header.section_count)?;

        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!("{} sections found", section_offsets.len()));
        }

        let ast = self.decode_sections(&data, &header, &section_offsets)?;

        self.context.statistics.total_bytes    = binary_data.len();
        self.context.statistics.total_sections = section_offsets.len();

        Ok(ast)
    }

    fn read_offset_table(
        &mut self,
        cursor: &mut Cursor<&[u8]>,
        section_count: i32,
    ) -> Result<Vec<SectionOffset>, BinarySerializationError> {
        let mut offsets = Vec::with_capacity(section_count as usize);
        for i in 0..section_count {
            let offset = SectionOffset::read_from(cursor).map_err(|e| {
                let err = BinarySerializationError::read_error(
                    format!("Failed to read offset entry {}: {}", i, e),
                    "OffsetTable",
                );
                self.context.add_error(err.error_type, err.message.clone());
                err
            })?;
            if self.context.debug_config.is_verbose {
                self.context.log_verbose(&format!("  Offset entry {}: {}", i, offset));
            }
            offsets.push(offset);
        }
        Ok(offsets)
    }

    fn decode_sections(
        &mut self,
        data: &[u8],
        header: &BinaryHeader,
        section_offsets: &[SectionOffset],
    ) -> Result<DixScript, BinarySerializationError> {
        let decodable: Vec<&SectionOffset> = section_offsets
            .iter()
            .filter(|o| self.section_should_decode(o.section_id, header))
            .collect();

        let results = if CONCURRENT_DESERIALIZATION_ENABLED
            && self.should_use_concurrent(&decodable)
        {
            if self.context.debug_config.is_enabled {
                self.context.log_info("Using concurrent section decoding (rayon)");
            }
            self.decode_sections_parallel(data, &decodable)?
        } else {
            if self.context.debug_config.is_enabled {
                let reason = if cfg!(target_arch = "wasm32") {
                    "(wasm32 — sequential only)"
                } else if !cfg!(feature = "rayon-support") {
                    "(rayon-support feature disabled)"
                } else if !CONCURRENT_DESERIALIZATION_ENABLED {
                    "(CONCURRENT_DESERIALIZATION_ENABLED = false)"
                } else {
                    "(insufficient sections)"
                };
                self.context
                    .log_info(&format!("Using sequential section decoding {}", reason));
            }
            self.decode_sections_sequential(data, &decodable)?
        };

        self.assemble_ast(results)
    }

    fn section_should_decode(&self, id: SectionId, header: &BinaryHeader) -> bool {
        match id {
            SectionId::Config   => header.has_section(SectionFlags::CONFIG),
            SectionId::Enums    => header.has_section(SectionFlags::ENUMS),
            SectionId::Data     => header.has_section(SectionFlags::DATA),
            SectionId::Security => header.has_section(SectionFlags::SECURITY),
            SectionId::Imports  => false,
        }
    }

    fn should_use_concurrent(&self, offsets: &[&SectionOffset]) -> bool {
        if cfg!(target_arch = "wasm32") || !cfg!(feature = "rayon-support") {
            return false;
        }
        offsets.len() >= 2 && !self.context.debug_config.is_verbose
    }

    fn decode_sections_parallel(
        &mut self,
        data: &[u8],
        offsets: &[&SectionOffset],
    ) -> Result<Vec<(SectionId, SectionDecodeResult)>, BinarySerializationError> {
        #[cfg(all(not(target_arch = "wasm32"), feature = "rayon-support"))]
        {
            use rayon::prelude::*;

            let results: Vec<(SectionId, SectionDecodeResult)> = offsets
                .par_iter()
                .map(|offset| {
                    let id     = offset.section_id;
                    let result = Self::decode_single_section(data, offset);
                    (id, result)
                })
                .collect();

            self.collect_decode_results(results)
        }

        // should_use_concurrent() always returns false on wasm32 or when
        // rayon-support is disabled, so this is never reached at runtime
        // in either of those cases. The cfg block above is compiled away
        // there, leaving only this sequential fallback.
        #[cfg(any(target_arch = "wasm32", not(feature = "rayon-support")))]
        self.decode_sections_sequential(data, offsets)
    }

    fn decode_sections_sequential(
        &mut self,
        data: &[u8],
        offsets: &[&SectionOffset],
    ) -> Result<Vec<(SectionId, SectionDecodeResult)>, BinarySerializationError> {
        let results: Vec<(SectionId, SectionDecodeResult)> = offsets
            .iter()
            .map(|offset| {
                let id     = offset.section_id;
                let result = Self::decode_single_section(data, offset);
                (id, result)
            })
            .collect();

        self.collect_decode_results(results)
    }

    fn collect_decode_results(
        &mut self,
        results: Vec<(SectionId, SectionDecodeResult)>,
    ) -> Result<Vec<(SectionId, SectionDecodeResult)>, BinarySerializationError> {
        let mut first_fatal: Option<BinarySerializationError> = None;

        for (id, ref result) in &results {
            if let Err(ref e) = result {
                self.context.error_manager.add_binary_serialization_error(
                    e.error_type,
                    e.message.clone(),
                    None, None, None, None,
                );
                if self.context.error_manager.should_terminate_parsing()
                    && first_fatal.is_none()
                {
                    first_fatal = Some(BinarySerializationError::new(
                        e.error_type,
                        e.message.clone(),
                        format!("{:?}", id),
                    ));
                }
            }
        }

        if let Some(e) = first_fatal {
            return Err(e);
        }

        Ok(results)
    }

    fn decode_single_section(data: &[u8], offset: &SectionOffset) -> SectionDecodeResult {
        let section_data = extract_section_data(data, offset)?;
        let mut cursor   = Cursor::new(section_data);
        let mut ctx      = BinarySerializationContext::new();

        match offset.section_id {
            SectionId::Config => {
                let mut decoder = ValueDecoder::new();
                let mut reader  = ConfigSectionReader::new(&mut ctx, &mut decoder);
                let section     = reader.read_section(&mut cursor, offset)?;
                Ok(Some(DecodedSection::Config(section)))
            }
            SectionId::Enums => {
                let mut reader = EnumsSectionReader::new(&mut ctx);
                let section    = reader.read_section(&mut cursor, offset)?;
                Ok(Some(DecodedSection::Enums(section)))
            }
            SectionId::Data => {
                let mut decoder = ValueDecoder::new();
                let mut reader  = DataSectionReader::new(&mut ctx, &mut decoder);
                let section     = reader.read_section(&mut cursor, offset)?;
                Ok(Some(DecodedSection::Data(section)))
            }
            SectionId::Security => {
                let mut decoder = ValueDecoder::new();
                let mut reader  = SecuritySectionReader::new(&mut ctx, &mut decoder);
                let section     = reader.read_section(&mut cursor, offset)?;
                Ok(Some(DecodedSection::Security(section)))
            }
            SectionId::Imports => Ok(None),
        }
    }

    fn assemble_ast(
        &mut self,
        results: Vec<(SectionId, SectionDecodeResult)>,
    ) -> Result<DixScript, BinarySerializationError> {
        let mut config   = None;
        let mut enums    = None;
        let mut data     = None;
        let mut security = None;

        for (id, result) in results {
            match result {
                Ok(Some(DecodedSection::Config(s))) => {
                    if self.context.debug_config.is_enabled {
                        self.context.log_info("CONFIG section decoded");
                    }
                    config = Some(s);
                    self.context.statistics.total_sections += 1;
                }
                Ok(Some(DecodedSection::Enums(s))) => {
                    if self.context.debug_config.is_enabled {
                        self.context.log_info("ENUMS section decoded");
                    }
                    enums = Some(s);
                    self.context.statistics.total_sections += 1;
                }
                Ok(Some(DecodedSection::Data(s))) => {
                    if self.context.debug_config.is_enabled {
                        self.context.log_info("DATA section decoded");
                    }
                    data = Some(s);
                    self.context.statistics.total_sections += 1;
                }
                Ok(Some(DecodedSection::Security(s))) => {
                    if self.context.debug_config.is_enabled {
                        self.context.log_info("SECURITY section decoded");
                    }
                    security = Some(s);
                    self.context.statistics.total_sections += 1;
                }
                Ok(None) => {}
                Err(_)   => {}
            }
        }

        Ok(DixScript {
            config,
            imports: None,
            dlm: None,
            enums,
            quick_functions: None,
            data,
            security,
        })
    }
}

fn extract_section_data<'a>(
    data: &'a [u8],
    offset: &SectionOffset,
) -> Result<&'a [u8], BinarySerializationError> {
    let start = offset.offset as usize;
    let end   = start + offset.length as usize;
    if end > data.len() {
        return Err(BinarySerializationError::corrupted_data(format!(
            "Section {} extends beyond file: offset={}, length={}, file_size={}",
            offset.section_id.name(),
            offset.offset,
            offset.length,
            data.len()
        )));
    }
    Ok(&data[start..end])
}

impl Default for BinaryUnpacker {
    fn default() -> Self { Self::new() }
                }
