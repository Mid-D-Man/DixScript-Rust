
use crate::Compiler::AST::*;

/// Base AST Visitor with default traversal logic
/// TResult is the return type of visit methods
pub trait AstVisitorBase {
    type Result: Default;

    /// Default result when no specific value is returned
    fn default_result(&self) -> Self::Result {
        Self::Result::default()
    }

    // ==================== ENTRY POINT ====================

    fn visit(&mut self, ast: &DixScript) -> Self::Result {
        if let Some(ref config) = ast.config {
            self.visit_config_section(config);
        }
        if let Some(ref imports) = ast.imports {
            self.visit_imports_section(imports);
        }
        if let Some(ref dlm) = ast.dlm {
            self.visit_dlm_section(dlm);
        }
        if let Some(ref enums) = ast.enums {
            self.visit_enums_section(enums);
        }
        if let Some(ref quick_functions) = ast.quick_functions {
            self.visit_quickfuncs_section(quick_functions);
        }
        if let Some(ref data) = ast.data {
            self.visit_data_section(data);
        }
        if let Some(ref security) = ast.security {
            self.visit_security_section(security);
        }

        self.default_result()
    }

    // ==================== SECTIONS ====================

    fn visit_config_section(&mut self, section: &ConfigSection) -> Self::Result {
        for entry in &section.entries {
            self.visit_config_entry(entry);
        }
        self.default_result()
    }

    fn visit_config_entry(&mut self, entry: &ConfigEntry) -> Self::Result {
        self.visit_config_value(&entry.value);
        self.default_result()
    }

    fn visit_config_value(&mut self, value: &ConfigValue) -> Self::Result {
        match value {
            ConfigValue::String(_) => self.visit_config_string_value(value),
            ConfigValue::Integer(_) => self.visit_config_integer_value(value),
            ConfigValue::Float(_) => self.visit_config_float_value(value),
            ConfigValue::Boolean(_) => self.visit_config_boolean_value(value),
            ConfigValue::Date(_) => self.visit_config_date_value(value),
            ConfigValue::Timestamp(_) => self.visit_config_timestamp_value(value),
            ConfigValue::Features(_) => self.visit_config_feature_value(value),
            ConfigValue::Debug(_) => self.visit_config_debug_value(value),
            ConfigValue::ErrorHandling(_) => self.visit_config_error_handling_value(value),
            ConfigValue::Compatibility(_) => self.visit_config_compatibility_value(value),
        }
    }

