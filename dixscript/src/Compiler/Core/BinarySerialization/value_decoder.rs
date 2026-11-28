// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/BinarySerialization/value_decoder.rs"
// ============================================================================
//! Decodes binary format to DixScript AST values

use std::io::{Read, Result as IoResult};
use crate::Compiler::AST::{Value, Position, ObjectProperty};
use crate::Compiler::AST::arena::{Arena, ObjectPropNode, ValueId, ValueNode};
use crate::ErrorManager::ErrorManager;
use super::binary_format::ValueTypeTag;
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Decodes binary format to AST values with type tag validation.
/// Context is passed per-call rather than stored — safe to use from parallel tasks.
pub struct ValueDecoder {
    error_manager: ErrorManager,
}

impl ValueDecoder {
    pub fn new() -> Self {
        Self::new_with_error_manager(ErrorManager::get_shared_instance())
    }

    pub fn new_with_error_manager(error_manager: ErrorManager) -> Self {
        ValueDecoder { error_manager }
    }

    // =========================================================================
    // MAIN DECODE ENTRY POINT
    // =========================================================================

    /// Decode any value from binary.
    /// Format: [Type Tag: 1 byte][Value Data: variable]
    pub fn decode_value<R: Read>(
        &mut self,
        reader:  &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut tag_buf = [0u8; 1];
        reader.read_exact(&mut tag_buf)?;
        let type_tag = ValueTypeTag::from_u8(tag_buf[0]).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                BinarySerializationError::invalid_type_tag(tag_buf[0], context.get_current_scope()),
            )
        })?;

        let value = match type_tag {
            ValueTypeTag::Int32     => self.decode_int32(reader, context)?,
            ValueTypeTag::Int64     => self.decode_int64(reader, context)?,
            ValueTypeTag::Float32   => self.decode_float32(reader, context)?,
            ValueTypeTag::Float64   => self.decode_float64(reader, context)?,
            ValueTypeTag::String    => self.decode_string(reader, context)?,
            ValueTypeTag::Bool      => self.decode_bool(reader, context)?,
            ValueTypeTag::Null      => self.decode_null(context)?,
            ValueTypeTag::Array     => self.decode_array(reader, context)?,
            ValueTypeTag::Object    => self.decode_object(reader, context)?,
            ValueTypeTag::Tuple     => self.decode_tuple(reader, context)?,
            ValueTypeTag::Date      => self.decode_date(reader, context)?,
            ValueTypeTag::Timestamp => self.decode_timestamp(reader, context)?,
            ValueTypeTag::Hex       => self.decode_hex(reader, context)?,
            ValueTypeTag::Blob      => self.decode_blob(reader, context)?,
            ValueTypeTag::Regex     => self.decode_regex(reader, context)?,
            ValueTypeTag::Enum      => self.decode_enum(reader, context)?,
            ValueTypeTag::Invalid   => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    BinarySerializationError::new(
                        crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::UnsupportedType,
                        "Encountered InvalidType tag (0xFF) during decode",
                        context.get_current_scope(),
                    ),
                ));
            }
        };

        context.statistics.increment_value_count(type_tag);
        Ok(value)
    }

    // =========================================================================
    // PRIMITIVE TYPE DECODERS
    // =========================================================================

    /// Decode Int32: [4 bytes little-endian]
    fn decode_int32<R: Read>(
        &mut self,
        reader:   &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(Value::Integer { value: i32::from_le_bytes(buf), position: Position::UNKNOWN })
    }

    /// Decode Int64 (Long): [8 bytes little-endian]
    fn decode_int64<R: Read>(
        &mut self,
        reader:   &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Ok(Value::Long { value: i64::from_le_bytes(buf), position: Position::UNKNOWN })
    }

    /// Decode Float32: [4 bytes IEEE 754]
    fn decode_float32<R: Read>(
        &mut self,
        reader:   &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(Value::Float { value: f32::from_le_bytes(buf), position: Position::UNKNOWN })
    }

    /// Decode Float64: [8 bytes IEEE 754]
    fn decode_float64<R: Read>(
        &mut self,
        reader:   &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Ok(Value::Double { value: f64::from_le_bytes(buf), position: Position::UNKNOWN })
    }

    /// Decode String: [Length: 4 bytes][UTF-8 bytes]
    fn decode_string<R: Read>(
        &mut self,
        reader:  &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let value = self.read_text(reader, context)?;
        Ok(Value::String { value, position: Position::UNKNOWN })
    }

    /// Decode Bool: [1 byte: 0x00 or 0x01]
    fn decode_bool<R: Read>(
        &mut self,
        reader:   &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        Ok(Value::Boolean { value: buf[0] != 0x00, position: Position::UNKNOWN })
    }

    /// Decode Null: (no payload)
    fn decode_null(&mut self, _context: &mut BinarySerializationContext) -> IoResult<Value> {
        Ok(Value::Null { position: Position::UNKNOWN })
    }

    // =========================================================================
    // COMPLEX TYPE DECODERS
    // =========================================================================

    /// Decode Array: [Count: 4 bytes][Element Type: 1 byte][Values...]
    fn decode_array<R: Read>(
        &mut self,
        reader:  &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        context.enter_nested("Array")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)?;
        let count = i32::from_le_bytes(count_buf) as usize;
        context.validate_array_length(count)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Element type byte — metadata only, actual type comes from each value's tag
        let mut _type_buf = [0u8; 1];
        reader.read_exact(&mut _type_buf)?;

        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.decode_value(reader, context)?);
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Value::Array { values, position: Position::UNKNOWN })
    }

    /// Decode Object: [Count: 4 bytes][Key-Value pairs...]
    fn decode_object<R: Read>(
        &mut self,
        reader:  &mut R,
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
            let mut key_len_buf = [0u8; 4];
            reader.read_exact(&mut key_len_buf)?;
            let key_length = i32::from_le_bytes(key_len_buf) as usize;
            let mut key_bytes = vec![0u8; key_length];
            reader.read_exact(&mut key_bytes)?;
            let key = String::from_utf8(key_bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            let value = self.decode_value(reader, context)?;
            properties.push(ObjectProperty::new(key, value, Position::UNKNOWN));
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Value::Object { properties, position: Position::UNKNOWN })
    }

    /// Decode Tuple: [Count: 1 byte (1-6)][Values...]
    fn decode_tuple<R: Read>(
        &mut self,
        reader:  &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        context.enter_nested("Tuple")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut count_buf = [0u8; 1];
        reader.read_exact(&mut count_buf)?;
        let count = count_buf[0] as usize;
        if !(1..=6).contains(&count) {
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
            prefix:    "t".to_string(),
            arguments,
            position:  Position::UNKNOWN,
        })
    }

    // =========================================================================
    // TEMPORAL TYPE DECODERS
    // =========================================================================

    /// Decode Date: [8 bytes ticks since epoch]
    fn decode_date<R: Read>(
        &mut self,
        reader:   &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        Ok(Value::Date { value: self.read_date_text(reader)?, position: Position::UNKNOWN })
    }

    /// Decode Timestamp: [8 bytes ticks since epoch]
    fn decode_timestamp<R: Read>(
        &mut self,
        reader:   &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        Ok(Value::Timestamp { value: self.read_timestamp_text(reader)?, position: Position::UNKNOWN })
    }

    // =========================================================================
    // SPECIAL TYPE DECODERS
    // =========================================================================

    /// Decode Hex Color: [4 bytes RGBA]
    fn decode_hex<R: Read>(
        &mut self,
        reader:   &mut R,
        _context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        Ok(Value::HexColor { value: self.read_hex_text(reader)?, position: Position::UNKNOWN })
    }

    /// Decode Blob: [Encoding: 1][Length: 4][Data bytes]
    fn decode_blob<R: Read>(
        &mut self,
        reader:  &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let mut _encoding_buf = [0u8; 1];
        reader.read_exact(&mut _encoding_buf)?;
        let data = self.read_text(reader, context)?;
        Ok(Value::PrefixedConstructor {
            prefix:    "b".to_string(),
            arguments: vec![Value::String { value: data, position: Position::UNKNOWN }],
            position:  Position::UNKNOWN,
        })
    }

    /// Decode Regex: [Length: 4][Pattern UTF-8 bytes]
    fn decode_regex<R: Read>(
        &mut self,
        reader:  &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let pattern = self.read_regex_pattern(reader, context)?;
        Ok(Value::PrefixedConstructor {
            prefix:    "r".to_string(),
            arguments: vec![Value::String { value: pattern, position: Position::UNKNOWN }],
            position:  Position::UNKNOWN,
        })
    }

    /// Decode Enum: [enum_name: 4-byte len + UTF-8][field_name: 4-byte len + UTF-8][4-byte i32 resolved value]
    ///
    /// Reconstructs a real `Value::EnumValue` AST node — the exact shape the
    /// parser already produces for a literal `Enum.FIELD` reference — so
    /// `DixData::from_ast`'s existing `ast_value_to_dix_value` branch builds a
    /// proper `DixValue::Enum` from decoded binary data with no changes needed
    /// on that side. The resolved i32 travels alongside the names on the wire
    /// (see `encode_enum` in value_encoder.rs) since data_section_reader.rs has
    /// no access to the @ENUMS table at decode time — self-contained, no
    /// cross-section lookup required.
    fn decode_enum<R: Read>(
        &mut self,
        reader:  &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<Value> {
        let enum_name  = self.read_text(reader, context)?;
        let field_name = self.read_text(reader, context)?;

        // The resolved i32 isn't carried by Value::EnumValue itself (that AST
        // node only has enum_name/value/position — see Compiler/AST/values.rs).
        // We still need to consume these 4 bytes off the wire since we wrote
        // them (see encode_enum); the resolved int is redundant here because
        // from_ast re-derives it from @ENUMS via enum_name+field_name anyway,
        // the same way it does for a freshly-parsed source file.
        let mut resolved_buf = [0u8; 4];
        reader.read_exact(&mut resolved_buf)?;

        Ok(Value::EnumValue { enum_name, value: field_name, position: Position::UNKNOWN })
    }
}

