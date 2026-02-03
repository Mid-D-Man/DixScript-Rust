//! Reads @IMPORTS section from binary format

use std::io::Read;
use crate::Compiler::AST::{ImportsSection, ImportDeclaration, Position};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Reads @IMPORTS section from binary format
/// Format: [Section ID: 4][Section Length: 4][Import Count: 4][Imports...]
/// Each import: [Alias Length: 4][Alias UTF-8][Path Length: 4][Path UTF-8]
///              [IsCloudImport: 1][HasHash: 1][Hash Length: 4][Hash UTF-8 if present]
pub struct ImportsSectionReader<'a> {
    context: &'a mut BinarySerializationContext,
    error_manager: ErrorManager,
}

impl<'a> ImportsSectionReader<'a> {
    /// Create new imports section reader
    pub fn new(context: &'a mut BinarySerializationContext) -> Self {
        ImportsSectionReader {
            context,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Read @IMPORTS section from binary
    pub fn read_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
    ) -> Result<ImportsSection, BinarySerializationError> {
        self.context.log_info(&format!(
            "Reading @IMPORTS section from offset {}",
            offset.offset
        ));

        // Read and validate section ID
        let mut id_buf = [0u8; 4];
        reader.read_exact(&mut id_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportsSection"))?;
        let section_id = u32::from_le_bytes(id_buf);

        if section_id != SectionId::Imports as u32 {
            return Err(BinarySerializationError::invalid_section_id(
                section_id,
                "ImportsSection",
            ));
        }

        // Read section length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportsSection"))?;
        let section_length = i32::from_le_bytes(len_buf);
        self.context.log_debug(&format!("Section length: {} bytes", section_length));

        // Read import count
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportsSection"))?;
        let import_count = i32::from_le_bytes(count_buf);
        self.context.log_info(&format!("Reading {} imports", import_count));

        // Read all imports
        let mut imports = Vec::with_capacity(import_count as usize);
        for _ in 0..import_count {
            let import = self.read_import_declaration(reader)?;
            imports.push(import);
        }

        self.context.log_info(&format!("✅ @IMPORTS section read: {} imports", import_count));

        Ok(ImportsSection::new(imports, Position::UNKNOWN))
    }

    /// Read individual import declaration
    /// Format: [Alias Length: 4][Alias UTF-8][Path Length: 4][Path UTF-8]
    ///         [IsCloudImport: 1][HasHash: 1][Hash Length: 4][Hash UTF-8 if present]
    fn read_import_declaration<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<ImportDeclaration, BinarySerializationError> {
        // Read alias length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;
        let alias_length = i32::from_le_bytes(len_buf) as usize;

        // Read alias
        let mut alias_bytes = vec![0u8; alias_length];
        reader.read_exact(&mut alias_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;
        let alias = String::from_utf8(alias_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;

        // Read path length
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;
        let path_length = i32::from_le_bytes(len_buf) as usize;

        // Read path
        let mut path_bytes = vec![0u8; path_length];
        reader.read_exact(&mut path_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;
        let path = String::from_utf8(path_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;

        // Read cloud import flag
        let mut flag_buf = [0u8; 1];
        reader.read_exact(&mut flag_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;
        let is_cloud_import = flag_buf[0] != 0x00;

        // Read hash flag
        reader.read_exact(&mut flag_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;
        let has_hash = flag_buf[0] != 0x00;

        // Read hash if present
        let verify_hash = if has_hash {
            let mut hash_len_buf = [0u8; 4];
            reader.read_exact(&mut hash_len_buf)
                .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;
            let hash_length = i32::from_le_bytes(hash_len_buf) as usize;

            let mut hash_bytes = vec![0u8; hash_length];
            reader.read_exact(&mut hash_bytes)
                .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;
            let hash = String::from_utf8(hash_bytes)
                .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ImportDeclaration"))?;
            Some(hash)
        } else {
            None
        };

        let import_type = if is_cloud_import { "from_cloud" } else { "from" };
        let hash_info = if has_hash {
            format!(" verify \"{}\"", verify_hash.as_ref().unwrap())
        } else {
            String::new()
        };
        self.context.log_debug(&format!(
            "  Import: {} {} \"{}\"{}",
            alias, import_type, path, hash_info
        ));

        Ok(ImportDeclaration::new(
            alias,
            path,
            is_cloud_import,
            verify_hash,
            Position::UNKNOWN,
        ))
    }
  }
