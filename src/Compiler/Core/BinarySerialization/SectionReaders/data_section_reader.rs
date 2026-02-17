//! Reads @DATA section from binary format (MOST COMPLEX)

use std::io::Read;
use crate::Compiler::AST::{DataSection, DataEntry, TablePath, PropertyAssignment, Value, Position};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_decoder::ValueDecoder;

/// Reads @DATA section from binary format (MOST COMPLEX)
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
///
/// Entry Types:
/// 1. Simple Property: [Type: 1][Name Length: 4][Name UTF-8][Value]
/// 2. Table Property: [Type: 2][Path Length: 4][Path UTF-8][Property Count: 4][Properties...]
/// 3. Group Array: [Type: 3][Path Length: 4][Path UTF-8][Item Count: 4][Items...]
/// 4. Object Property: [Type: 4][Name Length: 4][Name UTF-8][Object Value]
pub struct DataSectionReader<'a> {
    context: &'a mut BinarySerializationContext,
    value_decoder: &'a mut ValueDecoder,
    error_manager: ErrorManager,
}

/// Data entry type tags
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataEntryType {
    SimpleProperty = 0x01,
    TableProperty = 0x02,
    GroupArray = 0x03,
    ObjectProperty = 0x04,
}

impl DataEntryType {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(DataEntryType::SimpleProperty),
            0x02 => Some(DataEntryType::TableProperty),
            0x03 => Some(DataEntryType::GroupArray),
            0x04 => Some(DataEntryType::ObjectProperty),
            _ => None,
        }
    }
}

