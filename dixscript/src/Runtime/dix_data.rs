// src/Runtime/dix_value.rs
//! Consumer-facing runtime value type for DixScript.
//!
//! This is a flat enum — consumers (FFI, game engines, C# via csbindgen)
//! pattern match directly on variants. It is intentionally separate from
//! Builtins::Core::DixValue, which is the compiler-side interpreter value
//! used during QuickFuncs evaluation and carries arithmetic/comparison methods.

use std::collections::HashMap;
use serde::Serialize;

/// All value types that can appear in a loaded `.mdix` file.
///
/// Variants map 1:1 to the MdixType discriminants exposed by the C FFI.
/// Use `type_name()` to get a human-readable label for error messages.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum DixValue {
    Null,
    Bool(bool),
    Int(i32),
    Float(f32),
    Double(f64),
    String(String),
    /// Date stored as `YYYY-MM-DD` string.
    Date(String),
    /// Timestamp stored as ISO-8601 string.
    Timestamp(String),
    /// Hex color stored as `#RRGGBB` / `#RRGGBBAA` string.
    HexColor(String),
    Array(Vec<DixValue>),
    Tuple(Vec<DixValue>),
    Object(HashMap<String, DixValue>),
    /// Base64-encoded binary data.
    Blob(String),
    /// Regex pattern string.
    Regex(String),
    /// Resolved enum value.  `value` is the integer the field maps to.
    Enum {
        enum_name:  String,
        field_name: String,
        value:      i32,
    },
}

impl DixValue {
    // ==================== CONSTRUCTORS ====================

    /// Convenience constructor for `DixValue::String`.
    #[inline]
    pub fn string(s: impl Into<String>) -> Self {
        DixValue::String(s.into())
    }

    /// Convenience constructor for `DixValue::Array`.
    #[inline]
    pub fn array(items: Vec<DixValue>) -> Self {
        DixValue::Array(items)
    }

    /// Convenience constructor for `DixValue::Object`.
    #[inline]
    pub fn object(map: HashMap<String, DixValue>) -> Self {
        DixValue::Object(map)
    }

    // ==================== TYPE ACCESSORS ====================

    /// Returns the inner `&str` if this is a `String` variant, otherwise `None`.
    #[inline]
    pub fn as_string(&self) -> Option<&str> {
        match self {
            DixValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the inner `i32` if this is a numeric or enum variant, otherwise `None`.
    ///
    /// `Float` and `Double` are truncated toward zero.
    #[inline]
    pub fn as_int(&self) -> Option<i32> {
        match self {
            DixValue::Int(i)              => Some(*i),
            DixValue::Float(f)            => Some(*f as i32),
            DixValue::Double(d)           => Some(*d as i32),
            DixValue::Enum { value, .. }  => Some(*value),
            _ => None,
        }
    }

    /// Returns the value as `f64` if this is any numeric variant, otherwise `None`.
    #[inline]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            DixValue::Int(i)    => Some(*i as f64),
            DixValue::Float(f)  => Some(*f as f64),
            DixValue::Double(d) => Some(*d),
            _ => None,
        }
    }

