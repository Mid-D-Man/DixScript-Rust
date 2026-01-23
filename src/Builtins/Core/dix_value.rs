// src/Builtins/Core/dix_value.rs
//! DixValue - Runtime value type for DixScript
//!
//! Handles all value types and conversions for the builtin system.
//! CRITICAL: This is a hot-path type - avoid cloning in loops!

use super::dix_type::DixType;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::cmp::Ordering;

/// Core value type for DixScript runtime
#[derive(Debug, Clone, PartialEq)]
pub struct DixValue {
    value: ValueData,
    dix_type: DixType,
}

/// Internal value storage (uses Box for large types to keep DixValue small)
#[derive(Debug, Clone, PartialEq)]
enum ValueData {
    Int(i32),
    Float(f32),
    Double(f64),
    String(String),
    Bool(bool),
    Array(Box<Vec<DixValue>>),
    Tuple(Box<Vec<DixValue>>),
    Object(Box<HashMap<String, DixValue>>),
    Date(DateTime<Utc>),
    Timestamp(DateTime<Utc>),
    Regex(String),   // Store pattern as string
    Blob(String),    // Store as base64 string
    Hex(String),     // Store as hex color string
    Null,
}

impl DixValue {
    // ==================== CONSTRUCTORS ====================

    pub fn new(value: ValueData, dix_type: DixType) -> Self {
        DixValue { value, dix_type }
    }

    pub fn from_int(value: i32) -> Self {
        DixValue {
            value: ValueData::Int(value),
            dix_type: DixType::Int,
        }
    }

    pub fn from_float(value: f32) -> Self {
        DixValue {
            value: ValueData::Float(value),
            dix_type: DixType::Float,
        }
    }

    pub fn from_double(value: f64) -> Self {
        DixValue {
            value: ValueData::Double(value),
            dix_type: DixType::Double,
        }
    }

    pub fn from_string(value: String) -> Self {
        DixValue {
            value: ValueData::String(value),
            dix_type: DixType::String,
        }
    }

    pub fn from_bool(value: bool) -> Self {
        DixValue {
            value: ValueData::Bool(value),
            dix_type: DixType::Bool,
        }
    }

    pub fn from_array(values: Vec<DixValue>) -> Self {
        DixValue {
            value: ValueData::Array(Box::new(values)),
            dix_type: DixType::Array,
        }
    }

    pub fn from_tuple(values: Vec<DixValue>) -> Self {
        DixValue {
            value: ValueData::Tuple(Box::new(values)),
            dix_type: DixType::Tuple,
        }
    }

    pub fn from_object(obj: HashMap<String, DixValue>) -> Self {
        DixValue {
            value: ValueData::Object(Box::new(obj)),
            dix_type: DixType::Object,
        }
    }

    pub fn from_date(date: DateTime<Utc>) -> Self {
        DixValue {
            value: ValueData::Date(date),
            dix_type: DixType::Date,
        }
    }

    pub fn from_timestamp(timestamp: DateTime<Utc>) -> Self {
        DixValue {
            value: ValueData::Timestamp(timestamp),
            dix_type: DixType::Timestamp,
        }
    }

    pub fn from_regex(pattern: String) -> Result<Self, String> {
        // Validate regex pattern
        regex::Regex::new(&pattern)
            .map_err(|e| format!("Invalid regex pattern: {}", e))?;

        Ok(DixValue {
            value: ValueData::Regex(pattern),
            dix_type: DixType::Regex,
        })
    }

    pub fn from_blob(base64_data: String) -> Result<Self, String> {
        // Validate base64
        base64::decode(&base64_data)
            .map_err(|e| format!("Invalid base64 blob data: {}", e))?;

        Ok(DixValue {
            value: ValueData::Blob(base64_data),
            dix_type: DixType::Blob,
        })
    }

    pub fn from_hex(hex_color: String) -> Self {
        DixValue {
            value: ValueData::Hex(hex_color),
            dix_type: DixType::Hex,
        }
    }

