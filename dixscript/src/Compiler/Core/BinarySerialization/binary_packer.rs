// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/BinarySerialization/binary_packer.rs"
// ============================================================================
//! Main binary serialization orchestrator — packs DixScript AST into binary format.

use web_time::Instant;
use std::io::{Write, Cursor};
use crate::Compiler::AST::{ConfigSection, DixScript, EnumsSection};
use crate::Compiler::AST::arena::Document;
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

/// What the packer reads: an owned tree or an arena document. Only `@DATA` and `@SECURITY`
/// differ between the two; `@CONFIG` and `@ENUMS` are the same owned types in both.
#[derive(Clone, Copy)]
enum Source<'a> {
    Owned(&'a DixScript),
    Document(&'a Document),
}

impl<'a> Source<'a> {
    fn config(self) -> Option<&'a ConfigSection> {
        match self {
            Source::Owned(ast)      => ast.config.as_ref(),
            Source::Document(doc)   => doc.config.as_ref(),
        }
    }

    fn enums(self) -> Option<&'a EnumsSection> {
        match self {
            Source::Owned(ast)      => ast.enums.as_ref(),
            Source::Document(doc)   => doc.enums.as_ref(),
        }
    }

    /// The number of `@DATA` entries, or `None` without a `@DATA` section.
    fn data_entries(self) -> Option<usize> {
        match self {
            Source::Owned(ast)      => ast.data.as_ref().map(|d| d.entries.len()),
            Source::Document(doc)   => doc.data.as_ref().map(|d| d.entries.len()),
        }
    }

    fn has_security(self) -> bool {
        match self {
            Source::Owned(ast)      => ast.security.is_some(),
            Source::Document(doc)   => doc.security.is_some(),
        }
    }
}

pub struct BinaryPacker {
    context: BinarySerializationContext,
}

impl BinaryPacker {
    pub fn new() -> Self {
        BinaryPacker {
            context: BinarySerializationContext::new(),
        }
    }

    /// Packs an owned tree.
    pub fn pack(&mut self, ast: &DixScript) -> BinarySerializationResult {
        self.pack_source(Source::Owned(ast))
    }

    /// Packs a [`Document`] without exporting it to an owned tree first.
    ///
    /// Writes the bytes [`pack`](Self::pack) writes for the exported document. Only `@CONFIG`,
    /// `@ENUMS`, `@DATA` and `@SECURITY` are stored in the binary format; the other sections
    /// of the document are ignored.
    pub fn pack_document(&mut self, document: &Document) -> BinarySerializationResult {
        self.pack_source(Source::Document(document))
    }

