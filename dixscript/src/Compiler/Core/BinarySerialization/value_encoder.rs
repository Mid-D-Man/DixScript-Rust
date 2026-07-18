//! Encodes DixScript AST values to binary format

use std::collections::HashMap;
use std::io::{Write, Result as IoResult};
use crate::Compiler::AST::Value;
use crate::ErrorManager::ErrorManager;
use super::binary_format::{ValueTypeTag, BlobEncoding};
use super::binary_serialization_context::BinarySerializationContext;
use super::binary_serialization_error::BinarySerializationError;

/// Encodes AST values to binary format with type tags.
/// Context is passed per-call rather than stored — safe to use from parallel tasks.
pub struct ValueEncoder {
    error_manager: ErrorManager,
    /// enum_name -> (field_name -> resolved int value), built from the AST's
    /// own local `@ENUMS` section. Lets this encoder resolve `Value::EnumValue`
    /// itself at encode time — see the `Value::EnumValue` arm in `encode_value`
    /// for why this exists instead of relying on an upstream resolution pass.
    local_enums: HashMap<String, HashMap<String, i32>>,
}

impl ValueEncoder {
    pub fn new() -> Self {
        Self::new_with_error_manager(ErrorManager::get_shared_instance())
    }

    pub fn new_with_error_manager(error_manager: ErrorManager) -> Self {
        ValueEncoder { error_manager, local_enums: HashMap::new() }
    }

    /// Attaches a local enums lookup table (enum_name -> field_name -> int),
    /// so `encode_value` can resolve `Value::EnumValue` nodes on its own.
    /// Only covers enums declared in *this* file's own `@ENUMS` section —
    /// see the `Value::EnumValue` arm in `encode_value` for the known
    /// limitation around imported (cross-file) enums.
    pub fn with_enums(mut self, local_enums: HashMap<String, HashMap<String, i32>>) -> Self {
        self.local_enums = local_enums;
        self
    }

    /// Builds the enum_name -> field_name -> int table from an AST's local
    /// `@ENUMS` section, for use with `with_enums`. Mirrors
    /// `DixData::extract_enums_section`'s (Runtime/dix_data.rs, private)
    /// auto-increment field-value semantics exactly — a field without an
    /// explicit `= N` gets the previous field's value + 1, starting at 0 —
    /// since these two encoders are in separate modules and that helper
    /// isn't `pub`.
    pub fn build_local_enums(
        enums: Option<&crate::Compiler::AST::EnumsSection>,
    ) -> HashMap<String, HashMap<String, i32>> {
        let Some(section) = enums else { return HashMap::new() };
        section.enums.iter().map(|decl| {
            let mut auto_value = 0i32;
            let fields: HashMap<String, i32> = decl.fields.iter().map(|field| {
                let value = field.value.unwrap_or_else(|| {
                    let v = auto_value;
                    auto_value += 1;
                    v
                });
                auto_value = value + 1;
                (field.name.clone(), value)
            }).collect();
            (decl.name.clone(), fields)
        }).collect()
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

            // Resolve and encode EnumValue ourselves, using this file's own
            // @ENUMS table (see `with_enums` / `local_enums` above), writing
            // the real ValueTypeTag::Enum wire format (binary_format.rs) so
            // enum_name/field_name identity survives a binary round-trip
            // intact — not just the resolved int.
            //
            // This is the actual fix for the originally-reported bug: enum
            // fields used to silently encode as a hardcoded Int32(0) — no
            // enum tag, indistinguishable from a real value of 0 — whenever
            // the file had no QuickFuncs anywhere in scope, because the only
            // thing that used to resolve EnumValue at all (ValueResolver::
            // resolve(), Runtime/loader.rs Stage 7) was gated on function
            // presence, not enum presence. Beyond that: even when Stage 7 DID
            // run, its Phase 1 (resolve_all_enum_values) collapsed
            // Value::EnumValue into a bare Value::Integer, discarding
            // enum_name/field_name entirely — and the wire format itself had
            // no Enum tag to preserve that identity even if the AST had kept
            // it. Both are fixed now: Stage 7 stays gated on functions only
            // (confirmed necessary — mdix-python's `enums_db` fixture is a
            // pure @ENUMS-no-QuickFuncs file, loaded via `load_str`, whose
            // whole TestEnumGetters suite depends on Stage 7 being skipped so
            // DixData::from_ast's own independent EnumValue handling sees an
            // intact node), and this encoder resolves + tags enums itself,
            // independent of whether Stage 7 ran.
            //
            // KNOWN REMAINING GAP: `local_enums` only covers enums declared
            // in *this* file's own @ENUMS section. An enum imported from
            // another file, used directly in @DATA with no local @ENUMS
            // re-declaration, won't be found here — the fallback below still
            // writes a real ValueTypeTag::Enum entry (so enum_name/field_name
            // identity is preserved on the wire either way) but with
            // `resolved = 0`, and logs a warning rather than hard-failing the
            // whole compile. Closing this properly needs this encoder to also
            // receive the symbol table's imported-namespace enums, which
            // isn't threaded down to BinaryPacker::pack() today.
            Value::EnumValue { enum_name, value: field_name, .. } => {
                match self.local_enums.get(enum_name.as_str())
                    .and_then(|fields| fields.get(field_name.as_str()))
                {
                    Some(&resolved) => self.encode_enum(writer, enum_name, field_name, resolved, context),
                    None => {
                        self.error_manager.log_warning(&format!(
                            "EnumValue {}.{} not found in this file's local @ENUMS during binary \
                             encoding (likely an imported enum — cross-file resolution isn't \
                             wired into the encoder yet). Encoding with resolved value 0; \
                             enum_name/field_name identity is still preserved.",
                            enum_name, field_name
                        ));
                        self.encode_enum(writer, enum_name, field_name, 0, context)
                    }
                }
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
        if !(1..=6).contains(&count) {
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

    /// Encode Enum: [enum_name: 4-byte len + UTF-8][field_name: 4-byte len + UTF-8][4-byte i32 resolved value]
    ///
    /// See the long comment on the `Value::EnumValue` arm in `encode_value`
    /// for why this exists and what it fixes. Mirrors `encode_regex`'s
    /// length-prefixing convention for the two strings.
    fn encode_enum<W: Write>(
        &mut self,
        writer:    &mut W,
        enum_name: &str,
        field_name: &str,
        resolved:  i32,
        context:   &mut BinarySerializationContext,
    ) -> IoResult<()> {
        writer.write_all(&[ValueTypeTag::Enum as u8])?;

        for s in [enum_name, field_name] {
            let bytes = s.as_bytes();
            context.validate_string_length(bytes.len())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            writer.write_all(&(bytes.len() as i32).to_le_bytes())?;
            writer.write_all(bytes)?;
        }

        writer.write_all(&resolved.to_le_bytes())?;
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
            Value::EnumValue { .. }          => ValueTypeTag::Enum,
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
