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
    /// Human-readable variant label, used in error messages.
    ///
    /// Identical to the lowercase type name shown to users:
    /// `"int"`, `"string"`, `"array"`, etc.
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
            DixValue::HexColor(_)  => "hex",
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
    fn test_type_name() {
        assert_eq!(DixValue::Null.type_name(), "null");
        assert_eq!(DixValue::Int(1).type_name(), "int");
        assert_eq!(DixValue::Float(1.0).type_name(), "float");
        assert_eq!(DixValue::Double(1.0).type_name(), "double");
        assert_eq!(DixValue::Bool(true).type_name(), "bool");
        assert_eq!(DixValue::String("x".into()).type_name(), "string");
        assert_eq!(DixValue::Date("2025-01-01".into()).type_name(), "date");
        assert_eq!(DixValue::Timestamp("2025-01-01T00:00:00Z".into()).type_name(), "timestamp");
        assert_eq!(DixValue::HexColor("#FF0000".into()).type_name(), "hex");
        assert_eq!(DixValue::Array(vec![]).type_name(), "array");
        assert_eq!(DixValue::Tuple(vec![]).type_name(), "tuple");
        assert_eq!(DixValue::Object(HashMap::new()).type_name(), "object");
        assert_eq!(DixValue::Blob("data".into()).type_name(), "blob");
        assert_eq!(DixValue::Regex(".*".into()).type_name(), "regex");
        assert_eq!(
            DixValue::Enum {
                enum_name:  "AIType".into(),
                field_name: "BOSS".into(),
                value:      2,
            }.type_name(),
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
        assert_eq!(format!("{}", DixValue::Null),             "null");
        assert_eq!(format!("{}", DixValue::Bool(true)),       "true");
        assert_eq!(format!("{}", DixValue::Bool(false)),      "false");
        assert_eq!(format!("{}", DixValue::Int(42)),          "42");
        assert_eq!(format!("{}", DixValue::Float(1.5)),       "1.5f");
        assert_eq!(format!("{}", DixValue::Double(3.14)),     "3.14");
        assert_eq!(format!("{}", DixValue::String("hi".into())), "\"hi\"");
        assert_eq!(format!("{}", DixValue::Blob("abc".into())),  "b:(abc)");
        assert_eq!(format!("{}", DixValue::Regex(".*".into())),  "r:(.*)");
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
    fn test_serialize_primitives() {
        // Verify serde_json works — required by FFI mdix_get_json
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
