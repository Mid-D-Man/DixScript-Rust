use super::position::Position;
use super::data_types::DataType;
use super::values::Value;

/// Expression types in DixScript
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    // Simple expressions
    Identifier {
        name: String,
        position: Position,
    },
    
    /// Ambiguous qualified identifier: could be enum.value, object.property, namespace.function, etc.
    /// Resolved during semantic analysis when we have full context
    QualifiedIdentifier {
        parts: Vec<String>,
        arguments: Option<Vec<Expression>>, // null = property/enum access, non-null = function call
        position: Position,
    },
    
    // Function calls
    FunctionCall {
        name: String,
        arguments: Vec<Expression>,
        position: Position,
    },
    
    QuickFuncCall {
        name: String,
        arguments: Vec<Expression>,
        position: Position,
    },
    
    DixFunctionCall {
        function_name: String,
        arguments: Vec<Expression>,
        position: Position,
    },
    
    StaticMethodCall {
        object_name: String,
        method_name: String,
        arguments: Vec<Expression>,
        position: Position,
    },
    
    InstanceMethodCall {
        instance: Box<Expression>,
        method_name: String,
        arguments: Vec<Expression>,
        position: Position,
    },
    
    BuiltinFunction {
        target: Box<Expression>,
        method: String,
        arguments: Option<Vec<Expression>>,
        position: Position,
    },
    
    StaticFunction {
        class_name: String,
        method: String,
        arguments: Vec<Expression>,
        position: Position,
    },
    
    ImportedFunctionCall {
        namespace_name: String,
        function_name: String,
        arguments: Vec<Expression>,
        position: Position,
    },
    
    // Operators
    ArithmeticOp {
        left: Box<Expression>,
        operator: String,
        right: Box<Expression>,
        position: Position,
    },
    
    BitwiseOp {
        left: Box<Expression>,
        operator: String,
        right: Box<Expression>,
        position: Position,
    },
    
    ComparisonOp {
        left: Box<Expression>,
        operator: String,
        right: Box<Expression>,
        position: Position,
    },
    
    LogicalOp {
        left: Box<Expression>,
        operator: String,
        right: Box<Expression>,
        position: Position,
    },
    
    UnaryOp {
        operator: String,
        operand: Box<Expression>,
        position: Position,
    },
    
    // Access expressions
    ConfigAccess {
        key: String,
        position: Position,
    },
    
    EnumAccess {
        namespace_name: Option<String>, // null for local enums
        enum_name: String,
        value: String,
        position: Position,
    },
    
    ObjectAccess {
        path: Vec<String>,
        position: Position,
    },
    
    PropertyAccess {
        object: Box<Expression>,
        property: String,
        position: Position,
    },
    
    IndexAccess {
        object: Box<Expression>,
        index: Box<Expression>,
        position: Position,
    },
    
    // Value expression
    Value {
        value: Value,
        position: Position,
    },
    
    // Parenthesized expression
    Parenthesized {
        expression: Box<Expression>,
        position: Position,
    },
    
    // Conditional (ternary)
    Conditional {
        condition: Box<Expression>,
        true_value: Box<Expression>,
        false_value: Box<Expression>,
        position: Position,
    },
    
    // Type cast
    TypeCast {
        expression: Box<Expression>,
        target_type: DataType,
        position: Position,
    },
}

impl Expression {
    pub fn position(&self) -> Position {
        match self {
            Expression::Identifier { position, .. } => *position,
            Expression::QualifiedIdentifier { position, .. } => *position,
            Expression::FunctionCall { position, .. } => *position,
            Expression::QuickFuncCall { position, .. } => *position,
            Expression::DixFunctionCall { position, .. } => *position,
            Expression::StaticMethodCall { position, .. } => *position,
            Expression::InstanceMethodCall { position, .. } => *position,
            Expression::BuiltinFunction { position, .. } => *position,
            Expression::StaticFunction { position, .. } => *position,
            Expression::ImportedFunctionCall { position, .. } => *position,
            Expression::ArithmeticOp { position, .. } => *position,
            Expression::BitwiseOp { position, .. } => *position,
            Expression::ComparisonOp { position, .. } => *position,
            Expression::LogicalOp { position, .. } => *position,
            Expression::UnaryOp { position, .. } => *position,
            Expression::ConfigAccess { position, .. } => *position,
            Expression::EnumAccess { position, .. } => *position,
            Expression::ObjectAccess { position, .. } => *position,
            Expression::PropertyAccess { position, .. } => *position,
            Expression::IndexAccess { position, .. } => *position,
            Expression::Value { position, .. } => *position,
            Expression::Parenthesized { position, .. } => *position,
            Expression::Conditional { position, .. } => *position,
            Expression::TypeCast { position, .. } => *position,
        }
    }
}