impl<'a> DataSectionReader<'a> {
    /// Create new data section reader
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_decoder: &'a mut ValueDecoder,
    ) -> Self {
        DataSectionReader {
            context,
            value_decoder,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Read @DATA section from binary
    pub fn read_section<R: Read>(
        &mut self,
        reader: &mut R,
        offset: &SectionOffset,
    ) -> Result<DataSection, BinarySerializationError> {
        self.context.log_info(&format!(
            "Reading @DATA section from offset {}",
            offset.offset
        ));

        // Read and validate section ID
        let mut id_buf = [0u8; 4];
        reader.read_exact(&mut id_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "DataSection"))?;
        let section_id = u32::from_le_bytes(id_buf);

        if section_id != SectionId::Data as u32 {
            return Err(BinarySerializationError::invalid_section_id(
                section_id,
                "DataSection",
            ));
        }

        // Read section length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "DataSection"))?;
        let section_length = i32::from_le_bytes(len_buf);
        self.context.log_debug(&format!("Section length: {} bytes", section_length));

        // Read entry count
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "DataSection"))?;
        let entry_count = i32::from_le_bytes(count_buf);
        self.context.log_info(&format!("Reading {} data entries", entry_count));

        // Read all entries
        let mut entries = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let entry = self.read_data_entry(reader)?;
            entries.push(entry);
        }

        self.context.log_info(&format!("✅ @DATA section read: {} entries", entry_count));

        Ok(DataSection::new(entries, Position::UNKNOWN))
    }

    /// Read any data entry (dispatches to specific reader)
    fn read_data_entry<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        // Read entry type
        let mut type_buf = [0u8; 1];
        reader.read_exact(&mut type_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "DataEntry"))?;

        let entry_type = DataEntryType::from_u8(type_buf[0])
            .ok_or_else(|| {
                BinarySerializationError::new(
                    crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                    format!("Unknown data entry type: 0x{:02X}", type_buf[0]),
                    self.context.get_current_scope(),
                )
            })?;

        self.context.log_debug(&format!("Reading data entry type: {:?}", entry_type));

        match entry_type {
            DataEntryType::SimpleProperty => self.read_simple_property(reader),
            DataEntryType::TableProperty => self.read_table_property(reader),
            DataEntryType::GroupArray => self.read_group_array(reader),
            DataEntryType::ObjectProperty => self.read_object_property(reader),
        }
    }

    /// Read Simple Property: [Name Length: 4][Name UTF-8][Value]
    /// Example: app_name = "MyApp"
    fn read_simple_property<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        // Read property name length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SimpleProperty"))?;
        let name_length = i32::from_le_bytes(len_buf) as usize;

        // Read property name
        let mut name_bytes = vec![0u8; name_length];
        reader.read_exact(&mut name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SimpleProperty"))?;
        let name = String::from_utf8(name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SimpleProperty"))?;

        // Read value
        let value = self.value_decoder.decode_value(reader,self.context)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "SimpleProperty"))?;

        self.context.log_debug(&format!("  Simple: {} = {}", name, value));

        Ok(DataEntry::SimpleProperty {
            name,
            data_type: None,
            value,
            position: Position::UNKNOWN,
        })
    }

    /// Read Table Property: [Path Length: 4][Path UTF-8][Property Count: 4][Properties...]
    /// Example: server.config: host = "localhost", port = 8080
    fn read_table_property<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        // Read table path length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "TableProperty"))?;
        let path_length = i32::from_le_bytes(len_buf) as usize;

        // Read table path
        let mut path_bytes = vec![0u8; path_length];
        reader.read_exact(&mut path_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "TableProperty"))?;
        let path_str = String::from_utf8(path_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "TableProperty"))?;

        // Parse path segments
        let segments: Vec<String> = path_str
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let table_path = TablePath::new(segments);

        // Read property count
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "TableProperty"))?;
        let property_count = i32::from_le_bytes(count_buf);

        self.context.log_debug(&format!(
            "  Table: {} ({} properties)",
            path_str, property_count
        ));

        // Read all properties
        let mut properties = Vec::with_capacity(property_count as usize);
        for _ in 0..property_count {
            let prop = self.read_property_assignment(reader)?;
            properties.push(prop);
        }

        Ok(DataEntry::TableProperty {
            path: table_path,
            properties,
            position: Position::UNKNOWN,
        })
    }

    /// Read Property Assignment: [Name Length: 4][Name UTF-8][Value]
    fn read_property_assignment<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<PropertyAssignment, BinarySerializationError> {
        // Read property name length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "PropertyAssignment"))?;
        let name_length = i32::from_le_bytes(len_buf) as usize;

        // Read property name
        let mut name_bytes = vec![0u8; name_length];
        reader.read_exact(&mut name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "PropertyAssignment"))?;
        let name = String::from_utf8(name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "PropertyAssignment"))?;

        // Read value
        let value = self.value_decoder.decode_value(reader,self.context)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "PropertyAssignment"))?;

        self.context.log_debug(&format!("    Property: {} = {}", name, value));

        Ok(PropertyAssignment::new(
            name,
            None,
            value,
            Position::UNKNOWN,
        ))
    }

    /// Read Group Array: [Path Length: 4][Path UTF-8][Item Count: 4][Items...]
    /// Example: users.admins:: { name = "Alice" }, { name = "Bob" }
    fn read_group_array<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        // Read array path length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "GroupArray"))?;
        let path_length = i32::from_le_bytes(len_buf) as usize;

        // Read array path
        let mut path_bytes = vec![0u8; path_length];
        reader.read_exact(&mut path_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "GroupArray"))?;
        let path_str = String::from_utf8(path_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "GroupArray"))?;

        // Parse path segments
        let segments: Vec<String> = path_str
            .split('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let array_path = TablePath::new(segments);

        // Read item count
        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "GroupArray"))?;
        let item_count = i32::from_le_bytes(count_buf);

        self.context.log_debug(&format!("  Array: {} ({} items)", path_str, item_count));

        // Read all items
        let mut items = Vec::with_capacity(item_count as usize);
        for _ in 0..item_count {
            let item = self.value_decoder.decode_value(reader,self.context)
                .map_err(|e| BinarySerializationError::read_error(e.to_string(), "GroupArray"))?;
            items.push(item);
        }

        Ok(DataEntry::GroupArray {
            path: array_path,
            items,
            position: Position::UNKNOWN,
        })
    }

    /// Read Object Property: [Name Length: 4][Name UTF-8][Object Value]
    /// Example: config = { timeout = 30, retries = 3 }
    fn read_object_property<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<DataEntry, BinarySerializationError> {
        // Read property name length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ObjectProperty"))?;
        let name_length = i32::from_le_bytes(len_buf) as usize;

        // Read property name
        let mut name_bytes = vec![0u8; name_length];
        reader.read_exact(&mut name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ObjectProperty"))?;
        let name = String::from_utf8(name_bytes)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ObjectProperty"))?;

        // Read object value (ValueDecoder handles ObjectLiteral)
        let value = self.value_decoder.decode_value(reader,self.context)
            .map_err(|e| BinarySerializationError::read_error(e.to_string(), "ObjectProperty"))?;

        // Verify it's an object
        if !matches!(&value, Value::Object { .. }) {
            return Err(BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                format!("Expected object literal, got {:?}", value),
                self.context.get_current_scope(),
            ));
        }

        self.context.log_debug(&format!("  Object: {}", name));

        Ok(DataEntry::ObjectProperty {
            name,
            data_type: None,
            object: Box::new(value),
            position: Position::UNKNOWN,
        })
    }
}
