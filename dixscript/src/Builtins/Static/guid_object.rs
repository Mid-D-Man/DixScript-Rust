// src/Builtins/Static/guid_object.rs
//! Guid static object for GUID generation and manipulation
//! Zero external dependencies - uses uuid crate

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};
use uuid::Uuid;

/// Guid static object implementation
pub struct GuidObject {
    base: StaticObjectBase,
}

impl GuidObject {
    pub fn new() -> Self {
        let mut base = StaticObjectBase::new("Guid".to_string());
        Self::initialize_methods(&mut base);
        GuidObject { base }
    }

    fn initialize_methods(base: &mut StaticObjectBase) {
        // Guid.new() - Generate new GUID
        base.register_method(Box::new(BuiltinMethod::new(
            "new".to_string(),
            0,
            DixType::String,
            |_args| {
                let guid = Uuid::new_v4();
                Ok(DixValue::from_string(guid.to_string()))
            },
            "Generates a new GUID (UUID v4 format)".to_string(),
        )));

        // Guid.parse(str) - Parse GUID from string
        base.register_method(Box::new(BuiltinMethod::new(
            "parse".to_string(),
            1,
            DixType::String,
            |args| {
                let input = args[0].as_string();

                match Uuid::parse_str(&input) {
                    Ok(guid) => Ok(DixValue::from_string(guid.to_string())),
                    Err(_) => Err(format!("Invalid GUID format: {}", input)),
                }
            },
            "Parses a GUID from string (throws error if invalid)".to_string(),
        )));

        // Guid.tryParse(str) - Try parse, return null on failure
        base.register_method(Box::new(BuiltinMethod::new(
            "tryParse".to_string(),
            1,
            DixType::String,
            |args| {
                let input = args[0].as_string();

                match Uuid::parse_str(&input) {
                    Ok(guid) => Ok(DixValue::from_string(guid.to_string())),
                    Err(_) => Ok(DixValue::null()),
                }
            },
            "Tries to parse a GUID, returns null if invalid".to_string(),
        )));

        // Guid.validate(str) - Check if valid GUID format
        base.register_method(Box::new(BuiltinMethod::new(
            "validate".to_string(),
            1,
            DixType::Bool,
            |args| {
                let input = args[0].as_string();
                let is_valid = Uuid::parse_str(&input).is_ok();
                Ok(DixValue::from_bool(is_valid))
            },
            "Checks if a string is a valid GUID format".to_string(),
        )));

        // Guid.empty() - Return empty GUID
        base.register_method(Box::new(BuiltinMethod::new(
            "empty".to_string(),
            0,
            DixType::String,
            |_args| {
                let empty = Uuid::nil();
                Ok(DixValue::from_string(empty.to_string()))
            },
            "Returns the empty GUID (00000000-0000-0000-0000-000000000000)".to_string(),
        )));

        // Guid.format(str, format) - Format GUID in different styles
        base.register_method(Box::new(BuiltinMethod::new(
            "format".to_string(),
            2,
            DixType::String,
            |args| {
                let guid_str = args[0].as_string();
                let format_str = args[1].as_string();

                let guid = Uuid::parse_str(&guid_str)
                    .map_err(|_| format!("Invalid GUID: {}", guid_str))?;

                let formatted = match format_str.as_str() {
                    "N" | "n" => guid.simple().to_string(),
                    "D" | "d" => guid.hyphenated().to_string(),
                    "B" | "b" => format!("{{{}}}", guid.hyphenated()),
                    "P" | "p" => format!("({})", guid.hyphenated()),
                    "X" | "x" => format!(
                        "{{0x{:08x},0x{:04x},0x{:04x},{{0x{:02x},0x{:02x},0x{:02x},0x{:02x},0x{:02x},0x{:02x},0x{:02x},0x{:02x}}}}}",
                        guid.as_fields().0,
                        guid.as_fields().1,
                        guid.as_fields().2,
                        guid.as_fields().3[0],
                        guid.as_fields().3[1],
                        guid.as_fields().3[2],
                        guid.as_fields().3[3],
                        guid.as_fields().3[4],
                        guid.as_fields().3[5],
                        guid.as_fields().3[6],
                        guid.as_fields().3[7],
                    ),
                    _ => return Err(format!("Invalid GUID format specifier: {}. Valid: N, D, B, P, X", format_str)),
                };

                Ok(DixValue::from_string(formatted))
            },
            "Formats a GUID using specified format (N, D, B, P, X)".to_string(),
        )));

        // Guid.toBytes(str) - Convert GUID to byte array
        base.register_method(Box::new(BuiltinMethod::new(
            "toBytes".to_string(),
            1,
            DixType::Array,
            |args| {
                let guid_str = args[0].as_string();

                let guid = Uuid::parse_str(&guid_str)
                    .map_err(|_| format!("Invalid GUID: {}", guid_str))?;

                let bytes = guid.as_bytes();
                let byte_values: Vec<DixValue> = bytes.iter()
                    .map(|&b| DixValue::from_int(b as i32))
                    .collect();

                Ok(DixValue::from_array(byte_values))
            },
            "Converts a GUID to a byte array (16 bytes)".to_string(),
        )));

        // Guid.fromBytes(array) - Create GUID from byte array
        base.register_method(Box::new(BuiltinMethod::new(
            "fromBytes".to_string(),
            1,
            DixType::String,
            |args| {
                let byte_array = args[0].as_array();

                if byte_array.len() != 16 {
                    return Err(format!(
                        "GUID requires exactly 16 bytes, got {}",
                        byte_array.len()
                    ));
                }

                let mut bytes = [0u8; 16];
                for (i, value) in byte_array.iter().enumerate() {
                    let byte_val = value.as_int();
                    if !(0..=255).contains(&byte_val) {
                        return Err(format!("Byte value must be 0-255, got {}", byte_val));
                    }
                    bytes[i] = byte_val as u8;
                }

                let guid = Uuid::from_bytes(bytes);
                Ok(DixValue::from_string(guid.to_string()))
            },
            "Creates a GUID from a byte array (must be exactly 16 bytes)".to_string(),
        )));
    }
}

