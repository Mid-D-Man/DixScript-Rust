// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena/nodes.rs"
// ============================================================================
//! Node types of the index arena.
//!
//! One enum per owned type ([`ValueNode`], [`ExprNode`], [`StmtNode`]) plus the helper records
//! [`ObjectPropNode`], [`SwitchCaseNode`] and [`UnknownData`]. Each node mirrors the owned
//! type of the same name variant for variant and keeps its field names; only the
//! representation changes, as set out in the table on [`ValueNode`].
//!
//! Every node is `Copy`. The node types deliberately do not implement `PartialEq`: their
//! children are ids, so a derived `==` would compare ids, not trees. Compare trees through
//! the handles ([`Arena::value_ref`](super::Arena::value_ref) and friends), which compare
//! structurally.
//!
//! `DataType`, `DeclarationType` and `Position` are already `Copy` and are stored inline.
//! `Unknown` values carry a `CompatibilityResult` that owns strings, so they live in a side
//! pool and the node holds an id; that keeps every other `ValueNode` small.

use super::{
    CaseId, CaseList, ExprId, ExprList, PropList, Str, StmtList, StrList, UnknownId, ValueId,
    ValueList,
};
use crate::Compiler::AST::{DataType, DeclarationType, Position};
use crate::Compiler::VersionControl::CompatibilityResult;

/// Arena form of [`Value`](crate::Compiler::AST::Value).
///
/// Each variant mirrors the `Value` variant of the same name and keeps its field names, so
/// the field-level documentation of the owned variant applies and is not repeated here. Only
/// the representation of a field changes:
///
/// | Owned field | Arena field |
/// |---|---|
/// | `String` | [`Str`], a span of the arena's text buffer |
/// | `Vec<Value>`, `Vec<Expression>`, `Vec<String>`, `Vec<ObjectProperty>`, `Vec<QuickFuncStatement>` | [`ValueList`], [`ExprList`], [`StrList`], [`PropList`], [`StmtList`] |
/// | `Box<Value>`, `Box<Expression>` | [`ValueId`], [`ExprId`] |
/// | `Option<Vec<T>>` | an `Option` of the list type, so `None` stays different from an empty list |
/// | numbers, `bool`, `usize`, [`Position`], `DataType`, `DeclarationType` | unchanged |
///
/// Read children through the [`Arena`](super::Arena) that owns the node, for example
/// [`Arena::value_list`](super::Arena::value_list) for a [`ValueList`].
#[derive(Debug, Clone, Copy)]
pub enum ValueNode {
    /// A 32-bit integer literal.
    #[allow(missing_docs)]
    Integer { value: i32, position: Position },
    /// A 64-bit integer: written with an `L` suffix, or produced when a literal overflows `i32`.
    #[allow(missing_docs)]
    Long { value: i64, position: Position },
    /// A 32-bit float literal.
    #[allow(missing_docs)]
    Float { value: f32, position: Position },
    /// A 64-bit float literal.
    #[allow(missing_docs)]
    Double { value: f64, position: Position },
    /// A float written in scientific notation, such as `1.5e10`.
    #[allow(missing_docs)]
    ScientificNotation { value: f64, position: Position },
    /// A string literal. `value` is the text without the quotes.
    #[allow(missing_docs)]
    String { value: Str, position: Position },
    /// `true` or `false`.
    #[allow(missing_docs)]
    Boolean { value: bool, position: Position },
    /// An interpolated string. `template` is the text, `expressions` the embedded expressions.
    #[allow(missing_docs)]
    InterpolatedString { template: Str, expressions: ExprList, position: Position },
    /// A hex colour literal such as `#FF00AA`.
    #[allow(missing_docs)]
    HexColor { value: Str, position: Position },
    /// A date literal.
    #[allow(missing_docs)]
    Date { value: Str, position: Position },
    /// A timestamp literal.
    #[allow(missing_docs)]
    Timestamp { value: Str, position: Position },
    /// The `null` value.
    #[allow(missing_docs)]
    Null { position: Position },
    /// An array of values.
    #[allow(missing_docs)]
    Array { values: ValueList, position: Position },
    /// An array together with its nesting `level`.
    #[allow(missing_docs)]
    NestedArray { values: ValueList, level: usize, position: Position },
    /// An object literal: a list of key and value properties.
    #[allow(missing_docs)]
    Object { properties: PropList, position: Position },
    /// A constructor written with a prefix, `prefix:(arguments)`.
    #[allow(missing_docs)]
    PrefixedConstructor { prefix: Str, arguments: ValueList, position: Position },
    /// A value of an enum, written `Enum.Value`.
    #[allow(missing_docs)]
    EnumValue { enum_name: Str, value: Str, position: Position },
    /// A variable reference.
    #[allow(missing_docs)]
    Identifier { value: Str, position: Position },
    /// The result of calling a QuickFunc.
    #[allow(missing_docs)]
    QuickFuncCall { function_name: Str, arguments: ExprList, position: Position },
    /// An expression to be evaluated later; it prints as `<ToEvaluate: ...>`.
    #[allow(missing_docs)]
    Expression { expr: ExprId, position: Position },
    /// A range, `start..end`.
    #[allow(missing_docs)]
    Range { start: ValueId, end: ValueId, position: Position },
    /// A lambda. `statements` is non-empty for a block-body lambda `(x) => { ... }`, and `body`
    /// then holds the extracted return expression.
    #[allow(missing_docs)]
    Lambda { parameters: StrList, body: ExprId, statements: StmtList, position: Position },
    /// A value the parser could not read; `message` says why.
    #[allow(missing_docs)]
    ParseError { message: Str, position: Position },
    /// An error value; `message` describes it.
    #[allow(missing_docs)]
    Error { message: Str, position: Position },
    /// An element this version cannot interpret. Its type, name, data and compatibility result
    /// are held out of line; read them with [`Arena::unknown`](super::Arena::unknown).
    #[allow(missing_docs)]
    Unknown { data: UnknownId, position: Position },
}