    pub fn null() -> Self {
        DixValue {
            value: ValueData::Null,
            dix_type: DixType::Null,
        }
    }

    // ==================== TYPE QUERIES ====================

    #[inline]
    pub fn get_type(&self) -> DixType {
        self.dix_type
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.dix_type == DixType::Null
    }

    #[inline]
    pub fn is_numeric(&self) -> bool {
        self.dix_type.is_numeric()
    }

    #[inline]
    pub fn is_string(&self) -> bool {
        self.dix_type == DixType::String
    }

    #[inline]
    pub fn is_array(&self) -> bool {
        self.dix_type == DixType::Array
    }

    #[inline]
    pub fn is_object(&self) -> bool {
        self.dix_type == DixType::Object
    }

    // ==================== TYPE CONVERSIONS ====================

    pub fn as_string(&self) -> String {
        match &self.value {
            ValueData::String(s) => s.clone(),
            ValueData::Null => "null".to_string(),
            ValueData::Bool(b) => b.to_string().to_lowercase(),
            ValueData::Int(i) => i.to_string(),
            ValueData::Float(f) => f.to_string(),
            ValueData::Double(d) => d.to_string(),
            ValueData::Date(dt) => dt.format("%Y-%m-%d").to_string(),
            ValueData::Timestamp(dt) => dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            ValueData::Regex(p) => p.clone(),
            ValueData::Blob(b) => b.clone(),
            ValueData::Hex(h) => h.clone(),
            ValueData::Array(_) => "[...]".to_string(),
            ValueData::Tuple(_) => "t:(...)".to_string(),
            ValueData::Object(_) => "{...}".to_string(),
        }
    }

    pub fn as_int(&self) -> i32 {
        match &self.value {
            ValueData::Int(i) => *i,
            ValueData::Float(f) => *f as i32,
            ValueData::Double(d) => *d as i32,
            ValueData::String(s) => s.parse().unwrap_or(0),
            ValueData::Bool(b) => if *b { 1 } else { 0 },
            _ => 0,
        }
    }

