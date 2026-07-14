//! Encodes DixScript AST values to binary format

use std::io::{Write, Result as IoResult};
use crate::Compiler::AST::{Value, DataType};
use crate::ErrorManager::ErrorManager;
use super::binary_format::{ValueTypeTag, BlobEncoding, MAX_STRING_LENGTH};
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Encodes AST values to binary format with type tags.
/// Context is passed per-call rather than stored — safe to use from parallel tasks.
pub struct ValueEncoder {
    error_manager: ErrorManager,
}

impl ValueEncoder {
    pub fn new() -> Self {
        Self::new_with_error_manager(ErrorManager::get_shared_instance())
    }

    pub fn new_with_error_manager(error_manager: ErrorManager) -> Self {
        ValueEncoder { error_manager }
    }

    // =========================================================================
    // MAIN ENCODE ENTRY POINT
    // =========================================================================

    /// Encode any AST value to binary.
    /// Format: [Type Tag: 1 byte][Value Data: variable]
    pub fn encode_value<W: Write>(
        &mut self,
        writer:  &mut W,
        value:   &Value,
        context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        match value {
            Value::Integer { value, .. }            => self.encode_int32(writer, *value, context),
            Value::Long { value, .. }               => self.encode_int64(writer, *value, context),
            Value::Float { value, .. }              => self.encode_float32(writer, *value, context),
            Value::Double { value, .. }             => self.encode_float64(writer, *value, context),
            Value::ScientificNotation { value, .. } => self.encode_float64(writer, *value, context),
            Value::String { value, .. }             => self.encode_string(writer, value, context),
            Value::Boolean { value, .. }            => self.encode_bool(writer, *value, context),
            Value::Null { .. }                      => self.encode_null(writer, context),
            Value::Array { values, .. }             => self.encode_array(writer, values, context),
            Value::Object { properties, .. }        => self.encode_object(writer, properties, context),
            Value::HexColor { value, .. }           => self.encode_hex(writer, value, context),
            Value::Date { value, .. }               => self.encode_date(writer, value, context),
            Value::Timestamp { value, .. }          => self.encode_timestamp(writer, value, context),

            // Prefixed constructors
            Value::PrefixedConstructor { prefix, arguments, position } => {
                match prefix.as_str() {
                    "t" => self.encode_tuple(writer, arguments, context),
                    "b" => self.encode_blob(writer, arguments, context),
                    "r" => self.encode_regex(writer, arguments, context),
                    _ => {
                        let err = BinarySerializationError::with_position(
                            crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::UnsupportedType,
                            format!("Unsupported prefixed constructor: {}", prefix),
                            context.get_current_scope(),
                            *position,
                        );
                        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
                    }
                }
            }

            // EnumValue should be resolved to integers before serialization
            Value::EnumValue { enum_name, value, .. } => {
                self.error_manager.log_warning(&format!(
                    "EnumValue {}.{} encountered during serialization — should be resolved",
                    enum_name, value
                ));
                self.encode_int32(writer, 0, context)
            }

            _ => {
                let err = BinarySerializationError::new(
                    crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::UnsupportedType,
                    format!("Unsupported value type for serialization: {:?}", value),
                    context.get_current_scope(),
                );
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
            }
        }?;

        context.statistics.increment_value_count(self.get_type_tag_for_value(value));
        Ok(())
    }

    // =========================================================================
    // PRIMITIVE TYPE ENCODERS
    // =========================================================================

    /// Encode Int32: [0x01][4 bytes little-endian]
    fn encode_int32<W: Write>(
        &mut self,
        writer:   &mut W,
        value:    i32,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        writer.write_all(&[ValueTypeTag::Int32 as u8])?;
        writer.write_all(&value.to_le_bytes())?;
        Ok(())
    }

    /// Encode Int64 (Long): [0x02][8 bytes little-endian]
    fn encode_int64<W: Write>(
        &mut self,
        writer:   &mut W,
        value:    i64,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        writer.write_all(&[ValueTypeTag::Int64 as u8])?;
        writer.write_all(&value.to_le_bytes())?;
        Ok(())
    }

    /// Encode Float32: [0x03][4 bytes IEEE 754]
    fn encode_float32<W: Write>(
        &mut self,
        writer:   &mut W,
        value:    f32,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        writer.write_all(&[ValueTypeTag::Float32 as u8])?;
        writer.write_all(&value.to_le_bytes())?;
        Ok(())
    }

    /// Encode Float64: [0x04][8 bytes IEEE 754]
    fn encode_float64<W: Write>(
        &mut self,
        writer:   &mut W,
        value:    f64,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        writer.write_all(&[ValueTypeTag::Float64 as u8])?;
        writer.write_all(&value.to_le_bytes())?;
        Ok(())
    }