impl Default for GuidObject {
    fn default() -> Self {
        Self::new()
    }
}

impl IStaticObject for GuidObject {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn call_method(&self, method_name: &str, args: &[DixValue]) -> Result<DixValue, String> {
        self.base.call_method(method_name, args)
    }

    fn has_method(&self, method_name: &str) -> bool {
        self.base.has_method(method_name)
    }

    fn get_method_names(&self) -> Vec<String> {
        self.base.get_method_names()
    }

    fn get_method(&self, method_name: &str) -> Option<&dyn IBuiltinMethod> {
        self.base.get_method(method_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guid_generation() {
        let guid_obj = GuidObject::new();
        let result = guid_obj.call_method("new", &[]).unwrap();
        let guid_str = result.as_string();

        // Should be valid UUID format
        assert_eq!(guid_str.len(), 36);
        assert!(guid_str.contains('-'));
    }

    #[test]
    fn test_guid_validation() {
        let guid_obj = GuidObject::new();

        let valid_result = guid_obj
            .call_method(
                "validate",
                &[DixValue::from_string(
                    "550e8400-e29b-41d4-a716-446655440000".to_string(),
                )],
            )
            .unwrap();
        assert!(valid_result.as_bool());

        let invalid_result = guid_obj
            .call_method("validate", &[DixValue::from_string("not-a-guid".to_string())])
            .unwrap();
        assert!(!invalid_result.as_bool());
    }

    #[test]
    fn test_guid_format() {
        let guid_obj = GuidObject::new();
        let guid = "550e8400-e29b-41d4-a716-446655440000";

        let result = guid_obj
            .call_method(
                "format",
                &[
                    DixValue::from_string(guid.to_string()),
                    DixValue::from_string("N".to_string()),
                ],
            )
            .unwrap();

        // Simple format has no hyphens
        let formatted = result.as_string();
        assert!(!formatted.contains('-'));
        assert_eq!(formatted.len(), 32);
    }
}