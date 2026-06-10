// dixscript/src/Compiler/AST/Visitors/ast_visitor_base.rs

use crate::Compiler::AST::*;

/// Base AST Visitor with complete, exhaustive default traversal logic.
///
/// ## Usage
/// Implement the trait, set `type Result`, and override only the methods you
/// care about.  The default implementations recurse into every child node, so
/// an override that does not call `default_result()` / recurse manually will
/// silently prune that subtree — intentional for leaf handlers, a bug for
/// interior nodes.
///
/// ## Completeness guarantee
/// Every variant of `Expression` (24), `Value` (25), `QuickFuncStatement` (9),
/// and all seven section types is covered in a named match arm.
/// There are NO `_ => default_result()` escape hatches; the compiler will
/// error when new variants are added to these enums without updating this file.
pub trait AstVisitorBase {
    type Result: Default;

    #[inline]
    fn default_result(&self) -> Self::Result {
        Self::Result::default()
    }

    // ── Entry point ───────────────────────────────────────────────────────────

    fn visit(&mut self, ast: &DixScript) -> Self::Result {
        if let Some(ref c) = ast.config          { self.visit_config_section(c); }
        if let Some(ref i) = ast.imports         { self.visit_imports_section(i); }
        if let Some(ref d) = ast.dlm             { self.visit_dlm_section(d); }
        if let Some(ref e) = ast.enums           { self.visit_enums_section(e); }
        if let Some(ref q) = ast.quick_functions { self.visit_quickfuncs_section(q); }
        if let Some(ref d) = ast.data            { self.visit_data_section(d); }
        if let Some(ref s) = ast.security        { self.visit_security_section(s); }
        self.default_result()
    }

    // ── @CONFIG ───────────────────────────────────────────────────────────────

    fn visit_config_section(&mut self, section: &ConfigSection) -> Self::Result {
        for entry in &section.entries { self.visit_config_entry(entry); }
        self.default_result()
    }

    fn visit_config_entry(&mut self, entry: &ConfigEntry) -> Self::Result {
        self.visit_config_value(&entry.value);
        self.default_result()
    }

    fn visit_config_value(&mut self, value: &ConfigValue) -> Self::Result {
        match value {
            ConfigValue::String(_)        => self.visit_config_string_value(value),
            ConfigValue::Integer(_)       => self.visit_config_integer_value(value),
            ConfigValue::Float(_)         => self.visit_config_float_value(value),
            ConfigValue::Boolean(_)       => self.visit_config_boolean_value(value),
            ConfigValue::Date(_)          => self.visit_config_date_value(value),
            ConfigValue::Timestamp(_)     => self.visit_config_timestamp_value(value),
            ConfigValue::Features(_)      => self.visit_config_feature_value(value),
            ConfigValue::Debug(_)         => self.visit_config_debug_value(value),
            ConfigValue::ErrorHandling(_) => self.visit_config_error_handling_value(value),
            ConfigValue::Compatibility(_) => self.visit_config_compatibility_value(value),
        }
    }

    fn visit_config_string_value       (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }
    fn visit_config_integer_value      (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }
    fn visit_config_float_value        (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }
    fn visit_config_boolean_value      (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }
    fn visit_config_date_value         (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }
    fn visit_config_timestamp_value    (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }
    fn visit_config_feature_value      (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }
    fn visit_config_debug_value        (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }
    fn visit_config_error_handling_value (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }
    fn visit_config_compatibility_value  (&mut self, _v: &ConfigValue) -> Self::Result { self.default_result() }

    // ── @IMPORTS ──────────────────────────────────────────────────────────────

    fn visit_imports_section(&mut self, section: &ImportsSection) -> Self::Result {
        for import in &section.imports { self.visit_import_declaration(import); }
        self.default_result()
    }

    fn visit_import_declaration(&mut self, _import: &ImportDeclaration) -> Self::Result {
        self.default_result()
    }

    // ── @DLM ─────────────────────────────────────────────────────────────────

    fn visit_dlm_section(&mut self, section: &DLMSection) -> Self::Result {
        for module in &section.modules { self.visit_dlm_module(module); }
        self.default_result()
    }

    fn visit_dlm_module(&mut self, _module: &DLMModule) -> Self::Result { self.default_result() }

    // ── @ENUMS ────────────────────────────────────────────────────────────────

    fn visit_enums_section(&mut self, section: &EnumsSection) -> Self::Result {
        for enum_decl in &section.enums { self.visit_enum_declaration(enum_decl); }
        self.default_result()
    }

