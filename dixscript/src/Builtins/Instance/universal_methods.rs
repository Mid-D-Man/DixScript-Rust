// dixscript/src/Builtins/Instance/universal_methods.rs
// src/Builtins/Instance/universal_methods.rs
//! Universal instance methods available on all DixScript types

use crate::Builtins::Core::{DixType, DixValue, BuiltinMethod, IBuiltinMethod};
use std::collections::HashMap;

/// Get all universal methods
pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Universal.toString()
    methods.insert(
        "toString".to_string(),
        Box::new(BuiltinMethod::new(
            "toString".to_string(),
            1,
            DixType::String,
            |args| Ok(DixValue::from_string(args[0].as_string())),
            "Converts the value to its string representation".to_string(),
        )),
    );

    // Universal.type()
    methods.insert(
        "type".to_string(),
        Box::new(BuiltinMethod::new(
            "type".to_string(),
            1,
            DixType::String,
            |args| Ok(DixValue::from_string(args[0].get_type().get_type_name().to_string())),
            "Returns the type name of the value".to_string(),
        )),
    );

    // Universal.isNull()
    methods.insert(
        "isNull".to_string(),
        Box::new(BuiltinMethod::new(
            "isNull".to_string(),
            1,
            DixType::Bool,
            |args| Ok(DixValue::from_bool(args[0].is_null())),
            "Checks if the value is null".to_string(),
        )),
    );

    // Universal.isNotNull()
    methods.insert(
        "isNotNull".to_string(),
        Box::new(BuiltinMethod::new(
            "isNotNull".to_string(),
            1,
            DixType::Bool,
            |args| Ok(DixValue::from_bool(!args[0].is_null())),
            "Checks if the value is not null".to_string(),
        )),
    );

    // Universal.equals(other)
    methods.insert(
        "equals".to_string(),
        Box::new(BuiltinMethod::new(
            "equals".to_string(),
            2,
            DixType::Bool,
            |args| Ok(DixValue::from_bool(args[0].equal_to(&args[1]))),
            "Checks if this value equals another value".to_string(),
        )),
    );

    // Universal.notEquals(other)
    methods.insert(
        "notEquals".to_string(),
        Box::new(BuiltinMethod::new(
            "notEquals".to_string(),
            2,
            DixType::Bool,
            |args| Ok(DixValue::from_bool(!args[0].equal_to(&args[1]))),
            "Checks if this value does not equal another value".to_string(),
        )),
    );

    // Universal.hashCode()
    methods.insert(
        "hashCode".to_string(),
        Box::new(BuiltinMethod::new(
            "hashCode".to_string(),
            1,
            DixType::Int,
            |args| {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                args[0].to_string().hash(&mut hasher);
                let hash = hasher.finish();
                Ok(DixValue::from_int((hash % (i32::MAX as u64)) as i32))
            },
            "Returns the hash code of the value".to_string(),
        )),
    );

    // Universal.clone()
    methods.insert(
        "clone".to_string(),
        Box::new(BuiltinMethod::new(
            "clone".to_string(),
            1,
            DixType::Any,
            |args| Ok(args[0].deep_clone()),
            "Creates a deep copy of the value".to_string(),
        )),
    );

    // Universal.toBytes()
    methods.insert(
        "toBytes".to_string(),
        Box::new(BuiltinMethod::new(
            "toBytes".to_string(),
            1,
            DixType::Array,
            |args| {
                let bytes = convert_to_bytes(&args[0]);
                let byte_values: Vec<DixValue> = bytes.iter()
                    .map(|b| DixValue::from_int(*b as i32))
                    .collect();
                Ok(DixValue::from_array(byte_values))
            },
            "Converts the value to a byte array representation".to_string(),
        )),
    );

    // Universal.size()
    methods.insert(
        "size".to_string(),
        Box::new(BuiltinMethod::new(
            "size".to_string(),
            1,
            DixType::Int,
            |args| Ok(DixValue::from_int(estimate_size(&args[0]))),
            "Returns an estimate of the memory size of the value in bytes".to_string(),
        )),
    );

    // Universal.isEmpty()
    methods.insert(
        "isEmpty".to_string(),
        Box::new(BuiltinMethod::new(
            "isEmpty".to_string(),
            1,
            DixType::Bool,
            |args| Ok(DixValue::from_bool(is_empty(&args[0]))),
            "Checks if the value is considered empty (null, empty string, empty array, zero, etc.)".to_string(),
        )),
    );

    // Universal.isNotEmpty()
    methods.insert(
        "isNotEmpty".to_string(),
        Box::new(BuiltinMethod::new(
            "isNotEmpty".to_string(),
            1,
            DixType::Bool,
            |args| Ok(DixValue::from_bool(!is_empty(&args[0]))),
            "Checks if the value is not empty".to_string(),
        )),
    );

    // Universal.defaultIfNull(defaultValue)
    methods.insert(
        "defaultIfNull".to_string(),
        Box::new(BuiltinMethod::new(
            "defaultIfNull".to_string(),
            2,
            DixType::Any,
            |args| {
                if args[0].is_null() {
                    Ok(args[1].deep_clone())
                } else {
                    Ok(args[0].deep_clone())
                }
            },
            "Returns the default value if this value is null, otherwise returns this value".to_string(),
        )),
    );

    // Universal.defaultIfEmpty(defaultValue)
    methods.insert(
        "defaultIfEmpty".to_string(),
        Box::new(BuiltinMethod::new(
            "defaultIfEmpty".to_string(),
            2,
            DixType::Any,
            |args| {
                if is_empty(&args[0]) {
                    Ok(args[1].deep_clone())
                } else {
                    Ok(args[0].deep_clone())
                }
            },
            "Returns the default value if this value is empty, otherwise returns this value".to_string(),
        )),
    );

    // Universal.debug()
    methods.insert(
        "debug".to_string(),
        Box::new(BuiltinMethod::new(
            "debug".to_string(),
            1,
            DixType::String,
            |args| Ok(DixValue::from_string(get_debug_info(&args[0]))),
            "Returns a detailed debug representation of the value".to_string(),
        )),
    );

    // Universal.json()
    methods.insert(
        "json".to_string(),
        Box::new(BuiltinMethod::new(
            "json".to_string(),
            1,
            DixType::String,
            |args| Ok(DixValue::from_string(to_json_string(&args[0]))),
            "Converts the value to a JSON-compatible string representation".to_string(),
        )),
    );

    // Universal.isNumeric()
    methods.insert(
        "isNumeric".to_string(),
        Box::new(BuiltinMethod::new(
            "isNumeric".to_string(),
            1,
            DixType::Bool,
            |args| Ok(DixValue::from_bool(args[0].is_numeric())),
            "Checks if the value is a numeric type (int, long, float, or double)".to_string(),
        )),
    );

    methods
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Convert a DixValue to its raw byte representation.
fn convert_to_bytes(value: &DixValue) -> Vec<u8> {
    match value.get_type() {
        DixType::String    => value.as_string().into_bytes(),
        DixType::Int       => value.as_int().to_le_bytes().to_vec(),
        DixType::Long      => value.as_long().to_le_bytes().to_vec(),
        DixType::Float     => value.as_float().to_le_bytes().to_vec(),
        DixType::Double    => value.as_double().to_le_bytes().to_vec(),
        DixType::Bool      => vec![if value.as_bool() { 1 } else { 0 }],
        DixType::Date
        | DixType::Timestamp => {
            value.as_datetime().timestamp().to_le_bytes().to_vec()
        }
        DixType::Null      => vec![],
        DixType::Array     => convert_array_to_bytes(value.as_array()),
        DixType::Object    => convert_object_to_bytes(value.as_object()),
        // Hex, Blob, Regex, Enum, Tuple, Void, Any → fall back to UTF-8 string bytes
        _                  => value.to_string().into_bytes(),
    }
}