impl std::fmt::Display for Expression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expression::Identifier { name, .. } => write!(f, "{}", name),
            
            Expression::QualifiedIdentifier { parts, arguments, .. } => {
                write!(f, "{}", parts.join("."))?;
                if let Some(args) = arguments {
                    write!(f, "(")?;
                    for (i, arg) in args.iter().enumerate() {
                        write!(f, "{}", arg)?;
                        if i < args.len() - 1 {
                            write!(f, ", ")?;
                        }
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            
            Expression::FunctionCall { name, arguments, .. } => {
                write!(f, "{}(", name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i < arguments.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ")")
            }
            
            Expression::QuickFuncCall { name, arguments, .. } => {
                write!(f, "{}(", name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i < arguments.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ")")
            }
            
            Expression::DixFunctionCall { function_name, arguments, .. } => {
                write!(f, "Dix.{}(", function_name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i < arguments.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ")")
            }
            
            Expression::StaticMethodCall { object_name, method_name, arguments, .. } => {
                write!(f, "{}.{}(", object_name, method_name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i < arguments.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ")")
            }
            
            Expression::InstanceMethodCall { instance, method_name, arguments, .. } => {
                write!(f, "{}.{}(", instance, method_name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i < arguments.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ")")
            }
            
            Expression::BuiltinFunction { target, method, arguments, .. } => {
                write!(f, "{}.{}", target, method)?;
                if let Some(args) = arguments {
                    write!(f, "(")?;
                    for (i, arg) in args.iter().enumerate() {
                        write!(f, "{}", arg)?;
                        if i < args.len() - 1 {
                            write!(f, ", ")?;
                        }
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            
            Expression::StaticFunction { class_name, method, arguments, .. } => {
                write!(f, "{}.{}(", class_name, method)?;
                for (i, arg) in arguments.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i < arguments.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ")")
            }
            
            Expression::ImportedFunctionCall { namespace_name, function_name, arguments, .. } => {
                write!(f, "{}.{}(", namespace_name, function_name)?;
                for (i, arg) in arguments.iter().enumerate() {
                    write!(f, "{}", arg)?;
                    if i < arguments.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                write!(f, ")")
            }
            
            Expression::ArithmeticOp { left, operator, right, .. } => {
                write!(f, "({} {} {})", left, operator, right)
            }
            
            Expression::BitwiseOp { left, operator, right, .. } => {
                write!(f, "({} {} {})", left, operator, right)
            }
            
            Expression::ComparisonOp { left, operator, right, .. } => {
                write!(f, "({} {} {})", left, operator, right)
            }
            
            Expression::LogicalOp { left, operator, right, .. } => {
                write!(f, "({} {} {})", left, operator, right)
            }
            
            Expression::UnaryOp { operator, operand, .. } => {
                write!(f, "{}{}", operator, operand)
            }
            
            Expression::ConfigAccess { key, .. } => {
                write!(f, "config.{}", key)
            }
            
            Expression::EnumAccess { namespace_name, enum_name, value, .. } => {
                if let Some(ns) = namespace_name {
                    write!(f, "{}.{}.{}", ns, enum_name, value)
                } else {
                    write!(f, "{}.{}", enum_name, value)
                }
            }
            
            Expression::ObjectAccess { path, .. } => {
                write!(f, "{}", path.join("."))
            }
            
            Expression::PropertyAccess { object, property, .. } => {
                write!(f, "{}.{}", object, property)
            }
            
            Expression::IndexAccess { object, index, .. } => {
                write!(f, "{}[{}]", object, index)
            }
            
            Expression::Value { value, .. } => {
                write!(f, "{}", value)
            }
            
            Expression::Parenthesized { expression, .. } => {
                write!(f, "({})", expression)
            }
            
            Expression::Conditional { condition, true_value, false_value, .. } => {
                write!(f, "{} ? {} : {}", condition, true_value, false_value)
            }
            
            Expression::TypeCast { expression, target_type, .. } => {
                write!(f, "{} as<{}>", expression, target_type)
            }
        }
    }
  }
