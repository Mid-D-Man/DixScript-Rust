// src/Compiler/Core/SectionEnhancers/qualified_identifier_resolver.rs
//! Resolves QualifiedIdentifier nodes into concrete expression types
//! Uses resolution metadata from semantic analysis

use crate::Compiler::AST::*;
use crate::Compiler::Core::SectionEnhancers::qualified_identifier_resolution::{
    QualifiedIdentifierKey, QualifiedIdentifierResolution, QualifiedIdentifierType,
};
use crate::ErrorManager::ErrorManager;
use std::collections::HashMap;

/// Resolves QualifiedIdentifier nodes into concrete expression types
pub struct QualifiedIdentifierResolver {
    resolutions: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution>,
    error_manager: ErrorManager,
}

impl QualifiedIdentifierResolver {
    /// Create new resolver with resolution metadata
    pub fn new(
        resolutions: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution>,
    ) -> Self {
        QualifiedIdentifierResolver {
            resolutions,
            error_manager: ErrorManager::get_shared_instance(),
        }
    }
    
    // ==================== STATEMENT RESOLUTION ====================
    
    /// Resolve qualified identifiers in a statement
    pub fn resolve_statement(&self, statement: &QuickFuncStatement) -> QuickFuncStatement {
        match statement {
            QuickFuncStatement::Return { value, position } => {
                QuickFuncStatement::Return {
                    value: self.resolve_expression(value),
                    position: *position,
                }
            }
            
            QuickFuncStatement::Assignment { variable, value, position } => {
                QuickFuncStatement::Assignment {
                    variable: variable.clone(),
                    value: self.resolve_expression(value),
                    position: *position,
                }
            }
            
            QuickFuncStatement::ArithmeticAssignment { variable, operator, value, position } => {
                QuickFuncStatement::ArithmeticAssignment {
                    variable: variable.clone(),
                    operator: operator.clone(),
                    value: self.resolve_expression(value),
                    position: *position,
                }
            }
            
            QuickFuncStatement::VariableDeclaration {
                declaration_type,
                is_mutable,
                variable_name,
                data_type,
                value,
                position,
            } => {
                QuickFuncStatement::VariableDeclaration {
                    declaration_type: *declaration_type,
                    is_mutable: *is_mutable,
                    variable_name: variable_name.clone(),
                    data_type: *data_type,
                    value: self.resolve_expression(value),
                    position: *position,
                }
            }
            
            QuickFuncStatement::If { condition, then_branch, else_branch, position } => {
                QuickFuncStatement::If {
                    condition: self.resolve_expression(condition),
                    then_branch: self.resolve_statements(then_branch),
                    else_branch: else_branch.as_ref().map(|branch| self.resolve_statements(branch)),
                    position: *position,
                }
            }
            
            QuickFuncStatement::Switch { expression, cases, default_case, position } => {
                QuickFuncStatement::Switch {
                    expression: self.resolve_expression(expression),
                    cases: cases.iter().map(|case| self.resolve_switch_case(case)).collect(),
                    default_case: default_case.as_ref().map(|case| Box::new(self.resolve_switch_case(case))),
                    position: *position,
                }
            }
            
            QuickFuncStatement::Log { value, position } => {
                QuickFuncStatement::Log {
                    value: self.resolve_expression(value),
                    position: *position,
                }
            }
            
            QuickFuncStatement::ExpressionStatement { expression, position } => {
                QuickFuncStatement::ExpressionStatement {
                    expression: self.resolve_expression(expression),
                    position: *position,
                }
            }
            
            QuickFuncStatement::ObjectCreation { variable, object, position } => {
                QuickFuncStatement::ObjectCreation {
                    variable: variable.clone(),
                    object: self.resolve_value(object),
                    position: *position,
                }
            }
        }
    }
    
    fn resolve_statements(&self, statements: &[QuickFuncStatement]) -> Vec<QuickFuncStatement> {
        statements.iter().map(|stmt| self.resolve_statement(stmt)).collect()
    }
    
    fn resolve_switch_case(&self, case: &SwitchCase) -> SwitchCase {
        SwitchCase::new(
            self.resolve_value(&case.case_value),
            self.resolve_statements(&case.statements),
            case.position,
        )
    }
    
    // ==================== EXPRESSION RESOLUTION ====================
    
