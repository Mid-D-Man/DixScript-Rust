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
    value_encoder: &'a mut ValueEncoder,
    error_manager: ErrorManager,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataEntryType {
    SimpleProperty = 0x01,
    TableProperty = 0x02,
    GroupArray = 0x03,
    ObjectProperty = 0x04,
}

impl<'a> DataSectionWriter<'a> {
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_encoder: &'a mut ValueEncoder,
    ) -> Self {
        DataSectionWriter {
            context,
            value_encoder,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }

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

        writer.write_all(&(SectionId::Data as u32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;

        let length_position = writer.stream_position()
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;
        writer.write_all(&0i32.to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;

        writer.write_all(&(data_section.entries.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "DataSection"))?;

        for entry in &data_section.entries {
            self.write_data_entry(writer, entry)?;
        }

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

    fn write_simple_property<W: Write>(
        &mut self,
        writer: &mut W,
        name: &str,
        value: &Value,
    ) -> Result<(), BinarySerializationError> {
        writer.write_all(&[DataEntryType::SimpleProperty as u8])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SimpleProperty"))?;

        let name_bytes = name.as_bytes();
        writer.write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SimpleProperty"))?;
        writer.write_all(name_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SimpleProperty"))?;

        self.value_encoder.encode_value(writer, value, self.context)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "SimpleProperty"))?;

        self.context.log_debug(&format!("  Simple: {} = {}", name, value));

        Ok(())
    }

    fn write_table_property<W: Write>(
        &mut self,
        writer: &mut W,
        path: &TablePath,
        properties: &[PropertyAssignment],
    ) -> Result<(), BinarySerializationError> {
        writer.write_all(&[DataEntryType::TableProperty as u8])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "TableProperty"))?;

        let path_str = path.segments.join(".");
        let path_bytes = path_str.as_bytes();
        writer.write_all(&(path_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "TableProperty"))?;
        writer.write_all(path_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "TableProperty"))?;

        writer.write_all(&(properties.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "TableProperty"))?;

        self.context.log_debug(&format!(
            "  Table: {} ({} properties)",
            path_str,
            properties.len()
        ));

        for prop in properties {
            self.write_property_assignment(writer, prop)?;
        }

        Ok(())
    }

    fn write_property_assignment<W: Write>(
        &mut self,
        writer: &mut W,
        prop: &PropertyAssignment,
    ) -> Result<(), BinarySerializationError> {
        let name_bytes = prop.name.as_bytes();
        writer.write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "PropertyAssignment"))?;
        writer.write_all(name_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "PropertyAssignment"))?;

        self.value_encoder.encode_value(writer, &prop.value, self.context)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "PropertyAssignment"))?;

        self.context.log_debug(&format!("    Property: {} = {}", prop.name, prop.value));

        Ok(())
    }

    fn write_group_array<W: Write>(
        &mut self,
        writer: &mut W,
        path: &TablePath,
        items: &[Value],
    ) -> Result<(), BinarySerializationError> {
        writer.write_all(&[DataEntryType::GroupArray as u8])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;

        let path_str = path.segments.join(".");
        let path_bytes = path_str.as_bytes();
        writer.write_all(&(path_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;
        writer.write_all(path_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;

        writer.write_all(&(items.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;

        self.context.log_debug(&format!("  Array: {} ({} items)", path_str, items.len()));

        for item in items {
            self.value_encoder.encode_value(writer, item, self.context)
                .map_err(|e| BinarySerializationError::write_error(e.to_string(), "GroupArray"))?;
        }

        Ok(())
    }

    fn write_object_property<W: Write>(
        &mut self,
        writer: &mut W,
        name: &str,
        object: &Value,
    ) -> Result<(), BinarySerializationError> {
        writer.write_all(&[DataEntryType::ObjectProperty as u8])
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ObjectProperty"))?;

        let name_bytes = name.as_bytes();
        writer.write_all(&(name_bytes.len() as i32).to_le_bytes())
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ObjectProperty"))?;
        writer.write_all(name_bytes)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ObjectProperty"))?;

        if !matches!(object, Value::Object { .. }) {
            return Err(BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                format!("Expected object literal, got {:?}", object),
                self.context.get_current_scope(),
            ));
        }

        self.value_encoder.encode_value(writer, object, self.context)
            .map_err(|e| BinarySerializationError::write_error(e.to_string(), "ObjectProperty"))?;

        self.context.log_debug(&format!("  Object: {}", name));

        Ok(())
    }
}