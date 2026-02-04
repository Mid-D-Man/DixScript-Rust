//! Writes @DATA section to binary format (MOST COMPLEX)

use std::io::{Write, Seek, SeekFrom};
use crate::Compiler::AST::{DataSection, DataEntry, TablePath, PropertyAssignment, Value};
use crate::ErrorManager::ErrorManager;
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_encoder::ValueEncoder;

/// Writes @DATA section to binary format (MOST COMPLEX)
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
///
/// Entry Types:
/// 1. Simple Property: [Type: 1][Name Length: 4][Name UTF-8][Value]
/// 2. Table Property: [Type: 2][Path Length: 4][Path UTF-8][Property Count: 4][Properties...]
/// 3. Group Array: [Type: 3][Path Length: 4][Path UTF-8][Item Count: 4][Items...]
/// 4. Object Property: [Type: 4][Name Length: 4][Name UTF-8][Object Value]
pub struct DataSectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
    value_encoder: &'a mut ValueEncoder<'a>,
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

impl<'a> DataSectionWriter<'a> {
    /// Create new data section writer
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_encoder: &'a mut ValueEncoder<'a>,
    ) -> Self {
        DataSectionWriter {
            context,
            value_encoder,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

    /// Write @DATA section to binary
    /// Returns offset information for offset table
    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        data_section: &DataSection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        self.context.log_info(&format!(
            "Writing @DATA section ({} entries)",
            data_section.entries.len()
        ));

        let start_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?
            as i32;

        // Write section header
        writer.write_all(&(SectionId::Data as u32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;

        // Placeholder for section length
        let length_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;
        writer.write_all(&0i32.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;

        // Write entry count
        writer.write_all(&(data_section.entries.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;

        // Write each data entry
        for entry in &data_section.entries {
            self.write_data_entry(writer, entry)?;
        }

        // Calculate and update section length
        let end_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?
            as i32;
        let section_length = end_position - start_position - 8;

        writer.seek(SeekFrom::Start(length_position))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;
        writer.write_all(&section_length.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;
        writer.seek(SeekFrom::Start(end_position as u64))
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;

        self.context.log_info(&format!("✅ @DATA section written: {} bytes", section_length));
        self.context.statistics.record_section_size(SectionId::Data, section_length as usize);

        Ok(SectionOffset::new(
            SectionId::Data,
            start_position,
            end_position - start_position,
        ))
    }

    /// Write any data entry (dispatches to specific writer)
    fn write_data_entry<W: Write>(
        &mut self,
        writer: &mut W,
        entry: &DataEntry,
    ) -> Result<(), BinarySerializationError> {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                self.write_simple_property(writer, name, value)
            }
            DataEntry::TableProperty { path, properties, .. } => {
                self.write_table_property(writer, path, properties)
            }
            DataEntry::GroupArray { path, items, .. } => {
                self.write_group_array(writer, path, items)
            }
            DataEntry::ObjectProperty { name, object, .. } => {
                self.write_object_property(writer, name, object)
            }
        }
    }

    /// Write Simple Property: [Type: 1][Name Length: 4][Name UTF-8][Value]
    /// Example: app_name = "MyApp"
    fn write_simple_property<W: Write>(
        &mut self,
        writer: &mut W,
        name: &str,
        value: &Value,
    ) -> Result<(), BinarySerializationError> {
        // Write type tag
        writer.write_all(&[DataEntryType::SimpleProperty as u8])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SimpleProperty"))?;

        // Write property name
        let name_bytes = name.as_bytes();
        writer.write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SimpleProperty"))?;
        writer.write_all(name_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SimpleProperty"))?;

        // Write value
        self.value_encoder.encode_value(writer, value)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SimpleProperty"))?;

        self.context.log_debug(&format!("  Simple: {} = {}", name, value));

        Ok(())
    }

    /// Write Table Property: [Type: 2][Path Length: 4][Path UTF-8][Property Count: 4][Properties...]
    /// Example: server.config: host = "localhost", port = 8080
    fn write_table_property<W: Write>(
        &mut self,
        writer: &mut W,
        path: &TablePath,
        properties: &[PropertyAssignment],
    ) -> Result<(), BinarySerializationError> {
        // Write type tag
        writer.write_all(&[DataEntryType::TableProperty as u8])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "TableProperty"))?;

        // Write table path
        let path_str = path.segments.join(".");
        let path_bytes = path_str.as_bytes();
        writer.write_all(&(path_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "TableProperty"))?;
        writer.write_all(path_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "TableProperty"))?;

        // Write property count
        writer.write_all(&(properties.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "TableProperty"))?;

        self.context.log_debug(&format!(
            "  Table: {} ({} properties)",
            path_str,
            properties.len()
        ));

        // Write each property assignment
        for prop in properties {
            self.write_property_assignment(writer, prop)?;
        }

        Ok(())
    }

    /// Write Property Assignment: [Name Length: 4][Name UTF-8][Value]
    fn write_property_assignment<W: Write>(
        &mut self,
        writer: &mut W,
        prop: &PropertyAssignment,
    ) -> Result<(), BinarySerializationError> {
        // Write property name
        let name_bytes = prop.name.as_bytes();
        writer.write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "PropertyAssignment"))?;
        writer.write_all(name_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "PropertyAssignment"))?;

        // Write value
        self.value_encoder.encode_value(writer, &prop.value)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "PropertyAssignment"))?;

        self.context.log_debug(&format!("    Property: {} = {}", prop.name, prop.value));

        Ok(())
    }

    /// Write Group Array: [Type: 3][Path Length: 4][Path UTF-8][Item Count: 4][Items...]
    /// Example: users.admins:: { name = "Alice" }, { name = "Bob" }
    fn write_group_array<W: Write>(
        &mut self,
        writer: &mut W,
        path: &TablePath,
        items: &[Value],
    ) -> Result<(), BinarySerializationError> {
        // Write type tag
        writer.write_all(&[DataEntryType::GroupArray as u8])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;

        // Write array path
        let path_str = path.segments.join(".");
        let path_bytes = path_str.as_bytes();
        writer.write_all(&(path_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;
        writer.write_all(path_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;

        // Write item count
        writer.write_all(&(items.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;

        self.context.log_debug(&format!("  Array: {} ({} items)", path_str, items.len()));

        // Write each item
        for item in items {
            self.value_encoder.encode_value(writer, item)
                .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;
        }

        Ok(())
    }

    /// Write Object Property: [Type: 4][Name Length: 4][Name UTF-8][Object Value]
    /// Example: config = { timeout = 30, retries = 3 }
    fn write_object_property<W: Write>(
        &mut self,
        writer: &mut W,
        name: &str,
        object: &Value,
    ) -> Result<(), BinarySerializationError> {
        // Write type tag
        writer.write_all(&[DataEntryType::ObjectProperty as u8])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ObjectProperty"))?;

        // Write property name
        let name_bytes = name.as_bytes();
        writer.write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ObjectProperty"))?;
        writer.write_all(name_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ObjectProperty"))?;

        // Verify it's an object
        if !matches!(object.as_ref(), Value::Object { .. }) {
            return Err(BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                format!("Expected object literal, got {:?}", object),
                self.context.get_current_scope(),
            ));
        }

        // Write object value (ValueEncoder handles ObjectLiteral)
        self.value_encoder.encode_value(writer, object)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ObjectProperty"))?;

        self.context.log_debug(&format!("  Object: {}", name));

        Ok(())
    }
}
