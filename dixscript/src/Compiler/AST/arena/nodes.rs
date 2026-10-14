// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/arena-ast.md, section "Compiler/AST/arena"
// ============================================================================
//! Node types of the index arena. One enum per owned type, with the same variants and
//! the same field names; children are ids and lists are spans (see `mod.rs`). Every node
//! is `Copy`.
//!
//! The node types deliberately do not implement `PartialEq`: their children are ids, so a
//! derived `==` would compare ids, not trees. Compare trees through the handles
//! (`Arena::value_ref` and friends), which compare structurally.
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

/// Arena form of `Value`.
#[derive(Debug, Clone, Copy)]
pub enum ValueNode {
    Integer { value: i32, position: Position },
    Long { value: i64, position: Position },
    Float { value: f32, position: Position },
    Double { value: f64, position: Position },
    ScientificNotation { value: f64, position: Position },
    String { value: Str, position: Position },
    Boolean { value: bool, position: Position },
    InterpolatedString { template: Str, expressions: ExprList, position: Position },
    HexColor { value: Str, position: Position },
    Date { value: Str, position: Position },
    Timestamp { value: Str, position: Position },
    Null { position: Position },
    Array { values: ValueList, position: Position },
    NestedArray { values: ValueList, level: usize, position: Position },
    Object { properties: PropList, position: Position },
    PrefixedConstructor { prefix: Str, arguments: ValueList, position: Position },
    EnumValue { enum_name: Str, value: Str, position: Position },
    Identifier { value: Str, position: Position },
    QuickFuncCall { function_name: Str, arguments: ExprList, position: Position },
    Expression { expr: ExprId, position: Position },
    Range { start: ValueId, end: ValueId, position: Position },
    Lambda { parameters: StrList, body: ExprId, statements: StmtList, position: Position },
    ParseError { message: Str, position: Position },
    Error { message: Str, position: Position },
    Unknown { data: UnknownId, position: Position },
}

impl ValueNode {
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

/// Arena form of `Expression`.
#[derive(Debug, Clone, Copy)]
pub enum ExprNode {
    Identifier { name: Str, position: Position },
    QualifiedIdentifier { parts: StrList, arguments: Option<ExprList>, position: Position },
    FunctionCall { name: Str, arguments: ExprList, position: Position },
    QuickFuncCall { name: Str, arguments: ExprList, position: Position },
    DixFunctionCall { function_name: Str, arguments: ExprList, position: Position },
    StaticMethodCall { object_name: Str, method_name: Str, arguments: ExprList, position: Position },
    InstanceMethodCall { instance: ExprId, method_name: Str, arguments: ExprList, position: Position },
    BuiltinFunction { target: ExprId, method: Str, arguments: Option<ExprList>, position: Position },
    StaticFunction { class_name: Str, method: Str, arguments: ExprList, position: Position },
    ImportedFunctionCall { namespace_name: Str, function_name: Str, arguments: ExprList, position: Position },
    ArithmeticOp { left: ExprId, operator: Str, right: ExprId, position: Position },
    BitwiseOp { left: ExprId, operator: Str, right: ExprId, position: Position },
    ComparisonOp { left: ExprId, operator: Str, right: ExprId, position: Position },
    LogicalOp { left: ExprId, operator: Str, right: ExprId, position: Position },
    UnaryOp { operator: Str, operand: ExprId, position: Position },
    ConfigAccess { key: Str, position: Position },
    EnumAccess { namespace_name: Option<Str>, enum_name: Str, value: Str, position: Position },
    ObjectAccess { path: StrList, position: Position },
    PropertyAccess { object: ExprId, property: Str, position: Position },
    IndexAccess { object: ExprId, index: ExprId, position: Position },
    Value { value: ValueId, position: Position },
    Parenthesized { expression: ExprId, position: Position },
    Conditional { condition: ExprId, true_value: ExprId, false_value: ExprId, position: Position },
    TypeCast { expression: ExprId, target_type: DataType, position: Position },
}

impl ExprNode {
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

/// Arena form of `QuickFuncStatement`.
#[derive(Debug, Clone, Copy)]
pub enum StmtNode {
    Return { value: ExprId, position: Position },
    If { condition: ExprId, then_branch: StmtList, else_branch: Option<StmtList>, position: Position },
    Switch { expression: ExprId, cases: CaseList, default_case: Option<CaseId>, position: Position },
    Assignment { variable: Str, value: ExprId, position: Position },
    ArithmeticAssignment { variable: Str, operator: Str, value: ExprId, position: Position },
    ObjectCreation { variable: Str, object: ValueId, position: Position },
    Log { value: ExprId, position: Position },
    ExpressionStatement { expression: ExprId, position: Position },
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

/// Arena form of `ObjectProperty`.
#[derive(Debug, Clone, Copy)]
pub struct ObjectPropNode {
    pub key: Str,
    pub value: ValueId,
    pub position: Position,
}

/// Arena form of `SwitchCase`.
#[derive(Debug, Clone, Copy)]
pub struct SwitchCaseNode {
    pub case_value: ValueId,
    pub statements: StmtList,
    pub position: Position,
}

/// The out-of-line payload of `Value::Unknown`.
#[derive(Debug, Clone, PartialEq)]
pub struct UnknownData {
    pub element_type: String,
    pub element_name: String,
    pub element_data: String,
    pub compatibility_result: CompatibilityResult,
}
