use super::dix_type::DixType;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq)]
pub struct DixValue {
    value:    ValueData,
    dix_type: DixType,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ValueData {
    Int(i32),
    Long(i64),       // ← NEW
    Float(f32),
    Double(f64),
    String(String),
    Bool(bool),
    Array(Box<Vec<DixValue>>),
    Tuple(Box<Vec<DixValue>>),
    Object(Box<HashMap<String, DixValue>>),
    Date(DateTime<Utc>),
    Timestamp(DateTime<Utc>),
    Regex(String),
    Blob(String),
    Hex(String),
    Null,
}

impl DixValue {
    // ── Constructors ──────────────────────────────────────────────────────────

    pub(crate) fn new(value: ValueData, dix_type: DixType) -> Self {
        DixValue { value, dix_type }
    }

    pub fn from_int(value: i32) -> Self {
        DixValue { value: ValueData::Int(value), dix_type: DixType::Int }
    }

    /// Create a 64-bit integer value.
    pub fn from_long(value: i64) -> Self {
        DixValue { value: ValueData::Long(value), dix_type: DixType::Long }
    }

    pub fn from_float(value: f32) -> Self {
        DixValue { value: ValueData::Float(value), dix_type: DixType::Float }
    }

    pub fn from_double(value: f64) -> Self {
        DixValue { value: ValueData::Double(value), dix_type: DixType::Double }
    }

    pub fn from_string(value: String) -> Self {
        DixValue { value: ValueData::String(value), dix_type: DixType::String }
    }

    pub fn from_bool(value: bool) -> Self {
        DixValue { value: ValueData::Bool(value), dix_type: DixType::Bool }
    }

    pub fn from_array(values: Vec<DixValue>) -> Self {
        DixValue { value: ValueData::Array(Box::new(values)), dix_type: DixType::Array }
    }

    pub fn from_tuple(values: Vec<DixValue>) -> Self {
        DixValue { value: ValueData::Tuple(Box::new(values)), dix_type: DixType::Tuple }
    }

    pub fn from_object(obj: HashMap<String, DixValue>) -> Self {
        DixValue { value: ValueData::Object(Box::new(obj)), dix_type: DixType::Object }
    }

    pub fn from_date(date: DateTime<Utc>) -> Self {
        DixValue { value: ValueData::Date(date), dix_type: DixType::Date }
    }

    pub fn from_timestamp(timestamp: DateTime<Utc>) -> Self {
        DixValue { value: ValueData::Timestamp(timestamp), dix_type: DixType::Timestamp }
    }

    pub fn from_regex(pattern: String) -> Result<Self, String> {
        regex::Regex::new(&pattern).map_err(|e| format!("Invalid regex pattern: {}", e))?;
        Ok(DixValue { value: ValueData::Regex(pattern), dix_type: DixType::Regex })
    }

    pub fn from_blob(base64_data: String) -> Result<Self, String> {
        STANDARD.decode(&base64_data).map_err(|e| format!("Invalid base64 blob data: {}", e))?;
        Ok(DixValue { value: ValueData::Blob(base64_data), dix_type: DixType::Blob })
    }

    pub fn from_hex(hex_color: String) -> Self {
        DixValue { value: ValueData::Hex(hex_color), dix_type: DixType::Hex }
    }

    pub fn null() -> Self {
        DixValue { value: ValueData::Null, dix_type: DixType::Null }
    }

    // ── Type queries ──────────────────────────────────────────────────────────

    #[inline] pub fn get_type(&self) -> DixType { self.dix_type }
    #[inline] pub fn is_null(&self)    -> bool   { self.dix_type == DixType::Null }
    #[inline] pub fn is_numeric(&self) -> bool   { self.dix_type.is_numeric() }
    #[inline] pub fn is_string(&self)  -> bool   { self.dix_type == DixType::String }
    #[inline] pub fn is_array(&self)   -> bool   { self.dix_type == DixType::Array }
    #[inline] pub fn is_object(&self)  -> bool   { self.dix_type == DixType::Object }

    // ── Conversions ───────────────────────────────────────────────────────────

