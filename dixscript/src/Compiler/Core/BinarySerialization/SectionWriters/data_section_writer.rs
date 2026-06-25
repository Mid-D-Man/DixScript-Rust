//! Writes @DATA section to binary format.

use std::io::{Write, Seek, SeekFrom};
use crate::Compiler::AST::{DataSection, DataEntry, TablePath, PropertyAssignment, Value};
use super::binary_format::SectionId;
use super::section_offset::SectionOffset;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;
use super::value_encoder::ValueEncoder;

/// Entry type discriminants — must match reader.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataEntryType {
    SimpleProperty = 0x01,
    TableProperty  = 0x02,
    GroupArray     = 0x03,
    ObjectProperty = 0x04,
}

/// Writes @DATA section to binary format.
/// Format: [Section ID: 4][Section Length: 4][Entry Count: 4][Entries...]
///
/// Entry layout by type:
/// 1. Simple:  [Type: 1][Name Length: 4][Name UTF-8][Value]
/// 2. Table:   [Type: 1][Path Length: 4][Path UTF-8][Property Count: 4][Properties...]
/// 3. Array:   [Type: 1][Path Length: 4][Path UTF-8][Item Count: 4][Items...]
/// 4. Object:  [Type: 1][Name Length: 4][Name UTF-8][Object Value]
pub struct DataSectionWriter<'a> {
    context: &'a mut BinarySerializationContext,
    value_encoder: &'a mut ValueEncoder,
}

impl<'a> DataSectionWriter<'a> {
    pub fn new(
        context: &'a mut BinarySerializationContext,
        value_encoder: &'a mut ValueEncoder,
    ) -> Self {
        DataSectionWriter { context, value_encoder }
    }

    pub fn write_section<W: Write + Seek>(
        &mut self,
        writer: &mut W,
        data_section: &DataSection,
    ) -> Result<SectionOffset, BinarySerializationError> {
        if self.context.debug_config.is_enabled {
            self.context.log_info(&format!(
                "Writing @DATA section ({} entries)",
                data_section.entries.len()
            ));
        }

        let start_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "DataSection"))?
            as i32;

        writer
            .write_all(&(SectionId::Data as u32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "DataSection"))?;

        let length_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "DataSection"))?;
        writer
            .write_all(&0i32.to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "DataSection"))?;

        writer
            .write_all(&(data_section.entries.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "DataSection"))?;

        for entry in &data_section.entries {
            self.write_data_entry(writer, entry)?;
            if self.context.error_manager.should_terminate_parsing() {
                return Err(BinarySerializationError::invalid_state(
                    "Terminating DATA write due to accumulated errors",
                    "DataSection",
                ));
            }
        }

        let end_pos = writer
            .stream_position()
            .map_err(|e| self.write_err(e.to_string(), "DataSection"))?
            as i32;
        let section_length = end_pos - start_pos;

        writer
            .seek(SeekFrom::Start(length_pos))
            .map_err(|e| self.write_err(e.to_string(), "DataSection"))?;
        writer
            .write_all(&section_length.to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "DataSection"))?;
        writer
            .seek(SeekFrom::Start(end_pos as u64))
            .map_err(|e| self.write_err(e.to_string(), "DataSection"))?;

        if self.context.debug_config.is_enabled {
            self.context
                .log_info(&format!("@DATA written: {} bytes", section_length));
        }

        self.context
            .statistics
            .record_section_size(SectionId::Data, section_length as usize);

        Ok(SectionOffset::new(SectionId::Data, start_pos, end_pos - start_pos))
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
        writer
            .write_all(&[DataEntryType::SimpleProperty as u8])
            .map_err(|e| self.write_err(e.to_string(), "SimpleProperty"))?;

        self.write_string_field(writer, name, "SimpleProperty")?;

        self.value_encoder
            .encode_value(writer, value, self.context)
            .map_err(|e| self.write_err(e.to_string(), "SimpleProperty"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  simple: {} = {}", name, value));
        }

        Ok(())
    }

    fn write_table_property<W: Write>(
        &mut self,
        writer: &mut W,
        path: &TablePath,
        properties: &[PropertyAssignment],
    ) -> Result<(), BinarySerializationError> {
        writer
            .write_all(&[DataEntryType::TableProperty as u8])
            .map_err(|e| self.write_err(e.to_string(), "TableProperty"))?;

        let path_str = path.segments.join(".");
        self.write_string_field(writer, &path_str, "TableProperty")?;

        writer
            .write_all(&(properties.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "TableProperty"))?;

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  table: {} ({} properties)",
                path_str,
                properties.len()
            ));
        }

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
        self.write_string_field(writer, &prop.name, "PropertyAssignment")?;

        self.value_encoder
            .encode_value(writer, &prop.value, self.context)
            .map_err(|e| self.write_err(e.to_string(), "PropertyAssignment"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("    property: {} = {}", prop.name, prop.value));
        }

        Ok(())
    }

    fn write_group_array<W: Write>(
        &mut self,
        writer: &mut W,
        path: &TablePath,
        items: &[Value],
    ) -> Result<(), BinarySerializationError> {
        writer
            .write_all(&[DataEntryType::GroupArray as u8])
            .map_err(|e| self.write_err(e.to_string(), "GroupArray"))?;

        let path_str = path.segments.join(".");
        self.write_string_field(writer, &path_str, "GroupArray")?;

        writer
            .write_all(&(items.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), "GroupArray"))?;

        if self.context.debug_config.is_verbose {
            self.context.log_verbose(&format!(
                "  group array: {} ({} items)",
                path_str,
                items.len()
            ));
        }

        for item in items {
            self.value_encoder
                .encode_value(writer, item, self.context)
                .map_err(|e| self.write_err(e.to_string(), "GroupArray"))?;
        }

        Ok(())
    }

    fn write_object_property<W: Write>(
        &mut self,
        writer: &mut W,
        name: &str,
        object: &Value,
    ) -> Result<(), BinarySerializationError> {
        if !matches!(object, Value::Object { .. }) {
            let e = BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                format!("Expected object literal for property '{}', got {:?}", name, object),
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

        writer
            .write_all(&[DataEntryType::ObjectProperty as u8])
            .map_err(|e| self.write_err(e.to_string(), "ObjectProperty"))?;

        self.write_string_field(writer, name, "ObjectProperty")?;

        self.value_encoder
            .encode_value(writer, object, self.context)
            .map_err(|e| self.write_err(e.to_string(), "ObjectProperty"))?;

        if self.context.debug_config.is_verbose {
            self.context
                .log_verbose(&format!("  object property: {}", name));
        }

        Ok(())
    }

    /// Write a length-prefixed UTF-8 string field (no type tag).
    fn write_string_field<W: Write>(
        &mut self,
        writer: &mut W,
        value: &str,
        location: &str,
    ) -> Result<(), BinarySerializationError> {
        let bytes = value.as_bytes();
        writer
            .write_all(&(bytes.len() as i32).to_le_bytes())
            .map_err(|e| self.write_err(e.to_string(), location))?;
        writer
            .write_all(bytes)
            .map_err(|e| self.write_err(e.to_string(), location))?;
        Ok(())
    }

    fn write_err(&self, message: String, location: &str) -> BinarySerializationError {
        let e = BinarySerializationError::write_error(message, location);
        self.context
            .error_manager
            .add_binary_serialization_error(e.error_type, e.message.clone(), None, None, None, None);
        e
    }
}