impl ValueNode {
    /// The source position of this node.
    pub fn position(&self) -> Position {
        match self {
            ValueNode::Integer { position, .. }
            | ValueNode::Long { position, .. }
            | ValueNode::Float { position, .. }
            | ValueNode::Double { position, .. }
            | ValueNode::ScientificNotation { position, .. }
            | ValueNode::String { position, .. }
            | ValueNode::Boolean { position, .. }
            | ValueNode::InterpolatedString { position, .. }
            | ValueNode::HexColor { position, .. }
            | ValueNode::Date { position, .. }
            | ValueNode::Timestamp { position, .. }
            | ValueNode::Null { position, .. }
            | ValueNode::Array { position, .. }
            | ValueNode::NestedArray { position, .. }
            | ValueNode::Object { position, .. }
            | ValueNode::PrefixedConstructor { position, .. }
            | ValueNode::EnumValue { position, .. }
            | ValueNode::Identifier { position, .. }
            | ValueNode::QuickFuncCall { position, .. }
            | ValueNode::Expression { position, .. }
            | ValueNode::Range { position, .. }
            | ValueNode::Lambda { position, .. }
            | ValueNode::ParseError { position, .. }
            | ValueNode::Error { position, .. }
            | ValueNode::Unknown { position, .. } => *position,
        }
    }
}