    pub fn as_float(&self) -> f32 {
        match &self.value {
            ValueData::Float(f) => *f,
            ValueData::Int(i) => *i as f32,
            ValueData::Double(d) => *d as f32,
            ValueData::String(s) => s.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn as_double(&self) -> f64 {
        match &self.value {
            ValueData::Double(d) => *d,
            ValueData::Float(f) => *f as f64,
            ValueData::Int(i) => *i as f64,
            ValueData::String(s) => s.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn as_bool(&self) -> bool {
        match &self.value {
            ValueData::Bool(b) => *b,
            ValueData::Int(i) => *i != 0,
            ValueData::Float(f) => *f != 0.0,
            ValueData::Double(d) => *d != 0.0,
            ValueData::String(s) => !s.is_empty(),
            ValueData::Null => false,
            ValueData::Array(arr) => !arr.is_empty(),
            _ => true,
        }
    }

    pub fn as_array(&self) -> &Vec<DixValue> {
        match &self.value {
            ValueData::Array(arr) => arr,
            ValueData::Tuple(tup) => tup,
            _ => panic!("Cannot convert {:?} to array", self.dix_type),
        }
    }

    pub fn as_array_mut(&mut self) -> &mut Vec<DixValue> {
        match &mut self.value {
            ValueData::Array(arr) => arr,
            ValueData::Tuple(tup) => tup,
            _ => panic!("Cannot convert {:?} to array", self.dix_type),
        }
    }

    pub fn as_object(&self) -> &HashMap<String, DixValue> {
        match &self.value {
            ValueData::Object(obj) => obj,
            _ => panic!("Cannot convert {:?} to object", self.dix_type),
        }
    }

    pub fn as_object_mut(&mut self) -> &mut HashMap<String, DixValue> {
        match &mut self.value {
            ValueData::Object(obj) => obj,
            _ => panic!("Cannot convert {:?} to object", self.dix_type),
        }
    }

    pub fn as_datetime(&self) -> DateTime<Utc> {
        match &self.value {
            ValueData::Date(dt) | ValueData::Timestamp(dt) => *dt,
            ValueData::String(s) => {
                s.parse::<DateTime<Utc>>()
                    .unwrap_or_else(|_| Utc::now())
            }
            _ => Utc::now(),
        }
    }

    // ==================== ARITHMETIC OPERATIONS ====================

    pub fn add(&self, other: &DixValue) -> Result<DixValue, String> {
        if self.is_numeric() && other.is_numeric() {
            return Ok(match (self.dix_type, other.dix_type) {
                (DixType::Double, _) | (_, DixType::Double) => {
                    DixValue::from_double(self.as_double() + other.as_double())
                }
                (DixType::Float, _) | (_, DixType::Float) => {
                    DixValue::from_float(self.as_float() + other.as_float())
                }
                _ => DixValue::from_int(self.as_int() + other.as_int()),
            });
        }

        if self.is_string() || other.is_string() {
            return Ok(DixValue::from_string(
                self.as_string() + &other.as_string(),
            ));
        }

        if self.is_array() && other.is_array() {
            let mut combined = self.as_array().clone();
            combined.extend(other.as_array().iter().cloned());
            return Ok(DixValue::from_array(combined));
        }

        Err(format!(
            "Cannot add {:?} and {:?}",
            self.dix_type, other.dix_type
        ))
    }

    pub fn subtract(&self, other: &DixValue) -> Result<DixValue, String> {
        if !self.is_numeric() || !other.is_numeric() {
            return Err(format!(
                "Cannot subtract {:?} from {:?}",
                other.dix_type, self.dix_type
            ));
        }

        Ok(match (self.dix_type, other.dix_type) {
            (DixType::Double, _) | (_, DixType::Double) => {
                DixValue::from_double(self.as_double() - other.as_double())
            }
            (DixType::Float, _) | (_, DixType::Float) => {
                DixValue::from_float(self.as_float() - other.as_float())
            }
            _ => DixValue::from_int(self.as_int() - other.as_int()),
        })
    }

    pub fn multiply(&self, other: &DixValue) -> Result<DixValue, String> {
        if !self.is_numeric() || !other.is_numeric() {
            return Err(format!(
                "Cannot multiply {:?} and {:?}",
                self.dix_type, other.dix_type
            ));
        }

        Ok(match (self.dix_type, other.dix_type) {
            (DixType::Double, _) | (_, DixType::Double) => {
                DixValue::from_double(self.as_double() * other.as_double())
            }
            (DixType::Float, _) | (_, DixType::Float) => {
                DixValue::from_float(self.as_float() * other.as_float())
            }
            _ => DixValue::from_int(self.as_int() * other.as_int()),
        })
    }

    pub fn divide(&self, other: &DixValue) -> Result<DixValue, String> {
        if !self.is_numeric() || !other.is_numeric() {
            return Err(format!(
                "Cannot divide {:?} by {:?}",
                self.dix_type, other.dix_type
            ));
        }

        let divisor = other.as_double();
        if divisor == 0.0 {
            return Err("Division by zero".to_string());
        }

        Ok(DixValue::from_double(self.as_double() / divisor))
    }

    // ==================== COMPARISON OPERATIONS ====================

    pub fn compare_to(&self, other: &DixValue) -> Result<Ordering, String> {
        // Allow comparison between different numeric types
        if self.is_numeric() && other.is_numeric() {
            let left = self.as_double();
            let right = other.as_double();
            return Ok(left.partial_cmp(&right).unwrap_or(Ordering::Equal));
        }

        // For non-numeric types, require exact type match
        if self.dix_type != other.dix_type {
            return Err(format!(
                "Cannot compare {:?} with {:?}",
                self.dix_type, other.dix_type
            ));
        }

        Ok(match &self.value {
            ValueData::String(s1) => {
                if let ValueData::String(s2) = &other.value {
                    s1.cmp(s2)
                } else {
                    Ordering::Equal
                }
            }
            ValueData::Bool(b1) => {
                if let ValueData::Bool(b2) = &other.value {
                    b1.cmp(b2)
                } else {
                    Ordering::Equal
                }
            }
            ValueData::Date(dt1) | ValueData::Timestamp(dt1) => {
                let dt2 = other.as_datetime();
                dt1.cmp(&dt2)
            }
            _ => {
                return Err(format!("Type {:?} is not comparable", self.dix_type));
            }
        })
    }

    pub fn equal_to(&self, other: &DixValue) -> bool {
        // Handle numeric type coercion with epsilon
        if self.is_numeric() && other.is_numeric() {
            const EPSILON: f64 = 1e-10;
            (self.as_double() - other.as_double()).abs() < EPSILON
        } else {
            // FIX: Use PartialEq::eq explicitly instead of ==
            PartialEq::eq(self, other)
        }
    }

    pub fn greater_than(&self, other: &DixValue) -> Result<bool, String> {
        Ok(self.compare_to(other)? == Ordering::Greater)
    }

    pub fn less_than(&self, other: &DixValue) -> Result<bool, String> {
        Ok(self.compare_to(other)? == Ordering::Less)
    }

    // ==================== BLOB METHODS ====================

    /// Get blob as base64 string
    pub fn as_blob_base64(&self) -> Result<String, String> {
        match &self.value {
            ValueData::Blob(b) => Ok(b.clone()),
            _ => Err(format!("Cannot get blob data from {:?}", self.dix_type)),
        }
    }

    /// Get blob as byte array
    pub fn as_blob_bytes(&self) -> Result<Vec<u8>, String> {
        match &self.value {
            ValueData::Blob(b) => {
                base64::decode(b)
                    .map_err(|e| format!("Failed to decode blob: {}", e))
            }
            _ => Err(format!("Cannot convert {:?} to byte array", self.dix_type)),
        }
    }

    /// Detect MIME type from magic numbers
    fn detect_mime_type(bytes: &[u8]) -> String {
        if bytes.len() < 4 {
            return "application/octet-stream".to_string();
        }

        // Check magic numbers
        if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
            return "image/jpeg".to_string();
        }

        if bytes.len() >= 4 && bytes[0] == 0x89 && bytes[1] == 0x50 && bytes[2] == 0x4E && bytes[3] == 0x47 {
            return "image/png".to_string();
        }

        if bytes.len() >= 3 && bytes[0] == 0x47 && bytes[1] == 0x49 && bytes[2] == 0x46 {
            return "image/gif".to_string();
        }

        if bytes.len() >= 4 && bytes[0] == 0x00 && bytes[1] == 0x00 && bytes[2] == 0x00
            && (bytes[3] == 0x18 || bytes[3] == 0x20) {
            return "video/mp4".to_string();
        }

        if bytes.len() >= 4 && bytes[0] == 0x25 && bytes[1] == 0x50 && bytes[2] == 0x44 && bytes[3] == 0x46 {
            return "application/pdf".to_string();
        }

        if bytes.len() >= 2 && bytes[0] == 0x50 && bytes[1] == 0x4B {
            return "application/zip".to_string();
        }

        "application/octet-stream".to_string()
    }

    /// Get blob metadata (mime type, size, dimensions if image)
    pub fn get_blob_metadata(&self) -> Result<(String, usize, Option<String>), String> {
        let bytes = self.as_blob_bytes()?;
        let size_bytes = bytes.len();

        let mime_type = Self::detect_mime_type(&bytes);

        let dimensions = if mime_type.starts_with("image/") {
            Self::try_extract_image_dimensions(&bytes)
        } else {
            None
        };

        Ok((mime_type, size_bytes, dimensions))
    }

    /// Try to extract image dimensions from byte data
    fn try_extract_image_dimensions(bytes: &[u8]) -> Option<String> {
        // PNG dimensions (bytes 16-23)
        if bytes.len() > 23 && bytes[0] == 0x89 && bytes[1] == 0x50 {
            let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
            let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
            return Some(format!("{}x{}", width, height));
        }

        // GIF dimensions (bytes 6-9)
        if bytes.len() > 9 && bytes[0] == 0x47 && bytes[1] == 0x49 {
            let width = u16::from_le_bytes([bytes[6], bytes[7]]);
            let height = u16::from_le_bytes([bytes[8], bytes[9]]);
            return Some(format!("{}x{}", width, height));
        }

        // JPEG - more complex, skip for now
        None
    }

    // ==================== DEEP CLONE ====================

    pub fn deep_clone(&self) -> DixValue {
        // Already using Clone, but this is explicit for nested structures
        self.clone()
    }
}

impl std::fmt::Display for DixValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.value {
            ValueData::Null => write!(f, "null"),
            ValueData::String(s) => write!(f, "\"{}\"", s),
            ValueData::Bool(b) => write!(f, "{}", b.to_string().to_lowercase()),
            ValueData::Int(i) => write!(f, "{}", i),
            ValueData::Float(fl) => write!(f, "{}", fl),
            ValueData::Double(d) => write!(f, "{}", d),
            ValueData::Date(dt) => write!(f, "{}", dt.format("%Y-%m-%d")),
            ValueData::Timestamp(dt) => write!(f, "{}", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ")),
            ValueData::Regex(p) => write!(f, "r:({})", p),
            ValueData::Blob(b) => write!(f, "b:({})", b),
            ValueData::Hex(h) => write!(f, "{}", h),
            ValueData::Array(arr) => {
                write!(f, "[")?;
                for (i, item) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            ValueData::Tuple(tup) => {
                write!(f, "t:(")?;
                for (i, item) in tup.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            ValueData::Object(obj) => {
                write!(f, "{{")?;
                for (i, (key, value)) in obj.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", key, value)?;
                }
                write!(f, "}}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constructors() {
        let int_val = DixValue::from_int(42);
        assert_eq!(int_val.get_type(), DixType::Int);
        assert_eq!(int_val.as_int(), 42);

        let str_val = DixValue::from_string("hello".to_string());
        assert_eq!(str_val.get_type(), DixType::String);
        assert_eq!(str_val.as_string(), "hello");
    }

    #[test]
    fn test_numeric_conversions() {
        let int_val = DixValue::from_int(42);
        assert_eq!(int_val.as_float(), 42.0);
        assert_eq!(int_val.as_double(), 42.0);
    }

    #[test]
    fn test_arithmetic() {
        let a = DixValue::from_int(10);
        let b = DixValue::from_int(5);

        assert_eq!(a.add(&b).unwrap().as_int(), 15);
        assert_eq!(a.subtract(&b).unwrap().as_int(), 5);
        assert_eq!(a.multiply(&b).unwrap().as_int(), 50);
        assert_eq!(a.divide(&b).unwrap().as_int(), 2);
    }

    #[test]
    fn test_comparisons() {
        let a = DixValue::from_int(10);
        let b = DixValue::from_int(5);

        assert!(a.greater_than(&b).unwrap());
        assert!(b.less_than(&a).unwrap());
        assert!(a.equal_to(&DixValue::from_int(10)));
    }

    #[test]
    fn test_numeric_type_coercion() {
        let int_val = DixValue::from_int(10);
        let float_val = DixValue::from_float(10.0);

        assert!(int_val.equal_to(&float_val));
    }

    #[test]
    fn test_array_operations() {
        let arr = DixValue::from_array(vec![
            DixValue::from_int(1),
            DixValue::from_int(2),
            DixValue::from_int(3),
        ]);

        assert_eq!(arr.as_array().len(), 3);
        assert!(!arr.as_bool()); // Array with items is truthy
    }

    #[test]
    fn test_object_operations() {
        let mut obj = HashMap::new();
        obj.insert("key1".to_string(), DixValue::from_int(42));
        obj.insert("key2".to_string(), DixValue::from_string("value".to_string()));

        let obj_val = DixValue::from_object(obj);
        assert_eq!(obj_val.as_object().len(), 2);
    }
}