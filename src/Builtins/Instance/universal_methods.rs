// src/Builtins/Instance/universal_methods.rs
//! Universal instance methods available on all DixScript types

use crate::Builtins::Core::{DixType, DixValue, BuiltinMethod, IBuiltinMethod};
use std::collections::HashMap;

/// Get all universal methods
pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // Universal.toString() - Convert any value to string
    methods.insert(
        "toString".to_string(),
        Box::new(BuiltinMethod::new(
            "toString".to_string(),
            1,
            DixType::String,
            |args| {
                let value = &args[0];
                Ok(DixValue::from_string(value.as_string()))
            },
            "Converts the value to its string representation".to_string(),
        )),
    );

    // Universal.type() - Get type name
    methods.insert(
        "type".to_string(),
        Box::new(BuiltinMethod::new(
            "type".to_string(),
            1,
            DixType::String,
            |args| {
                let value = &args[0];
                Ok(DixValue::from_string(value.get_type().get_type_name().to_string()))
            },
            "Returns the type name of the value".to_string(),
        )),
    );

    // Universal.isNull() - Check if value is null
    methods.insert(
        "isNull".to_string(),
        Box::new(BuiltinMethod::new(
            "isNull".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = &args[0];
                Ok(DixValue::from_bool(value.is_null()))
            },
            "Checks if the value is null".to_string(),
        )),
    );

    // Universal.isNotNull() - Check if value is not null
    methods.insert(
        "isNotNull".to_string(),
        Box::new(BuiltinMethod::new(
            "isNotNull".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = &args[0];
                Ok(DixValue::from_bool(!value.is_null()))
            },
            "Checks if the value is not null".to_string(),
        )),
    );

    // Universal.equals(other) - Check equality
    methods.insert(
        "equals".to_string(),
        Box::new(BuiltinMethod::new(
            "equals".to_string(),
            2,
            DixType::Bool,
            |args| {
                let value1 = &args[0];
                let value2 = &args[1];
                Ok(DixValue::from_bool(value1.equal_to(value2)))
            },
            "Checks if this value equals another value".to_string(),
        )),
    );

    // Universal.notEquals(other) - Check inequality
    methods.insert(
        "notEquals".to_string(),
        Box::new(BuiltinMethod::new(
            "notEquals".to_string(),
            2,
            DixType::Bool,
            |args| {
                let value1 = &args[0];
                let value2 = &args[1];
                Ok(DixValue::from_bool(!value1.equal_to(value2)))
            },
            "Checks if this value does not equal another value".to_string(),
        )),
    );

    // Universal.hashCode() - Get hash code
    methods.insert(
        "hashCode".to_string(),
        Box::new(BuiltinMethod::new(
            "hashCode".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = &args[0];
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};

                let mut hasher = DefaultHasher::new();
                // Hash the string representation as a simple approach
                value.to_string().hash(&mut hasher);
                let hash = hasher.finish();
                Ok(DixValue::from_int((hash % (i32::MAX as u64)) as i32))
            },
            "Returns the hash code of the value".to_string(),
        )),
    );

    // Universal.clone() - Deep clone the value
    methods.insert(
        "clone".to_string(),
        Box::new(BuiltinMethod::new(
            "clone".to_string(),
            1,
            DixType::String, // Returns same type as input
            |args| {
                let value = &args[0];
                Ok(value.deep_clone())
            },
            "Creates a deep copy of the value".to_string(),
        )),
    );

    // Universal.toBytes() - Convert to byte array representation
    methods.insert(
        "toBytes".to_string(),
        Box::new(BuiltinMethod::new(
            "toBytes".to_string(),
            1,
            DixType::Array,
            |args| {
                let value = &args[0];
                let bytes = convert_to_bytes(value);
                let byte_values: Vec<DixValue> = bytes.iter()
                    .map(|b| DixValue::from_int(*b as i32))
                    .collect();
                Ok(DixValue::from_array(byte_values))
            },
            "Converts the value to a byte array representation".to_string(),
        )),
    );

    // Universal.size() - Get memory size estimate
    methods.insert(
        "size".to_string(),
        Box::new(BuiltinMethod::new(
            "size".to_string(),
            1,
            DixType::Int,
            |args| {
                let value = &args[0];
                let size = estimate_size(value);
                Ok(DixValue::from_int(size))
            },
            "Returns an estimate of the memory size of the value".to_string(),
        )),
    );

    // Universal.isEmpty() - Check if value is considered empty
    methods.insert(
        "isEmpty".to_string(),
        Box::new(BuiltinMethod::new(
            "isEmpty".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = &args[0];
                let is_empty = is_empty(value);
                Ok(DixValue::from_bool(is_empty))
            },
            "Checks if the value is considered empty (null, empty string, empty array, etc.)".to_string(),
        )),
    );

    // Universal.isNotEmpty() - Check if value is not empty
    methods.insert(
        "isNotEmpty".to_string(),
        Box::new(BuiltinMethod::new(
            "isNotEmpty".to_string(),
            1,
            DixType::Bool,
            |args| {
                let value = &args[0];
                let is_empty = is_empty(value);
                Ok(DixValue::from_bool(!is_empty))
            },
            "Checks if the value is not empty".to_string(),
        )),
    );

    // Universal.defaultIfNull(defaultValue) - Return default if null
    methods.insert(
        "defaultIfNull".to_string(),
        Box::new(BuiltinMethod::new(
            "defaultIfNull".to_string(),
            2,
            DixType::String, // Returns same type as input
            |args| {
                let value = &args[0];
                let default_value = &args[1];

                if value.is_null() {
                    Ok(default_value.deep_clone())
                } else {
                    Ok(value.deep_clone())
                }
            },
            "Returns the default value if this value is null, otherwise returns this value".to_string(),
        )),
    );

    // Universal.defaultIfEmpty(defaultValue) - Return default if empty
    methods.insert(
        "defaultIfEmpty".to_string(),
        Box::new(BuiltinMethod::new(
            "defaultIfEmpty".to_string(),
            2,
            DixType::String, // Returns same type as input
            |args| {
                let value = &args[0];
                let default_value = &args[1];

                if is_empty(value) {
                    Ok(default_value.deep_clone())
                } else {
                    Ok(value.deep_clone())
                }
            },
            "Returns the default value if this value is empty, otherwise returns this value".to_string(),
        )),
    );

    // Universal.debug() - Debug representation
    methods.insert(
        "debug".to_string(),
        Box::new(BuiltinMethod::new(
            "debug".to_string(),
            1,
            DixType::String,
            |args| {
                let value = &args[0];
                let debug_info = get_debug_info(value);
                Ok(DixValue::from_string(debug_info))
            },
            "Returns a detailed debug representation of the value".to_string(),
        )),
    );

    // Universal.json() - Convert to JSON-like string
    methods.insert(
        "json".to_string(),
        Box::new(BuiltinMethod::new(
            "json".to_string(),
            1,
            DixType::String,
            |args| {
                let value = &args[0];
                let json = to_json_string(value);
                Ok(DixValue::from_string(json))
            },
            "Converts the value to a JSON-like string representation".to_string(),
        )),
    );

    methods
}

