// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/SectionEnhancers/qualified_identifier_resolver.rs"
// ============================================================================
//! Resolves QualifiedIdentifier nodes into concrete expression types
//! using resolution metadata produced by semantic analysis.
//!
//! There is one implementation, and it works on the arena: [`resolve_statement_list`],
//! [`resolve_statement_id`] and [`resolve_expression_id`] read the nodes of an
//! [`Arena`] and allocate the nodes that replace the ones that change. The owned entry points
//! ([`resolve_statement`], [`resolve_statements`], [`resolve_expression`]) copy their argument into
//! a scratch arena, call the arena implementation and copy the result back out.
//!
//! [`resolve_statement_list`]: QualifiedIdentifierResolver::resolve_statement_list
//! [`resolve_statement_id`]: QualifiedIdentifierResolver::resolve_statement_id
//! [`resolve_expression_id`]: QualifiedIdentifierResolver::resolve_expression_id
//! [`resolve_statement`]: QualifiedIdentifierResolver::resolve_statement
//! [`resolve_statements`]: QualifiedIdentifierResolver::resolve_statements
//! [`resolve_expression`]: QualifiedIdentifierResolver::resolve_expression

use crate::Compiler::AST::arena::{
    Arena, ExprId, ExprList, ExprNode, ObjectPropNode, StmtId, StmtList, StmtNode, StrList,
    SwitchCaseNode, ValueId, ValueList, ValueNode,
};
use crate::Compiler::AST::*;
use crate::Compiler::Core::SectionEnhancers::{
    QualifiedIdentifierKey, QualifiedIdentifierResolution, QualifiedIdentifierType,
};
use crate::ErrorManager::{DebugConfig, ErrorManager};
use crate::Utilities::RustcHash::FxHashMap;
use std::collections::HashMap;
use crate::Builtins::Resolver::builtin_call_resolver;