    fn visit_enum_declaration(&mut self, enum_decl: &EnumDeclaration) -> Self::Result {
        for field in &enum_decl.fields { self.visit_enum_field(field); }
        self.default_result()
    }

    fn visit_enum_field(&mut self, _field: &EnumField) -> Self::Result { self.default_result() }

    // ── @QUICKFUNCS ───────────────────────────────────────────────────────────

    fn visit_quickfuncs_section(&mut self, section: &QuickFuncsSection) -> Self::Result {
        for func in &section.functions { self.visit_quick_function(func); }
        self.default_result()
    }

    fn visit_quick_function(&mut self, func: &QuickFunction) -> Self::Result {
        for param in &func.parameters { self.visit_quick_func_param(param); }
        for stmt  in &func.body       { self.visit_quick_func_statement(stmt); }
        self.default_result()
    }

    fn visit_quick_func_param(&mut self, param: &QuickFuncParam) -> Self::Result {
        if let Some(ref default_value) = param.default_value {
            self.visit_expression(default_value);
        }
        self.default_result()
    }

    // All nine QuickFuncStatement variants — exhaustive, no `_` arm.
    fn visit_quick_func_statement(&mut self, statement: &QuickFuncStatement) -> Self::Result {
        match statement {
            QuickFuncStatement::Return { value, .. } => {
                self.visit_return_statement(value)
            }
            QuickFuncStatement::If { condition, then_branch, else_branch, .. } => {
                self.visit_if_statement(condition, then_branch, else_branch.as_ref())
            }
            QuickFuncStatement::Switch { expression, cases, default_case, .. } => {
                self.visit_switch_statement(expression, cases, default_case.as_ref())
            }
            QuickFuncStatement::Assignment { variable, value, .. } => {
                self.visit_assignment_statement(variable, value)
            }
            QuickFuncStatement::ArithmeticAssignment { variable, operator, value, .. } => {
                self.visit_arithmetic_assignment_statement(variable, operator, value)
            }
            QuickFuncStatement::ObjectCreation { variable, object, .. } => {
                self.visit_object_creation_statement(variable, object)
            }
            QuickFuncStatement::Log { value, .. } => {
                self.visit_log_statement(value)
            }
            QuickFuncStatement::VariableDeclaration { .. } => {
                self.visit_variable_declaration_statement(statement)
            }
            QuickFuncStatement::ExpressionStatement { expression, .. } => {
                self.visit_expression_statement(expression)
            }
        }
    }

    fn visit_variable_declaration_statement(&mut self, statement: &QuickFuncStatement) -> Self::Result {
        if let QuickFuncStatement::VariableDeclaration { value, .. } = statement {
            self.visit_expression(value);
        }
        self.default_result()
    }

    fn visit_return_statement(&mut self, value: &Expression) -> Self::Result {
        self.visit_expression(value);
        self.default_result()
    }

    fn visit_if_statement(
        &mut self,
        condition:   &Expression,
        then_branch: &[QuickFuncStatement],
        else_branch: Option<&Vec<QuickFuncStatement>>,
    ) -> Self::Result {
        self.visit_expression(condition);
        for stmt in then_branch { self.visit_quick_func_statement(stmt); }
        if let Some(else_stmts) = else_branch {
            for stmt in else_stmts { self.visit_quick_func_statement(stmt); }
        }
        self.default_result()
    }

    fn visit_switch_statement(
        &mut self,
        expression:   &Expression,
        cases:        &[SwitchCase],
        default_case: Option<&SwitchCase>,
    ) -> Self::Result {
        self.visit_expression(expression);
        for case in cases { self.visit_switch_case(case); }
        if let Some(default) = default_case { self.visit_switch_case(default); }
        self.default_result()
    }

    /// Visit a single switch case, including its `case_value` and all body statements.
    fn visit_switch_case(&mut self, case: &SwitchCase) -> Self::Result {
        self.visit_value(&case.case_value);
        for stmt in &case.statements { self.visit_quick_func_statement(stmt); }
        self.default_result()
    }

    fn visit_assignment_statement(&mut self, _variable: &str, value: &Expression) -> Self::Result {
        self.visit_expression(value);
        self.default_result()
    }

    fn visit_arithmetic_assignment_statement(
        &mut self,
        _variable: &str,
        _operator: &str,
        value:     &Expression,
    ) -> Self::Result {
        self.visit_expression(value);
        self.default_result()
    }

    fn visit_object_creation_statement(&mut self, _variable: &str, object: &Value) -> Self::Result {
        self.visit_value(object);
        self.default_result()
    }