    /// ⭐ KEY METHOD: Resolve qualified identifiers in an expression
    /// Transforms QualifiedIdentifier to concrete types based on resolution metadata
    pub fn resolve_expression(&self, expr: &Expression) -> Expression {
        match expr {
            // ⭐ QUALIFIED IDENTIFIER - Transform to concrete type
            Expression::QualifiedIdentifier { .. } => self.transform_qualified_identifier(expr),
            
            // Recurse into compound expressions
            Expression::ArithmeticOp { left, operator, right, position } => {
                Expression::ArithmeticOp {
                    left: Box::new(self.resolve_expression(left)),
                    operator: operator.clone(),
                    right: Box::new(self.resolve_expression(right)),
                    position: *position,
                }
            }
            
            Expression::BitwiseOp { left, operator, right, position } => {
                Expression::BitwiseOp {
                    left: Box::new(self.resolve_expression(left)),
                    operator: operator.clone(),
                    right: Box::new(self.resolve_expression(right)),
                    position: *position,
                }
            }
            
            Expression::ComparisonOp { left, operator, right, position } => {
                Expression::ComparisonOp {
                    left: Box::new(self.resolve_expression(left)),
                    operator: operator.clone(),
                    right: Box::new(self.resolve_expression(right)),
                    position: *position,
                }
            }
            
            Expression::LogicalOp { left, operator, right, position } => {
                Expression::LogicalOp {
                    left: Box::new(self.resolve_expression(left)),
                    operator: operator.clone(),
                    right: Box::new(self.resolve_expression(right)),
                    position: *position,
                }
            }
            
            Expression::UnaryOp { operator, operand, position } => {
                Expression::UnaryOp {
                    operator: operator.clone(),
                    operand: Box::new(self.resolve_expression(operand)),
                    position: *position,
                }
            }
            
            Expression::Conditional { condition, true_value, false_value, position } => {
                Expression::Conditional {
                    condition: Box::new(self.resolve_expression(condition)),
                    true_value: Box::new(self.resolve_expression(true_value)),
                    false_value: Box::new(self.resolve_expression(false_value)),
                    position: *position,
                }
            }
            
            Expression::Parenthesized { expression, position } => {
                Expression::Parenthesized {
                    expression: Box::new(self.resolve_expression(expression)),
                    position: *position,
                }
            }
            
            Expression::PropertyAccess { object, property, position } => {
                Expression::PropertyAccess {
                    object: Box::new(self.resolve_expression(object)),
                    property: property.clone(),
                    position: *position,
                }
            }
            
            Expression::IndexAccess { object, index, position } => {
                Expression::IndexAccess {
                    object: Box::new(self.resolve_expression(object)),
                    index: Box::new(self.resolve_expression(index)),
                    position: *position,
                }
            }
            
            Expression::InstanceMethodCall { instance, method_name, arguments, position } => {
                Expression::InstanceMethodCall {
                    instance: Box::new(self.resolve_expression(instance)),
                    method_name: method_name.clone(),
                    arguments: self.resolve_expressions(arguments),
                    position: *position,
                }
            }
            
            Expression::StaticMethodCall { object_name, method_name, arguments, position } => {
                Expression::StaticMethodCall {
                    object_name: object_name.clone(),
                    method_name: method_name.clone(),
                    arguments: self.resolve_expressions(arguments),
                    position: *position,
                }
            }
            
            Expression::QuickFuncCall { name, arguments, position } => {
                Expression::QuickFuncCall {
                    name: name.clone(),
                    arguments: self.resolve_expressions(arguments),
                    position: *position,
                }
            }
            
            Expression::ImportedFunctionCall { namespace_name, function_name, arguments, position } => {
                Expression::ImportedFunctionCall {
                    namespace_name: namespace_name.clone(),
                    function_name: function_name.clone(),
                    arguments: self.resolve_expressions(arguments),
                    position: *position,
                }
            }
            
            Expression::Value { value, position } => {
                Expression::Value {
                    value: self.resolve_value(value),
                    position: *position,
                }
            }
            
            // Already concrete types - pass through
            _ => expr.clone(),
        }
    }
    
    fn resolve_expressions(&self, expressions: &[Expression]) -> Vec<Expression> {
        expressions.iter().map(|expr| self.resolve_expression(expr)).collect()
    }
    
