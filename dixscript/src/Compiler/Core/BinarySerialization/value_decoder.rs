//! Decodes binary format to DixScript AST values

use std::io::{Read, Result as IoResult};
use crate::Compiler::AST::{Value, Position, ObjectProperty};
use crate::ErrorManager::ErrorManager;
use super::binary_format::ValueTypeTag;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Decodes binary format to AST values with type tag validation
/// NOTE: Does NOT store context - context must be passed to each method
pub struct ValueDecoder {
    error_manager: ErrorManager,
}

impl ValueDecoder {
    /// Create new value decoder
    pub fn new() -> Self {

           Self::new_with_error_manager(ErrorManager::get_shared_instance())

    }
    pub fn new_with_error_manager(_error_manager:ErrorManager) -> Self {
        ValueDecoder {
            error_manager: _error_manager,
        }
    }
    // ==================== MAIN DECODE ENTRY POINT ====================

    /// Decode any value from binary
    /// Format: [Type Tag: 1 byte] [Value Data: variable]
    pub fn decode_value<R: Read>(
        &mut self,
        reader: &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        // Read type tag
        let mut tag_buf = [0u8; 1];
        reader.read_exact(&mut tag_buf)?;
        let type_tag = ValueTypeTag::from_u8(tag_buf[0]).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                BinarySerializationError::invalid_type_tag(tag_buf[0], context.get_current_scope()),
            )
        })?;

        // Decode based on type tag
        let value = match type_tag {
            ValueTypeTag::Int32 => self.decode_int32(reader, context)?,
            ValueTypeTag::Float32 => self.decode_float32(reader, context)?,
            ValueTypeTag::Float64 => self.decode_float64(reader, context)?,
            ValueTypeTag::String => self.decode_string(reader, context)?,
            ValueTypeTag::Bool => self.decode_bool(reader, context)?,
            ValueTypeTag::Null => self.decode_null(context)?,
            ValueTypeTag::Array => self.decode_array(reader, context)?,
            ValueTypeTag::Object => self.decode_object(reader, context)?,
            ValueTypeTag::Tuple => self.decode_tuple(reader, context)?,
            ValueTypeTag::Date => self.decode_date(reader, context)?,
            ValueTypeTag::Timestamp => self.decode_timestamp(reader, context)?,
            ValueTypeTag::Hex => self.decode_hex(reader, context)?,
            ValueTypeTag::Blob => self.decode_blob(reader, context)?,
            ValueTypeTag::Regex => self.decode_regex(reader, context)?,
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    BinarySerializationError::new(
                        crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::UnsupportedType,
                        format!("Unsupported type tag: {:?}", type_tag),
                        context.get_current_scope(),
                    ),
                ));
            }
        };

        context.statistics.increment_value_count(type_tag);
        Ok(value)
    }

    // ==================== PRIMITIVE TYPE DECODERS ====================

    /// Decode Int32: [4 bytes little-endian]
    fn decode_int32<R: Read>(
        &mut self,
        reader: &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        let value = i32::from_le_bytes(buf);
        Ok(Value::Integer {
            value,
            position: Position::UNKNOWN,
        })
    }

    /// Decode Float32: [4 bytes IEEE 754]
    fn decode_float32<R: Read>(
        &mut self,
        reader: &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        let value = f32::from_le_bytes(buf);
        Ok(Value::Float {
            value,
            position: Position::UNKNOWN,
        })
    }

    /// Decode Float64: [8 bytes IEEE 754]
    fn decode_float64<R: Read>(
        &mut self,
        reader: &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        let value = f64::from_le_bytes(buf);
        Ok(Value::Double {
            value,
            position: Position::UNKNOWN,
        })
    }

    /// Decode String: [Length: 4 bytes][UTF-8 bytes]
    fn decode_string<R: Read>(
        &mut self,
        reader: &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let length = i32::from_le_bytes(len_buf) as usize;

        context.validate_string_length(length)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut bytes = vec![0u8; length];
        reader.read_exact(&mut bytes)?;

        let value = String::from_utf8(bytes).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;

        Ok(Value::String {
            value,
            position: Position::UNKNOWN,
        })
    }

    /// Decode Bool: [1 byte: 0x00 or 0x01]
    fn decode_bool<R: Read>(
        &mut self,
        reader: &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        let value = buf[0] != 0x00;
        Ok(Value::Boolean {
            value,
            position: Position::UNKNOWN,
        })
    }

    /// Decode Null: (no data)
    fn decode_null(&mut self, _context: &mut BinarySerializationContext) -> IoResult<Value> {
        Ok(Value::Null {
            position: Position::UNKNOWN,
        })
    }

    // ==================== COMPLEX TYPE DECODERS ====================

    /// Decode Array: [Count: 4 bytes][Element Type: 1 byte][Values...]
    fn decode_array<R: Read>(
        &mut self,
        reader: &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        context.enter_nested("Array")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)?;
        let count = i32::from_le_bytes(count_buf) as usize;

        context.validate_array_length(count)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Read element type (not used for decoding, just metadata)
        let mut _type_buf = [0u8; 1];
        reader.read_exact(&mut _type_buf)?;

        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.decode_value(reader, context)?);
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(Value::Array {
            values,
            position: Position::UNKNOWN,
        })
    }

    /// Decode Object: [Count: 4 bytes][Key-Value pairs...]
    fn decode_object<R: Read>(
        &mut self,
        reader: &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        context.enter_nested("Object")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)?;
        let count = i32::from_le_bytes(count_buf) as usize;

        context.validate_object_property_count(count)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut properties = Vec::with_capacity(count);
        for _ in 0..count {
            // Read key length
            let mut key_len_buf = [0u8; 4];
            reader.read_exact(&mut key_len_buf)?;
            let key_length = i32::from_le_bytes(key_len_buf) as usize;

            // Read key
            let mut key_bytes = vec![0u8; key_length];
            reader.read_exact(&mut key_bytes)?;
            let key = String::from_utf8(key_bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            // Read value
            let value = self.decode_value(reader, context)?;

            properties.push(ObjectProperty::new(key, value, Position::UNKNOWN));
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(Value::Object {
            properties,
            position: Position::UNKNOWN,
        })
    }

    /// Decode Tuple: [Count: 1 byte (1-6)][Values...]
    fn decode_tuple<R: Read>(
        &mut self,
        reader: &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        context.enter_nested("Tuple")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut count_buf = [0u8; 1];
        reader.read_exact(&mut count_buf)?;
        let count = count_buf[0] as usize;

        if count < 1 || count > 6 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid tuple count: {} (must be 1-6)", count),
            ));
        }

        let mut arguments = Vec::with_capacity(count);
        for _ in 0..count {
            arguments.push(self.decode_value(reader, context)?);
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(Value::PrefixedConstructor {
            prefix: "t".to_string(),
            arguments,
            position: Position::UNKNOWN,
        })
    }

    // ==================== TEMPORAL TYPE DECODERS ====================

    /// Decode Date: [8 bytes ticks since epoch]
    fn decode_date<R: Read>(
        &mut self,
        reader: &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        let ticks = i64::from_le_bytes(buf);

        use chrono::{DateTime, Utc, NaiveDateTime};
        let seconds = ticks / 10_000_000;
        let naive = NaiveDateTime::from_timestamp_opt(seconds, 0)
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid date timestamp",
            ))?;
        let datetime = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
        let date_str = datetime.format("%Y-%m-%d").to_string();

        Ok(Value::Date {
            value: date_str,
            position: Position::UNKNOWN,
        })
    }

    /// Decode Timestamp: [8 bytes ticks since epoch]
    fn decode_timestamp<R: Read>(
        &mut self,
        reader: &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        let ticks = i64::from_le_bytes(buf);

        use chrono::{DateTime, Utc, NaiveDateTime};
        let seconds = ticks / 10_000_000;
        let naive = NaiveDateTime::from_timestamp_opt(seconds, 0)
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid timestamp",
            ))?;
        let datetime = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
        let timestamp_str = datetime.to_rfc3339();

        Ok(Value::Timestamp {
            value: timestamp_str,
            position: Position::UNKNOWN,
        })
    }

    // ==================== SPECIAL TYPE DECODERS ====================

    /// Decode Hex Color: [4 bytes RGBA]
    fn decode_hex<R: Read>(
        &mut self,
        reader: &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        let (r, g, b, a) = (buf[0], buf[1], buf[2], buf[3]);

        // Always use 8-character format for consistency
        let hex_str = format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a);

        Ok(Value::HexColor {
            value: hex_str,
            position: Position::UNKNOWN,
        })
    }

    /// Decode Blob: [Encoding: 1][Length: 4][Data bytes]
    fn decode_blob<R: Read>(
        &mut self,
        reader: &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        // Read encoding (not used for decoding, just metadata)
        let mut _encoding_buf = [0u8; 1];
        reader.read_exact(&mut _encoding_buf)?;

        // Read data length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let length = i32::from_le_bytes(len_buf) as usize;

        context.validate_string_length(length)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Read data
        let mut bytes = vec![0u8; length];
        reader.read_exact(&mut bytes)?;
        let data = String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(Value::PrefixedConstructor {
            prefix: "b".to_string(),
            arguments: vec![Value::String {
                value: data,
                position: Position::UNKNOWN,
            }],
            position: Position::UNKNOWN,
        })
    }

    /// Decode Regex: [Length: 4][Pattern UTF-8 bytes]
    fn decode_regex<R: Read>(
        &mut self,
        reader: &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        // Read pattern length
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let length = i32::from_le_bytes(len_buf) as usize;

        context.validate_string_length(length)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Read pattern
        let mut bytes = vec![0u8; length];
        reader.read_exact(&mut bytes)?;
        let pattern = String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Validate regex
        regex::Regex::new(&pattern).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid regex pattern: {}", e),
            )
        })?;

        Ok(Value::PrefixedConstructor {
            prefix: "r".to_string(),
            arguments: vec![Value::String {
                value: pattern,
                position: Position::UNKNOWN,
            }],
            position: Position::UNKNOWN,
        })
    }
}

impl Default for ValueDecoder {
    fn default() -> Self {
        Self::new()
    }
}