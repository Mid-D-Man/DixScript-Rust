//! Resolves QualifiedIdentifier nodes into concrete expression types
//! using resolution metadata produced by semantic analysis.

use crate::Compiler::AST::*;
use crate::Compiler::Core::SectionEnhancers::{
    QualifiedIdentifierKey, QualifiedIdentifierResolution, QualifiedIdentifierType,
};
use crate::ErrorManager::{DebugConfig, ErrorManager};
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use crate::Builtins::Resolver::builtin_call_resolver;
pub struct QualifiedIdentifierResolver {
    resolutions: FxHashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution>,
    error_manager: ErrorManager,
    debug_config: DebugConfig,
}

impl QualifiedIdentifierResolver {
    /// Create a resolver from the semantic analysis resolution map.
    /// The map is converted to FxHashMap once at construction for faster per-lookup hashing.
    pub fn new(
        resolutions: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution>,
        debug_config: DebugConfig,
    ) -> Self {
        Self::new_with_error_manager(resolutions,debug_config,ErrorManager::get_shared_instance())

    }
    pub fn new_with_error_manager(
        resolutions: HashMap<QualifiedIdentifierKey, QualifiedIdentifierResolution>,
        debug_config: DebugConfig,error_manager:ErrorManager
    ) -> Self {
        QualifiedIdentifierResolver {
            resolutions: resolutions.into_iter().collect(),
            error_manager,
            debug_config,
        }
    }
    // ==================== STATEMENT RESOLUTION ====================