    fn visit_log_statement(&mut self, value: &Expression) -> Self::Result {
        self.visit_expression(value);
        self.default_result()
    }

    fn visit_expression_statement(&mut self, expression: &Expression) -> Self::Result {
        self.visit_expression(expression);
        self.default_result()
    }

    // ── Expressions ───────────────────────────────────────────────────────────
    //
    // All 24 Expression variants — exhaustive, no `_` arm.
    // Add new arms here when Expression gains new variants.

    fn visit_expression(&mut self, expr: &Expression) -> Self::Result {
        match expr {
            Expression::Identifier { .. } => {
                self.visit_identifier(expr)
            }
            Expression::QualifiedIdentifier { .. } => {
                self.visit_qualified_identifier(expr)
            }
            // General function call (not QuickFunc / static / imported namespace).
            Expression::FunctionCall { arguments, .. } => {
                self.visit_function_call(arguments)
            }
            Expression::QuickFuncCall { arguments, .. } => {
                self.visit_quick_func_call(arguments)
            }
            Expression::DixFunctionCall { arguments, .. } => {
                self.visit_dix_function_call(arguments)
            }
            Expression::StaticMethodCall { arguments, .. } => {
                self.visit_static_method_call(arguments)
            }
            Expression::InstanceMethodCall { instance, arguments, .. } => {
                self.visit_instance_method_call(instance, arguments)
            }
            // Built-in method or property: `target.method(args?)` / `target.prop`.
            // `arguments` is None for property access, Some(&[]) for no-arg calls.
            Expression::BuiltinFunction { target, arguments, .. } => {
                self.visit_builtin_function(target, arguments.as_deref())
            }
            // Alternative static-call representation used by some parser paths.
            Expression::StaticFunction { arguments, .. } => {
                self.visit_static_function(arguments)
            }
            Expression::ImportedFunctionCall { arguments, .. } => {
                self.visit_imported_function_call(arguments)
            }
            Expression::ArithmeticOp { left, right, .. } => {
                self.visit_arithmetic_op(left, right)
            }
            Expression::BitwiseOp { left, right, .. } => {
                self.visit_bitwise_op(left, right)
            }
            Expression::ComparisonOp { left, right, .. } => {
                self.visit_comparison_op(left, right)
            }
            Expression::LogicalOp { left, right, .. } => {
                self.visit_logical_op(left, right)
            }
            Expression::UnaryOp { operand, .. } => {
                self.visit_unary_op(operand)
            }
            Expression::ConfigAccess { .. } => {
                self.visit_config_access(expr)
            }
            Expression::EnumAccess { .. } => {
                self.visit_enum_access(expr)
            }
            // Multi-segment path access: `a.b.c` (not a method call).
            Expression::ObjectAccess { .. } => {
                self.visit_object_access(expr)
            }
            Expression::PropertyAccess { object, .. } => {
                self.visit_property_access(object)
            }
            Expression::IndexAccess { object, index, .. } => {
                self.visit_index_access(object, index)
            }
            Expression::Conditional { condition, true_value, false_value, .. } => {
                self.visit_conditional_expression(condition, true_value, false_value)
            }
            Expression::Value { value, .. } => {
                self.visit_value_expression(value)
            }
            Expression::Parenthesized { expression, .. } => {
                self.visit_parenthesized_expression(expression)
            }
            // Type cast: `expr as<Type>`.
            Expression::TypeCast { expression, .. } => {
                self.visit_type_cast(expression)
            }
        }
    }

    // ── Expression leaf / branch handlers ─────────────────────────────────────

    fn visit_identifier(&mut self, _expr: &Expression) -> Self::Result { self.default_result() }

    fn visit_qualified_identifier(&mut self, _expr: &Expression) -> Self::Result { self.default_result() }

    /// General function call: `name(args)`.
    fn visit_function_call(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments { self.visit_expression(arg); }
        self.default_result()
    }

    fn visit_quick_func_call(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments { self.visit_expression(arg); }
        self.default_result()
    }

    fn visit_dix_function_call(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments { self.visit_expression(arg); }
        self.default_result()
    }

    fn visit_static_method_call(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments { self.visit_expression(arg); }
        self.default_result()
    }

    /// Alternative static call form: `ClassName.method(args)`.
    fn visit_static_function(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments { self.visit_expression(arg); }
        self.default_result()
    }

    fn visit_instance_method_call(
        &mut self,
        instance:  &Expression,
        arguments: &[Expression],
    ) -> Self::Result {
        self.visit_expression(instance);
        for arg in arguments { self.visit_expression(arg); }
        self.default_result()
    }