// ==================== HELPER FUNCTIONS ====================

/// Convert a DixValue to bytes
fn convert_to_bytes(value: &DixValue) -> Vec<u8> {
    match value.get_type() {
        DixType::String => value.as_string().into_bytes(),
        DixType::Int => value.as_int().to_le_bytes().to_vec(),
        DixType::Float => value.as_float().to_le_bytes().to_vec(),
        DixType::Double => value.as_double().to_le_bytes().to_vec(),
        DixType::Bool => vec![if value.as_bool() { 1 } else { 0 }],
        DixType::Date | DixType::Timestamp => {
            let dt = value.as_datetime();
            dt.timestamp().to_le_bytes().to_vec()
        }
        DixType::Null => vec![],
        DixType::Array => convert_array_to_bytes(value.as_array()),
        DixType::Object => convert_object_to_bytes(value.as_object()),
        _ => value.to_string().into_bytes(),
    }
}

/// Convert array to bytes
fn convert_array_to_bytes(array: &Vec<DixValue>) -> Vec<u8> {
    let mut result = Vec::new();
    for item in array {
        let item_bytes = convert_to_bytes(item);
        // Add length prefix (4 bytes)
        result.extend_from_slice(&(item_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(&item_bytes);
    }
    result
}

/// Convert object to bytes
fn convert_object_to_bytes(obj: &HashMap<String, DixValue>) -> Vec<u8> {
    let mut result = Vec::new();
    for (key, value) in obj {
        let key_bytes = key.as_bytes();
        let value_bytes = convert_to_bytes(value);

        // Add key length + key
        result.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(key_bytes);

        // Add value length + value
        result.extend_from_slice(&(value_bytes.len() as u32).to_le_bytes());
        result.extend_from_slice(&value_bytes);
    }
    result
}

/// Estimate the size of a DixValue
fn estimate_size(value: &DixValue) -> i32 {
    let size = match value.get_type() {
        DixType::String => value.as_string().len() * 2, // Unicode estimate
        DixType::Int => std::mem::size_of::<i32>(),
        DixType::Float => std::mem::size_of::<f32>(),
        DixType::Double => std::mem::size_of::<f64>(),
        DixType::Bool => std::mem::size_of::<bool>(),
        DixType::Date | DixType::Timestamp => std::mem::size_of::<i64>(),
        DixType::Null => 0,
        DixType::Array => estimate_array_size(value.as_array()),
        DixType::Object => estimate_object_size(value.as_object()),
        _ => value.to_string().len() * 2,
    };
    size as i32
}

/// Estimate array size
fn estimate_array_size(array: &Vec<DixValue>) -> usize {
    let mut size = 8; // Base array overhead
    for item in array {
        size += estimate_size(item) as usize + 8; // Item + pointer overhead
    }
    size
}

/// Estimate object size
fn estimate_object_size(obj: &HashMap<String, DixValue>) -> usize {
    let mut size = 16; // Base dictionary overhead
    for (key, value) in obj {
        size += key.len() * 2; // Key
        size += estimate_size(value) as usize; // Value
        size += 16; // Dictionary entry overhead
    }
    size
}

/// Check if a value is considered empty
fn is_empty(value: &DixValue) -> bool {
    match value.get_type() {
        DixType::Null => true,
        DixType::String => value.as_string().is_empty(),
        DixType::Array => value.as_array().is_empty(),
        DixType::Object => value.as_object().is_empty(),
        DixType::Int => value.as_int() == 0,
        DixType::Float => value.as_float() == 0.0,
        DixType::Double => value.as_double() == 0.0,
        DixType::Bool => !value.as_bool(),
        _ => false,
    }
}

/// Get debug information about a value
fn get_debug_info(value: &DixValue) -> String {
    let mut info = String::new();

    info.push_str(&format!("Type: {}\n", value.get_type()));
    info.push_str(&format!("Value: {}\n", value));
    info.push_str(&format!("IsNull: {}\n", value.is_null()));
    info.push_str(&format!("IsEmpty: {}\n", is_empty(value)));
    info.push_str(&format!("EstimatedSize: {} bytes\n", estimate_size(value)));

    // Type-specific debug info
    match value.get_type() {
        DixType::Array => {
            info.push_str(&format!("ArrayLength: {}\n", value.as_array().len()));
        }
        DixType::Object => {
            info.push_str(&format!("ObjectKeys: {}\n", value.as_object().len()));
        }
        DixType::String => {
            info.push_str(&format!("StringLength: {}\n", value.as_string().len()));
        }
        _ => {}
    }

    info.trim_end().to_string()
}

/// Convert value to JSON-like string
fn to_json_string(value: &DixValue) -> String {
    match value.get_type() {
        DixType::Null => "null".to_string(),
        DixType::String => format!("\"{}\"", value.as_string()),
        DixType::Bool => value.as_bool().to_string().to_lowercase(),
        DixType::Int => value.as_int().to_string(),
        DixType::Float => format!("{:.6}", value.as_float()),
        DixType::Double => format!("{:.6}", value.as_double()),
        DixType::Array => {
            let items: Vec<String> = value.as_array()
                .iter()
                .map(to_json_string)
                .collect();
            format!("[{}]", items.join(","))
        }
        DixType::Object => {
            let items: Vec<String> = value.as_object()
                .iter()
                .map(|(k, v)| format!("\"{}\":{}", k, to_json_string(v)))
                .collect();
            format!("{{{}}}", items.join(","))
        }
        DixType::Date => {
            let dt = value.as_datetime();
            format!("\"{}\"", dt.format("%Y-%m-%d"))
        }
        DixType::Timestamp => {
            let dt = value.as_datetime();
            format!("\"{}\"", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ"))
        }
        _ => format!("\"{}\"", value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_string() {
        let methods = get_methods();
        let to_string = methods.get("toString").unwrap();

        let args = vec![DixValue::from_int(42)];
        let result = to_string.call(&args).unwrap();
        assert_eq!(result.as_string(), "42");
    }

    #[test]
    fn test_type() {
        let methods = get_methods();
        let type_method = methods.get("type").unwrap();

        let args = vec![DixValue::from_string("hello".to_string())];
        let result = type_method.call(&args).unwrap();
        assert_eq!(result.as_string(), "string");
    }

    #[test]
    fn test_is_null() {
        let methods = get_methods();
        let is_null = methods.get("isNull").unwrap();

        let args_null = vec![DixValue::null()];
        assert!(is_null.call(&args_null).unwrap().as_bool());

        let args_not_null = vec![DixValue::from_int(42)];
        assert!(!is_null.call(&args_not_null).unwrap().as_bool());
    }

    #[test]
    fn test_equals() {
        let methods = get_methods();
        let equals = methods.get("equals").unwrap();

        let args = vec![DixValue::from_int(42), DixValue::from_int(42)];
        assert!(equals.call(&args).unwrap().as_bool());

        let args_not_equal = vec![DixValue::from_int(42), DixValue::from_int(43)];
        assert!(!equals.call(&args_not_equal).unwrap().as_bool());
    }

    #[test]
    fn test_is_empty() {
        let methods = get_methods();
        let is_empty_method = methods.get("isEmpty").unwrap();

        let args_empty_string = vec![DixValue::from_string("".to_string())];
        assert!(is_empty_method.call(&args_empty_string).unwrap().as_bool());

        let args_empty_array = vec![DixValue::from_array(vec![])];
        assert!(is_empty_method.call(&args_empty_array).unwrap().as_bool());

        let args_not_empty = vec![DixValue::from_string("hello".to_string())];
        assert!(!is_empty_method.call(&args_not_empty).unwrap().as_bool());
    }

    #[test]
    fn test_json() {
        let methods = get_methods();
        let json = methods.get("json").unwrap();

        let args = vec![DixValue::from_int(42)];
        let result = json.call(&args).unwrap();
        assert_eq!(result.as_string(), "42");

        let args_string = vec![DixValue::from_string("hello".to_string())];
        let result = json.call(&args_string).unwrap();
        assert_eq!(result.as_string(), "\"hello\"");
    }
}