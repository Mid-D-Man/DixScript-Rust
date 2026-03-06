//! Helper functions for creating AST nodes
//! 
//! Simple construction helpers - no validation, no complex logic
//! Just makes it easier to build AST nodes for testing and parsing

use super::position::Position;
use super::data_types::DataType;
use super::config::{ConfigEntry, ConfigValue};
use super::imports::ImportDeclaration;
use super::enums::{EnumField, EnumDeclaration};
use super::data::{DataEntry, TablePath, PropertyAssignment};
use super::values::{Value, ObjectProperty};
use super::expressions::Expression;
use super::statements::QuickFuncStatement;

// ==================== EXPRESSION HELPERS ====================

pub fn create_identifier(name: String, position: Position) -> Expression {
    Expression::Identifier { name, position }
}

pub fn create_value_expr(value: Value, position: Position) -> Expression {
    Expression::Value { value, position }
}

pub fn create_arithmetic(
    left: Expression,
    operator: String,
    right: Expression,
    position: Position,
) -> Expression {
    Expression::ArithmeticOp {
        left: Box::new(left),
        operator,
        right: Box::new(right),
        position,
    }
}

pub fn create_bitwise(
    left: Expression,
    operator: String,
    right: Expression,
    position: Position,
) -> Expression {
    Expression::BitwiseOp {
        left: Box::new(left),
        operator,
        right: Box::new(right),
        position,
    }
}

pub fn create_static_call(
    object_name: String,
    method_name: String,
    arguments: Vec<Expression>,
    position: Position,
) -> Expression {
    Expression::StaticMethodCall {
        object_name,
        method_name,
        arguments,
        position,
    }
}

pub fn create_instance_call(
    instance: Expression,
    method_name: String,
    arguments: Vec<Expression>,
    position: Position,
) -> Expression {
    Expression::InstanceMethodCall {
        instance: Box::new(instance),
        method_name,
        arguments,
        position,
    }
}

// ==================== VALUE HELPERS ====================

pub fn create_int(value: i32, position: Position) -> Value {
    Value::Integer { value, position }
}

pub fn create_float(value: f32, position: Position) -> Value {
    Value::Float { value, position }
}

pub fn create_double(value: f64, position: Position) -> Value {
    Value::Double { value, position }
}

pub fn create_string(value: String, position: Position) -> Value {
    Value::String { value, position }
}

pub fn create_bool(value: bool, position: Position) -> Value {
    Value::Boolean { value, position }
}

pub fn create_array(values: Vec<Value>, position: Position) -> Value {
    Value::Array { values, position }
}

pub fn create_object(properties: Vec<ObjectProperty>, position: Position) -> Value {
    Value::Object { properties, position }
}

pub fn create_null(position: Position) -> Value {
    Value::Null { position }
}

// ==================== STATEMENT HELPERS ====================

pub fn create_assignment(variable: String, value: Expression, position: Position) -> QuickFuncStatement {
    QuickFuncStatement::Assignment {
        variable,
        value,
        position,
    }
}

pub fn create_return(value: Expression, position: Position) -> QuickFuncStatement {
    QuickFuncStatement::Return { value, position }
}

pub fn create_if(
    condition: Expression,
    then_branch: Vec<QuickFuncStatement>,
    else_branch: Option<Vec<QuickFuncStatement>>,
    position: Position,
) -> QuickFuncStatement {
    QuickFuncStatement::If {
        condition,
        then_branch,
        else_branch,
        position,
    }
}

pub fn create_log(value: Expression, position: Position) -> QuickFuncStatement {
    QuickFuncStatement::Log { value, position }
}

// ==================== CONFIG HELPERS ====================

pub fn create_config_entry(key: String, value: ConfigValue, position: Position) -> ConfigEntry {
    ConfigEntry::new(key, value, position)
}

pub fn create_config_string(value: String) -> ConfigValue {
    ConfigValue::String(value)
}

pub fn create_config_bool(value: bool) -> ConfigValue {
    ConfigValue::Boolean(value)
}

pub fn create_config_int(value: i32) -> ConfigValue {
    ConfigValue::Integer(value)
}

pub fn create_config_float(value: f32) -> ConfigValue {
    ConfigValue::Float(value)
}

// ==================== ENUM HELPERS ====================

pub fn create_enum_field(name: String, value: Option<i32>, position: Position) -> EnumField {
    EnumField::new(name, value, position)
}

pub fn create_enum(name: String, fields: Vec<EnumField>, position: Position) -> EnumDeclaration {
    EnumDeclaration::new(name, fields, position)
}

// ==================== DATA SECTION HELPERS ====================

pub fn create_simple_property(
    name: String,
    value: Value,
    data_type: Option<DataType>,
    position: Position,
) -> DataEntry {
    DataEntry::SimpleProperty {
        name,
        data_type,
        value,
        position,
    }
}

pub fn create_table_property(
    path: TablePath,
    properties: Vec<PropertyAssignment>,
    position: Position,
) -> DataEntry {
    DataEntry::TableProperty {
        path,
        properties,
        position,
    }
}

pub fn create_group_array(
    path: TablePath,
    items: Vec<Value>,
    position: Position,
) -> DataEntry {
    DataEntry::GroupArray {
        path,
        items,
        position,
    }
}

pub fn create_table_path(segments: Vec<String>) -> TablePath {
    TablePath::new(segments)
}

pub fn create_property_assignment(
    name: String,
    value: Value,
    data_type: Option<DataType>,
    position: Position,
) -> PropertyAssignment {
    PropertyAssignment::new(name, data_type, value, position)
}

// ==================== IMPORTS HELPERS ====================

pub fn create_import_declaration(
    alias: String,
    path: String,
    verify_hash: Option<String>,
    position: Position,
) -> ImportDeclaration {
    ImportDeclaration::local(alias, path, verify_hash, position)
}

pub fn create_cloud_import_declaration(
    alias: String,
    path: String,
    verify_hash: Option<String>,
    position: Position,
) -> ImportDeclaration {
    ImportDeclaration::new(alias, path, true, verify_hash, position)
}

// ==================== EXPRESSION BUILDERS ====================

pub fn create_imported_function_call(
    namespace_name: String,
    function_name: String,
    arguments: Vec<Expression>,
    position: Position,
) -> Expression {
    Expression::ImportedFunctionCall {
        namespace_name,
        function_name,
        arguments,
        position,
    }
}

pub fn create_namespaced_enum_access(
    namespace_name: String,
    enum_name: String,
    value: String,
    position: Position,
) -> Expression {
    Expression::EnumAccess {
        namespace_name: Some(namespace_name),
        enum_name,
        value,
        position,
    }
}

pub fn create_enum_value(enum_name: String, value: String, position: Position) -> Value {
    Value::EnumValue {
        enum_name,
        value,
        position,
    }
}

pub fn create_enum_access(enum_name: String, value: String, position: Position) -> Expression {
    Expression::EnumAccess {
        namespace_name: None,
        enum_name,
        value,
        position,
    }
}
