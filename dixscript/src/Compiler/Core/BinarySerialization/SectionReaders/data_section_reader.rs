// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/BinarySerialization/SectionReaders/data_section_reader.rs"
// ============================================================================
//! Reads @DATA section from binary format.

use std::io::Read;
use crate::Compiler::AST::{DataSection, DataEntry, TablePath, PropertyAssignment, Value, Position};
use crate::Compiler::AST::arena::{Arena, DocData, DocDataEntry, DocPropertyAssignment, ValueNode};
use crate::ErrorManager::ErrorTypes::BinarySerializationErrorType;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_decoder::ValueDecoder;

/// Entry type discriminants — must match writer.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataEntryType {
    SimpleProperty = 0x01,
    TableProperty  = 0x02,
    GroupArray     = 0x03,
    ObjectProperty = 0x04,
}

impl DataEntryType {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::SimpleProperty),
            0x02 => Some(Self::TableProperty),
            0x03 => Some(Self::GroupArray),
            0x04 => Some(Self::ObjectProperty),
            _ => None,
        }
    }
}

/// Reads @DATA section from binary format.
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
pub struct DataSectionReader<'a> {
    context: &'a mut BinarySerializationContext,
    value_decoder: &'a mut ValueDecoder,
}

impl<'a> DataSectionReader<'a> {
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_decoder: &'a mut ValueDecoder,
    ) -> Self {
        DataSectionReader { context, value_decoder }
    }

    pub fn read_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
    ) -> Result<DataSection, BinarySerializationError> {
        let entries = self.read_entries(reader, offset, |this, reader| this.read_data_entry(reader))?;
        Ok(DataSection::new(entries, Position::UNKNOWN))
    }

    /// Reads the `@DATA` section into `arena` and returns it as a [`DocData`].
    ///
    /// Reads the same bytes and fails with the same errors as [`read_section`](Self::read_section);
    /// exporting the result gives the section `read_section` returns. The nodes are appended
    /// to `arena`.
    pub fn read_document_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
        arena: &mut Arena,
    ) -> Result<DocData, BinarySerializationError> {
        let entries = self.read_entries(reader, offset, |this, reader| this.read_doc_entry(reader, arena))?;
        Ok(DocData { entries, position: Position::UNKNOWN })
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
                "Reading @DATA section from offset {}",
                offset.offset
            ));
        }

        let mut id_buf = [0u8; 4];
        reader
            .read_exact(&mut id_buf)
            .map_err(|e| self.read_err(e.to_string(), "DataSection"))?;
        let section_id = u32::from_le_bytes(id_buf);
        if section_id != SectionId::Data as u32 {
            return Err(BinarySerializationError::invalid_section_id(section_id, "DataSection"));
        }

        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .map_err(|e| self.read_err(e.to_string(), "DataSection"))?;
        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  section length: {} bytes",
                i32::from_le_bytes(len_buf)
            ));
        }

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "DataSection"))?;
        let entry_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_enabled {
            self.context
                .log_info(&format!("  reading {} data entries", entry_count));
        }

        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let entry = read_entry(self, reader)?;
            entries.push(entry);
            if self.context.error_manager.should_terminate_parsing() {
                return Err(BinarySerializationError::invalid_state(
                    "Terminating DATA read due to accumulated errors",
                    "DataSection",
                ));
            }
        }

        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!("@DATA read: {} entries", entry_count));
        }

        Ok(entries)
    }

    fn read_data_entry<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        match self.read_entry_type(reader)? {
            DataEntryType::SimpleProperty => self.read_simple_property(reader),
            DataEntryType::TableProperty => self.read_table_property(reader),
            DataEntryType::GroupArray => self.read_group_array(reader),
            DataEntryType::ObjectProperty => self.read_object_property(reader),
        }
    }

    fn read_simple_property<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        let name = self.read_string(reader, "SimpleProperty")?;
        let value = self
            .value_decoder
            .decode_value(reader, self.context)
            .map_err(|e| self.read_err(e.to_string(), "SimpleProperty"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  simple: {} = {}", name, value));
        }

        Ok(DataEntry::SimpleProperty {
            name,
            data_type: None,
            value,
            position: Position::UNKNOWN,
        })
    }

    fn read_table_property<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        let path_str = self.read_string(reader, "TableProperty")?;
        let segments: Vec<String> = path_str
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let table_path = TablePath::new(segments);

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "TableProperty"))?;
        let property_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  table: {} ({} properties)",
                path_str, property_count
            ));
        }

        let mut properties = Vec::with_capacity(property_count as usize);
        for _ in 0..property_count {
            properties.push(self.read_property_assignment(reader)?);
        }

        Ok(DataEntry::TableProperty {
            path: table_path,
            properties,
            position: Position::UNKNOWN,
        })
    }

    fn read_property_assignment<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<PropertyAssignment, BinarySerializationError> {
        let name = self.read_string(reader, "PropertyAssignment")?;
        let value = self
            .value_decoder
            .decode_value(reader, self.context)
            .map_err(|e| self.read_err(e.to_string(), "PropertyAssignment"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("    property: {} = {}", name, value));
        }

        Ok(PropertyAssignment::new(name, None, value, Position::UNKNOWN))
    }

    fn read_group_array<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        let path_str = self.read_string(reader, "GroupArray")?;
        let segments: Vec<String> = path_str
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let array_path = TablePath::new(segments);

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "GroupArray"))?;
        let item_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  group array: {} ({} items)",
                path_str, item_count
            ));
        }

        let mut items = Vec::with_capacity(item_count as usize);
        for _ in 0..item_count {
            items.push(
                self.value_decoder
                    .decode_value(reader, self.context)
                    .map_err(|e| self.read_err(e.to_string(), "GroupArray"))?,
            );
        }

        Ok(DataEntry::GroupArray {
            path: array_path,
            items,
            position: Position::UNKNOWN,
        })
    }

    fn read_object_property<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        let name = self.read_string(reader, "ObjectProperty")?;
        let value = self
            .value_decoder
            .decode_value(reader, self.context)
            .map_err(|e| self.read_err(e.to_string(), "ObjectProperty"))?;

        if !matches!(&value, Value::Object { .. }) {
            let e = BinarySerializationError::new(
                BinarySerializationErrorType::InvalidFormat,
                format!("Expected object literal for '{}', got {:?}", name, value),
                self.context.get_current_scope(),
            );
            self.context.error_manager.add_binary_serialization_error(
                e.error_type,
                e.message.clone(),
                None,
                None,
                None,
                None,
            );
            return Err(e);
        }

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  object property: {}", name));
        }

        Ok(DataEntry::ObjectProperty {
            name,
            data_type: None,
            object: Box::new(value),
            position: Position::UNKNOWN,
        })
    }

    fn read_entry_type<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntryType, BinarySerializationError> {
        let mut type_buf = [0u8; 1];
        reader
            .read_exact(&mut type_buf)
            .map_err(|e| self.read_err(e.to_string(), "DataEntry"))?;

        let entry_type = DataEntryType::from_u8(type_buf[0]).ok_or_else(|| {
            let e = BinarySerializationError::new(
                BinarySerializationErrorType::InvalidFormat,
                format!("Unknown data entry type: 0x{:02X}", type_buf[0]),
                self.context.get_current_scope(),
            );
            self.context.error_manager.add_binary_serialization_error(
                e.error_type,
                e.message.clone(),
                None,
                None,
                None,
                None,
            );
            e
        })?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  data entry type: {:?}", entry_type));
        }

        Ok(entry_type)
    }

    fn read_doc_entry<R: Read>(
        &mut self,
        reader: &mut R,
        arena: &mut Arena,
    ) -> Result<DocDataEntry, BinarySerializationError> {
        match self.read_entry_type(reader)? {
            DataEntryType::SimpleProperty => self.read_doc_simple_property(reader, arena),
            DataEntryType::TableProperty => self.read_doc_table_property(reader, arena),
            DataEntryType::GroupArray => self.read_doc_group_array(reader, arena),
            DataEntryType::ObjectProperty => self.read_doc_object_property(reader, arena),
        }
    }

    fn read_doc_simple_property<R: Read>(
        &mut self,
        reader: &mut R,
        arena: &mut Arena,
    ) -> Result<DocDataEntry, BinarySerializationError> {
        let name = self.read_string(reader, "SimpleProperty")?;
        let value = self
            .value_decoder
            .decode_node(reader, arena, self.context)
            .map_err(|e| self.read_err(e.to_string(), "SimpleProperty"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  simple: {} = {}", name, arena.value_ref(value)));
        }

        Ok(DocDataEntry::SimpleProperty {
            name,
            data_type: None,
            value,
            position: Position::UNKNOWN,
        })
    }

    fn read_doc_table_property<R: Read>(
        &mut self,
        reader: &mut R,
        arena: &mut Arena,
    ) -> Result<DocDataEntry, BinarySerializationError> {
        let path_str = self.read_string(reader, "TableProperty")?;
        let segments: Vec<String> = path_str
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let table_path = TablePath::new(segments);

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "TableProperty"))?;
        let property_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  table: {} ({} properties)",
                path_str, property_count
            ));
        }

        let mut properties = Vec::with_capacity(property_count as usize);
        for _ in 0..property_count {
            let name = self.read_string(reader, "PropertyAssignment")?;
            let value = self
                .value_decoder
                .decode_node(reader, arena, self.context)
                .map_err(|e| self.read_err(e.to_string(), "PropertyAssignment"))?;

            if self.context.debug_config.is_verbose {
                self.context
                    .log_verbose(&format!("    property: {} = {}", name, arena.value_ref(value)));
            }

            properties.push(DocPropertyAssignment {
                name,
                data_type: None,
                value,
                position: Position::UNKNOWN,
            });
        }

        Ok(DocDataEntry::TableProperty {
            path: table_path,
            properties,
            position: Position::UNKNOWN,
        })
    }

    fn read_doc_group_array<R: Read>(
        &mut self,
        reader: &mut R,
        arena: &mut Arena,
    ) -> Result<DocDataEntry, BinarySerializationError> {
        let path_str = self.read_string(reader, "GroupArray")?;
        let segments: Vec<String> = path_str
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let array_path = TablePath::new(segments);

        let mut count_buf = [0u8; 4];
        reader
            .read_exact(&mut count_buf)
            .map_err(|e| self.read_err(e.to_string(), "GroupArray"))?;
        let item_count = i32::from_le_bytes(count_buf);

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  group array: {} ({} items)",
                path_str, item_count
            ));
        }

        let mut ids = Vec::with_capacity(item_count as usize);
        for _ in 0..item_count {
            ids.push(
                self.value_decoder
                    .decode_node(reader, arena, self.context)
                    .map_err(|e| self.read_err(e.to_string(), "GroupArray"))?,
            );
        }

        Ok(DocDataEntry::GroupArray {
            path: array_path,
            items: arena.alloc_value_list(&ids),
            position: Position::UNKNOWN,
        })
    }

    fn read_doc_object_property<R: Read>(
        &mut self,
        reader: &mut R,
        arena: &mut Arena,
    ) -> Result<DocDataEntry, BinarySerializationError> {
        let name = self.read_string(reader, "ObjectProperty")?;
        let value = self
            .value_decoder
            .decode_node(reader, arena, self.context)
            .map_err(|e| self.read_err(e.to_string(), "ObjectProperty"))?;

        if !matches!(arena.value(value), ValueNode::Object { .. }) {
            let e = BinarySerializationError::new(
                BinarySerializationErrorType::InvalidFormat,
                format!("Expected object literal for '{}', got {:?}", name, arena.value_ref(value)),
                self.context.get_current_scope(),
            );
            self.context.error_manager.add_binary_serialization_error(
                e.error_type,
                e.message.clone(),
                None,
                None,
                None,
                None,
            );
            return Err(e);
        }

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  object property: {}", name));
        }

        Ok(DocDataEntry::ObjectProperty {
            name,
            data_type: None,
            object: value,
            position: Position::UNKNOWN,
        })
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