    /// Encode String: [0x05][Length: 4 bytes][UTF-8 bytes]
    fn encode_string<W: Write>(
        &mut self,
        writer:  &mut W,
        value:   &str,
        context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        let bytes  = value.as_bytes();
        let length = bytes.len();
        context.validate_string_length(length)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writer.write_all(&[ValueTypeTag::String as u8])?;
        writer.write_all(&(length as i32).to_le_bytes())?;
        writer.write_all(bytes)?;
        Ok(())
    }

    /// Encode Bool: [0x06][1 byte: 0x00 or 0x01]
    fn encode_bool<W: Write>(
        &mut self,
        writer:   &mut W,
        value:    bool,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        writer.write_all(&[ValueTypeTag::Bool as u8])?;
        writer.write_all(&[if value { 0x01 } else { 0x00 }])?;
        Ok(())
    }

    /// Encode Null: [0x07]
    fn encode_null<W: Write>(
        &mut self,
        writer:   &mut W,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        writer.write_all(&[ValueTypeTag::Null as u8])?;
        Ok(())
    }

    // =========================================================================
    // COMPLEX TYPE ENCODERS
    // =========================================================================

    /// Encode Array: [0x08][Count: 4 bytes][Element Type: 1 byte][Values...]
    fn encode_array<W: Write>(
        &mut self,
        writer:  &mut W,
        values:  &[Value],
        context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        context.enter_nested("Array")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let count = values.len();
        context.validate_array_length(count)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        writer.write_all(&[ValueTypeTag::Array as u8])?;
        writer.write_all(&(count as i32).to_le_bytes())?;

        let element_type = if !values.is_empty() {
            self.get_type_tag_for_value(&values[0])
        } else {
            ValueTypeTag::Null
        };
        writer.write_all(&[element_type as u8])?;

        for value in values {
            self.encode_value(writer, value, context)?;
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(())
    }

    /// Encode Object: [0x09][Count: 4 bytes][Key-Value pairs...]
    /// Format per pair: [Key Length: 4][Key UTF-8][Value]
    fn encode_object<W: Write>(
        &mut self,
        writer:     &mut W,
        properties: &[crate::Compiler::AST::ObjectProperty],
        context:    &mut BinarySerializationContext,
    ) -> IoResult<()> {
        context.enter_nested("Object")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let count = properties.len();
        context.validate_object_property_count(count)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        writer.write_all(&[ValueTypeTag::Object as u8])?;
        writer.write_all(&(count as i32).to_le_bytes())?;

        for prop in properties {
            let key_bytes = prop.key.as_bytes();
            writer.write_all(&(key_bytes.len() as i32).to_le_bytes())?;
            writer.write_all(key_bytes)?;
            self.encode_value(writer, &prop.value, context)?;
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(())
    }

    /// Encode Tuple: [0x0D][Count: 1 byte (1-6)][Values...]
    fn encode_tuple<W: Write>(
        &mut self,
        writer:  &mut W,
        values:  &[Value],
        context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        context.enter_nested("Tuple")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let count = values.len();
        if count < 1 || count > 6 {
            let err = BinarySerializationError::new(
                crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::InvalidFormat,
                format!("Tuple must have 1-6 elements, got {}", count),
                context.get_current_scope(),
            );
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, err));
        }

        writer.write_all(&[ValueTypeTag::Tuple as u8])?;
        writer.write_all(&[count as u8])?;

        for value in values {
            self.encode_value(writer, value, context)?;
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(())
    }

    // =========================================================================
    // TEMPORAL TYPE ENCODERS
    // =========================================================================

    /// Encode Date: [0x0A][8 bytes ticks since epoch]
    fn encode_date<W: Write>(
        &mut self,
        writer:   &mut W,
        date_str: &str,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        use chrono::NaiveDate;
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(date_str, "%Y/%m/%d"))
            .map_err(|e| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid date format: {}", e),
            ))?;
        let ticks = date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() * 10_000_000;
        writer.write_all(&[ValueTypeTag::Date as u8])?;
        writer.write_all(&ticks.to_le_bytes())?;
        Ok(())
    }

    /// Encode Timestamp: [0x0B][8 bytes ticks since epoch]
    fn encode_timestamp<W: Write>(
        &mut self,
        writer:         &mut W,
        timestamp_str:  &str,
        _context:       &mut BinarySerializationContext,
    ) -> IoResult<()> {
        use chrono::DateTime;
        let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
            .or_else(|_| DateTime::parse_from_str(timestamp_str, "%Y-%m-%dT%H:%M:%S%.fZ"))
            .map_err(|e| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid timestamp format: {}", e),
            ))?;
        let ticks = timestamp.timestamp() * 10_000_000;
        writer.write_all(&[ValueTypeTag::Timestamp as u8])?;
        writer.write_all(&ticks.to_le_bytes())?;
        Ok(())
    }

    // =========================================================================
    // SPECIAL TYPE ENCODERS
    // =========================================================================

    /// Encode Hex Color: [0x0C][4 bytes RGBA]
    fn encode_hex<W: Write>(
        &mut self,
        writer:   &mut W,
        hex_str:  &str,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<()> {
        let hex_str = hex_str.strip_prefix('#').unwrap_or(hex_str);
        let (r, g, b, a) = match hex_str.len() {
            3 => {
                let r = u8::from_str_radix(&hex_str[0..1].repeat(2), 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let g = u8::from_str_radix(&hex_str[1..2].repeat(2), 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let b = u8::from_str_radix(&hex_str[2..3].repeat(2), 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                (r, g, b, 255u8)
            }
            4 => {
                let r = u8::from_str_radix(&hex_str[0..1].repeat(2), 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let g = u8::from_str_radix(&hex_str[1..2].repeat(2), 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let b = u8::from_str_radix(&hex_str[2..3].repeat(2), 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let a = u8::from_str_radix(&hex_str[3..4].repeat(2), 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                (r, g, b, a)
            }
            6 => {
                let r = u8::from_str_radix(&hex_str[0..2], 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let g = u8::from_str_radix(&hex_str[2..4], 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let b = u8::from_str_radix(&hex_str[4..6], 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                (r, g, b, 255u8)
            }
            8 => {
                let r = u8::from_str_radix(&hex_str[0..2], 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let g = u8::from_str_radix(&hex_str[2..4], 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let b = u8::from_str_radix(&hex_str[4..6], 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let a = u8::from_str_radix(&hex_str[6..8], 16)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                (r, g, b, a)
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid hex color length: {}", hex_str.len()),
                ));
            }
        };
        writer.write_all(&[ValueTypeTag::Hex as u8])?;
        writer.write_all(&[r, g, b, a])?;
        Ok(())
    }

    /// Encode Blob: [0x0E][Encoding: 1][Length: 4][Data bytes]
    fn encode_blob<W: Write>(
        &mut self,
        writer:    &mut W,
        arguments: &[Value],
        context:   &mut BinarySerializationContext,
    ) -> IoResult<()> {
        let data = if let Some(Value::String { value, .. }) = arguments.first() {
            value.as_str()
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Blob constructor requires string argument",
            ));
        };
        let encoding = BlobEncoding::detect(data);
        if !encoding.validate(data) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid blob data for encoding {:?}", encoding),
            ));
        }
        writer.write_all(&[ValueTypeTag::Blob as u8])?;
        writer.write_all(&[encoding as u8])?;
        let bytes = data.as_bytes();
        context.validate_string_length(bytes.len())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writer.write_all(&(bytes.len() as i32).to_le_bytes())?;
        writer.write_all(bytes)?;
        Ok(())
    }

    /// Encode Regex: [0x0F][Length: 4][Pattern UTF-8 bytes]
    fn encode_regex<W: Write>(
        &mut self,
        writer:    &mut W,
        arguments: &[Value],
        context:   &mut BinarySerializationContext,
    ) -> IoResult<()> {
        let pattern = if let Some(Value::String { value, .. }) = arguments.first() {
            value.as_str()
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Regex constructor requires string argument",
            ));
        };
        regex::Regex::new(pattern).map_err(|e| std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Invalid regex pattern: {}", e),
        ))?;
        writer.write_all(&[ValueTypeTag::Regex as u8])?;
        let bytes = pattern.as_bytes();
        context.validate_string_length(bytes.len())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        writer.write_all(&(bytes.len() as i32).to_le_bytes())?;
        writer.write_all(bytes)?;
        Ok(())
    }

    // =========================================================================
    // HELPER METHODS
    // =========================================================================

    fn get_type_tag_for_value(&self, value: &Value) -> ValueTypeTag {
        match value {
            Value::Integer { .. }            => ValueTypeTag::Int32,
            Value::Long { .. }               => ValueTypeTag::Int64,
            Value::Float { .. }              => ValueTypeTag::Float32,
            Value::Double { .. }
            | Value::ScientificNotation { .. } => ValueTypeTag::Float64,
            Value::String { .. }             => ValueTypeTag::String,
            Value::Boolean { .. }            => ValueTypeTag::Bool,
            Value::Null { .. }               => ValueTypeTag::Null,
            Value::Array { .. }              => ValueTypeTag::Array,
            Value::Object { .. }             => ValueTypeTag::Object,
            Value::HexColor { .. }           => ValueTypeTag::Hex,
            Value::Date { .. }               => ValueTypeTag::Date,
            Value::Timestamp { .. }          => ValueTypeTag::Timestamp,
            Value::PrefixedConstructor { prefix, .. } => match prefix.as_str() {
                "t" => ValueTypeTag::Tuple,
                "b" => ValueTypeTag::Blob,
                "r" => ValueTypeTag::Regex,
                _   => ValueTypeTag::Invalid,
            },
            _ => ValueTypeTag::Invalid,
        }
    }
}

impl Default for ValueEncoder {
    fn default() -> Self { Self::new() }
                            }
