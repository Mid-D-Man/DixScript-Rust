// src/Runtime/dix_value.rs

use std::collections::HashMap;
use serde::Serialize;

/// Runtime value type for loaded DixScript data.
///
/// Flat enum — consumers and the FFI layer pattern-match directly on variants.
/// Separate from `Builtins::Core::DixValue`, which is the compiler-side
/// interpreter value used during QuickFuncs evaluation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DixValue {
    Null,
    Bool(bool),
    Int(i32),
    Float(f32),
    Double(f64),
    String(String),
    Date(String),
    Timestamp(String),
    HexColor(String),
    Blob(String),
    Regex(String),
    Array(Vec<DixValue>),
    Object(HashMap<String, DixValue>),
    Tuple(Vec<DixValue>),
    Enum { enum_name: String, field_name: String, value: i32 },
}

impl DixValue {
    #[inline]
    pub fn bool(value: bool) -> Self {
        DixValue::Bool(value)
    }

    #[inline]
    pub fn int(value: i32) -> Self {
        DixValue::Int(value)
    }

    #[inline]
    pub fn float(value: f32) -> Self {
        DixValue::Float(value)
    }

    #[inline]
    pub fn double(value: f64) -> Self {
        DixValue::Double(value)
    }

    pub fn string(value: impl Into<String>) -> Self {
        DixValue::String(value.into())
    }

    pub fn array(values: Vec<DixValue>) -> Self {
        DixValue::Array(values)
    }

    pub fn object(properties: HashMap<String, DixValue>) -> Self {
        DixValue::Object(properties)
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        matches!(self, DixValue::Null)
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DixValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            DixValue::Int(i)    => Some(*i),
            DixValue::Float(f)  => Some(*f as i32),
            DixValue::Double(d) => Some(*d as i32),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            DixValue::Int(i)    => Some(*i as f64),
            DixValue::Float(f)  => Some(*f as f64),
            DixValue::Double(d) => Some(*d),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            DixValue::String(s)    => Some(s.as_str()),
            DixValue::Date(s)      => Some(s.as_str()),
            DixValue::Timestamp(s) => Some(s.as_str()),
            DixValue::HexColor(s)  => Some(s.as_str()),
            DixValue::Blob(s)      => Some(s.as_str()),
            DixValue::Regex(s)     => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[DixValue]> {
        match self {
            DixValue::Array(arr) => Some(arr.as_slice()),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, DixValue>> {
        match self {
            DixValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            DixValue::Null         => "null",
            DixValue::Bool(_)      => "bool",
            DixValue::Int(_)       => "int",
            DixValue::Float(_)     => "float",
            DixValue::Double(_)    => "double",
            DixValue::String(_)    => "string",
            DixValue::Date(_)      => "date",
            DixValue::Timestamp(_) => "timestamp",
            DixValue::HexColor(_)  => "hexcolor",
            DixValue::Blob(_)      => "blob",
            DixValue::Regex(_)     => "regex",
            DixValue::Array(_)     => "array",
            DixValue::Object(_)    => "object",
            DixValue::Tuple(_)     => "tuple",
            DixValue::Enum { .. }  => "enum",
        }
    }
}

impl std::fmt::Display for DixValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DixValue::Null                                   => write!(f, "null"),
            DixValue::Bool(b)                                => write!(f, "{}", b),
            DixValue::Int(i)                                 => write!(f, "{}", i),
            DixValue::Float(fl)                              => write!(f, "{}f", fl),
            DixValue::Double(d)                              => write!(f, "{}", d),
            DixValue::String(s)                              => write!(f, "\"{}\"", s),
            DixValue::Date(d)                                => write!(f, "{}", d),
            DixValue::Timestamp(t)                           => write!(f, "{}", t),
            DixValue::HexColor(c)                            => write!(f, "{}", c),
            DixValue::Blob(b)                                => write!(f, "b:({})", b),
            DixValue::Regex(r)                               => write!(f, "r:({})", r),
            DixValue::Enum { enum_name, field_name, value } => {
                write!(f, "{}.{} = {}", enum_name, field_name, value)
            }
            DixValue::Array(arr) => {
                write!(f, "[")?;
                for (i, item) in arr.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            DixValue::Object(obj) => {
                write!(f, "{{")?;
                for (i, (k, v)) in obj.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
                }
                write!(f, "}}")
            }
            DixValue::Tuple(items) => {
                write!(f, "t:(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
        }
    }
}

impl From<bool>                    for DixValue { fn from(v: bool)                    -> Self { DixValue::Bool(v) } }
impl From<i32>                     for DixValue { fn from(v: i32)                     -> Self { DixValue::Int(v) } }
impl From<f32>                     for DixValue { fn from(v: f32)                     -> Self { DixValue::Float(v) } }
impl From<f64>                     for DixValue { fn from(v: f64)                     -> Self { DixValue::Double(v) } }
impl From<String>                  for DixValue { fn from(v: String)                  -> Self { DixValue::String(v) } }
impl From<&str>                    for DixValue { fn from(v: &str)                    -> Self { DixValue::String(v.to_string()) } }
impl From<Vec<DixValue>>           for DixValue { fn from(v: Vec<DixValue>)           -> Self { DixValue::Array(v) } }
impl From<HashMap<String, DixValue>> for DixValue { fn from(v: HashMap<String, DixValue>) -> Self { DixValue::Object(v) } }