/// Arena form of [`Expression`](crate::Compiler::AST::Expression).
///
/// Each variant mirrors the `Expression` variant of the same name and keeps its field names;
/// the representation changes exactly as described on [`ValueNode`]. Call arguments are an
/// [`ExprList`], and an `Option<Vec<Expression>>` becomes `Option<ExprList>`.
#[derive(Debug, Clone, Copy)]
pub enum ExprNode {
    /// A plain identifier.
    #[allow(missing_docs)]
    Identifier { name: Str, position: Position },
    /// An ambiguous qualified identifier such as `a.b.c`. It may be an enum value, an object
    /// property or a namespaced function, and is resolved during semantic analysis.
    /// `arguments` is `None` for a property or enum access and `Some` for a call.
    #[allow(missing_docs)]
    QualifiedIdentifier { parts: StrList, arguments: Option<ExprList>, position: Position },
    /// A function call, `name(arguments)`.
    #[allow(missing_docs)]
    FunctionCall { name: Str, arguments: ExprList, position: Position },
    /// A call to a QuickFunc, `name(arguments)`.
    #[allow(missing_docs)]
    QuickFuncCall { name: Str, arguments: ExprList, position: Position },
    /// A call to a built-in `Dix` function, `Dix.name(arguments)`.
    #[allow(missing_docs)]
    DixFunctionCall { function_name: Str, arguments: ExprList, position: Position },
    /// A static method call, `Object.method(arguments)`.
    #[allow(missing_docs)]
    StaticMethodCall { object_name: Str, method_name: Str, arguments: ExprList, position: Position },
    /// A method call on an instance, `instance.method(arguments)`.
    #[allow(missing_docs)]
    InstanceMethodCall { instance: ExprId, method_name: Str, arguments: ExprList, position: Position },
    /// A built-in method on a target, `target.method`, with `(arguments)` when `arguments` is
    /// `Some`.
    #[allow(missing_docs)]
    BuiltinFunction { target: ExprId, method: Str, arguments: Option<ExprList>, position: Position },
    /// A static function call, `Class.method(arguments)`.
    #[allow(missing_docs)]
    StaticFunction { class_name: Str, method: Str, arguments: ExprList, position: Position },
    /// A call to a function of an imported namespace, `namespace.function(arguments)`.
    #[allow(missing_docs)]
    ImportedFunctionCall { namespace_name: Str, function_name: Str, arguments: ExprList, position: Position },
    /// A binary arithmetic operation.
    #[allow(missing_docs)]
    ArithmeticOp { left: ExprId, operator: Str, right: ExprId, position: Position },
    /// A binary bitwise operation.
    #[allow(missing_docs)]
    BitwiseOp { left: ExprId, operator: Str, right: ExprId, position: Position },
    /// A comparison.
    #[allow(missing_docs)]
    ComparisonOp { left: ExprId, operator: Str, right: ExprId, position: Position },
    /// A logical operation.
    #[allow(missing_docs)]
    LogicalOp { left: ExprId, operator: Str, right: ExprId, position: Position },
    /// A prefix operation such as negation.
    #[allow(missing_docs)]
    UnaryOp { operator: Str, operand: ExprId, position: Position },
    /// A read of a `@CONFIG` entry, `config.key`.
    #[allow(missing_docs)]
    ConfigAccess { key: Str, position: Position },
    /// An enum access, `Enum.Value`, with an optional namespace in front.
    #[allow(missing_docs)]
    EnumAccess { namespace_name: Option<Str>, enum_name: Str, value: Str, position: Position },
    /// A dotted path into an object.
    #[allow(missing_docs)]
    ObjectAccess { path: StrList, position: Position },
    /// A property read, `object.property`.
    #[allow(missing_docs)]
    PropertyAccess { object: ExprId, property: Str, position: Position },
    /// An index read, `object[index]`.
    #[allow(missing_docs)]
    IndexAccess { object: ExprId, index: ExprId, position: Position },
    /// A literal value used as an expression.
    #[allow(missing_docs)]
    Value { value: ValueId, position: Position },
    /// A parenthesised expression.
    #[allow(missing_docs)]
    Parenthesized { expression: ExprId, position: Position },
    /// A conditional, `condition ? true_value : false_value`.
    #[allow(missing_docs)]
    Conditional { condition: ExprId, true_value: ExprId, false_value: ExprId, position: Position },
    /// A type cast, `expression as<target_type>`.
    #[allow(missing_docs)]
    TypeCast { expression: ExprId, target_type: DataType, position: Position },
}

impl ExprNode {
    /// The source position of this node.
    pub fn position(&self) -> Position {
        match self {
            ExprNode::Identifier { position, .. }
            | ExprNode::QualifiedIdentifier { position, .. }
            | ExprNode::FunctionCall { position, .. }
            | ExprNode::QuickFuncCall { position, .. }
            | ExprNode::DixFunctionCall { position, .. }
            | ExprNode::StaticMethodCall { position, .. }
            | ExprNode::InstanceMethodCall { position, .. }
            | ExprNode::BuiltinFunction { position, .. }
            | ExprNode::StaticFunction { position, .. }
            | ExprNode::ImportedFunctionCall { position, .. }
            | ExprNode::ArithmeticOp { position, .. }
            | ExprNode::BitwiseOp { position, .. }
            | ExprNode::ComparisonOp { position, .. }
            | ExprNode::LogicalOp { position, .. }
            | ExprNode::UnaryOp { position, .. }
            | ExprNode::ConfigAccess { position, .. }
            | ExprNode::EnumAccess { position, .. }
            | ExprNode::ObjectAccess { position, .. }
            | ExprNode::PropertyAccess { position, .. }
            | ExprNode::IndexAccess { position, .. }
            | ExprNode::Value { position, .. }
            | ExprNode::Parenthesized { position, .. }
            | ExprNode::Conditional { position, .. }
            | ExprNode::TypeCast { position, .. } => *position,
        }
    }
}