/// The reads the owned and the arena decoders share: everything that turns bytes into a
/// string, so the two entry points cannot disagree about what a string, a date or a colour is.
impl ValueDecoder {
    /// A length-prefixed UTF-8 string: `[Length: 4 bytes][UTF-8 bytes]`, with the length checked
    /// against [`MAX_STRING_LENGTH`](super::MAX_STRING_LENGTH).
    fn read_text<R: Read>(
        &mut self,
        reader:  &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<String> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let length = i32::from_le_bytes(len_buf) as usize;
        context.validate_string_length(length)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut bytes = vec![0u8; length];
        reader.read_exact(&mut bytes)?;
        String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// A regex pattern: a string that has to compile.
    fn read_regex_pattern<R: Read>(
        &mut self,
        reader:  &mut R,
        context: &mut BinarySerializationContext,
    ) -> IoResult<String> {
        let pattern = self.read_text(reader, context)?;
        regex::Regex::new(&pattern).map_err(|e| std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Invalid regex pattern: {}", e),
        ))?;
        Ok(pattern)
    }

    /// A date: 8 bytes of ticks, shown as `%Y-%m-%d`.
    fn read_date_text<R: Read>(&mut self, reader: &mut R) -> IoResult<String> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        let ticks   = i64::from_le_bytes(buf);
        let seconds = ticks / 10_000_000;
        use chrono::{DateTime, Utc};
        let datetime = DateTime::<Utc>::from_timestamp(seconds, 0)
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData, "Invalid date timestamp",
            ))?;
        Ok(datetime.format("%Y-%m-%d").to_string())
    }

    /// A timestamp: 8 bytes of ticks, shown as RFC 3339.
    fn read_timestamp_text<R: Read>(&mut self, reader: &mut R) -> IoResult<String> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        let ticks   = i64::from_le_bytes(buf);
        let seconds = ticks / 10_000_000;
        use chrono::{DateTime, Utc};
        let datetime = DateTime::<Utc>::from_timestamp(seconds, 0)
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData, "Invalid timestamp",
            ))?;
        Ok(datetime.to_rfc3339())
    }

    /// A hex colour: 4 bytes of RGBA, shown as `#RRGGBBAA`.
    fn read_hex_text<R: Read>(&mut self, reader: &mut R) -> IoResult<String> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        let (r, g, b, a) = (buf[0], buf[1], buf[2], buf[3]);
        Ok(format!("#{:02X}{:02X}{:02X}{:02X}", r, g, b, a))
    }
}

