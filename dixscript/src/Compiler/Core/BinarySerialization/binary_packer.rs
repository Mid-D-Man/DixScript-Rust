//! Main binary serialization orchestrator — packs DixScript AST into binary format.

use web_time::Instant;
use std::io::{Write, Cursor};
use crate::Compiler::AST::DixScript;
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;
use super::{
    binary_format::{HEADER_SIZE, SectionFlags, SectionId},
    binary_header::BinaryHeader,
    section_offset::SectionOffset,
    checksum_validator::ChecksumValidator,
    binary_serialization_context::{BinarySerializationContext, BinarySerializationStatistics},
    binary_serialization_result::BinarySerializationResult,
    binary_serialization_error::BinarySerializationError,
    SectionWriters::{ConfigSectionWriter, EnumsSectionWriter, DataSectionWriter, SecuritySectionWriter},
    ValueEncoder,
};

#[cfg(not(target_arch = "wasm32"))]
const CONCURRENT_SERIALIZATION_ENABLED: bool = true;
#[cfg(target_arch = "wasm32")]
const CONCURRENT_SERIALIZATION_ENABLED: bool = false;

const CANONICAL_SECTION_ORDER: &[SectionId] = &[
    SectionId::Config,
    SectionId::Enums,
    SectionId::Data,
    SectionId::Security,
];

type SectionEncodeResult = Result<(Vec<u8>, BinarySerializationStatistics), BinarySerializationError>;

pub struct BinaryPacker {
    context: BinarySerializationContext,
}

impl BinaryPacker {
    pub fn new() -> Self {
        BinaryPacker {
            context: BinarySerializationContext::new(),
        }
    }

    pub fn pack(&mut self, ast: &DixScript) -> BinarySerializationResult {
        let start = Instant::now();

        if self.context.debug_config.is_enabled {
            self.context.log_info("Starting binary serialization");
        }

        let original_size = self.estimate_original_size(ast);

        match self.pack_internal(ast) {
            Ok(binary_data) => {
                let duration = start.elapsed();
                if self.context.debug_config.is_enabled {
                    self.context.log_info(&format!(
                        "Serialization complete: {} bytes in {:.2}ms",
                        binary_data.len(),
                        duration.as_secs_f64() * 1000.0
                    ));
                }
                BinarySerializationResult::success(
                    binary_data,
                    original_size,
                    duration,
                    self.context.statistics.clone(),
                )
            }
            Err(e) => {
                let duration = start.elapsed();
                self.context.error_manager.log_error(&format!(
                    "[BinaryPacker] Serialization failed: {}", e
                ));
                BinarySerializationResult::failure(
                    vec![e.to_string()],
                    Vec::new(),
                    duration,
                )
            }
        }
    }

    fn pack_internal(&mut self, ast: &DixScript) -> Result<Vec<u8>, BinarySerializationError> {
        let sections_to_encode = self.determine_sections(ast);

        if sections_to_encode.is_empty() {
            return Err(BinarySerializationError::invalid_state(
                "No serializable sections present in AST",
                "BinaryPacker",
            ));
        }

        let encoded = if CONCURRENT_SERIALIZATION_ENABLED
            && self.should_use_concurrent(&sections_to_encode)
        {
            if self.context.debug_config.is_enabled {
                self.context.log_info("Using concurrent section encoding (rayon)");
            }
            self.encode_sections_parallel(ast, &sections_to_encode)?
        } else {
            if self.context.debug_config.is_enabled {
                let reason = if cfg!(target_arch = "wasm32") {
                    "(wasm32 — sequential only)"
                } else if !cfg!(feature = "rayon-support") {
                    "(rayon-support feature disabled)"
                } else if !CONCURRENT_SERIALIZATION_ENABLED {
                    "(CONCURRENT_SERIALIZATION_ENABLED = false)"
                } else {
                    "(insufficient sections for parallel)"
                };
                self.context.log_info(&format!(
                    "Using sequential section encoding {}", reason
                ));
            }
            self.encode_sections_sequential(ast, &sections_to_encode)?
        };

        self.assemble_binary(encoded)
    }

    fn determine_sections(&self, ast: &DixScript) -> Vec<SectionId> {
        let mut sections = Vec::with_capacity(4);
        for &id in CANONICAL_SECTION_ORDER {
            if self.section_present(id, ast) {
                sections.push(id);
            }
        }
        sections
    }

