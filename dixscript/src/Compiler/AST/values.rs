// dixscript/src/Compiler/AST/values.rs
use super::position::Position;
use super::expressions::Expression;
use crate::Compiler::VersionControl::CompatibilityResult;

/// Value types in DixScript
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    // Primitive types
    Integer {
        value: i32,
        position: Position,
    },
    /// 64-bit integer. Source literal uses L suffix: `9_000_000_000L`.
    /// Also produced by auto-promotion when a literal overflows i32.
    Long {
        value: i64,
        position: Position,
    },
    Float {
        value: f32,
        position: Position,
    },
    Double {
        value: f64,
        position: Position,
    },
    ScientificNotation {
        value: f64,
        position: Position,
    },
    String {
        value: String,
        position: Position,
    },
    Boolean {
        value: bool,
        position: Position,
    },

    // Special string types
    InterpolatedString {
        template: String,
        expressions: Vec<Expression>,
        position: Position,
    },

    // Special types
    HexColor {
        value: String,
        position: Position,
    },
    Date {
        value: String,
        position: Position,
    },
    Timestamp {
        value: String,
        position: Position,
    },
    Null {
        position: Position,
    },

    // Collection types
    Array {
        values: Vec<Value>,
        position: Position,
    },
    NestedArray {
        values: Vec<Value>,
        level: usize,
        position: Position,
    },
    Object {
        properties: Vec<ObjectProperty>,
        position: Position,
    },

    // Prefixed constructors
    PrefixedConstructor {
        prefix: String,
        arguments: Vec<Value>,
        position: Position,
    },

    // Enum value
    EnumValue {
        enum_name: String,
        value: String,
        position: Position,
    },

    // Identifier value (variable reference)
    Identifier {
        value: String,
        position: Position,
    },

    // Function call result
    QuickFuncCall {
        function_name: String,
        arguments: Vec<Expression>,
        position: Position,
    },

    // Expression to be evaluated
    Expression {
        expr: Box<Expression>,
        position: Position,
    },

    // Range value
    Range {
        start: Box<Value>,
        end: Box<Value>,
        position: Position,
    },

    // Lambda/closure
    Lambda {
        parameters: Vec<String>,
        body: Box<Expression>,
        position: Position,
    },

    // Error types
    ParseError {
        message: String,
        position: Position,
    },
    Error {
        message: String,
        position: Position,
    },

    // Unknown type (for compatibility)
    Unknown {
        element_type: String,
        element_name: String,
        element_data: String,
        compatibility_result: CompatibilityResult,
        position: Position,
    },
}

impl Value {
    pub fn position(&self) -> Position {
        match self {
            Value::Integer { position, .. }            => *position,
            Value::Long { position, .. }               => *position,
            Value::Float { position, .. }              => *position,
            Value::Double { position, .. }             => *position,
            Value::ScientificNotation { position, .. } => *position,
            Value::String { position, .. }             => *position,
            Value::Boolean { position, .. }            => *position,
            Value::InterpolatedString { position, .. } => *position,
            Value::HexColor { position, .. }           => *position,
            Value::Date { position, .. }               => *position,
            Value::Timestamp { position, .. }          => *position,
            Value::Null { position }                   => *position,
            Value::Array { position, .. }              => *position,
            Value::NestedArray { position, .. }        => *position,
            Value::Object { position, .. }             => *position,
            Value::PrefixedConstructor { position, .. } => *position,
            Value::EnumValue { position, .. }          => *position,
            Value::Identifier { position, .. }         => *position,
            Value::QuickFuncCall { position, .. }      => *position,
            Value::Expression { position, .. }         => *position,
            Value::Range { position, .. }              => *position,
            Value::Lambda { position, .. }             => *position,
            Value::ParseError { position, .. }         => *position,
            Value::Error { position, .. }              => *position,
            Value::Unknown { position, .. }            => *position,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer { value, .. }            => write!(f, "{}", value),
            Value::Long { value, .. }               => write!(f, "{}L", value),
            Value::Float { value, .. }              => write!(f, "{}f", value),
            Value::Double { value, .. }             => write!(f, "{}", value),
            Value::ScientificNotation { value, .. } => write!(f, "{:e}", value),
            Value::String { value, .. }             => write!(f, "\"{}\"", value),
            Value::Boolean { value, .. }            => write!(f, "{}", if *value { "true" } else { "false" }),
            Value::InterpolatedString { template, .. } => write!(f, "$\"{}\"", template),
            Value::HexColor { value, .. }           => write!(f, "{}", value),
            Value::Date { value, .. }               => write!(f, "{}", value),
            Value::Timestamp { value, .. }          => write!(f, "{}", value),
            Value::Null { .. }                      => write!(f, "null"),
            Value::Array { values, .. } => {
                write!(f, "[")?;
                for (i, val) in values.iter().enumerate() {
                    write!(f, "{}", val)?;
                    if i < values.len() - 1 { write!(f, ", ")?; }
                }
                write!(f, "]")
            }
            Value::NestedArray { values, .. } => {
                write!(f, "[")?;
                for (i, val) in values.iter().enumerate() {
                    write!(f, "{}", val)?;
                    if i < values.len() - 1 { write!(f, ", ")?; }
                }
                write!(f, "]")
            }
            Value::Object { properties, .. } => {
                write!(f, "{{")?;
                for (i, prop) in properties.iter().enumerate() {
                    write!(f, "{} = {}", prop.key, prop.value)?;
                    if i < properties.len() - 1 { write!(f, ", ")?; }
                }
                write!(f, "}}")
            }
            Value::PrefixedConstructor { prefix, arguments, .. } => {
                write!(f, "{}:(", prefix)?;
                for (i, arg) in arguments.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i < arguments.len() - 1 { write!(f, ", ")?; }
                }
                write!(f, ")")
            }
            Value::EnumValue { enum_name, value, .. }    => write!(f, "{}.{}", enum_name, value),
            Value::Identifier { value, .. }              => write!(f, "{}", value),
            Value::QuickFuncCall { function_name, arguments, .. } => {
                write!(f, "{}(", function_name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i < arguments.len() - 1 { write!(f, ", ")?; }
                }
                write!(f, ")")
            }
            Value::Expression { expr, .. }               => write!(f, "<ToEvaluate: {}>", expr),
            Value::Range { start, end, .. }              => write!(f, "{}..{}", start, end),
            Value::Lambda { parameters, body, .. }       => {
                write!(f, "({}) => {}", parameters.join(", "), body)
            }
            Value::ParseError { message, .. }            => write!(f, "ParseError: {}", message),
            Value::Error { message, .. }                 => write!(f, "Error: {}", message),
            Value::Unknown { element_type, element_name, compatibility_result, .. } => {
                write!(f, "Unknown {}: {} (Compatibility: {:?})", element_type, element_name, compatibility_result)
            }
        }
    }
}

/// Object property
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectProperty {
    pub key:      String,
    pub value:    Value,
    pub position: Position,
}

impl ObjectProperty {
    pub fn new(key: String, value: Value, position: Position) -> Self {
        ObjectProperty { key, value, position }
    }
}

impl std::fmt::Display for ObjectProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} = {}", self.key, self.value)
    }
}