/// Replaces the qualified identifiers of a function body with the nodes semantic analysis found
/// them to be.
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
    /// Like [`new`](Self::new), with the error manager the resolver logs to.
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

    // ==================== OWNED ENTRY POINTS ====================

    /// Resolves one owned statement. Copies it into a scratch arena, resolves it there and copies
    /// the result back out.
    pub fn resolve_statement(&self, statement: &QuickFuncStatement) -> QuickFuncStatement {
        let mut arena = Arena::new();
        let id = arena.import_stmt(statement);
        let resolved = self.resolve_statement_id(&mut arena, id);
        arena.export_stmt(resolved)
    }

    /// Resolves a run of owned statements through one scratch arena.
    pub fn resolve_statements(&self, statements: &[QuickFuncStatement]) -> Vec<QuickFuncStatement> {
        let mut arena = Arena::new();
        let ids: Vec<StmtId> = statements.iter().map(|s| arena.import_stmt(s)).collect();
        let resolved: Vec<StmtId> = ids.into_iter().map(|id| self.resolve_statement_id(&mut arena, id)).collect();
        resolved.into_iter().map(|id| arena.export_stmt(id)).collect()
    }

    /// Resolves one owned expression through a scratch arena.
    pub fn resolve_expression(&self, expr: &Expression) -> Expression {
        let mut arena = Arena::new();
        let id = arena.import_expr(expr);
        let resolved = self.resolve_expression_id(&mut arena, id);
        arena.export_expr(resolved)
    }

    // ==================== STATEMENT RESOLUTION ====================

    /// Resolves a run of statements of `arena`.
    ///
    /// Returns `list` itself when nothing in it changed, otherwise a new run. The nodes that did
    /// not change are shared with the original, not copied.
    pub fn resolve_statement_list(&self, arena: &mut Arena, list: StmtList) -> StmtList {
        let ids: Vec<StmtId> = arena.stmt_list(list).to_vec();
        let resolved: Vec<StmtId> = ids.iter().map(|&id| self.resolve_statement_id(arena, id)).collect();
        if resolved == ids {
            list
        } else {
            arena.alloc_stmt_list(&resolved)
        }
    }

    /// Resolves one statement of `arena`, returning `id` itself when nothing in it changed.
    pub fn resolve_statement_id(&self, arena: &mut Arena, id: StmtId) -> StmtId {
        let node = *arena.stmt(id);
        let resolved = match node {
            StmtNode::Return { value, position } => {
                let value = self.resolve_expression_id(arena, value);
                StmtNode::Return { value, position }
            }

            StmtNode::Assignment { variable, value, position } => {
                let value = self.resolve_expression_id(arena, value);
                StmtNode::Assignment { variable, value, position }
            }

            StmtNode::ArithmeticAssignment { variable, operator, value, position } => {
                let value = self.resolve_expression_id(arena, value);
                StmtNode::ArithmeticAssignment { variable, operator, value, position }
            }

            StmtNode::VariableDeclaration {
                declaration_type,
                is_mutable,
                variable_name,
                data_type,
                value,
                position,
            } => {
                let value = self.resolve_expression_id(arena, value);
                StmtNode::VariableDeclaration {
                    declaration_type,
                    is_mutable,
                    variable_name,
                    data_type,
                    value,
                    position,
                }
            }

            StmtNode::If { condition, then_branch, else_branch, position } => {
                let condition = self.resolve_expression_id(arena, condition);
                let then_branch = self.resolve_statement_list(arena, then_branch);
                let else_branch = else_branch.map(|b| self.resolve_statement_list(arena, b));
                StmtNode::If { condition, then_branch, else_branch, position }
            }

            StmtNode::Switch { expression, cases, default_case, position } => {
                let expression = self.resolve_expression_id(arena, expression);
                let old_cases: Vec<SwitchCaseNode> = arena.case_list(cases).to_vec();
                let new_cases: Vec<SwitchCaseNode> =
                    old_cases.iter().map(|c| self.resolve_switch_case(arena, *c)).collect();
                let cases = if new_cases_equal(&old_cases, &new_cases) {
                    cases
                } else {
                    arena.alloc_case_list(&new_cases)
                };
                let default_case = default_case.map(|case_id| {
                    let old = *arena.case(case_id);
                    let new = self.resolve_switch_case(arena, old);
                    if new_cases_equal(&[old], &[new]) { case_id } else { arena.alloc_case(new) }
                });
                StmtNode::Switch { expression, cases, default_case, position }
            }

            StmtNode::Log { value, position } => {
                let value = self.resolve_expression_id(arena, value);
                StmtNode::Log { value, position }
            }

            StmtNode::ExpressionStatement { expression, position } => {
                let expression = self.resolve_expression_id(arena, expression);
                StmtNode::ExpressionStatement { expression, position }
            }

            StmtNode::ObjectCreation { variable, object, position } => {
                let object = self.resolve_value_id(arena, object);
                StmtNode::ObjectCreation { variable, object, position }
            }
        };
        if same_stmt(&resolved, &node) {
            id
        } else {
            arena.alloc_stmt(resolved)
        }
    }

    fn resolve_switch_case(&self, arena: &mut Arena, case: SwitchCaseNode) -> SwitchCaseNode {
        SwitchCaseNode {
            case_value: self.resolve_value_id(arena, case.case_value),
            statements: self.resolve_statement_list(arena, case.statements),
            position: case.position,
        }
    }

    // ==================== EXPRESSION RESOLUTION ====================

    /// Resolves one expression of `arena`, returning `id` itself when nothing in it changed.
    ///
    /// A qualified identifier always changes: it is replaced by the node its resolution names, or
    /// by the structural fallback when semantic analysis recorded none.
    pub fn resolve_expression_id(&self, arena: &mut Arena, id: ExprId) -> ExprId {
        let node = *arena.expr(id);
        let resolved = match node {
            ExprNode::QualifiedIdentifier { parts, arguments, position } => {
                return self.transform_qualified_identifier(arena, parts, arguments, position);
            }

            ExprNode::ArithmeticOp { left, operator, right, position } => {
                let left = self.resolve_expression_id(arena, left);
                let right = self.resolve_expression_id(arena, right);
                ExprNode::ArithmeticOp { left, operator, right, position }
            }

            ExprNode::BitwiseOp { left, operator, right, position } => {
                let left = self.resolve_expression_id(arena, left);
                let right = self.resolve_expression_id(arena, right);
                ExprNode::BitwiseOp { left, operator, right, position }
            }

            ExprNode::ComparisonOp { left, operator, right, position } => {
                let left = self.resolve_expression_id(arena, left);
                let right = self.resolve_expression_id(arena, right);
                ExprNode::ComparisonOp { left, operator, right, position }
            }

            ExprNode::LogicalOp { left, operator, right, position } => {
                let left = self.resolve_expression_id(arena, left);
                let right = self.resolve_expression_id(arena, right);
                ExprNode::LogicalOp { left, operator, right, position }
            }

            ExprNode::UnaryOp { operator, operand, position } => {
                let operand = self.resolve_expression_id(arena, operand);
                ExprNode::UnaryOp { operator, operand, position }
            }

            ExprNode::Conditional { condition, true_value, false_value, position } => {
                let condition = self.resolve_expression_id(arena, condition);
                let true_value = self.resolve_expression_id(arena, true_value);
                let false_value = self.resolve_expression_id(arena, false_value);
                ExprNode::Conditional { condition, true_value, false_value, position }
            }

            ExprNode::Parenthesized { expression, position } => {
                let expression = self.resolve_expression_id(arena, expression);
                ExprNode::Parenthesized { expression, position }
            }

            ExprNode::PropertyAccess { object, property, position } => {
                let object = self.resolve_expression_id(arena, object);
                ExprNode::PropertyAccess { object, property, position }
            }

            ExprNode::IndexAccess { object, index, position } => {
                let object = self.resolve_expression_id(arena, object);
                let index = self.resolve_expression_id(arena, index);
                ExprNode::IndexAccess { object, index, position }
            }

            ExprNode::InstanceMethodCall { instance, method_name, arguments, position } => {
                let instance = self.resolve_expression_id(arena, instance);
                let arguments = self.resolve_expression_list(arena, arguments);
                ExprNode::InstanceMethodCall { instance, method_name, arguments, position }
            }

            ExprNode::StaticMethodCall { object_name, method_name, arguments, position } => {
                let arguments = self.resolve_expression_list(arena, arguments);
                ExprNode::StaticMethodCall { object_name, method_name, arguments, position }
            }

            ExprNode::QuickFuncCall { name, arguments, position } => {
                let arguments = self.resolve_expression_list(arena, arguments);
                ExprNode::QuickFuncCall { name, arguments, position }
            }

            ExprNode::ImportedFunctionCall { namespace_name, function_name, arguments, position } => {
                let arguments = self.resolve_expression_list(arena, arguments);
                ExprNode::ImportedFunctionCall { namespace_name, function_name, arguments, position }
            }

            ExprNode::Value { value, position } => {
                let value = self.resolve_value_id(arena, value);
                ExprNode::Value { value, position }
            }

            // Every other form is left as it is, including the ones that hold expressions
            // (casts, built-in and static function calls, plain function calls).
            _ => return id,
        };
        if same_expr(&resolved, &node) {
            id
        } else {
            arena.alloc_expr(resolved)
        }
    }

    fn resolve_expression_list(&self, arena: &mut Arena, list: ExprList) -> ExprList {
        let ids: Vec<ExprId> = arena.expr_list(list).to_vec();
        let resolved: Vec<ExprId> = ids.iter().map(|&id| self.resolve_expression_id(arena, id)).collect();
        if resolved == ids {
            list
        } else {
            arena.alloc_expr_list(&resolved)
        }
    }

    // ==================== QUALIFIEDIDENTIFIER TRANSFORMATION ====================

    fn transform_qualified_identifier(
        &self,
        arena: &mut Arena,
        parts_list: StrList,
        arguments: Option<ExprList>,
        position: Position,
    ) -> ExprId {
        let parts: Vec<String> =
            arena.str_list(parts_list).iter().map(|s| arena.text(*s).to_string()).collect();

        let key = QualifiedIdentifierKey {
            position,
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
            return self.apply_resolution(arena, &parts, arguments, position, resolution);
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
                    let object_name = arena.alloc_str(&parts[0]);
                    let method_name = arena.alloc_str(&parts[1]);
                    let arguments = self.resolve_expression_list(arena, args);
                    return arena.alloc_expr(ExprNode::StaticMethodCall {
                        object_name,
                        method_name,
                        arguments,
                        position,
                    });
                }
                // e.g.  s.trim()  or  arr.reverse()  where the first part is a
                // variable / lambda parameter — generate an InstanceMethodCall.
                // build_instance_method_call calls resolve_expression_list internally.
                return self.build_instance_method_call(arena, &parts, args, position);
            }

            // Multi-part chain or single identifier with args — best-effort QuickFuncCall.
            let name = arena.alloc_str(&parts.join("."));
            let arguments = self.resolve_expression_list(arena, args);
            arena.alloc_expr(ExprNode::QuickFuncCall { name, arguments, position })
        } else {
            self.build_property_access_chain(arena, &parts, position)
        }
    }

    fn apply_resolution(
        &self,
        arena: &mut Arena,
        parts: &[String],
        arguments: Option<ExprList>,
        position: Position,
        resolution: &QualifiedIdentifierResolution,
    ) -> ExprId {
        match resolution.resolved_type {
            QualifiedIdentifierType::LocalEnumAccess => {
                let enum_name = arena.alloc_str(&parts[0]);
                let value = arena.alloc_str(&parts[1]);
                arena.alloc_expr(ExprNode::EnumAccess { namespace_name: None, enum_name, value, position })
            }

            QualifiedIdentifierType::ImportedEnumAccess => {
                let namespace_name = Some(arena.alloc_str(&parts[0]));
                let enum_name = arena.alloc_str(&parts[1]);
                let value = arena.alloc_str(&parts[2]);
                arena.alloc_expr(ExprNode::EnumAccess { namespace_name, enum_name, value, position })
            }

            QualifiedIdentifierType::ImportedFunctionCall => {
                let namespace_name = arena.alloc_str(&parts[0]);
                let function_name = arena.alloc_str(&parts[1]);
                let arguments = match arguments {
                    Some(a) => self.resolve_expression_list(arena, a),
                    None => arena.alloc_expr_list(&[]),
                };
                arena.alloc_expr(ExprNode::ImportedFunctionCall {
                    namespace_name,
                    function_name,
                    arguments,
                    position,
                })
            }

            QualifiedIdentifierType::StaticObjectAccess => {
                if let Some(args) = arguments {
                    let object_name = arena.alloc_str(&parts[0]);
                    let method_name = arena.alloc_str(&parts[1]);
                    let arguments = self.resolve_expression_list(arena, args);
                    arena.alloc_expr(ExprNode::StaticMethodCall {
                        object_name,
                        method_name,
                        arguments,
                        position,
                    })
                } else {
                    self.build_property_access_chain(arena, parts, position)
                }
            }

            QualifiedIdentifierType::ObjectPropertyAccess => {
                if let Some(args) = arguments {
                    self.build_instance_method_call(arena, parts, args, position)
                } else {
                    self.build_property_access_chain(arena, parts, position)
                }
            }

            QualifiedIdentifierType::NamespaceEnumReference => {
                let path = arena.alloc_str_list(parts);
                arena.alloc_expr(ExprNode::ObjectAccess { path, position })
            }

            _ => self.build_property_access_chain(arena, parts, position),
        }
    }

    fn build_property_access_chain(&self, arena: &mut Arena, parts: &[String], position: Position) -> ExprId {
        let name = arena.alloc_str(&parts[0]);
        let mut current = arena.alloc_expr(ExprNode::Identifier { name, position });
        for part in &parts[1..] {
            let property = arena.alloc_str(part);
            current = arena.alloc_expr(ExprNode::PropertyAccess { object: current, property, position });
        }
        current
    }

    /// Builds `parts[..n-1]` as the instance and `parts[n-1]` as the method. `arguments` are the
    /// ones of the qualified identifier, not yet resolved.
    fn build_instance_method_call(
        &self,
        arena: &mut Arena,
        parts: &[String],
        arguments: ExprList,
        position: Position,
    ) -> ExprId {
        if parts.len() < 2 {
            return self.build_property_access_chain(arena, parts, position);
        }

        let instance = if parts.len() == 2 {
            let name = arena.alloc_str(&parts[0]);
            arena.alloc_expr(ExprNode::Identifier { name, position })
        } else {
            self.build_property_access_chain(arena, &parts[..parts.len() - 1], position)
        };

        let method_name = arena.alloc_str(&parts[parts.len() - 1]);
        let arguments = self.resolve_expression_list(arena, arguments);
        arena.alloc_expr(ExprNode::InstanceMethodCall { instance, method_name, arguments, position })
    }

    // ==================== VALUE RESOLUTION ====================

    fn resolve_value_id(&self, arena: &mut Arena, id: ValueId) -> ValueId {
        let node = *arena.value(id);
        let resolved = match node {
            ValueNode::Array { values, position } => {
                let values = self.resolve_value_list(arena, values);
                ValueNode::Array { values, position }
            }

            ValueNode::Object { properties, position } => {
                let old: Vec<ObjectPropNode> = arena.prop_list(properties).to_vec();
                let new: Vec<ObjectPropNode> = old
                    .iter()
                    .map(|p| ObjectPropNode {
                        key: p.key,
                        value: self.resolve_value_id(arena, p.value),
                        position: p.position,
                    })
                    .collect();
                let unchanged = old.iter().zip(&new).all(|(a, b)| a.value == b.value);
                let properties = if unchanged { properties } else { arena.alloc_prop_list(&new) };
                ValueNode::Object { properties, position }
            }

            ValueNode::PrefixedConstructor { prefix, arguments, position } => {
                let arguments = self.resolve_value_list(arena, arguments);
                ValueNode::PrefixedConstructor { prefix, arguments, position }
            }

            ValueNode::Expression { expr, position } => {
                let expr = self.resolve_expression_id(arena, expr);
                ValueNode::Expression { expr, position }
            }

            ValueNode::InterpolatedString { template, expressions, position } => {
                let expressions = self.resolve_expression_list(arena, expressions);
                ValueNode::InterpolatedString { template, expressions, position }
            }

            ValueNode::Identifier { value, position } => {
                let text = arena.text(value).to_string();
                return self.resolve_identifier_value(arena, id, &text, position);
            }

            // ── CRITICAL FIX: resolve QualifiedIdentifiers inside lambda bodies ──
            ValueNode::Lambda { parameters, body, statements, position } => {
                let body = self.resolve_expression_id(arena, body);
                let statements = self.resolve_statement_list(arena, statements);
                ValueNode::Lambda { parameters, body, statements, position }
            }

            _ => return id,
        };
        if same_value(&resolved, &node) {
            id
        } else {
            arena.alloc_value(resolved)
        }
    }

    fn resolve_value_list(&self, arena: &mut Arena, list: ValueList) -> ValueList {
        let ids: Vec<ValueId> = arena.value_list(list).to_vec();
        let resolved: Vec<ValueId> = ids.iter().map(|&id| self.resolve_value_id(arena, id)).collect();
        if resolved == ids {
            list
        } else {
            arena.alloc_value_list(&resolved)
        }
    }

    /// Converts dotted identifier values like "Status.COMPLETED" to EnumValue.
    fn resolve_identifier_value(&self, arena: &mut Arena, id: ValueId, id_value: &str, position: Position) -> ValueId {
        if !id_value.contains('.') {
            return id;
        }

        let mut parts = id_value.splitn(3, '.');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(enum_name), Some(value), None) => {
                let enum_name = arena.alloc_str(enum_name);
                let value = arena.alloc_str(value);
                arena.alloc_value(ValueNode::EnumValue { enum_name, value, position })
            }
            _ => id,
        }
    }
}