    pub fn as_string(&self) -> String {
        match &self.value {
            ValueData::String(s)    => s.clone(),
            ValueData::Null         => "null".to_string(),
            ValueData::Bool(b)      => b.to_string().to_lowercase(),
            ValueData::Int(i)       => i.to_string(),
            ValueData::Long(l)      => l.to_string(),
            ValueData::Float(f)     => f.to_string(),
            ValueData::Double(d)    => d.to_string(),
            ValueData::Date(dt)     => dt.format("%Y-%m-%d").to_string(),
            ValueData::Timestamp(dt) => dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            ValueData::Regex(p)     => p.clone(),
            ValueData::Blob(b)      => b.clone(),
            ValueData::Hex(h)       => h.clone(),
            ValueData::Array(_)     => "[...]".to_string(),
            ValueData::Tuple(_)     => "t:(...)".to_string(),
            ValueData::Object(_)    => "{...}".to_string(),
        }
    }

    pub fn as_int(&self) -> i32 {
        match &self.value {
            ValueData::Int(i)    => *i,
            ValueData::Long(l)   => *l as i32,
            ValueData::Float(f)  => *f as i32,
            ValueData::Double(d) => *d as i32,
            ValueData::String(s) => s.parse().unwrap_or(0),
            ValueData::Bool(b)   => if *b { 1 } else { 0 },
            _ => 0,
        }
    }

    /// Returns the value as i64. Lossless for Long; widens Int without truncation.
    pub fn as_long(&self) -> i64 {
        match &self.value {
            ValueData::Long(l)   => *l,
            ValueData::Int(i)    => *i as i64,
            ValueData::Float(f)  => *f as i64,
            ValueData::Double(d) => *d as i64,
            ValueData::String(s) => s.parse().unwrap_or(0),
            ValueData::Bool(b)   => if *b { 1 } else { 0 },
            _ => 0,
        }
    }