    fn pack_source(&mut self, source: Source<'_>) -> BinarySerializationResult {
        let start = Instant::now();

        if self.context.debug_config.is_enabled {
            self.context.log_info("Starting binary serialization");
        }

        let original_size = self.estimate_original_size(source);

        match self.pack_internal(source) {
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

    fn pack_internal(&mut self, source: Source<'_>) -> Result<Vec<u8>, BinarySerializationError> {
        let sections_to_encode = self.determine_sections(source);

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
            self.encode_sections_parallel(source, &sections_to_encode)?
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
            self.encode_sections_sequential(source, &sections_to_encode)?
        };

        self.assemble_binary(encoded)
    }

    fn determine_sections(&self, source: Source<'_>) -> Vec<SectionId> {
        let mut sections = Vec::with_capacity(4);
        for &id in CANONICAL_SECTION_ORDER {
            if self.section_present(id, source) {
                sections.push(id);
            }
        }
        sections
    }

    fn section_present(&self, id: SectionId, source: Source<'_>) -> bool {
        match id {
            SectionId::Config   => source.config().is_some(),
            SectionId::Enums    => source.enums().is_some(),
            SectionId::Data     => source.data_entries().is_some(),
            SectionId::Security => source.has_security(),
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
        source: Source<'_>,
        sections: &[SectionId],
    ) -> Result<Vec<(SectionId, Vec<u8>)>, BinarySerializationError> {
        #[cfg(all(not(target_arch = "wasm32"), feature = "rayon-support"))]
        {
            use rayon::prelude::*;

            let results: Vec<(SectionId, SectionEncodeResult)> = sections
                .par_iter()
                .map(|&id| (id, Self::encode_single_section(id, source)))
                .collect();

            self.collect_encode_results(results, sections)
        }

        // should_use_concurrent() always returns false on wasm32 or when
        // rayon-support is disabled, so this is never reached at runtime
        // in either of those cases. The cfg block above is compiled away
        // there, leaving only this sequential fallback.
        #[cfg(any(target_arch = "wasm32", not(feature = "rayon-support")))]
        self.encode_sections_sequential(source, sections)
    }

    fn encode_sections_sequential(
        &mut self,
        source: Source<'_>,
        sections: &[SectionId],
    ) -> Result<Vec<(SectionId, Vec<u8>)>, BinarySerializationError> {
        let results: Vec<(SectionId, SectionEncodeResult)> = sections
            .iter()
            .map(|&id| (id, Self::encode_single_section(id, source)))
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

    fn encode_single_section(section_id: SectionId, source: Source<'_>) -> SectionEncodeResult {
        let mut buf = Cursor::new(Vec::new());
        let mut ctx = BinarySerializationContext::new();

        match section_id {
            SectionId::Config => {
                let config = source.config().ok_or_else(|| {
                    BinarySerializationError::invalid_state(
                        "CONFIG section absent during encoding", "ConfigSection",
                    )
                })?;
                let mut enc = ValueEncoder::new();
                ConfigSectionWriter::new(&mut ctx, &mut enc)
                    .write_section(&mut buf, config)?;
            }
            SectionId::Enums => {
                let enums = source.enums().ok_or_else(|| {
                    BinarySerializationError::invalid_state(
                        "ENUMS section absent during encoding", "EnumsSection",
                    )
                })?;
                EnumsSectionWriter::new(&mut ctx).write_section(&mut buf, enums)?;
            }
            SectionId::Data => {
                let absent = || BinarySerializationError::invalid_state(
                    "DATA section absent during encoding", "DataSection",
                );
                let mut enc = ValueEncoder::new()
                    .with_enums(ValueEncoder::build_local_enums(source.enums()));
                let mut writer = DataSectionWriter::new(&mut ctx, &mut enc);
                match source {
                    Source::Owned(ast) => {
                        let data = ast.data.as_ref().ok_or_else(absent)?;
                        writer.write_section(&mut buf, data)?;
                    }
                    Source::Document(doc) => {
                        let data = doc.data.as_ref().ok_or_else(absent)?;
                        writer.write_document_section(&mut buf, &doc.arena, data)?;
                    }
                }
            }
            SectionId::Security => {
                let absent = || BinarySerializationError::invalid_state(
                    "SECURITY section absent during encoding", "SecuritySection",
                );
                let mut enc = ValueEncoder::new();
                let mut writer = SecuritySectionWriter::new(&mut ctx, &mut enc);
                match source {
                    Source::Owned(ast) => {
                        let security = ast.security.as_ref().ok_or_else(absent)?;
                        writer.write_section(&mut buf, security)?;
                    }
                    Source::Document(doc) => {
                        let security = doc.security.as_ref().ok_or_else(absent)?;
                        writer.write_document_section(&mut buf, &doc.arena, security)?;
                    }
                }
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
            .write_all(&[0u8; HEADER_SIZE])
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

    fn estimate_original_size(&self, source: Source<'_>) -> usize {
        let mut size = 100;
        if source.config().is_some()  { size += 200; }
        if let Some(e) = source.enums() { size += e.enums.len() * 100; }
        if let Some(entries) = source.data_entries() { size += entries * 150; }
        if source.has_security() { size += 100; }
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