    // ==================== QUALIFIED IDENTIFIER TRANSFORMATION ====================
    
    /// ⭐ CRITICAL: Transform QualifiedIdentifier into the appropriate concrete expression type
    /// This is where the magic happens - we use resolution metadata to determine the exact type
    fn transform_qualified_identifier(&self, expr: &Expression) -> Expression {
        let (parts, arguments, position) = if let Expression::QualifiedIdentifier { parts, arguments, position } = expr {
            (parts, arguments, position)
        } else {
            return expr.clone();
        };
        
        let parts_str = parts.join(".");
        
        // Create lookup key
        let key = QualifiedIdentifierKey {
            position: *position,
            parts: parts.clone(),
            is_call: arguments.is_some(),
        };
        
        // ⭐ DIAGNOSTIC: Check if we have resolution metadata
        self.error_manager.log_debug(&format!("[QualIdResolver] Attempting to resolve: {}", parts_str));
        
        // Check if we have resolution metadata for this node
        if let Some(resolution) = self.resolutions.get(&key) {
            self.error_manager.log_debug(&format!(
                "[QualIdResolver] ✅ FOUND RESOLUTION for {}: {}",
                parts_str, resolution.resolved_type
            ));
            
            // ⭐ Transform based on resolved type
            return self.apply_resolution(parts, arguments.as_ref(), *position, resolution);
        }
        
        // No metadata - log and default to property access chain or function call
        self.error_manager.log_debug(&format!(
            "[QualIdResolver] ❌ NO RESOLUTION FOUND for {}",
            parts_str
        ));
        
        if arguments.is_some() {
            // It's a call but we don't know what kind - treat as regular function call
            Expression::QuickFuncCall {
                name: parts.join("."),
                arguments: arguments.as_ref().map(|args| self.resolve_expressions(args)).unwrap_or_default(),
                position: *position,
            }
        } else {
            // Property access chain: a.b.c
            self.build_property_access_chain(parts, *position)
        }
    }
    
    /// Apply resolution to create the appropriate expression type
    fn apply_resolution(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
        position: Position,
        resolution: &QualifiedIdentifierResolution,
    ) -> Expression {
        match resolution.resolved_type {
            QualifiedIdentifierType::LocalEnumAccess => {
                // Status.ACTIVE → EnumAccess(null, "Status", "ACTIVE")
                Expression::EnumAccess {
                    namespace_name: None,
                    enum_name: parts[0].clone(),
                    value: parts[1].clone(),
                    position,
                }
            }
            
            QualifiedIdentifierType::ImportedEnumAccess => {
                // utils.Status.ACTIVE → EnumAccess("utils", "Status", "ACTIVE")
                Expression::EnumAccess {
                    namespace_name: Some(parts[0].clone()),
                    enum_name: parts[1].clone(),
                    value: parts[2].clone(),
                    position,
                }
            }
            
            QualifiedIdentifierType::ImportedFunctionCall => {
                // utils.calculateTax() → ImportedFunctionCall("utils", "calculateTax", args)
                Expression::ImportedFunctionCall {
                    namespace_name: parts[0].clone(),
                    function_name: parts[1].clone(),
                    arguments: arguments.map(|args| self.resolve_expressions(args)).unwrap_or_default(),
                    position,
                }
            }
            
            QualifiedIdentifierType::StaticObjectAccess => {
                // Math.sqrt() → StaticMethodCall("Math", "sqrt", args)
                // Math.PI → PropertyAccess chain
                if arguments.is_some() {
                    Expression::StaticMethodCall {
                        object_name: parts[0].clone(),
                        method_name: parts[1].clone(),
                        arguments: arguments.map(|args| self.resolve_expressions(args)).unwrap_or_default(),
                        position,
                    }
                } else {
                    self.build_property_access_chain(parts, position)
                }
            }
            
            QualifiedIdentifierType::ObjectPropertyAccess => {
                // user.name or text.upper() → PropertyAccess chain or InstanceMethodCall
                if arguments.is_some() {
                    self.build_instance_method_call(parts, arguments.unwrap(), position)
                } else {
                    self.build_property_access_chain(parts, position)
                }
            }
            
            QualifiedIdentifierType::NamespaceEnumReference => {
                // utils.Status (not a value access) → ObjectAccess
                Expression::ObjectAccess {
                    path: parts.to_vec(),
                    position,
                }
            }
            
            _ => self.build_property_access_chain(parts, position),
        }
    }
    
