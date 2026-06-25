
use std::collections::HashMap;
use serde::Serialize;

/// Runtime value type for loaded DixScript data.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DixValue {
    Null,
    Bool(bool),
    Int(i32), ///32-bit integer
    /// 64-bit integer. Produced by `L`-suffixed literals or auto-promotion.
    Long(i64),
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
    #[inline] pub fn bool(value: bool)   -> Self { DixValue::Bool(value) }
    #[inline] pub fn int(value: i32)     -> Self { DixValue::Int(value) }
    #[inline] pub fn long(value: i64)    -> Self { DixValue::Long(value) }
    #[inline] pub fn float(value: f32)   -> Self { DixValue::Float(value) }
    #[inline] pub fn double(value: f64)  -> Self { DixValue::Double(value) }
    pub fn string(value: impl Into<String>) -> Self { DixValue::String(value.into()) }
    pub fn array(values: Vec<DixValue>)     -> Self { DixValue::Array(values) }
    pub fn object(properties: HashMap<String, DixValue>) -> Self { DixValue::Object(properties) }

    #[inline]
    pub fn is_null(&self) -> bool { matches!(self, DixValue::Null) }

    pub fn as_bool(&self) -> Option<bool> {
        match self { DixValue::Bool(b) => Some(*b), _ => None }
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            DixValue::Int(i)    => Some(*i),
            DixValue::Long(l)   => Some(*l as i32),
            DixValue::Float(f)  => Some(*f as i32),
            DixValue::Double(d) => Some(*d as i32),
            _ => None,
        }
    }

    /// Returns the numeric value as i64, lossless for both Int and Long.
    pub fn as_long(&self) -> Option<i64> {
        match self {
            DixValue::Long(l)   => Some(*l),
            DixValue::Int(i)    => Some(*i as i64),
            DixValue::Float(f)  => Some(*f as i64),
            DixValue::Double(d) => Some(*d as i64),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            DixValue::Int(i)    => Some(*i as f64),
            DixValue::Long(l)   => Some(*l as f64),
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
        match self { DixValue::Array(arr) => Some(arr.as_slice()), _ => None }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, DixValue>> {
        match self { DixValue::Object(obj) => Some(obj), _ => None }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            DixValue::Null         => "null",
            DixValue::Bool(_)      => "bool",
            DixValue::Int(_)       => "int",
            DixValue::Long(_)      => "long",
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
            DixValue::Long(l)                                => write!(f, "{}L", l),
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

// ── From impls ────────────────────────────────────────────────────────────────

impl From<bool>   for DixValue { fn from(v: bool)   -> Self { DixValue::Bool(v) } }
impl From<i32>    for DixValue { fn from(v: i32)     -> Self { DixValue::Int(v) } }
impl From<i64>    for DixValue { fn from(v: i64)     -> Self { DixValue::Long(v) } }
impl From<f32>    for DixValue { fn from(v: f32)     -> Self { DixValue::Float(v) } }
impl From<f64>    for DixValue { fn from(v: f64)     -> Self { DixValue::Double(v) } }
impl From<String> for DixValue { fn from(v: String)  -> Self { DixValue::String(v) } }
impl From<&str>   for DixValue { fn from(v: &str)    -> Self { DixValue::String(v.to_string()) } }
impl From<Vec<DixValue>>             for DixValue { fn from(v: Vec<DixValue>)             -> Self { DixValue::Array(v) } }
impl From<HashMap<String, DixValue>> for DixValue { fn from(v: HashMap<String, DixValue>) -> Self { DixValue::Object(v) } }

// NOTE: TryFrom<DixValue> for i64 lives in dix_data.rs — do NOT duplicate here.

// ── Shared AST -> DixValue conversion ───────────────────────────────────────
//
// Previously this exact match (all 18-ish `Value` variants) was implemented
// twice, nearly verbatim, in `dix_data.rs` (`DixData::ast_value_to_dix_value`)
// and `converter.rs` (`DixConverter::convert_ast_value_to_dix_value`). Two
// copies of the same logic in the same crate is exactly the kind of drift
// risk that produced the ScientificNotation/InterpolatedString/NestedArray
// gaps documented in converter.rs's history — a fix applied to one copy
// silently doesn't apply to the other. There is now exactly one
// implementation; both call sites delegate to it.

/// Convert a single AST `Value` node into a runtime `DixValue`.
///
/// `enums` resolves `Value::EnumValue { enum_name, value: field_name }`
/// references to their declared integer value; pass `None` when no enum
/// table is available (the field then resolves to `0`).
///
/// Runtime-only / unresolved AST nodes (`Lambda`, `Range`, `Identifier`,
/// `QuickFuncCall`, `Expression`, error/diagnostic variants) are not
/// representable as static data and return `None`.
pub(crate) fn ast_value_to_dix_value(
    value: &crate::Compiler::AST::Value,
    enums: Option<&HashMap<String, HashMap<String, i32>>>,
) -> Option<DixValue> {
    use crate::Compiler::AST::Value;

    match value {
        Value::Null { .. }                          => Some(DixValue::Null),
        Value::Boolean { value: b, .. }              => Some(DixValue::Bool(*b)),
        Value::Integer { value: i, .. }              => Some(DixValue::Int(*i)),
        Value::Long { value: l, .. }                 => Some(DixValue::Long(*l)),
        Value::Float { value: f, .. }                => Some(DixValue::Float(*f)),
        Value::Double { value: d, .. }                => Some(DixValue::Double(*d)),
        Value::ScientificNotation { value: d, .. }    => Some(DixValue::Double(*d)),
        Value::String { value: s, .. }                => Some(DixValue::String(s.clone())),
        Value::Date { value: d, .. }                  => Some(DixValue::Date(d.clone())),
        Value::Timestamp { value: t, .. }             => Some(DixValue::Timestamp(t.clone())),
        Value::HexColor { value: c, .. }              => Some(DixValue::HexColor(c.clone())),
        Value::InterpolatedString { template, .. }    => Some(DixValue::String(template.clone())),

        Value::Array { values, .. } | Value::NestedArray { values, .. } => {
            let items: Vec<DixValue> = values.iter()
                .filter_map(|v| ast_value_to_dix_value(v, enums))
                .collect();
            Some(DixValue::Array(items))
        }

        Value::Object { properties, .. } => {
            let mut obj = HashMap::new();
            for prop in properties {
                if let Some(dix_value) = ast_value_to_dix_value(&prop.value, enums) {
                    obj.insert(prop.key.clone(), dix_value);
                }
            }
            Some(DixValue::Object(obj))
        }

        Value::EnumValue { enum_name, value: field_name, .. } => {
            let resolved = enums
                .and_then(|e| e.get(enum_name.as_str()))
                .and_then(|fields| fields.get(field_name.as_str()))
                .copied()
                .unwrap_or(0);
            Some(DixValue::Enum {
                enum_name:  enum_name.clone(),
                field_name: field_name.clone(),
                value:      resolved,
            })
        }

        Value::PrefixedConstructor { prefix, arguments, .. } => match prefix.as_str() {
            "t" => {
                let items: Vec<DixValue> = arguments.iter()
                    .filter_map(|v| ast_value_to_dix_value(v, enums))
                    .collect();
                Some(DixValue::Tuple(items))
            }
            "b" => match arguments.first() {
                Some(Value::String { value: s, .. }) => Some(DixValue::Blob(s.clone())),
                _ => None,
            },
            "r" => match arguments.first() {
                Some(Value::String { value: s, .. }) => Some(DixValue::Regex(s.clone())),
                _ => None,
            },
            _ => None,
        },

        _ => None,
    }
                }