    fn section_present(&self, id: SectionId, ast: &DixScript) -> bool {
        match id {
            SectionId::Config   => ast.config.is_some(),
            SectionId::Enums    => ast.enums.is_some(),
            SectionId::Data     => ast.data.is_some(),
            SectionId::Security => ast.security.is_some(),
            SectionId::Imports  => false,
        }
    }

    fn should_use_concurrent(&self, sections: &[SectionId]) -> bool {
        if cfg!(target_arch = "wasm32") || !cfg!(feature = "rayon-support") {
            return false;
        }
        sections.len() >= 2 && !self.context.debug_config.is_verbose
    }

    fn encode_sections_parallel(
        &mut self,
        ast: &DixScript,
        sections: &[SectionId],
    ) -> Result<Vec<(SectionId, Vec<u8>)>, BinarySerializationError> {
        #[cfg(all(not(target_arch = "wasm32"), feature = "rayon-support"))]
        {
            use rayon::prelude::*;

            let results: Vec<(SectionId, SectionEncodeResult)> = sections
                .par_iter()
                .map(|&id| (id, Self::encode_single_section(id, ast)))
                .collect();

            return self.collect_encode_results(results, sections);
        }

        // should_use_concurrent() always returns false on wasm32 or when
        // rayon-support is disabled, so this is never reached at runtime
        // in either of those cases. The cfg block above is compiled away
        // there, leaving only this sequential fallback.
        #[cfg(any(target_arch = "wasm32", not(feature = "rayon-support")))]
        self.encode_sections_sequential(ast, sections)
    }

    fn encode_sections_sequential(
        &mut self,
        ast: &DixScript,
        sections: &[SectionId],
    ) -> Result<Vec<(SectionId, Vec<u8>)>, BinarySerializationError> {
        let results: Vec<(SectionId, SectionEncodeResult)> = sections
            .iter()
            .map(|&id| (id, Self::encode_single_section(id, ast)))
            .collect();

        self.collect_encode_results(results, sections)
    }

    fn collect_encode_results(
        &mut self,
        results: Vec<(SectionId, SectionEncodeResult)>,
        canonical_order: &[SectionId],
    ) -> Result<Vec<(SectionId, Vec<u8>)>, BinarySerializationError> {
        let mut map = std::collections::HashMap::with_capacity(results.len());
        let mut first_fatal: Option<BinarySerializationError> = None;

        for (id, result) in results {
            match result {
                Ok((bytes, stats)) => {
                    self.context.merge_statistics(stats);
                    map.insert(id, bytes);
                }
                Err(e) => {
                    self.context.error_manager.add_binary_serialization_error(
                        e.error_type,
                        e.message.clone(),
                        None, None, None, None,
                    );
                    if self.context.error_manager.should_terminate_parsing()
                        && first_fatal.is_none()
                    {
                        first_fatal = Some(e);
                    }
                }
            }
        }

        if let Some(e) = first_fatal {
            return Err(e);
        }

        let mut ordered = Vec::with_capacity(map.len());
        for &id in canonical_order {
            if let Some(bytes) = map.remove(&id) {
                ordered.push((id, bytes));
            }
        }
        Ok(ordered)
    }