    /// Built-in method/property: `target.method(args?)` or `target.prop`.
    /// `arguments` is `None` for property access, `Some(&[])` for no-arg method calls.
    fn visit_builtin_function(
        &mut self,
        target:    &Expression,
        arguments: Option<&[Expression]>,
    ) -> Self::Result {
        self.visit_expression(target);
        if let Some(args) = arguments {
            for arg in args { self.visit_expression(arg); }
        }
        self.default_result()
    }

    fn visit_imported_function_call(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments { self.visit_expression(arg); }
        self.default_result()
    }

    fn visit_enum_access   (&mut self, _expr: &Expression) -> Self::Result { self.default_result() }
    fn visit_config_access (&mut self, _expr: &Expression) -> Self::Result { self.default_result() }
    /// Multi-segment path access (`a.b.c`) with no call.
    fn visit_object_access (&mut self, _expr: &Expression) -> Self::Result { self.default_result() }

    fn visit_arithmetic_op(&mut self, left: &Expression, right: &Expression) -> Self::Result {
        self.visit_expression(left);
        self.visit_expression(right);
        self.default_result()
    }

    fn visit_comparison_op(&mut self, left: &Expression, right: &Expression) -> Self::Result {
        self.visit_expression(left);
        self.visit_expression(right);
        self.default_result()
    }

    fn visit_logical_op(&mut self, left: &Expression, right: &Expression) -> Self::Result {
        self.visit_expression(left);
        self.visit_expression(right);
        self.default_result()
    }

    fn visit_bitwise_op(&mut self, left: &Expression, right: &Expression) -> Self::Result {
        self.visit_expression(left);
        self.visit_expression(right);
        self.default_result()
    }

    fn visit_unary_op(&mut self, operand: &Expression) -> Self::Result {
        self.visit_expression(operand);
        self.default_result()
    }

    fn visit_property_access(&mut self, object: &Expression) -> Self::Result {
        self.visit_expression(object);
        self.default_result()
    }

    fn visit_index_access(&mut self, object: &Expression, index: &Expression) -> Self::Result {
        self.visit_expression(object);
        self.visit_expression(index);
        self.default_result()
    }

    fn visit_conditional_expression(
        &mut self,
        condition:   &Expression,
        true_value:  &Expression,
        false_value: &Expression,
    ) -> Self::Result {
        self.visit_expression(condition);
        self.visit_expression(true_value);
        self.visit_expression(false_value);
        self.default_result()
    }

    fn visit_value_expression(&mut self, value: &Value) -> Self::Result {
        self.visit_value(value);
        self.default_result()
    }

    fn visit_parenthesized_expression(&mut self, expression: &Expression) -> Self::Result {
        self.visit_expression(expression);
        self.default_result()
    }

    /// Type cast expression: `expr as<Type>`.
    fn visit_type_cast(&mut self, expression: &Expression) -> Self::Result {
        self.visit_expression(expression);
        self.default_result()
    }

    // ── @DATA ─────────────────────────────────────────────────────────────────

    fn visit_data_section(&mut self, section: &DataSection) -> Self::Result {
        for entry in &section.entries { self.visit_data_entry(entry); }
        self.default_result()
    }

    fn visit_data_entry(&mut self, entry: &DataEntry) -> Self::Result {
        match entry {
            DataEntry::SimpleProperty { value, .. }     => self.visit_simple_property(value),
            DataEntry::TableProperty  { properties, .. } => self.visit_table_property(properties),
            DataEntry::GroupArray     { items, .. }      => self.visit_group_array(items),
            DataEntry::ObjectProperty { object, .. }     => self.visit_object_property(object.as_ref()),
        }
    }

    fn visit_simple_property(&mut self, value: &Value) -> Self::Result {
        self.visit_value(value);
        self.default_result()
    }

    fn visit_table_property(&mut self, properties: &[PropertyAssignment]) -> Self::Result {
        for prop in properties { self.visit_value(&prop.value); }
        self.default_result()
    }

    fn visit_group_array(&mut self, items: &[Value]) -> Self::Result {
        for item in items { self.visit_value(item); }
        self.default_result()
    }

    fn visit_object_property(&mut self, object: &Value) -> Self::Result {
        self.visit_value(object);
        self.default_result()
    }

    // ── Values ────────────────────────────────────────────────────────────────
    //
    // All 25 Value variants — exhaustive, no `_` arm.
    // Add new arms here when Value gains new variants.