// ==================== "DID THIS NODE CHANGE" ====================
//
// A node is rebuilt from its resolved children. These compare the rebuilt node with the one it
// came from, by the ids and lists inside, so that a node none of whose children changed is kept
// as it is instead of being allocated again.

fn same_stmt(a: &StmtNode, b: &StmtNode) -> bool {
    match (a, b) {
        (StmtNode::Return { value: x, .. }, StmtNode::Return { value: y, .. }) => x == y,
        (StmtNode::Assignment { value: x, .. }, StmtNode::Assignment { value: y, .. }) => x == y,
        (StmtNode::ArithmeticAssignment { value: x, .. }, StmtNode::ArithmeticAssignment { value: y, .. }) => x == y,
        (StmtNode::VariableDeclaration { value: x, .. }, StmtNode::VariableDeclaration { value: y, .. }) => x == y,
        (
            StmtNode::If { condition: c1, then_branch: t1, else_branch: e1, .. },
            StmtNode::If { condition: c2, then_branch: t2, else_branch: e2, .. },
        ) => c1 == c2 && t1 == t2 && e1 == e2,
        (
            StmtNode::Switch { expression: x1, cases: c1, default_case: d1, .. },
            StmtNode::Switch { expression: x2, cases: c2, default_case: d2, .. },
        ) => x1 == x2 && c1 == c2 && d1 == d2,
        (StmtNode::Log { value: x, .. }, StmtNode::Log { value: y, .. }) => x == y,
        (StmtNode::ExpressionStatement { expression: x, .. }, StmtNode::ExpressionStatement { expression: y, .. }) => x == y,
        (StmtNode::ObjectCreation { object: x, .. }, StmtNode::ObjectCreation { object: y, .. }) => x == y,
        _ => false,
    }
}