    pub fn resolve_statement(&self, statement: &QuickFuncStatement) -> QuickFuncStatement {
        match statement {
            QuickFuncStatement::Return { value, position } => QuickFuncStatement::Return {
                value: self.resolve_expression(value),
                position: *position,
            },

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
            } => QuickFuncStatement::VariableDeclaration {
                declaration_type: *declaration_type,
                is_mutable: *is_mutable,
                variable_name: variable_name.clone(),
                data_type: *data_type,
                value: self.resolve_expression(value),
                position: *position,
            },

            QuickFuncStatement::If { condition, then_branch, else_branch, position } => {
                QuickFuncStatement::If {
                    condition: self.resolve_expression(condition),
                    then_branch: self.resolve_statements(then_branch),
                    else_branch: else_branch.as_ref().map(|b| self.resolve_statements(b)),
                    position: *position,
                }
            }

            QuickFuncStatement::Switch { expression, cases, default_case, position } => {
                QuickFuncStatement::Switch {
                    expression: self.resolve_expression(expression),
                    cases: cases.iter().map(|c| self.resolve_switch_case(c)).collect(),
                    default_case: default_case.as_ref().map(|c| self.resolve_switch_case(c)),
                    position: *position,
                }
            }

            QuickFuncStatement::Log { value, position } => QuickFuncStatement::Log {
                value: self.resolve_expression(value),
                position: *position,
            },

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

    pub fn resolve_statements(&self, statements: &[QuickFuncStatement]) -> Vec<QuickFuncStatement> {
        statements.iter().map(|s| self.resolve_statement(s)).collect()
    }

    fn resolve_switch_case(&self, case: &SwitchCase) -> SwitchCase {
        SwitchCase::new(
            self.resolve_value(&case.case_value),
            self.resolve_statements(&case.statements),
            case.position,
        )
    }

    // ==================== EXPRESSION RESOLUTION ====================

    pub fn resolve_expression(&self, expr: &Expression) -> Expression {
        match expr {
            Expression::QualifiedIdentifier { .. } => self.transform_qualified_identifier(expr),

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

            Expression::UnaryOp { operator, operand, position } => Expression::UnaryOp {
                operator: operator.clone(),
                operand: Box::new(self.resolve_expression(operand)),
                position: *position,
            },

            Expression::Conditional { condition, true_value, false_value, position } => {
                Expression::Conditional {
                    condition: Box::new(self.resolve_expression(condition)),
                    true_value: Box::new(self.resolve_expression(true_value)),
                    false_value: Box::new(self.resolve_expression(false_value)),
                    position: *position,
                }
            }

            Expression::Parenthesized { expression, position } => Expression::Parenthesized {
                expression: Box::new(self.resolve_expression(expression)),
                position: *position,
            },

            Expression::PropertyAccess { object, property, position } => {
                Expression::PropertyAccess {
                    object: Box::new(self.resolve_expression(object)),
                    property: property.clone(),
                    position: *position,
                }
            }

            Expression::IndexAccess { object, index, position } => Expression::IndexAccess {
                object: Box::new(self.resolve_expression(object)),
                index: Box::new(self.resolve_expression(index)),
                position: *position,
            },

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

            Expression::ImportedFunctionCall {
                namespace_name,
                function_name,
                arguments,
                position,
            } => Expression::ImportedFunctionCall {
                namespace_name: namespace_name.clone(),
                function_name: function_name.clone(),
                arguments: self.resolve_expressions(arguments),
                position: *position,
            },

            Expression::Value { value, position } => Expression::Value {
                value: self.resolve_value(value),
                position: *position,
            },

            _ => expr.clone(),
        }
    }

    fn resolve_expressions(&self, expressions: &[Expression]) -> Vec<Expression> {
        expressions.iter().map(|e| self.resolve_expression(e)).collect()
    }

    // ==================== QUALIFIEDIDENTIFIER TRANSFORMATION ====================

    fn transform_qualified_identifier(&self, expr: &Expression) -> Expression {
        let (parts, arguments, position) =
            if let Expression::QualifiedIdentifier { parts, arguments, position } = expr {
                (parts, arguments, position)
            } else {
                return expr.clone();
            };

        let key = QualifiedIdentifierKey {
            position: *position,
            parts: parts.clone(),
            is_call: arguments.is_some(),
        };

        if let Some(resolution) = self.resolutions.get(&key) {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug(&format!(
                    "[QualIdResolver] Resolved {}: {}",
                    parts.join("."),
                    resolution.resolved_type
                ));
            }
            return self.apply_resolution(parts, arguments.as_ref(), *position, resolution);
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "[QualIdResolver] No resolution found for {}",
                parts.join(".")
            ));
        }

        // Fallback: no resolution was recorded by the semantic analyser (common for
        // qualified identifiers inside lambda bodies, which the analyser does not visit).
        // Use structural heuristics to generate the correct expression node.
        if let Some(args) = arguments {
            if parts.len() == 2 {
                if builtin_call_resolver::has_static_object(&parts[0]) {
                    // e.g.  Array.range(1, n)  or  Math.max(a, b)  inside a lambda
                    return Expression::StaticMethodCall {
                        object_name: parts[0].clone(),
                        method_name: parts[1].clone(),
                        arguments:   self.resolve_expressions(args),
                        position:    *position,
                    };
                }
                // e.g.  s.trim()  or  arr.reverse()  where the first part is a
                // variable / lambda parameter — generate an InstanceMethodCall.
                // build_instance_method_call calls resolve_expressions internally.
                return self.build_instance_method_call(parts, args, *position);
            }

            // Multi-part chain or single identifier with args — best-effort QuickFuncCall.
            Expression::QuickFuncCall {
                name:      parts.join("."),
                arguments: self.resolve_expressions(args),
                position:  *position,
            }
        } else {
            self.build_property_access_chain(parts, *position)
        }
    }
    fn apply_resolution(
        &self,
        parts: &[String],
        arguments: Option<&Vec<Expression>>,
        position: Position,
        resolution: &QualifiedIdentifierResolution,
    ) -> Expression {
        match resolution.resolved_type {
            QualifiedIdentifierType::LocalEnumAccess => Expression::EnumAccess {
                namespace_name: None,
                enum_name: parts[0].clone(),
                value: parts[1].clone(),
                position,
            },

            QualifiedIdentifierType::ImportedEnumAccess => Expression::EnumAccess {
                namespace_name: Some(parts[0].clone()),
                enum_name: parts[1].clone(),
                value: parts[2].clone(),
                position,
            },

            QualifiedIdentifierType::ImportedFunctionCall => Expression::ImportedFunctionCall {
                namespace_name: parts[0].clone(),
                function_name: parts[1].clone(),
                arguments: arguments
                    .map(|a| self.resolve_expressions(a))
                    .unwrap_or_default(),
                position,
            },

            QualifiedIdentifierType::StaticObjectAccess => {
                if arguments.is_some() {
                    Expression::StaticMethodCall {
                        object_name: parts[0].clone(),
                        method_name: parts[1].clone(),
                        arguments: arguments
                            .map(|a| self.resolve_expressions(a))
                            .unwrap_or_default(),
                        position,
                    }
                } else {
                    self.build_property_access_chain(parts, position)
                }
            }

            QualifiedIdentifierType::ObjectPropertyAccess => {
                if arguments.is_some() {
                    self.build_instance_method_call(parts, arguments.unwrap(), position)
                } else {
                    self.build_property_access_chain(parts, position)
                }
            }

            QualifiedIdentifierType::NamespaceEnumReference => Expression::ObjectAccess {
                path: parts.to_vec(),
                position,
            },

            _ => self.build_property_access_chain(parts, position),
        }
    }

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

    fn build_instance_method_call(
        &self,
        parts: &[String],
        arguments: &[Expression],
        position: Position,
    ) -> Expression {
        if parts.len() < 2 {
            return self.build_property_access_chain(parts, position);
        }

        let instance = if parts.len() == 2 {
            Expression::Identifier { name: parts[0].clone(), position }
        } else {
            self.build_property_access_chain(&parts[..parts.len() - 1], position)
        };

        Expression::InstanceMethodCall {
            instance: Box::new(instance),
            method_name: parts[parts.len() - 1].clone(),
            arguments: self.resolve_expressions(arguments),
            position,
        }
    }

    // ==================== VALUE RESOLUTION ====================
