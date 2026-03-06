//! Writes @IMPORTS section to binary format

use std::io::{Write, Seek, SeekFrom};
use crate::Compiler::AST::{ImportsSection, ImportDeclaration};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Writes @IMPORTS section to binary format
/// Format: [Section ID: 4][Section Length: 4][Import Count: 4][Imports...]
/// Each import: [Alias Length: 4][Alias UTF-8][Path Length: 4][Path UTF-8]
///              [IsCloudImport: 1][HasHash: 1][Hash Length: 4][Hash UTF-8 if present]
pub struct ImportsSectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
    error_manager: ErrorManager,
}

impl<'a> ImportsSectionWriter<'a> {
    /// Create new imports section writer
    pub fn new(context: &'a mut BinarySerializationContext) -> Self {
        ImportsSectionWriter {
            context,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Write @IMPORTS section to binary
    /// Returns offset information for offset table
    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        imports_section: &ImportsSection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        self.context.log_info(&format!(
            "Writing @IMPORTS section ({} imports)",
            imports_section.imports.len()
        ));

        let start_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?
            as i32;

        // Write section header
        writer.write_all(&(SectionId::Imports as u32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?;

        // Placeholder for section length
        let length_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?;
        writer.write_all(&0i32.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?;

        // Write import count
        writer.write_all(&(imports_section.imports.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?;

        // Write each import
        for import in &imports_section.imports {
            self.write_import_declaration(writer, import)?;
        }

        // Calculate and update section length
        let end_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?
            as i32;
        let section_length = end_position - start_position - 8;

        writer.seek(SeekFrom::Start(length_position))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?;
        writer.write_all(&section_length.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?;
        writer.seek(SeekFrom::Start(end_position as u64))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportsSection"))?;

        self.context.log_info(&format!("✅ @IMPORTS section written: {} bytes", section_length));
        self.context.statistics.record_section_size(SectionId::Imports, section_length as usize);

        Ok(SectionOffset::new(
            SectionId::Imports,
            start_position,
            end_position - start_position,
        ))
    }

    /// Write individual import declaration
    /// Format: [Alias Length: 4][Alias UTF-8][Path Length: 4][Path UTF-8]
    ///         [IsCloudImport: 1][HasHash: 1][Hash Length: 4][Hash UTF-8 if present]
    fn write_import_declaration<W: Write>(
        &mut self,
        writer: &mut W,
        import: &ImportDeclaration,
    ) -> Result<(), BinarySerializationError> {
        // Write alias
        let alias_bytes = import.alias.as_bytes();
        writer.write_all(&(alias_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportDeclaration"))?;
        writer.write_all(alias_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportDeclaration"))?;

        // Write path
        let path_bytes = import.path.as_bytes();
        writer.write_all(&(path_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportDeclaration"))?;
        writer.write_all(path_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportDeclaration"))?;

        // Write cloud import flag
        writer.write_all(&[if import.is_cloud_import { 0x01 } else { 0x00 }])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportDeclaration"))?;

        // Write hash flag and hash if present
        let has_hash = import.verify_hash.is_some();
        writer.write_all(&[if has_hash { 0x01 } else { 0x00 }])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportDeclaration"))?;

        if let Some(ref hash) = import.verify_hash {
            let hash_bytes = hash.as_bytes();
            writer.write_all(&(hash_bytes.len() as i32).to_le_bytes())
                .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportDeclaration"))?;
            writer.write_all(hash_bytes)
                .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ImportDeclaration"))?;
        }

        let import_type = if import.is_cloud_import { "from_cloud" } else { "from" };
        let hash_info = if has_hash {
            format!(" verify \"{}\"", import.verify_hash.as_ref().unwrap())
        } else {
            String::new()
        };
        self.context.log_debug(&format!(
            "  Import: {} {} \"{}\"{}",
            import.alias, import_type, import.path, hash_info
        ));

        Ok(())
    }
                                                               }