    fn encode_single_section(section_id: SectionId, ast: &DixScript) -> SectionEncodeResult {
        let mut buf = Cursor::new(Vec::new());
        let mut ctx = BinarySerializationContext::new();

        match section_id {
            SectionId::Config => {
                let config = ast.config.as_ref().ok_or_else(|| {
                    BinarySerializationError::invalid_state(
                        "CONFIG section absent during encoding", "ConfigSection",
                    )
                })?;
                let mut enc = ValueEncoder::new();
                ConfigSectionWriter::new(&mut ctx, &mut enc)
                    .write_section(&mut buf, config)?;
            }
            SectionId::Enums => {
                let enums = ast.enums.as_ref().ok_or_else(|| {
                    BinarySerializationError::invalid_state(
                        "ENUMS section absent during encoding", "EnumsSection",
                    )
                })?;
                EnumsSectionWriter::new(&mut ctx).write_section(&mut buf, enums)?;
            }
            SectionId::Data => {
                let data = ast.data.as_ref().ok_or_else(|| {
                    BinarySerializationError::invalid_state(
                        "DATA section absent during encoding", "DataSection",
                    )
                })?;
                let mut enc = ValueEncoder::new();
                DataSectionWriter::new(&mut ctx, &mut enc)
                    .write_section(&mut buf, data)?;
            }
            SectionId::Security => {
                let security = ast.security.as_ref().ok_or_else(|| {
                    BinarySerializationError::invalid_state(
                        "SECURITY section absent during encoding", "SecuritySection",
                    )
                })?;
                let mut enc = ValueEncoder::new();
                SecuritySectionWriter::new(&mut ctx, &mut enc)
                    .write_section(&mut buf, security)?;
            }
            SectionId::Imports => {
                return Ok((Vec::new(), BinarySerializationStatistics::new()));
            }
        }

        Ok((buf.into_inner(), ctx.statistics))
    }

    fn assemble_binary(
        &mut self,
        encoded_sections: Vec<(SectionId, Vec<u8>)>,
    ) -> Result<Vec<u8>, BinarySerializationError> {
        let estimated_cap = HEADER_SIZE
            + encoded_sections.iter().map(|(_, b)| b.len()).sum::<usize>()
            + encoded_sections.len() * 12
            + 32;
        let mut buffer = Cursor::new(Vec::with_capacity(estimated_cap));

        buffer
            .write_all(&vec![0u8; HEADER_SIZE])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "Header"))?;

        let mut header          = BinaryHeader::new();
        let mut section_offsets = Vec::with_capacity(encoded_sections.len());

        for (section_id, bytes) in &encoded_sections {
            if bytes.is_empty() { continue; }
            let offset = buffer.position() as i32;
            buffer
                .write_all(bytes)
                .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SectionData"))?;

            let length = bytes.len() as i32;
            section_offsets.push(SectionOffset::new(*section_id, offset, length));
            header.add_section(section_flag_for(*section_id));

            if self.context.debug_config.is_enabled {
                self.context.log_info(&format!(
                    "{} written: offset={}, length={}",
                    section_id.name(), offset, length
                ));
            }
        }

        let offset_table_position = buffer.position() as i32;
        header.offset_table_position = offset_table_position;
        header.section_count = section_offsets.len() as i32;

        for offset in &section_offsets {
            offset
                .write_to(&mut buffer)
                .map_err(|e| BinarySerializationError::write_error(e.to_string(), "OffsetTable"))?;
        }

        let mut binary_data   = buffer.into_inner();
        let header_bytes      = self.serialise_header(&header)?;
        binary_data[0..HEADER_SIZE].copy_from_slice(&header_bytes);

        let binary_data = ChecksumValidator::append_checksum(&binary_data);

        self.context.statistics.total_bytes    = binary_data.len();
        self.context.statistics.total_sections = section_offsets.len();

        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!(
                "Assembly complete: {} sections, {} bytes total",
                section_offsets.len(), binary_data.len()
            ));
        }

        Ok(binary_data)
    }

    fn serialise_header(
        &self,
        header: &BinaryHeader,
    ) -> Result<Vec<u8>, BinarySerializationError> {
        let mut buf = Cursor::new(Vec::with_capacity(HEADER_SIZE));
        header
            .write_to(&mut buf)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "Header"))?;
        Ok(buf.into_inner())
    }

    fn estimate_original_size(&self, ast: &DixScript) -> usize {
        let mut size = 100;
        if ast.config.is_some()  { size += 200; }
        if let Some(ref e) = ast.enums { size += e.enums.len() * 100; }
        if let Some(ref d) = ast.data  { size += d.entries.len() * 150; }
        if ast.security.is_some() { size += 100; }
        size
    }
}

impl Default for BinaryPacker {
    fn default() -> Self { Self::new() }
}

fn section_flag_for(id: SectionId) -> SectionFlags {
    match id {
        SectionId::Config   => SectionFlags::CONFIG,
        SectionId::Enums    => SectionFlags::ENUMS,
        SectionId::Data     => SectionFlags::DATA,
        SectionId::Security => SectionFlags::SECURITY,
        SectionId::Imports  => SectionFlags::IMPORTS,
    }
        }
