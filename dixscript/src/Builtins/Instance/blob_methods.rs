// dixscript/src/Builtins/Instance/blob_methods.rs
//! Blob instance methods for DixScript
//! Provides methods for working with binary data stored as base64

use crate::Builtins::Core::{DixValue, DixType, IBuiltinMethod, BuiltinMethod};
use base64::{Engine as _, engine::general_purpose};
use std::collections::HashMap;

// ── Magic-byte MIME detection ─────────────────────────────────────────────────
//
// Inspects the leading bytes of decoded blob data to identify common file types.
// Returns "application/octet-stream" when no signature matches.
//
// References:
//   PNG  — https://www.w3.org/TR/PNG/#5PNG-file-signature  (8 bytes)
//   JPEG — ISO/IEC 10918-1, SOI marker FF D8, followed by FF
//   GIF  — GIF87a / GIF89a: bytes 0-3 = "GIF8"
//   PDF  — ISO 32000: bytes 0-3 = "%PDF"
//   ZIP  — PK\x03\x04 (covers docx, xlsx, jar, apk, …)
//   WEBP — "RIFF" at 0-3, "WEBP" at 8-11
//   BMP  — "BM" at 0-1
//   ICO  — 00 00 01 00

fn detect_mime_type(bytes: &[u8]) -> &'static str {
    let len = bytes.len();

    // PNG: 89 50 4E 47 0D 0A 1A 0A (8 bytes)
    if len >= 8
        && bytes[0] == 0x89
        && bytes[1] == 0x50
        && bytes[2] == 0x4E
        && bytes[3] == 0x47
        && bytes[4] == 0x0D
        && bytes[5] == 0x0A
        && bytes[6] == 0x1A
        && bytes[7] == 0x0A
    {
        return "image/png";
    }

    // JPEG: FF D8 FF
    if len >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return "image/jpeg";
    }

    // GIF: 47 49 46 38 ("GIF8")
    if len >= 4 && bytes[0] == 0x47 && bytes[1] == 0x49 && bytes[2] == 0x46 && bytes[3] == 0x38 {
        return "image/gif";
    }

    // PDF: 25 50 44 46 ("%PDF")
    if len >= 4 && bytes[0] == 0x25 && bytes[1] == 0x50 && bytes[2] == 0x44 && bytes[3] == 0x46 {
        return "application/pdf";
    }

    // ZIP / PK: 50 4B 03 04
    if len >= 4 && bytes[0] == 0x50 && bytes[1] == 0x4B && bytes[2] == 0x03 && bytes[3] == 0x04 {
        return "application/zip";
    }

    // WEBP: "RIFF" at [0..4] and "WEBP" at [8..12]
    if len >= 12
        && bytes[0] == 0x52
        && bytes[1] == 0x49
        && bytes[2] == 0x46
        && bytes[3] == 0x46
        && bytes[8]  == 0x57
        && bytes[9]  == 0x45
        && bytes[10] == 0x42
        && bytes[11] == 0x50
    {
        return "image/webp";
    }

    // BMP: 42 4D ("BM")
    if len >= 2 && bytes[0] == 0x42 && bytes[1] == 0x4D {
        return "image/bmp";
    }

    // ICO: 00 00 01 00
    if len >= 4
        && bytes[0] == 0x00
        && bytes[1] == 0x00
        && bytes[2] == 0x01
        && bytes[3] == 0x00
    {
        return "image/x-icon";
    }

    "application/octet-stream"
}

// ── Method registry ───────────────────────────────────────────────────────────

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
    //
    // FIX (Group H): Previously delegated to get_blob_metadata() which always
    // returned "application/octet-stream". Now decodes the blob bytes directly
    // and runs magic-byte detection via detect_mime_type().
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
                let bytes = blob.as_blob_bytes()?;
                let mime = detect_mime_type(&bytes);
                Ok(DixValue::from_string(mime.to_string()))
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

                let sliced  = &bytes[start_idx..end_idx];
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
        // PNG magic bytes: 89 50 4E 47 0D 0A 1A 0A
        let png_bytes = [0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let png_header = general_purpose::STANDARD.encode(png_bytes);
        let blob = DixValue::from_blob(png_header).unwrap();
        let methods = get_methods();
        let mime_method = methods.get("mimeType").unwrap();
        let result = mime_method.call(&[blob]).unwrap();
        assert_eq!(result.as_string(), "image/png");
    }

    #[test]
    fn test_detect_mime_type_jpeg() {
        let jpeg_bytes = [0xFFu8, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        assert_eq!(detect_mime_type(&jpeg_bytes), "image/jpeg");
    }

    #[test]
    fn test_detect_mime_type_gif() {
        let gif_bytes = b"GIF89a\x01\x00";
        assert_eq!(detect_mime_type(gif_bytes), "image/gif");
    }

    #[test]
    fn test_detect_mime_type_pdf() {
        let pdf_bytes = b"%PDF-1.7";
        assert_eq!(detect_mime_type(pdf_bytes), "application/pdf");
    }

    #[test]
    fn test_detect_mime_type_zip() {
        let zip_bytes = [0x50u8, 0x4B, 0x03, 0x04, 0x14, 0x00];
        assert_eq!(detect_mime_type(&zip_bytes), "application/zip");
    }

    #[test]
    fn test_detect_mime_type_webp() {
        let mut webp = [0u8; 12];
        webp[0..4].copy_from_slice(b"RIFF");
        webp[4..8].copy_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        webp[8..12].copy_from_slice(b"WEBP");
        assert_eq!(detect_mime_type(&webp), "image/webp");
    }

    #[test]
    fn test_detect_mime_type_unknown_returns_octet_stream() {
        let unknown = [0x00u8, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        assert_eq!(detect_mime_type(&unknown), "application/octet-stream");
    }

    #[test]
    fn test_detect_mime_type_empty() {
        assert_eq!(detect_mime_type(&[]), "application/octet-stream");
    }

    #[test]
    fn test_blob_mime_type_unknown_data() {
        // Random data that doesn't match any signature
        let random = general_purpose::STANDARD.encode([0x01u8, 0x02, 0x03, 0x04]);
        let blob = DixValue::from_blob(random).unwrap();
        let methods = get_methods();
        let mime_method = methods.get("mimeType").unwrap();
        let result = mime_method.call(&[blob]).unwrap();
        assert_eq!(result.as_string(), "application/octet-stream");
    }
        }