fn convert_array_to_bytes(array: &Vec<DixValue>) -> Vec<u8> {
    let mut result = Vec::new();
    for item in array {
        let item_bytes = convert_to_bytes(item);
        result.extend_from_slice(&(item_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(&item_bytes);
    }
    result
}

fn convert_object_to_bytes(obj: &HashMap<String, DixValue>) -> Vec<u8> {
    let mut result = Vec::new();
    for (key, value) in obj {
        let key_bytes   = key.as_bytes();
        let value_bytes = convert_to_bytes(value);
        result.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(key_bytes);
        result.extend_from_slice(&(value_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(&value_bytes);
    }
    result
}

/// Estimate the in-memory size of a DixValue (approximate, in bytes).
fn estimate_size(value: &DixValue) -> i32 {
    let size: usize = match value.get_type() {
        DixType::String    => value.as_string().len() * 2,      // UTF-16 estimate
        DixType::Int       => std::mem::size_of::<i32>(),
        DixType::Long      => std::mem::size_of::<i64>(),
        DixType::Float     => std::mem::size_of::<f32>(),
        DixType::Double    => std::mem::size_of::<f64>(),
        DixType::Bool      => std::mem::size_of::<bool>(),
        DixType::Date
        | DixType::Timestamp => std::mem::size_of::<i64>(),
        DixType::Null      => 0,
        DixType::Array     => estimate_array_size(value.as_array()),
        DixType::Object    => estimate_object_size(value.as_object()),
        _                  => value.to_string().len() * 2,
    };
    size as i32
}

fn estimate_array_size(array: &Vec<DixValue>) -> usize {
    let mut size = 8; // base overhead
    for item in array {
        size += estimate_size(item) as usize + 8; // item + pointer overhead
    }
    size
}

fn estimate_object_size(obj: &HashMap<String, DixValue>) -> usize {
    let mut size = 16; // base overhead
    for (key, value) in obj {
        size += key.len() * 2;               // key string
        size += estimate_size(value) as usize; // value
        size += 16;                           // hashmap entry overhead
    }
    size
}

/// True when the value is considered "empty" for its type.
///
/// - Null        → always empty
/// - Int/Long    → empty when zero
/// - Float/Double → empty when zero
/// - Bool        → empty when false
/// - String      → empty when ""
/// - Array/Object → empty when len == 0
/// - Everything else → not empty
fn is_empty(value: &DixValue) -> bool {
    match value.get_type() {
        DixType::Null      => true,
        DixType::Int       => value.as_int()  == 0,
        DixType::Long      => value.as_long() == 0,
        DixType::Float     => value.as_float()  == 0.0,
        DixType::Double    => value.as_double() == 0.0,
        DixType::Bool      => !value.as_bool(),
        DixType::String    => value.as_string().is_empty(),
        DixType::Array     => value.as_array().is_empty(),
        DixType::Object    => value.as_object().is_empty(),
        _                  => false,
    }
}

/// Build a multi-line debug string for a value.
fn get_debug_info(value: &DixValue) -> String {
    let mut info = String::new();
    info.push_str(&format!("Type: {}\n",          value.get_type()));
    info.push_str(&format!("Value: {}\n",         value));
    info.push_str(&format!("IsNull: {}\n",        value.is_null()));
    info.push_str(&format!("IsNumeric: {}\n",     value.is_numeric()));
    info.push_str(&format!("IsEmpty: {}\n",       is_empty(value)));
    info.push_str(&format!("EstimatedSize: {} bytes\n", estimate_size(value)));

    match value.get_type() {
        DixType::Array  => info.push_str(&format!("Length: {}\n",   value.as_array().len())),
        DixType::Object => info.push_str(&format!("Keys: {}\n",     value.as_object().len())),
        DixType::String => info.push_str(&format!("Length: {}\n",   value.as_string().len())),
        DixType::Long   => info.push_str(&format!("Fits i32: {}\n",
            value.as_long() >= i32::MIN as i64 && value.as_long() <= i32::MAX as i64)),
        _ => {}
    }

    info.trim_end().to_string()
}

/// Convert a DixValue to a JSON-compatible string.
///
/// Long values are emitted as bare integers (no `L` suffix) because JSON
/// represents all numbers the same way. Very large i64 values that exceed
/// JavaScript's safe integer range (> 2^53) are emitted as strings to avoid
/// precision loss in JSON consumers that use f64 for all numbers.
fn to_json_string(value: &DixValue) -> String {
    const JS_SAFE_MAX: i64 =  9_007_199_254_740_991_i64; // 2^53 - 1
    const JS_SAFE_MIN: i64 = -9_007_199_254_740_991_i64;

    match value.get_type() {
        DixType::Null      => "null".to_string(),
        DixType::String    => format!("\"{}\"", value.as_string()),
        DixType::Bool      => value.as_bool().to_string().to_lowercase(),
        DixType::Int       => value.as_int().to_string(),
        DixType::Long      => {
            let l = value.as_long();
            // Emit as number when safe, string otherwise (preserves precision)
            if (JS_SAFE_MIN..=JS_SAFE_MAX).contains(&l) {
                l.to_string()
            } else {
                format!("\"{}\"", l)
            }
        }
        DixType::Float     => format!("{:.6}", value.as_float()),
        DixType::Double    => format!("{:.6}", value.as_double()),
        DixType::Array     => {
            let items: Vec<String> = value.as_array()
                .iter()
                .map(to_json_string)
                .collect();
            format!("[{}]", items.join(","))
        }
        DixType::Object    => {
            let items: Vec<String> = value.as_object()
                .iter()
                .map(|(k, v)| format!("\"{}\":{}", k, to_json_string(v)))
                .collect();
            format!("{{{}}}", items.join(","))
        }
        DixType::Date      => format!("\"{}\"", value.as_datetime().format("%Y-%m-%d")),
        DixType::Timestamp => format!("\"{}\"", value.as_datetime().format("%Y-%m-%dT%H:%M:%S%.3fZ")),
        _                  => format!("\"{}\"", value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_string() {
        let methods = get_methods();
        let result = methods.get("toString").unwrap()
            .call(&[DixValue::from_int(42)]).unwrap();
        assert_eq!(result.as_string(), "42");
    }

    #[test]
    fn test_type_name_long() {
        let methods = get_methods();
        let result = methods.get("type").unwrap()
            .call(&[DixValue::from_long(123_i64)]).unwrap();
        assert_eq!(result.as_string(), "long");
    }

    #[test]
    fn test_is_empty_long_zero() {
        let methods = get_methods();
        let empty = methods.get("isEmpty").unwrap()
            .call(&[DixValue::from_long(0_i64)]).unwrap();
        assert!(empty.as_bool());

        let not_empty = methods.get("isEmpty").unwrap()
            .call(&[DixValue::from_long(1_i64)]).unwrap();
        assert!(!not_empty.as_bool());
    }

    #[test]
    fn test_is_empty_int_zero() {
        let methods = get_methods();
        let result = methods.get("isEmpty").unwrap()
            .call(&[DixValue::from_int(0)]).unwrap();
        assert!(result.as_bool());
    }

    #[test]
    fn test_is_empty_string() {
        let methods = get_methods();
        let empty = methods.get("isEmpty").unwrap()
            .call(&[DixValue::from_string(String::new())]).unwrap();
        assert!(empty.as_bool());

        let not_empty = methods.get("isEmpty").unwrap()
            .call(&[DixValue::from_string("hello".to_string())]).unwrap();
        assert!(!not_empty.as_bool());
    }

    #[test]
    fn test_is_empty_array() {
        let methods = get_methods();
        let empty = methods.get("isEmpty").unwrap()
            .call(&[DixValue::from_array(vec![])]).unwrap();
        assert!(empty.as_bool());
    }

    #[test]
    fn test_json_int() {
        let methods = get_methods();
        let result = methods.get("json").unwrap()
            .call(&[DixValue::from_int(42)]).unwrap();
        assert_eq!(result.as_string(), "42");
    }

    #[test]
    fn test_json_long_safe_range() {
        let methods = get_methods();
        // Within JS safe integer range — emitted as bare number
        let result = methods.get("json").unwrap()
            .call(&[DixValue::from_long(9_000_000_000_i64)]).unwrap();
        assert_eq!(result.as_string(), "9000000000");
    }

    #[test]
    fn test_json_long_unsafe_range_emits_string() {
        let methods = get_methods();
        // Exceeds 2^53-1 — emitted as JSON string to preserve precision
        let big = 9_999_999_999_999_999_i64;
        let result = methods.get("json").unwrap()
            .call(&[DixValue::from_long(big)]).unwrap();
        assert_eq!(result.as_string(), format!("\"{}\"", big));
    }

    #[test]
    fn test_json_string() {
        let methods = get_methods();
        let result = methods.get("json").unwrap()
            .call(&[DixValue::from_string("hello".to_string())]).unwrap();
        assert_eq!(result.as_string(), "\"hello\"");
    }

    #[test]
    fn test_json_bool() {
        let methods = get_methods();
        let result = methods.get("json").unwrap()
            .call(&[DixValue::from_bool(true)]).unwrap();
        assert_eq!(result.as_string(), "true");
    }

    #[test]
    fn test_size_long() {
        let methods = get_methods();
        let result = methods.get("size").unwrap()
            .call(&[DixValue::from_long(0_i64)]).unwrap();
        assert_eq!(result.as_int(), 8); // i64 is 8 bytes
    }

    #[test]
    fn test_size_int() {
        let methods = get_methods();
        let result = methods.get("size").unwrap()
            .call(&[DixValue::from_int(0)]).unwrap();
        assert_eq!(result.as_int(), 4); // i32 is 4 bytes
    }

    #[test]
    fn test_to_bytes_long() {
        let methods = get_methods();
        let result = methods.get("toBytes").unwrap()
            .call(&[DixValue::from_long(1_i64)]).unwrap();
        // i64 little-endian: 1 followed by 7 zeros
        let arr = result.as_array();
        assert_eq!(arr.len(), 8);
        assert_eq!(arr[0].as_int(), 1);
        assert_eq!(arr[1].as_int(), 0);
    }

    #[test]
    fn test_is_numeric_long() {
        let methods = get_methods();
        let result = methods.get("isNumeric").unwrap()
            .call(&[DixValue::from_long(42_i64)]).unwrap();
        assert!(result.as_bool());
    }

    #[test]
    fn test_is_numeric_string_false() {
        let methods = get_methods();
        let result = methods.get("isNumeric").unwrap()
            .call(&[DixValue::from_string("42".to_string())]).unwrap();
        assert!(!result.as_bool());
    }

    #[test]
    fn test_debug_long_shows_fits_in_int() {
        let methods = get_methods();
        // Small long — fits in i32
        let result = methods.get("debug").unwrap()
            .call(&[DixValue::from_long(42_i64)]).unwrap();
        assert!(result.as_string().contains("Fits i32: true"));

        // Large long — does not fit
        let result2 = methods.get("debug").unwrap()
            .call(&[DixValue::from_long(i64::MAX)]).unwrap();
        assert!(result2.as_string().contains("Fits i32: false"));
    }

    #[test]
    fn test_equals_long_and_int_same_value() {
        let methods = get_methods();
        // Long(42) and Int(42) — equal_to uses as_long comparison for mixed int/long
        let result = methods.get("equals").unwrap()
            .call(&[DixValue::from_long(42_i64), DixValue::from_int(42)]).unwrap();
        assert!(result.as_bool());
    }

    #[test]
    fn test_default_if_null_with_long() {
        let methods = get_methods();
        let result = methods.get("defaultIfNull").unwrap()
            .call(&[DixValue::null(), DixValue::from_long(99_i64)]).unwrap();
        assert_eq!(result.as_long(), 99_i64);
    }
                }
