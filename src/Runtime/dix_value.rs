// src/Runtime/dix_value.rs

use std::collections::HashMap;

/// Runtime value representation for DixScript
/// 
/// This is the "object" equivalent in C# - represents any value at runtime
/// Used for:
/// - Flattened data storage in DixData
/// - Conversion to/from HashMap
/// - FFI boundary (serialized to JSON)
#[derive(Debug, Clone, PartialEq)]
pub enum DixValue {
    Null,
    Bool(bool),
    Int(i32),
    Float(f32),
    Double(f64),
    String(String),
    Date(String),       // ISO 8601 date string
    Timestamp(String),  // ISO 8601 timestamp string
    HexColor(String),   // #RRGGBB format
    Blob(String),       // Base64-encoded binary data
    Regex(String),      // Regex pattern
    Array(Vec<DixValue>),
    Object(HashMap<String, DixValue>),
    Tuple(Vec<DixValue>),
    Enum { enum_name: String, field_name: String, value: i32 },
}

impl DixValue {
    /// Create a DixValue from a boolean
    #[inline]
    pub fn bool(value: bool) -> Self {
        DixValue::Bool(value)
    }
    
    /// Create a DixValue from an integer
    #[inline]
    pub fn int(value: i32) -> Self {
        DixValue::Int(value)
    }
    
    /// Create a DixValue from a float
    #[inline]
    pub fn float(value: f32) -> Self {
        DixValue::Float(value)
    }
    
    /// Create a DixValue from a double
    #[inline]
    pub fn double(value: f64) -> Self {
        DixValue::Double(value)
    }
    
    /// Create a DixValue from a string
    pub fn string(value: impl Into<String>) -> Self {
        DixValue::String(value.into())
    }
    
    /// Create a DixValue array
    pub fn array(values: Vec<DixValue>) -> Self {
        DixValue::Array(values)
    }
    
    /// Create a DixValue object
    pub fn object(properties: HashMap<String, DixValue>) -> Self {
        DixValue::Object(properties)
    }
    
    /// Check if value is null
    #[inline]
    pub fn is_null(&self) -> bool {
        matches!(self, DixValue::Null)
    }
    
    /// Try to get as boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DixValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
    
    /// Try to get as integer
    pub fn as_int(&self) -> Option<i32> {
        match self {
            DixValue::Int(i) => Some(*i),
            DixValue::Float(f) => Some(*f as i32),
            DixValue::Double(d) => Some(*d as i32),
            _ => None,
        }
    }
    
    /// Try to get as float
    pub fn as_float(&self) -> Option<f64> {
        match self {
            DixValue::Int(i) => Some(*i as f64),
            DixValue::Float(f) => Some(*f as f64),
            DixValue::Double(d) => Some(*d),
            _ => None,
        }
    }
    
    /// Try to get as string
    pub fn as_string(&self) -> Option<&str> {
        match self {
            DixValue::String(s) => Some(s.as_str()),
            DixValue::Date(s) => Some(s.as_str()),
            DixValue::Timestamp(s) => Some(s.as_str()),
            DixValue::HexColor(s) => Some(s.as_str()),
            DixValue::Blob(s) => Some(s.as_str()),
            DixValue::Regex(s) => Some(s.as_str()),
            _ => None,
        }
    }
    
    /// Try to get as array
    pub fn as_array(&self) -> Option<&[DixValue]> {
        match self {
            DixValue::Array(arr) => Some(arr.as_slice()),
            _ => None,
        }
    }
    
    /// Try to get as object
    pub fn as_object(&self) -> Option<&HashMap<String, DixValue>> {
        match self {
            DixValue::Object(obj) => Some(obj),
            _ => None,
        }
    }
    
    /// Get type name as string
    pub fn type_name(&self) -> &'static str {
        match self {
            DixValue::Null => "null",
            DixValue::Bool(_) => "bool",
            DixValue::Int(_) => "int",
            DixValue::Float(_) => "float",
            DixValue::Double(_) => "double",
            DixValue::String(_) => "string",
            DixValue::Date(_) => "date",
            DixValue::Timestamp(_) => "timestamp",
            DixValue::HexColor(_) => "hexcolor",
            DixValue::Blob(_) => "blob",
            DixValue::Regex(_) => "regex",
            DixValue::Array(_) => "array",
            DixValue::Object(_) => "object",
            DixValue::Tuple(_) => "tuple",
            DixValue::Enum { .. } => "enum",
        }
    }
}

impl std::fmt::Display for DixValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DixValue::Null => write!(f, "null"),
            DixValue::Bool(b) => write!(f, "{}", b),
            DixValue::Int(i) => write!(f, "{}", i),
            DixValue::Float(fl) => write!(f, "{}f", fl),
            DixValue::Double(d) => write!(f, "{}", d),
            DixValue::String(s) => write!(f, "\"{}\"", s),
            DixValue::Date(d) => write!(f, "{}", d),
            DixValue::Timestamp(t) => write!(f, "{}", t),
            DixValue::HexColor(c) => write!(f, "{}", c),
            DixValue::Blob(b) => write!(f, "b:({})", b),
            DixValue::Regex(r) => write!(f, "r:({})", r),
            DixValue::Array(arr) => {
                write!(f, "[")?;
                for (i, item) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            DixValue::Object(obj) => {
                write!(f, "{{")?;
                for (i, (k, v)) in obj.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            DixValue::Tuple(items) => {
                write!(f, "t:(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            DixValue::Enum { enum_name, field_name, value } => {
                write!(f, "{}.{} = {}", enum_name, field_name, value)
            }
        }
    }
}

impl From<bool> for DixValue {
    fn from(b: bool) -> Self {
        DixValue::Bool(b)
    }
}

impl From<i32> for DixValue {
    fn from(i: i32) -> Self {
        DixValue::Int(i)
    }
}

impl From<f32> for DixValue {
    fn from(f: f32) -> Self {
        DixValue::Float(f)
    }
}

impl From<f64> for DixValue {
    fn from(d: f64) -> Self {
        DixValue::Double(d)
    }
}

impl From<String> for DixValue {
    fn from(s: String) -> Self {
        DixValue::String(s)
    }
}

impl From<&str> for DixValue {
    fn from(s: &str) -> Self {
        DixValue::String(s.to_string())
    }
}

impl From<Vec<DixValue>> for DixValue {
    fn from(v: Vec<DixValue>) -> Self {
        DixValue::Array(v)
    }
}

impl From<HashMap<String, DixValue>> for DixValue {
    fn from(m: HashMap<String, DixValue>) -> Self {
        DixValue::Object(m)
    }
  }