/// Arena form of [`QuickFuncStatement`](crate::Compiler::AST::QuickFuncStatement).
///
/// Each variant mirrors the `QuickFuncStatement` variant of the same name and keeps its field
/// names; the representation changes exactly as described on [`ValueNode`]. A statement list is
/// a [`StmtList`].
#[derive(Debug, Clone, Copy)]
pub enum StmtNode {
    /// `return value`.
    #[allow(missing_docs)]
    Return { value: ExprId, position: Position },
    /// An `if` statement with an optional `else` branch. `None` and an empty `else` differ.
    #[allow(missing_docs)]
    If { condition: ExprId, then_branch: StmtList, else_branch: Option<StmtList>, position: Position },
    /// A `chk` (switch) statement. `cases` is a run of [`SwitchCaseNode`]; `default_case`
    /// identifies the default case, which is stored outside that run. Read cases with
    /// [`Arena::case_list`](super::Arena::case_list) and [`Arena::case`](super::Arena::case).
    #[allow(missing_docs)]
    Switch { expression: ExprId, cases: CaseList, default_case: Option<CaseId>, position: Position },
    /// `variable = value`.
    #[allow(missing_docs)]
    Assignment { variable: Str, value: ExprId, position: Position },
    /// An arithmetic assignment such as `variable += value`; `operator` holds the whole
    /// operator text.
    #[allow(missing_docs)]
    ArithmeticAssignment { variable: Str, operator: Str, value: ExprId, position: Position },
    /// `variable = { ... }` with an object value.
    #[allow(missing_docs)]
    ObjectCreation { variable: Str, object: ValueId, position: Position },
    /// `log: value`.
    #[allow(missing_docs)]
    Log { value: ExprId, position: Position },
    /// An expression evaluated for its effect.
    #[allow(missing_docs)]
    ExpressionStatement { expression: ExprId, position: Position },
    /// A variable declaration with an optional data type annotation.
    #[allow(missing_docs)]
    VariableDeclaration {
        declaration_type: DeclarationType,
        is_mutable: bool,
        variable_name: Str,
        data_type: Option<DataType>,
        value: ExprId,
        position: Position,
    },
}

impl StmtNode {
    /// The source position of this node.
    pub fn position(&self) -> Position {
        match self {
            StmtNode::Return { position, .. }
            | StmtNode::If { position, .. }
            | StmtNode::Switch { position, .. }
            | StmtNode::Assignment { position, .. }
            | StmtNode::ArithmeticAssignment { position, .. }
            | StmtNode::ObjectCreation { position, .. }
            | StmtNode::Log { position, .. }
            | StmtNode::ExpressionStatement { position, .. }
            | StmtNode::VariableDeclaration { position, .. } => *position,
        }
    }
}

/// Arena form of [`ObjectProperty`](crate::Compiler::AST::ObjectProperty): one key and value
/// of an object literal.
#[derive(Debug, Clone, Copy)]
pub struct ObjectPropNode {
    /// The property name.
    pub key: Str,
    /// The property value.
    pub value: ValueId,
    /// Where the property starts in the source.
    pub position: Position,
}

/// Arena form of [`SwitchCase`](crate::Compiler::AST::SwitchCase): one case of a `chk`
/// statement.
#[derive(Debug, Clone, Copy)]
pub struct SwitchCaseNode {
    /// The value this case matches.
    pub case_value: ValueId,
    /// The statements that run when the case matches.
    pub statements: StmtList,
    /// Where the case starts in the source.
    pub position: Position,
}

/// The out-of-line payload of [`ValueNode::Unknown`].
///
/// It owns its strings and a `CompatibilityResult`, which is why it is kept in a side pool
/// instead of inside the node.
#[derive(Debug, Clone, PartialEq)]
pub struct UnknownData {
    /// The kind of element, as written in the source.
    pub element_type: String,
    /// The element's name.
    pub element_name: String,
    /// The element's raw data.
    pub element_data: String,
    /// Whether, and how, this version can handle the element.
    pub compatibility_result: CompatibilityResult,
}