fn same_expr(a: &ExprNode, b: &ExprNode) -> bool {
    match (a, b) {
        (
            ExprNode::ArithmeticOp { left: l1, right: r1, .. },
            ExprNode::ArithmeticOp { left: l2, right: r2, .. },
        )
        | (
            ExprNode::BitwiseOp { left: l1, right: r1, .. },
            ExprNode::BitwiseOp { left: l2, right: r2, .. },
        )
        | (
            ExprNode::ComparisonOp { left: l1, right: r1, .. },
            ExprNode::ComparisonOp { left: l2, right: r2, .. },
        )
        | (
            ExprNode::LogicalOp { left: l1, right: r1, .. },
            ExprNode::LogicalOp { left: l2, right: r2, .. },
        ) => l1 == l2 && r1 == r2,
        (ExprNode::UnaryOp { operand: x, .. }, ExprNode::UnaryOp { operand: y, .. }) => x == y,
        (
            ExprNode::Conditional { condition: c1, true_value: t1, false_value: f1, .. },
            ExprNode::Conditional { condition: c2, true_value: t2, false_value: f2, .. },
        ) => c1 == c2 && t1 == t2 && f1 == f2,
        (ExprNode::Parenthesized { expression: x, .. }, ExprNode::Parenthesized { expression: y, .. }) => x == y,
        (ExprNode::PropertyAccess { object: x, .. }, ExprNode::PropertyAccess { object: y, .. }) => x == y,
        (
            ExprNode::IndexAccess { object: o1, index: i1, .. },
            ExprNode::IndexAccess { object: o2, index: i2, .. },
        ) => o1 == o2 && i1 == i2,
        (
            ExprNode::InstanceMethodCall { instance: i1, arguments: a1, .. },
            ExprNode::InstanceMethodCall { instance: i2, arguments: a2, .. },
        ) => i1 == i2 && a1 == a2,
        (ExprNode::StaticMethodCall { arguments: x, .. }, ExprNode::StaticMethodCall { arguments: y, .. })
        | (ExprNode::QuickFuncCall { arguments: x, .. }, ExprNode::QuickFuncCall { arguments: y, .. })
        | (ExprNode::ImportedFunctionCall { arguments: x, .. }, ExprNode::ImportedFunctionCall { arguments: y, .. }) => x == y,
        (ExprNode::Value { value: x, .. }, ExprNode::Value { value: y, .. }) => x == y,
        _ => false,
    }
}

fn same_value(a: &ValueNode, b: &ValueNode) -> bool {
    match (a, b) {
        (ValueNode::Array { values: x, .. }, ValueNode::Array { values: y, .. }) => x == y,
        (ValueNode::Object { properties: x, .. }, ValueNode::Object { properties: y, .. }) => x == y,
        (ValueNode::PrefixedConstructor { arguments: x, .. }, ValueNode::PrefixedConstructor { arguments: y, .. }) => x == y,
        (ValueNode::Expression { expr: x, .. }, ValueNode::Expression { expr: y, .. }) => x == y,
        (ValueNode::InterpolatedString { expressions: x, .. }, ValueNode::InterpolatedString { expressions: y, .. }) => x == y,
        (
            ValueNode::Lambda { body: b1, statements: s1, .. },
            ValueNode::Lambda { body: b2, statements: s2, .. },
        ) => b1 == b2 && s1 == s2,
        _ => false,
    }
}

fn new_cases_equal(old: &[SwitchCaseNode], new: &[SwitchCaseNode]) -> bool {
    old.len() == new.len()
        && old.iter().zip(new).all(|(a, b)| a.case_value == b.case_value && a.statements == b.statements)
}