    fn visit_config_string_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }
    fn visit_config_integer_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }
    fn visit_config_float_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }
    fn visit_config_boolean_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }
    fn visit_config_date_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }
    fn visit_config_timestamp_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }
    fn visit_config_feature_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }
    fn visit_config_debug_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }
    fn visit_config_error_handling_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }
    fn visit_config_compatibility_value(&mut self, _value: &ConfigValue) -> Self::Result {
        self.default_result()
    }

    fn visit_imports_section(&mut self, section: &ImportsSection) -> Self::Result {
        for import in &section.imports {
            self.visit_import_declaration(import);
        }
        self.default_result()
    }

    fn visit_import_declaration(&mut self, _import: &ImportDeclaration) -> Self::Result {
        self.default_result()
    }

    fn visit_dlm_section(&mut self, section: &DLMSection) -> Self::Result {
        for module in &section.modules {
            self.visit_dlm_module(module);
        }
        self.default_result()
    }

    fn visit_dlm_module(&mut self, _module: &DLMModule) -> Self::Result {
        self.default_result()
    }

    fn visit_enums_section(&mut self, section: &EnumsSection) -> Self::Result {
        for enum_decl in &section.enums {
            self.visit_enum_declaration(enum_decl);
        }
        self.default_result()
    }

    fn visit_enum_declaration(&mut self, enum_decl: &EnumDeclaration) -> Self::Result {
        for field in &enum_decl.fields {
            self.visit_enum_field(field);
        }
        self.default_result()
    }

    fn visit_enum_field(&mut self, _field: &EnumField) -> Self::Result {
        self.default_result()
    }

    fn visit_quickfuncs_section(&mut self, section: &QuickFuncsSection) -> Self::Result {
        for func in &section.functions {
            self.visit_quick_function(func);
        }
        self.default_result()
    }

    fn visit_quick_function(&mut self, func: &QuickFunction) -> Self::Result {
        for param in &func.parameters {
            self.visit_quick_func_param(param);
        }

        for statement in &func.body {
            self.visit_quick_func_statement(statement);
        }

        self.default_result()
    }

    fn visit_quick_func_param(&mut self, param: &QuickFuncParam) -> Self::Result {
        if let Some(ref default_value) = param.default_value {
            self.visit_expression(default_value);
        }
        self.default_result()
    }

    fn visit_quick_func_statement(&mut self, statement: &QuickFuncStatement) -> Self::Result {
        match statement {
            QuickFuncStatement::Return { value, .. } => self.visit_return_statement(value),
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
            QuickFuncStatement::Log { value, .. } => self.visit_log_statement(value),
            QuickFuncStatement::VariableDeclaration { value, .. } => {
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
        condition: &Expression,
        then_branch: &[QuickFuncStatement],
        else_branch: Option<&Vec<QuickFuncStatement>>,
    ) -> Self::Result {
        self.visit_expression(condition);

        for stmt in then_branch {
            self.visit_quick_func_statement(stmt);
        }

        if let Some(else_stmts) = else_branch {
            for stmt in else_stmts {
                self.visit_quick_func_statement(stmt);
            }
        }

        self.default_result()
    }

    fn visit_switch_statement(
        &mut self,
        expression: &Expression,
        cases: &[SwitchCase],
        default_case: Option<&SwitchCase>,
    ) -> Self::Result {
        self.visit_expression(expression);

        for case in cases {
            for stmt in &case.statements {
                self.visit_quick_func_statement(stmt);
            }
        }

        if let Some(default) = default_case {
            for stmt in &default.statements {
                self.visit_quick_func_statement(stmt);
            }
        }

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
        value: &Expression,
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

    // ==================== EXPRESSIONS ====================

    fn visit_expression(&mut self, expr: &Expression) -> Self::Result {
        match expr {
            Expression::Identifier { .. } => self.visit_identifier(expr),
            Expression::QualifiedIdentifier { .. } => self.visit_qualified_identifier(expr),
            Expression::PropertyAccess { object, .. } => self.visit_property_access(object),
            Expression::InstanceMethodCall { instance, arguments, .. } => {
                self.visit_instance_method_call(instance, arguments)
            }
            Expression::StaticMethodCall { arguments, .. } => self.visit_static_method_call(arguments),
            Expression::QuickFuncCall { arguments, .. } => self.visit_quick_func_call(arguments),
            Expression::ImportedFunctionCall { arguments, .. } => {
                self.visit_imported_function_call(arguments)
            }
            Expression::DixFunctionCall { arguments, .. } => self.visit_dix_function_call(arguments),
            Expression::EnumAccess { .. } => self.visit_enum_access(expr),
            Expression::ConfigAccess { .. } => self.visit_config_access(expr),
            Expression::ArithmeticOp { left, right, .. } => self.visit_arithmetic_op(left, right),
            Expression::ComparisonOp { left, right, .. } => self.visit_comparison_op(left, right),
            Expression::LogicalOp { left, right, .. } => self.visit_logical_op(left, right),
            Expression::BitwiseOp { left, right, .. } => self.visit_bitwise_op(left, right),
            Expression::UnaryOp { operand, .. } => self.visit_unary_op(operand),
            Expression::IndexAccess { object, index, .. } => self.visit_index_access(object, index),
            Expression::Conditional { condition, true_value, false_value, .. } => {
                self.visit_conditional_expression(condition, true_value, false_value)
            }
            Expression::Value { value, .. } => self.visit_value_expression(value),
            Expression::Parenthesized { expression, .. } => self.visit_parenthesized_expression(expression),
            _ => self.default_result(),
        }
    }

    fn visit_qualified_identifier(&mut self, _qual_id: &Expression) -> Self::Result {
        self.default_result()
    }

    fn visit_identifier(&mut self, _expr: &Expression) -> Self::Result {
        self.default_result()
    }

    fn visit_property_access(&mut self, object: &Expression) -> Self::Result {
        self.visit_expression(object);
        self.default_result()
    }

    fn visit_instance_method_call(&mut self, instance: &Expression, arguments: &[Expression]) -> Self::Result {
        self.visit_expression(instance);
        for arg in arguments {
            self.visit_expression(arg);
        }
        self.default_result()
    }

    fn visit_static_method_call(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments {
            self.visit_expression(arg);
        }
        self.default_result()
    }

    fn visit_quick_func_call(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments {
            self.visit_expression(arg);
        }
        self.default_result()
    }

    fn visit_imported_function_call(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments {
            self.visit_expression(arg);
        }
        self.default_result()
    }

    fn visit_dix_function_call(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments {
            self.visit_expression(arg);
        }
        self.default_result()
    }

    fn visit_enum_access(&mut self, _expr: &Expression) -> Self::Result {
        self.default_result()
    }

    fn visit_config_access(&mut self, _expr: &Expression) -> Self::Result {
        self.default_result()
    }

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

    fn visit_index_access(&mut self, object: &Expression, index: &Expression) -> Self::Result {
        self.visit_expression(object);
        self.visit_expression(index);
        self.default_result()
    }

    fn visit_conditional_expression(
        &mut self,
        condition: &Expression,
        true_value: &Expression,
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

    // ==================== DATA SECTION ====================

    fn visit_data_section(&mut self, section: &DataSection) -> Self::Result {
        for entry in &section.entries {
            self.visit_data_entry(entry);
        }
        self.default_result()
    }

    fn visit_data_entry(&mut self, entry: &DataEntry) -> Self::Result {
        match entry {
            DataEntry::SimpleProperty { value, .. } => self.visit_simple_property(value),
            DataEntry::TableProperty { properties, .. } => self.visit_table_property(properties),
            DataEntry::GroupArray { items, .. } => self.visit_group_array(items),
            DataEntry::ObjectProperty { object, .. } => self.visit_object_property(object.as_ref()),
        }
    }

    fn visit_simple_property(&mut self, value: &Value) -> Self::Result {
        self.visit_value(value);
        self.default_result()
    }

    fn visit_table_property(&mut self, properties: &[PropertyAssignment]) -> Self::Result {
        for prop in properties {
            self.visit_value(&prop.value);
        }
        self.default_result()
    }

    fn visit_group_array(&mut self, items: &[Value]) -> Self::Result {
        for item in items {
            self.visit_value(item);
        }
        self.default_result()
    }

    fn visit_object_property(&mut self, object: &Value) -> Self::Result {
        self.visit_value(object);
        self.default_result()
    }

    // ==================== VALUES ====================

    fn visit_value(&mut self, value: &Value) -> Self::Result {
        match value {
            Value::Integer { .. } | Value::Float { .. } | Value::Double { .. }
            | Value::ScientificNotation { .. } | Value::String { .. } | Value::Boolean { .. }
            | Value::HexColor { .. } | Value::Date { .. } | Value::Timestamp { .. }
            | Value::Null { .. } => self.default_result(),

            Value::InterpolatedString { expressions, .. } => self.visit_interpolated_string(expressions),
            Value::Array { values, .. } => self.visit_array_value(values),
            Value::NestedArray { values, .. } => self.visit_array_value(values),
            Value::Object { properties, .. } => self.visit_object_literal(properties),
            Value::PrefixedConstructor { arguments, .. } => self.visit_prefixed_constructor(arguments),
            Value::EnumValue { .. } => self.default_result(),
            Value::QuickFuncCall { arguments, .. } => self.visit_quick_func_call_value(arguments),
            Value::Expression { expr, .. } => self.visit_expression_value(expr),
            Value::Lambda { body, .. } => self.visit_lambda_value(body),
            _ => self.default_result(),
        }
    }

    fn visit_interpolated_string(&mut self, expressions: &[Expression]) -> Self::Result {
        for expr in expressions {
            self.visit_expression(expr);
        }
        self.default_result()
    }

    fn visit_array_value(&mut self, values: &[Value]) -> Self::Result {
        for item in values {
            self.visit_value(item);
        }
        self.default_result()
    }

    fn visit_object_literal(&mut self, properties: &[ObjectProperty]) -> Self::Result {
        for prop in properties {
            self.visit_value(&prop.value);
        }
        self.default_result()
    }

    fn visit_prefixed_constructor(&mut self, arguments: &[Value]) -> Self::Result {
        for arg in arguments {
            self.visit_value(arg);
        }
        self.default_result()
    }

    fn visit_quick_func_call_value(&mut self, arguments: &[Expression]) -> Self::Result {
        for arg in arguments {
            self.visit_expression(arg);
        }
        self.default_result()
    }

    fn visit_expression_value(&mut self, expr: &Expression) -> Self::Result {
        self.visit_expression(expr);
        self.default_result()
    }

    fn visit_lambda_value(&mut self, body: &Expression) -> Self::Result {
        self.visit_expression(body);
        self.default_result()
    }

    // ==================== SECURITY SECTION ====================

    fn visit_security_section(&mut self, section: &SecuritySection) -> Self::Result {
        for entry in &section.entries {
            self.visit_security_entry(entry);
        }
        self.default_result()
    }

    fn visit_security_entry(&mut self, entry: &SecurityEntry) -> Self::Result {
        for field in &entry.fields {
            self.visit_security_field(field);
        }
        self.default_result()
    }

    fn visit_security_field(&mut self, field: &SecurityField) -> Self::Result {
        self.visit_value(&field.value);
        self.default_result()
    }
          }
