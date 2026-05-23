// dixscript/src/Builtins/Instance/blob_methods.rs
//! Blob instance methods for DixScript
//! Provides methods for working with binary data stored as base64

use crate::Builtins::Core::{DixValue, DixType, IBuiltinMethod, BuiltinMethod};
use base64::{Engine as _, engine::general_purpose};
use std::collections::HashMap;

/// Get all blob instance methods
pub fn get_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut methods: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // blob.size() - Get byte count
    methods.insert(
        "size".to_string(),
        Box::new(BuiltinMethod::new(
            "size".to_string(),
            1,
            DixType::Int,
            |args| {
                let blob = &args[0];
                if blob.get_type() != DixType::Blob {
                    return Err(format!("size() requires a blob, got {:?}", blob.get_type()));
                }
                let bytes = blob.as_blob_bytes()?;
                Ok(DixValue::from_int(bytes.len() as i32))
            },
            "Returns the size of the blob in bytes".to_string(),
        )),
    );

    // blob.mimeType() - Detect MIME type from magic bytes
    methods.insert(
        "mimeType".to_string(),
        Box::new(BuiltinMethod::new(
            "mimeType".to_string(),
            1,
            DixType::String,
            |args| {
                let blob = &args[0];
                if blob.get_type() != DixType::Blob {
                    return Err(format!("mimeType() requires a blob, got {:?}", blob.get_type()));
                }
                let (mime_type, _, _) = blob.get_blob_metadata()?;
                Ok(DixValue::from_string(mime_type))
            },
            "Detects and returns the MIME type based on magic bytes".to_string(),
        )),
    );

    // blob.toHex() - Convert to hexadecimal string
    methods.insert(
        "toHex".to_string(),
        Box::new(BuiltinMethod::new(
            "toHex".to_string(),
            1,
            DixType::String,
            |args| {
                let blob = &args[0];
                if blob.get_type() != DixType::Blob {
                    return Err(format!("toHex() requires a blob, got {:?}", blob.get_type()));
                }
                let bytes = blob.as_blob_bytes()?;
                let hex = bytes.iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>();
                Ok(DixValue::from_string(hex))
            },
            "Converts the blob to a hexadecimal string representation".to_string(),
        )),
    );

    // blob.isValid() - Check if blob contains valid base64 data
    methods.insert(
        "isValid".to_string(),
        Box::new(BuiltinMethod::new(
            "isValid".to_string(),
            1,
            DixType::Bool,
            |args| {
                let blob = &args[0];
                if blob.get_type() != DixType::Blob {
                    return Ok(DixValue::from_bool(false));
                }
                let is_valid = blob.as_blob_bytes().is_ok();
                Ok(DixValue::from_bool(is_valid))
            },
            "Checks if the blob contains valid base64-encoded data".to_string(),
        )),
    );

    // blob.slice(start, end) - Extract a portion of the blob
    methods.insert(
        "slice".to_string(),
        Box::new(BuiltinMethod::new(
            "slice".to_string(),
            3,
            DixType::Blob,
            |args| {
                let blob  = &args[0];
                let start = &args[1];
                let end   = &args[2];

                if blob.get_type() != DixType::Blob {
                    return Err(format!("slice() requires a blob, got {:?}", blob.get_type()));
                }
                if !start.is_numeric() || !end.is_numeric() {
                    return Err("slice() requires numeric start and end indices".to_string());
                }

                let bytes     = blob.as_blob_bytes()?;
                let start_idx = start.as_int().max(0) as usize;
                let end_idx   = end.as_int().max(0) as usize;

                if start_idx > bytes.len() {
                    return Err(format!(
                        "Start index {} out of bounds (size: {})",
                        start_idx, bytes.len()
                    ));
                }
                let end_idx = end_idx.min(bytes.len());
                if start_idx > end_idx {
                    return Err(format!(
                        "Start index {} cannot be greater than end index {}",
                        start_idx, end_idx
                    ));
                }

                let sliced = &bytes[start_idx..end_idx];
                // Use engine API — base64::encode() is deprecated since 0.21
                let encoded = general_purpose::STANDARD.encode(sliced);
                DixValue::from_blob(encoded)
            },
            "Returns a new blob containing bytes from start to end index (exclusive)".to_string(),
        )),
    );

    // blob.toBytes() - Convert blob to byte array
    methods.insert(
        "toBytes".to_string(),
        Box::new(BuiltinMethod::new(
            "toBytes".to_string(),
            1,
            DixType::Array,
            |args| {
                let blob = &args[0];
                if blob.get_type() != DixType::Blob {
                    return Err(format!("toBytes() requires a blob, got {:?}", blob.get_type()));
                }
                let bytes = blob.as_blob_bytes()?;
                let byte_values: Vec<DixValue> = bytes.iter()
                    .map(|&b| DixValue::from_int(b as i32))
                    .collect();
                Ok(DixValue::from_array(byte_values))
            },
            "Converts the blob to an array of byte values (0-255)".to_string(),
        )),
    );

    methods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_size() {
        // "Hello" in base64 is "SGVsbG8="
        let blob = DixValue::from_blob("SGVsbG8=".to_string()).unwrap();
        let methods = get_methods();
        let size_method = methods.get("size").unwrap();
        let result = size_method.call(&[blob]).unwrap();
        assert_eq!(result.as_int(), 5);
    }

    #[test]
    fn test_blob_to_hex() {
        // "Hi" in base64 is "SGk="
        let blob = DixValue::from_blob("SGk=".to_string()).unwrap();
        let methods = get_methods();
        let to_hex_method = methods.get("toHex").unwrap();
        let result = to_hex_method.call(&[blob]).unwrap();
        assert_eq!(result.as_string(), "4869");
    }

    #[test]
    fn test_blob_is_valid() {
        let valid_blob = DixValue::from_blob("SGVsbG8=".to_string()).unwrap();
        let methods = get_methods();
        let is_valid_method = methods.get("isValid").unwrap();
        let result = is_valid_method.call(&[valid_blob]).unwrap();
        assert!(result.as_bool());
    }

    #[test]
    fn test_blob_slice() {
        // "Hello World" in base64
        let blob = DixValue::from_blob("SGVsbG8gV29ybGQ=".to_string()).unwrap();
        let methods = get_methods();
        let slice_method = methods.get("slice").unwrap();
        let result = slice_method.call(&[
            blob,
            DixValue::from_int(0),
            DixValue::from_int(5),
        ]).unwrap();
        let bytes = result.as_blob_bytes().unwrap();
        assert_eq!(bytes, b"Hello");
    }

    #[test]
    fn test_blob_mime_type() {
        // PNG magic bytes in base64 — use engine API
        let png_bytes = [0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let png_header = general_purpose::STANDARD.encode(png_bytes);
        let blob = DixValue::from_blob(png_header).unwrap();
        let methods = get_methods();
        let mime_method = methods.get("mimeType").unwrap();
        let result = mime_method.call(&[blob]).unwrap();
        assert_eq!(result.as_string(), "image/png");
    }
}