    /// Build PropertyAccess chain from parts
    /// Example: a.b.c → PropertyAccess(PropertyAccess(Identifier(a), b), c)
    fn build_property_access_chain(&self, parts: &[String], position: Position) -> Expression {
        let mut current = Expression::Identifier {
            name: parts[0].clone(),
            position,
        };
        
        for part in &parts[1..] {
            current = Expression::PropertyAccess {
                object: Box::new(current),
                property: part.clone(),
                position,
            };
        }
        
        current
    }
    
    /// Build InstanceMethodCall from parts
    /// Example: text.upper() → InstanceMethodCall(Identifier(text), "upper", args)
    fn build_instance_method_call(
        &self,
        parts: &[String],
        arguments: &[Expression],
        position: Position,
    ) -> Expression {
        if parts.len() < 2 {
            // Fallback to property access
            return self.build_property_access_chain(parts, position);
        }
        
        // Build instance expression from all parts except the last one
        let instance = if parts.len() == 2 {
            // Simple case: text.upper()
            Expression::Identifier {
                name: parts[0].clone(),
                position,
            }
        } else {
            // Complex case: user.address.city.format()
            // Build property access chain for all except last part
            self.build_property_access_chain(&parts[..parts.len() - 1], position)
        };
        
        // Last part is the method name
        let method_name = parts[parts.len() - 1].clone();
        
        Expression::InstanceMethodCall {
            instance: Box::new(instance),
            method_name,
            arguments: self.resolve_expressions(arguments),
            position,
        }
    }
    
    // ==================== VALUE RESOLUTION ====================
    
    fn resolve_value(&self, value: &Value) -> Value {
        match value {
            Value::Array { values, position } => {
                Value::Array {
                    values: values.iter().map(|v| self.resolve_value(v)).collect(),
                    position: *position,
                }
            }
            
            Value::Object { properties, position } => {
                Value::Object {
                    properties: properties.iter().map(|prop| {
                        ObjectProperty::new(
                            prop.key.clone(),
                            self.resolve_value(&prop.value),
                            prop.position,
                        )
                    }).collect(),
                    position: *position,
                }
            }
            
            Value::PrefixedConstructor { prefix, arguments, position } => {
                Value::PrefixedConstructor {
                    prefix: prefix.clone(),
                    arguments: arguments.iter().map(|v| self.resolve_value(v)).collect(),
                    position: *position,
                }
            }
            
            Value::Expression { expr, position } => {
                Value::Expression {
                    expr: Box::new(self.resolve_expression(expr)),
                    position: *position,
                }
            }
            
            Value::InterpolatedString { template, expressions, position } => {
                Value::InterpolatedString {
                    template: template.clone(),
                    expressions: self.resolve_expressions(expressions),
                    position: *position,
                }
            }
            
            Value::Identifier { value: id_value, position } => {
                // ⭐ Handle dotted enum access in identifiers
                self.resolve_identifier_value(id_value, *position)
            }
            
            _ => value.clone(),
        }
    }
    
    /// ⭐ Resolve IdentifierValue that might contain qualified identifiers
    /// Converts "Status.COMPLETED" → EnumValue("Status", "COMPLETED")
    fn resolve_identifier_value(&self, id_value: &str, position: Position) -> Value {
        // Check if this contains a dot (qualified identifier pattern)
        if !id_value.contains('.') {
            // Simple identifier, return as-is
            return Value::Identifier {
                value: id_value.to_string(),
                position,
            };
        }
        
        // Parse the dotted identifier
        let parts: Vec<&str> = id_value.split('.').collect();
        
        if parts.len() == 2 {
            // Pattern: EnumName.VALUE (local enum)
            // Transform to EnumValue
            Value::EnumValue {
                enum_name: parts[0].to_string(),
                value: parts[1].to_string(),
                position,
            }
        } else {
            // Complex property chain or imported enum
            // Keep as IdentifierValue - will be handled at runtime
            Value::Identifier {
                value: id_value.to_string(),
                position,
            }
        }
    }
  }