    pub fn as_float(&self) -> f32 {
        match &self.value {
            ValueData::Float(f)  => *f,
            ValueData::Int(i)    => *i as f32,
            ValueData::Long(l)   => *l as f32,
            ValueData::Double(d) => *d as f32,
            ValueData::String(s) => s.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn as_double(&self) -> f64 {
        match &self.value {
            ValueData::Double(d) => *d,
            ValueData::Float(f)  => *f as f64,
            ValueData::Int(i)    => *i as f64,
            ValueData::Long(l)   => *l as f64,
            ValueData::String(s) => s.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn as_bool(&self) -> bool {
        match &self.value {
            ValueData::Bool(b)   => *b,
            ValueData::Int(i)    => *i != 0,
            ValueData::Long(l)   => *l != 0,
            ValueData::Float(f)  => *f != 0.0,
            ValueData::Double(d) => *d != 0.0,
            ValueData::String(s) => !s.is_empty(),
            ValueData::Null      => false,
            ValueData::Array(a)  => !a.is_empty(),
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
            ValueData::String(s) => s.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
            _ => Utc::now(),
        }
    }

    // ── Arithmetic ────────────────────────────────────────────────────────────

    pub fn add(&self, other: &DixValue) -> Result<DixValue, String> {
        if self.is_numeric() && other.is_numeric() {
            return Ok(match (self.dix_type, other.dix_type) {
                (DixType::Double, _) | (_, DixType::Double) => {
                    DixValue::from_double(self.as_double() + other.as_double())
                }
                (DixType::Float, _) | (_, DixType::Float) => {
                    DixValue::from_float(self.as_float() + other.as_float())
                }
                (DixType::Long, _) | (_, DixType::Long) => {
                    DixValue::from_long(self.as_long() + other.as_long())
                }
                _ => DixValue::from_int(self.as_int() + other.as_int()),
            });
        }
        if self.is_string() || other.is_string() {
            return Ok(DixValue::from_string(self.as_string() + &other.as_string()));
        }
        if self.is_array() && other.is_array() {
            let mut combined = self.as_array().clone();
            combined.extend(other.as_array().iter().cloned());
            return Ok(DixValue::from_array(combined));
        }
        Err(format!("Cannot add {:?} and {:?}", self.dix_type, other.dix_type))
    }

    pub fn subtract(&self, other: &DixValue) -> Result<DixValue, String> {
        if !self.is_numeric() || !other.is_numeric() {
            return Err(format!("Cannot subtract {:?} from {:?}", other.dix_type, self.dix_type));
        }
        Ok(match (self.dix_type, other.dix_type) {
            (DixType::Double, _) | (_, DixType::Double) => {
                DixValue::from_double(self.as_double() - other.as_double())
            }
            (DixType::Float, _) | (_, DixType::Float) => {
                DixValue::from_float(self.as_float() - other.as_float())
            }
            (DixType::Long, _) | (_, DixType::Long) => {
                DixValue::from_long(self.as_long() - other.as_long())
            }
            _ => DixValue::from_int(self.as_int() - other.as_int()),
        })
    }

    pub fn multiply(&self, other: &DixValue) -> Result<DixValue, String> {
        if !self.is_numeric() || !other.is_numeric() {
            return Err(format!("Cannot multiply {:?} and {:?}", self.dix_type, other.dix_type));
        }
        Ok(match (self.dix_type, other.dix_type) {
            (DixType::Double, _) | (_, DixType::Double) => {
                DixValue::from_double(self.as_double() * other.as_double())
            }
            (DixType::Float, _) | (_, DixType::Float) => {
                DixValue::from_float(self.as_float() * other.as_float())
            }
            (DixType::Long, _) | (_, DixType::Long) => {
                DixValue::from_long(self.as_long() * other.as_long())
            }
            _ => DixValue::from_int(self.as_int() * other.as_int()),
        })
    }

    pub fn divide(&self, other: &DixValue) -> Result<DixValue, String> {
        if !self.is_numeric() || !other.is_numeric() {
            return Err(format!("Cannot divide {:?} by {:?}", self.dix_type, other.dix_type));
        }
        if other.as_double() == 0.0 { return Err("Division by zero".to_string()); }
        Ok(DixValue::from_double(self.as_double() / other.as_double()))
    }

    // ── Comparison ────────────────────────────────────────────────────────────

    pub fn compare_to(&self, other: &DixValue) -> Result<Ordering, String> {
        if self.is_numeric() && other.is_numeric() {
            // Use i64 path when both are integer types to avoid f64 precision loss
            if matches!(self.dix_type, DixType::Int | DixType::Long)
                && matches!(other.dix_type, DixType::Int | DixType::Long)
            {
                return Ok(self.as_long().cmp(&other.as_long()));
            }
            let left  = self.as_double();
            let right = other.as_double();
            return Ok(left.partial_cmp(&right).unwrap_or(Ordering::Equal));
        }
        if self.dix_type != other.dix_type {
            return Err(format!("Cannot compare {:?} with {:?}", self.dix_type, other.dix_type));
        }
        Ok(match &self.value {
            ValueData::String(s1) => {
                if let ValueData::String(s2) = &other.value { s1.cmp(s2) } else { Ordering::Equal }
            }
            ValueData::Bool(b1) => {
                if let ValueData::Bool(b2) = &other.value { b1.cmp(b2) } else { Ordering::Equal }
            }
            ValueData::Date(dt1) | ValueData::Timestamp(dt1) => dt1.cmp(&other.as_datetime()),
            _ => return Err(format!("Type {:?} is not comparable", self.dix_type)),
        })
    }

    pub fn equal_to(&self, other: &DixValue) -> bool {
        if self.is_numeric() && other.is_numeric() {
            // Integer-only comparison: exact, no epsilon needed
            if matches!(self.dix_type, DixType::Int | DixType::Long)
                && matches!(other.dix_type, DixType::Int | DixType::Long)
            {
                return self.as_long() == other.as_long();
            }
            const EPSILON: f64 = 1e-10;
            (self.as_double() - other.as_double()).abs() < EPSILON
        } else {
            PartialEq::eq(self, other)
        }
    }

    pub fn greater_than(&self, other: &DixValue) -> Result<bool, String> {
        Ok(self.compare_to(other)? == Ordering::Greater)
    }

    pub fn less_than(&self, other: &DixValue) -> Result<bool, String> {
        Ok(self.compare_to(other)? == Ordering::Less)
    }

    // ── Blob helpers ──────────────────────────────────────────────────────────

    pub fn as_blob_base64(&self) -> Result<String, String> {
        match &self.value {
            ValueData::Blob(b) => Ok(b.clone()),
            _ => Err(format!("Cannot get blob data from {:?}", self.dix_type)),
        }
    }

    pub fn as_blob_bytes(&self) -> Result<Vec<u8>, String> {
        match &self.value {
            ValueData::Blob(b) => STANDARD.decode(b).map_err(|e| format!("Failed to decode blob: {}", e)),
            _ => Err(format!("Cannot convert {:?} to byte array", self.dix_type)),
        }
    }

    /// Returns `(mime_type, byte_length, raw_bytes)` for a Blob value.
    ///
    /// Blobs in DixScript are stored as raw base64 without an embedded MIME
    /// header, so `mime_type` is always `"application/octet-stream"`.
    /// Call `as_blob_base64()` if you only need the encoded string.
    pub fn get_blob_metadata(&self) -> Result<(String, usize, Vec<u8>), String> {
        match &self.value {
            ValueData::Blob(b) => {
                let bytes = STANDARD.decode(b)
                    .map_err(|e| format!("Failed to decode blob: {}", e))?;
                let size = bytes.len();
                Ok(("application/octet-stream".to_string(), size, bytes))
            }
            _ => Err(format!("Cannot get blob metadata from {:?}", self.dix_type)),
        }
    }

    pub fn deep_clone(&self) -> DixValue { self.clone() }
}

impl std::fmt::Display for DixValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.value {
            ValueData::Null          => write!(f, "null"),
            ValueData::String(s)     => write!(f, "\"{}\"", s),
            ValueData::Bool(b)       => write!(f, "{}", b.to_string().to_lowercase()),
            ValueData::Int(i)        => write!(f, "{}", i),
            ValueData::Long(l)       => write!(f, "{}L", l),
            ValueData::Float(fl)     => write!(f, "{}", fl),
            ValueData::Double(d)     => write!(f, "{}", d),
            ValueData::Date(dt)      => write!(f, "{}", dt.format("%Y-%m-%d")),
            ValueData::Timestamp(dt) => write!(f, "{}", dt.format("%Y-%m-%dT%H:%M:%S%.3fZ")),
            ValueData::Regex(p)      => write!(f, "r:({})", p),
            ValueData::Blob(b)       => write!(f, "b:({})", b),
            ValueData::Hex(h)        => write!(f, "{}", h),
            ValueData::Array(arr)    => {
                write!(f, "[")?;
                for (i, item) in arr.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            ValueData::Tuple(tup) => {
                write!(f, "t:(")?;
                for (i, item) in tup.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            ValueData::Object(obj) => {
                write!(f, "{{")?;
                for (i, (key, value)) in obj.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
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
    fn test_long_constructor() {
        let v = DixValue::from_long(9_000_000_000_i64);
        assert_eq!(v.get_type(), DixType::Long);
        assert_eq!(v.as_long(), 9_000_000_000_i64);
    }

    #[test]
    fn test_long_as_int_truncates() {
        let v = DixValue::from_long(i64::MAX);
        let _ = v.as_int(); // just verify no panic
    }

    #[test]
    fn test_long_arithmetic() {
        let a = DixValue::from_long(5_000_000_000_i64);
        let b = DixValue::from_long(3_000_000_000_i64);
        let sum = a.add(&b).unwrap();
        assert_eq!(sum.get_type(), DixType::Long);
        assert_eq!(sum.as_long(), 8_000_000_000_i64);
    }

    #[test]
    fn test_long_int_mixed_arithmetic_promotes() {
        let a = DixValue::from_long(5_000_000_000_i64);
        let b = DixValue::from_int(1);
        let sum = a.add(&b).unwrap();
        assert_eq!(sum.get_type(), DixType::Long);
    }

    #[test]
    fn test_long_comparison_exact() {
        let a = DixValue::from_long(i64::MAX);
        let b = DixValue::from_long(i64::MAX - 1);
        assert!(a.greater_than(&b).unwrap());
        assert!(a.equal_to(&DixValue::from_long(i64::MAX)));
    }

    #[test]
    fn test_display_long() {
        let v = DixValue::from_long(42_i64);
        assert_eq!(v.to_string(), "42L");
    }

    #[test]
    fn test_get_blob_metadata() {
        // base64 of "hello"
        let blob = DixValue::from_blob("aGVsbG8=".to_string()).unwrap();
        let (mime, size, bytes) = blob.get_blob_metadata().unwrap();
        assert_eq!(mime, "application/octet-stream");
        assert_eq!(size, 5);
        assert_eq!(bytes, b"hello");
    }
                      }