    fn visit_value(&mut self, value: &Value) -> Self::Result {
        match value {
            // ── Primitive / leaf nodes — no children to traverse ──────────────
            //
            // FIX: `Long` and `Identifier` were previously falling to `_ =>
            // default_result()`, silently skipping them in every semantic
            // analysis pass that tracks variable usage or literal types.
            Value::Integer { .. }
            | Value::Long { .. }            // ← was missing (i64 literal)
            | Value::Float { .. }
            | Value::Double { .. }
            | Value::ScientificNotation { .. }
            | Value::String { .. }
            | Value::Boolean { .. }
            | Value::HexColor { .. }
            | Value::Date { .. }
            | Value::Timestamp { .. }
            | Value::Null { .. }
            | Value::EnumValue { .. }       // ← enum leaf, no children
            | Value::Identifier { .. }      // ← variable reference leaf
            => self.default_result(),

            // ── Compound / traversable nodes ──────────────────────────────────

            Value::InterpolatedString { expressions, .. } => {
                self.visit_interpolated_string(expressions)
            }
            Value::Array { values, .. } => {
                self.visit_array_value(values)
            }
            Value::NestedArray { values, .. } => {
                self.visit_array_value(values)
            }
            Value::Object { properties, .. } => {
                self.visit_object_literal(properties)
            }
            Value::PrefixedConstructor { arguments, .. } => {
                self.visit_prefixed_constructor(arguments)
            }
            Value::QuickFuncCall { arguments, .. } => {
                self.visit_quick_func_call_value(arguments)
            }
            Value::Expression { expr, .. } => {
                self.visit_expression_value(expr)
            }
            // FIX: Range was previously `_ => default_result()`,
            // silently skipping the start and end sub-values.
            Value::Range { start, end, .. } => {
                self.visit_range_value(start, end)
            }
            // FIX: Lambda now also visits `statements` (the block body).
            // Previously only `body` (the return expression) was visited.
            Value::Lambda { body, statements, .. } => {
                self.visit_lambda_value(body, statements)
            }

            // ── Error / diagnostic nodes — no traversal ───────────────────────
            Value::ParseError { .. }
            | Value::Error { .. }
            | Value::Unknown { .. }
            => self.default_result(),
        }
    }

    // ── Value branch handlers ─────────────────────────────────────────────────

    fn visit_interpolated_string(&mut self, expressions: &[Expression]) -> Self::Result {
        for expr in expressions { self.visit_expression(expr); }
        self.default_result()
    }

    fn visit_array_value(&mut self, values: &[Value]) -> Self::Result {
        for item in values { self.visit_value(item); }
        self.default_result()
    }

    fn visit_object_literal(&mut self, properties: &[ObjectProperty]) -> Self::Result {
        for prop in properties { self.visit_value(&prop.value); }
        self.default_result()
    }

    fn visit_prefixed_constructor(&mut self, arguments: &[Value]) -> Self::Result {
        for arg in arguments { self.visit_value(arg); }
        self.default_result()
    }

    fn visit_quick_func_call_value(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments { self.visit_expression(arg); }
        self.default_result()
    }

    fn visit_expression_value(&mut self, expr: &Expression) -> Self::Result {
        self.visit_expression(expr);
        self.default_result()
    }

    /// Range value: `start_val..end_val`.
    fn visit_range_value(&mut self, start: &Value, end: &Value) -> Self::Result {
        self.visit_value(start);
        self.visit_value(end);
        self.default_result()
    }

    /// Lambda / closure value: `(params) => { stmts; body }`.
    ///
    /// The default visits all block statements first (execution order), then
    /// the return expression.  The `statements` slice is empty for pure
    /// expression lambdas `(x) => x * 2`.
    ///
    /// **Breaking change from previous version**: the signature now includes
    /// `statements` — override sites must be updated accordingly.
    fn visit_lambda_value(
        &mut self,
        body:       &Expression,
        statements: &[QuickFuncStatement],
    ) -> Self::Result {
        for stmt in statements { self.visit_quick_func_statement(stmt); }
        self.visit_expression(body);
        self.default_result()
    }

    // ── @SECURITY ─────────────────────────────────────────────────────────────

    fn visit_security_section(&mut self, section: &SecuritySection) -> Self::Result {
        for entry in &section.entries { self.visit_security_entry(entry); }
        self.default_result()
    }

    fn visit_security_entry(&mut self, entry: &SecurityEntry) -> Self::Result {
        for field in &entry.fields { self.visit_security_field(field); }
        self.default_result()
    }

    fn visit_security_field(&mut self, field: &SecurityField) -> Self::Result {
        self.visit_value(&field.value);
        self.default_result()
    }
    }