fn resolve_value(&self, value: &Value) -> Value {
        match value {
            Value::Array { values, position } => Value::Array {
                values: values.iter().map(|v| self.resolve_value(v)).collect(),
                position: *position,
            },

            Value::Object { properties, position } => Value::Object {
                properties: properties
                    .iter()
                    .map(|p| ObjectProperty::new(
                        p.key.clone(),
                        self.resolve_value(&p.value),
                        p.position,
                    ))
                    .collect(),
                position: *position,
            },

            Value::PrefixedConstructor { prefix, arguments, position } => {
                Value::PrefixedConstructor {
                    prefix: prefix.clone(),
                    arguments: arguments.iter().map(|v| self.resolve_value(v)).collect(),
                    position: *position,
                }
            }

            Value::Expression { expr, position } => Value::Expression {
                expr: Box::new(self.resolve_expression(expr)),
                position: *position,
            },

            Value::InterpolatedString { template, expressions, position } => {
                Value::InterpolatedString {
                    template: template.clone(),
                    expressions: self.resolve_expressions(expressions),
                    position: *position,
                }
            }

            Value::Identifier { value: id_value, position } => {
                self.resolve_identifier_value(id_value, *position)
            }

            // ── CRITICAL FIX: resolve QualifiedIdentifiers inside lambda bodies ──
            Value::Lambda { parameters, body, statements, position } => Value::Lambda {
                parameters: parameters.clone(),
                body: Box::new(self.resolve_expression(body)),
                statements: statements.iter().map(|s| self.resolve_statement(s)).collect(),
                position: *position,
            },

            _ => value.clone(),
        }
    }

    /// Converts dotted identifier values like "Status.COMPLETED" to EnumValue.
    fn resolve_identifier_value(&self, id_value: &str, position: Position) -> Value {
        if !id_value.contains('.') {
            return Value::Identifier { value: id_value.to_string(), position };
        }

        let mut parts = id_value.splitn(3, '.');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(enum_name), Some(value), None) => Value::EnumValue {
                enum_name: enum_name.to_string(),
                value: value.to_string(),
                position,
            },
            _ => Value::Identifier { value: id_value.to_string(), position },
        }
    }
}