/// The arena entry points: the same decoding as above, building nodes in an [`Arena`] instead
/// of an owned `Value` tree.
impl ValueDecoder {
    /// Decodes one value into `arena` and returns its id.
    ///
    /// Reads the same bytes, records the same statistics and fails with the same errors as
    /// [`decode_value`](Self::decode_value); `Document::export` of the result is the value
    /// `decode_value` returns. Nodes are appended to `arena`, so decoding several values into
    /// one arena leaves the earlier ids valid.
    pub fn decode_node<R: Read>(
        &mut self,
        reader:  &mut R,
        arena:   &mut Arena,
        context: &mut BinarySerializationContext,
    ) -> IoResult<ValueId> {
        let mut tag_buf = [0u8; 1];
        reader.read_exact(&mut tag_buf)?;
        let type_tag = ValueTypeTag::from_u8(tag_buf[0]).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                BinarySerializationError::invalid_type_tag(tag_buf[0], context.get_current_scope()),
            )
        })?;

        let id = match type_tag {
            ValueTypeTag::Int32 => {
                let mut buf = [0u8; 4];
                reader.read_exact(&mut buf)?;
                arena.alloc_value(ValueNode::Integer { value: i32::from_le_bytes(buf), position: Position::UNKNOWN })
            }
            ValueTypeTag::Int64 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf)?;
                arena.alloc_value(ValueNode::Long { value: i64::from_le_bytes(buf), position: Position::UNKNOWN })
            }
            ValueTypeTag::Float32 => {
                let mut buf = [0u8; 4];
                reader.read_exact(&mut buf)?;
                arena.alloc_value(ValueNode::Float { value: f32::from_le_bytes(buf), position: Position::UNKNOWN })
            }
            ValueTypeTag::Float64 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf)?;
                arena.alloc_value(ValueNode::Double { value: f64::from_le_bytes(buf), position: Position::UNKNOWN })
            }
            ValueTypeTag::String => {
                let text = self.read_text(reader, context)?;
                let value = arena.alloc_str(&text);
                arena.alloc_value(ValueNode::String { value, position: Position::UNKNOWN })
            }
            ValueTypeTag::Bool => {
                let mut buf = [0u8; 1];
                reader.read_exact(&mut buf)?;
                arena.alloc_value(ValueNode::Boolean { value: buf[0] != 0x00, position: Position::UNKNOWN })
            }
            ValueTypeTag::Null      => arena.alloc_value(ValueNode::Null { position: Position::UNKNOWN }),
            ValueTypeTag::Array     => self.decode_array_node(reader, arena, context)?,
            ValueTypeTag::Object    => self.decode_object_node(reader, arena, context)?,
            ValueTypeTag::Tuple     => self.decode_tuple_node(reader, arena, context)?,
            ValueTypeTag::Date      => {
                let text = self.read_date_text(reader)?;
                let value = arena.alloc_str(&text);
                arena.alloc_value(ValueNode::Date { value, position: Position::UNKNOWN })
            }
            ValueTypeTag::Timestamp => {
                let text = self.read_timestamp_text(reader)?;
                let value = arena.alloc_str(&text);
                arena.alloc_value(ValueNode::Timestamp { value, position: Position::UNKNOWN })
            }
            ValueTypeTag::Hex       => {
                let text = self.read_hex_text(reader)?;
                let value = arena.alloc_str(&text);
                arena.alloc_value(ValueNode::HexColor { value, position: Position::UNKNOWN })
            }
            ValueTypeTag::Blob      => {
                let mut _encoding_buf = [0u8; 1];
                reader.read_exact(&mut _encoding_buf)?;
                let data = self.read_text(reader, context)?;
                self.alloc_constructor_node(arena, "b", &data)
            }
            ValueTypeTag::Regex     => {
                let pattern = self.read_regex_pattern(reader, context)?;
                self.alloc_constructor_node(arena, "r", &pattern)
            }
            ValueTypeTag::Enum      => {
                let enum_name  = self.read_text(reader, context)?;
                let field_name = self.read_text(reader, context)?;
                // The resolved i32 is on the wire but the node has nowhere to keep it; see
                // `decode_enum`.
                let mut resolved_buf = [0u8; 4];
                reader.read_exact(&mut resolved_buf)?;
                let enum_name = arena.alloc_str(&enum_name);
                let value     = arena.alloc_str(&field_name);
                arena.alloc_value(ValueNode::EnumValue { enum_name, value, position: Position::UNKNOWN })
            }
            ValueTypeTag::Invalid   => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    BinarySerializationError::new(
                        crate::ErrorManager::ErrorTypes::BinarySerializationErrorType::UnsupportedType,
                        "Encountered InvalidType tag (0xFF) during decode",
                        context.get_current_scope(),
                    ),
                ));
            }
        };

        context.statistics.increment_value_count(type_tag);
        Ok(id)
    }

    /// A prefixed constructor with one string argument: the shape of a blob and a regex.
    fn alloc_constructor_node(&self, arena: &mut Arena, prefix: &str, argument: &str) -> ValueId {
        let text      = arena.alloc_str(argument);
        let argument  = arena.alloc_value(ValueNode::String { value: text, position: Position::UNKNOWN });
        let arguments = arena.alloc_value_list(&[argument]);
        let prefix    = arena.alloc_str(prefix);
        arena.alloc_value(ValueNode::PrefixedConstructor { prefix, arguments, position: Position::UNKNOWN })
    }

    fn decode_array_node<R: Read>(
        &mut self,
        reader:  &mut R,
        arena:   &mut Arena,
        context: &mut BinarySerializationContext,
    ) -> IoResult<ValueId> {
        context.enter_nested("Array")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)?;
        let count = i32::from_le_bytes(count_buf) as usize;
        context.validate_array_length(count)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Element type byte: metadata only, the actual type comes from each value's tag
        let mut _type_buf = [0u8; 1];
        reader.read_exact(&mut _type_buf)?;

        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(self.decode_node(reader, arena, context)?);
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let values = arena.alloc_value_list(&ids);
        Ok(arena.alloc_value(ValueNode::Array { values, position: Position::UNKNOWN }))
    }

    fn decode_object_node<R: Read>(
        &mut self,
        reader:  &mut R,
        arena:   &mut Arena,
        context: &mut BinarySerializationContext,
    ) -> IoResult<ValueId> {
        context.enter_nested("Object")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut count_buf = [0u8; 4];
        reader.read_exact(&mut count_buf)?;
        let count = i32::from_le_bytes(count_buf) as usize;
        context.validate_object_property_count(count)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut properties = Vec::with_capacity(count);
        for _ in 0..count {
            let mut key_len_buf = [0u8; 4];
            reader.read_exact(&mut key_len_buf)?;
            let key_length = i32::from_le_bytes(key_len_buf) as usize;
            let mut key_bytes = vec![0u8; key_length];
            reader.read_exact(&mut key_bytes)?;
            let key = String::from_utf8(key_bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            let value = self.decode_node(reader, arena, context)?;
            let key = arena.alloc_str(&key);
            properties.push(ObjectPropNode { key, value, position: Position::UNKNOWN });
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let properties = arena.alloc_prop_list(&properties);
        Ok(arena.alloc_value(ValueNode::Object { properties, position: Position::UNKNOWN }))
    }

    fn decode_tuple_node<R: Read>(
        &mut self,
        reader:  &mut R,
        arena:   &mut Arena,
        context: &mut BinarySerializationContext,
    ) -> IoResult<ValueId> {
        context.enter_nested("Tuple")
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut count_buf = [0u8; 1];
        reader.read_exact(&mut count_buf)?;
        let count = count_buf[0] as usize;
        if !(1..=6).contains(&count) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid tuple count: {} (must be 1-6)", count),
            ));
        }

        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(self.decode_node(reader, arena, context)?);
        }

        context.exit_nested()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let prefix    = arena.alloc_str("t");
        let arguments = arena.alloc_value_list(&ids);
        Ok(arena.alloc_value(ValueNode::PrefixedConstructor { prefix, arguments, position: Position::UNKNOWN }))
    }
}

impl Default for ValueDecoder {
    fn default() -> Self { Self::new() }
            }