    /// Returns a reference to the inner `Vec<DixValue>` if this is an `Array`
    /// variant, otherwise `None`.
    #[inline]
    pub fn as_array(&self) -> Option<&Vec<DixValue>> {
        match self {
            DixValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Returns a reference to the inner `HashMap` if this is an `Object`
    /// variant, otherwise `None`.
    #[inline]
    pub fn as_object(&self) -> Option<&HashMap<String, DixValue>> {
        match self {
            DixValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    // ==================== METADATA ====================

    /// Human-readable variant label, used in error messages.
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
            DixValue::Array(_)     => "array",
            DixValue::Tuple(_)     => "tuple",
            DixValue::Object(_)    => "object",
            DixValue::Blob(_)      => "blob",
            DixValue::Regex(_)     => "regex",
            DixValue::Enum { .. }  => "enum",
        }
    }

    /// Returns `true` if this value is `Null`.
    #[inline]
    pub fn is_null(&self) -> bool {
        matches!(self, DixValue::Null)
    }
}

// ==================== FROM TRAIT IMPLEMENTATIONS ====================

impl From<bool> for DixValue {
    fn from(v: bool) -> Self { DixValue::Bool(v) }
}

impl From<i32> for DixValue {
    fn from(v: i32) -> Self { DixValue::Int(v) }
}

impl From<f32> for DixValue {
    fn from(v: f32) -> Self { DixValue::Float(v) }
}

impl From<f64> for DixValue {
    fn from(v: f64) -> Self { DixValue::Double(v) }
}

impl From<&str> for DixValue {
    fn from(v: &str) -> Self { DixValue::String(v.to_string()) }
}

impl From<String> for DixValue {
    fn from(v: String) -> Self { DixValue::String(v) }
}

// ==================== DISPLAY ====================

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
                write!(f, "{}.{} ({})", enum_name, field_name, value)
            }
            DixValue::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            DixValue::Tuple(items) => {
                write!(f, "t:(")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", v)?;
                }
                write!(f, ")")
            }
            DixValue::Object(obj) => {
                write!(f, "{{")?;
                for (i, (k, v)) in obj.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", k, v)?;
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
        let s = DixValue::string("hello");
        assert_eq!(s.type_name(), "string");
        assert_eq!(s.as_string(), Some("hello"));

        let arr = DixValue::array(vec![DixValue::Int(1), DixValue::Int(2)]);
        assert_eq!(arr.type_name(), "array");
        assert_eq!(arr.as_array().map(|a| a.len()), Some(2));

        let mut map = HashMap::new();
        map.insert("k".to_string(), DixValue::Bool(true));
        let obj = DixValue::object(map);
        assert_eq!(obj.type_name(), "object");
        assert_eq!(obj.as_object().map(|o| o.len()), Some(1));
    }

    #[test]
    fn test_as_int_coercions() {
        assert_eq!(DixValue::Int(42).as_int(),         Some(42));
        assert_eq!(DixValue::Float(3.9_f32).as_int(),  Some(3));
        assert_eq!(DixValue::Double(7.1).as_int(),     Some(7));
        assert_eq!(DixValue::String("x".into()).as_int(), None);
        assert_eq!(
            DixValue::Enum { enum_name: "E".into(), field_name: "A".into(), value: 5 }.as_int(),
            Some(5)
        );
    }

    #[test]
    fn test_as_float_coercions() {
        assert!((DixValue::Int(5).as_float().unwrap() - 5.0).abs() < f64::EPSILON);
        assert!((DixValue::Float(1.5_f32).as_float().unwrap() - 1.5).abs() < 1e-6);
        assert!((DixValue::Double(3.14).as_float().unwrap() - 3.14).abs() < f64::EPSILON);
        assert_eq!(DixValue::Bool(true).as_float(), None);
    }

    #[test]
    fn test_type_name() {
        assert_eq!(DixValue::Null.type_name(),                              "null");
        assert_eq!(DixValue::Int(1).type_name(),                            "int");
        assert_eq!(DixValue::Float(1.0).type_name(),                        "float");
        assert_eq!(DixValue::Double(1.0).type_name(),                       "double");
        assert_eq!(DixValue::Bool(true).type_name(),                        "bool");
        assert_eq!(DixValue::String("x".into()).type_name(),                "string");
        assert_eq!(DixValue::Date("2025-01-01".into()).type_name(),         "date");
        assert_eq!(DixValue::Timestamp("2025-01-01T00:00:00Z".into()).type_name(), "timestamp");
        assert_eq!(DixValue::HexColor("#FF0000".into()).type_name(),        "hexcolor");
        assert_eq!(DixValue::Array(vec![]).type_name(),                     "array");
        assert_eq!(DixValue::Tuple(vec![]).type_name(),                     "tuple");
        assert_eq!(DixValue::Object(HashMap::new()).type_name(),            "object");
        assert_eq!(DixValue::Blob("data".into()).type_name(),               "blob");
        assert_eq!(DixValue::Regex(".*".into()).type_name(),                "regex");
        assert_eq!(
            DixValue::Enum { enum_name: "AIType".into(), field_name: "BOSS".into(), value: 2 }
                .type_name(),
            "enum"
        );
    }

    #[test]
    fn test_is_null() {
        assert!(DixValue::Null.is_null());
        assert!(!DixValue::Int(0).is_null());
        assert!(!DixValue::Bool(false).is_null());
    }

    #[test]
    fn test_display_primitives() {
        assert_eq!(format!("{}", DixValue::Null),                    "null");
        assert_eq!(format!("{}", DixValue::Bool(true)),              "true");
        assert_eq!(format!("{}", DixValue::Bool(false)),             "false");
        assert_eq!(format!("{}", DixValue::Int(42)),                 "42");
        assert_eq!(format!("{}", DixValue::Float(1.5)),              "1.5f");
        assert_eq!(format!("{}", DixValue::Double(3.14)),            "3.14");
        assert_eq!(format!("{}", DixValue::String("hi".into())),     "\"hi\"");
        assert_eq!(format!("{}", DixValue::Blob("abc".into())),      "b:(abc)");
        assert_eq!(format!("{}", DixValue::Regex(".*".into())),      "r:(.*)");
    }

    #[test]
    fn test_display_array() {
        let arr = DixValue::Array(vec![
            DixValue::Int(1),
            DixValue::Int(2),
            DixValue::Int(3),
        ]);
        assert_eq!(format!("{}", arr), "[1, 2, 3]");
    }

    #[test]
    fn test_display_enum() {
        let e = DixValue::Enum {
            enum_name:  "AIType".into(),
            field_name: "BOSS".into(),
            value:      2,
        };
        assert_eq!(format!("{}", e), "AIType.BOSS (2)");
    }

    #[test]
    fn test_from_impls() {
        let v_bool:   DixValue = true.into();
        let v_int:    DixValue = 42_i32.into();
        let v_float:  DixValue = 1.5_f32.into();
        let v_double: DixValue = 3.14_f64.into();
        let v_str:    DixValue = "hello".into();
        let v_owned:  DixValue = "owned".to_string().into();

        assert_eq!(v_bool.type_name(),   "bool");
        assert_eq!(v_int.type_name(),    "int");
        assert_eq!(v_float.type_name(),  "float");
        assert_eq!(v_double.type_name(), "double");
        assert_eq!(v_str.type_name(),    "string");
        assert_eq!(v_owned.type_name(),  "string");
    }

    #[test]
    fn test_serialize_primitives() {
        let cases: Vec<(&str, DixValue)> = vec![
            ("Null",   DixValue::Null),
            ("Bool",   DixValue::Bool(true)),
            ("Int",    DixValue::Int(42)),
            ("String", DixValue::String("hello".into())),
        ];
        for (label, v) in cases {
            let json = serde_json::to_string(&v).expect(label);
            assert!(json.contains(label), "expected '{}' tag in {}", label, json);
        }
    }

    #[test]
    fn test_serialize_enum_variant() {
        let v = DixValue::Enum {
            enum_name:  "AIType".into(),
            field_name: "BOSS".into(),
            value:      2,
        };
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("Enum"));
        assert!(json.contains("AIType"));
        assert!(json.contains("BOSS"));
        assert!(json.contains('2'));
    }

    #[test]
    fn test_clone_and_eq() {
        let a = DixValue::Array(vec![DixValue::Int(1), DixValue::Bool(false)]);
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn test_object_roundtrip() {
        let mut map = HashMap::new();
        map.insert("x".to_string(), DixValue::Int(10));
        map.insert("y".to_string(), DixValue::String("hello".into()));
        let obj = DixValue::Object(map);
        let json = serde_json::to_string(&obj).unwrap();
        assert!(json.contains("Object"));
    }
}
