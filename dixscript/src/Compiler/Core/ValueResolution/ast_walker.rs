
//! ASTWalker — discovers every QuickFunction call inside @DATA.
//!
//! Responsibilities:
//!   - Recursively visits all Values and Expressions in every DATA entry.
//!   - Detects `namespace.function` patterns and splits them using the
//!     SymbolTable's imported-namespace registry.
//!   - Maintains accurate scope context via ScopeTracker.
//!   - Resets scope between top-level entries to prevent state bleeding.
//!
//! Improvements over C# version:
//!   - All debug logging is gated by a cached `DebugConfig` — `format!(…)`
//!     is never evaluated when debug mode is off.
//!   - Handles Value variants the C# walker skipped: NestedArray, Range, Lambda.
//!   - `ParentEntry` reference removed; `entry_path` is the sole stable
//!     identifier (the C# comment already flagged it as the reliable key).
//!   - Namespace splitting uses a single `find('.')` + length check instead
//!     of allocating a split array.

use rustc_hash::FxHashMap;

use crate::Compiler::AST::{
    DataEntry, DataSection, Expression, ObjectProperty, Position,
    PropertyAssignment, TablePath, Value,
};
use crate::Compiler::Core::DebugMode;
use crate::Compiler::Utilities::SymbolTable;
use crate::ErrorManager::{DebugConfig, ErrorManager};

use super::supporting_classes::{FunctionCallInfo, ScopeTracker};

/// Traverses @DATA and collects every QuickFunction call site.
///
/// Lifetime `'a` ties the walker to the borrowed SymbolTable used for
/// namespace resolution.
pub struct ASTWalker<'a> {
    error_manager: ErrorManager,
    symbol_table: &'a SymbolTable,
    found_calls: Vec<FunctionCallInfo>,
    scope_tracker: ScopeTracker,
    current_entry_path: String,
    debug_config: DebugConfig,
}

impl<'a> ASTWalker<'a> {
    pub fn new(
        error_manager: ErrorManager,
        symbol_table: &'a SymbolTable,
        debug_mode: DebugMode,
    ) -> Self {
        ASTWalker {
            error_manager,
            symbol_table,
            // 16 is a reasonable starting estimate; grows as needed.
            found_calls: Vec::with_capacity(16),
            scope_tracker: ScopeTracker::new(),
            current_entry_path: String::new(),
            debug_config: DebugConfig::from_debug_mode(debug_mode),
        }
    }

    /// Walk the entire DATA section and return all discovered function calls.
    ///
    /// The result vec is moved out via `mem::take` — zero-copy; `found_calls`
    /// is left empty and ready for a subsequent call if needed.
    pub fn find_all(&mut self, data_section: &DataSection) -> Vec<FunctionCallInfo> {
        self.found_calls.clear();

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Walking DATA section with {} entries",
                data_section.entries.len()
            ));
        }

        for (i, entry) in data_section.entries.iter().enumerate() {
            self.scope_tracker.reset_to_root();

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "[ASTWalker] Processing entry {} (scope reset to ROOT)",
                    i
                ));
            }

            self.visit_data_entry(entry);
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Found {} function calls in DATA section",
                self.found_calls.len()
            ));
        }

        if self.found_calls.is_empty() {
            self.error_manager
                .log_warning("[ASTWalker] Found ZERO function calls!");
        } else if self.debug_config.is_verbose {
            let mut groups: FxHashMap<String, usize> = FxHashMap::default();
            for call in &self.found_calls {
                *groups.entry(call.fully_qualified_name()).or_insert(0) += 1;
            }
            self.error_manager
                .log_debug("[ASTWalker] Function calls breakdown:");
            for (name, count) in &groups {
                self.error_manager
                    .log_debug(&format!("[ASTWalker]   {}: {} calls", name, count));
            }
        }

        std::mem::take(&mut self.found_calls)
    }

    fn visit_data_entry(&mut self, entry: &DataEntry) {
        self.current_entry_path = get_entry_path(entry);

        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                self.visit_simple_property(name, value);
            }
            DataEntry::TableProperty { path, properties, .. } => {
                self.visit_table_property(path, properties);
            }
            DataEntry::GroupArray { path, items, .. } => {
                self.visit_group_array(path, items);
            }
            DataEntry::ObjectProperty { name, object, .. } => {
                self.visit_object_property(name, object);
            }
        }
    }

    fn visit_simple_property(&mut self, name: &str, value: &Value) {
        self.scope_tracker.enter_scope(name);

        let full_path = self.scope_tracker.get_current_path();
        self.scope_tracker.register_variable(name, &full_path);

        if self.debug_config.is_verbose {
            self.error_manager
                .log_debug(&format!("[ASTWalker]   [SimpleProperty] {}", name));
            self.error_manager
                .log_debug(&format!("[ASTWalker]     Full path: {}", full_path));
            self.error_manager.log_debug(&format!(
                "[ASTWalker]     Value type: {}",
                value_variant_name(value)
            ));
        }

        self.visit_value(value);
        self.scope_tracker.exit_scope();
    }

    fn visit_table_property(
        &mut self,
        path: &TablePath,
        properties: &[PropertyAssignment],
    ) {
        for segment in &path.segments {
            self.scope_tracker.enter_scope(segment);
        }

        self.scope_tracker.clear_scope_variables();

        let current_path = self.scope_tracker.get_current_path();

        // Pass 1: register all properties so they are visible during value visits.
        for assignment in properties {
            let full_path = format!("{}.{}", current_path, assignment.name);
            self.scope_tracker
                .register_variable(&assignment.name, &full_path);

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "Registered table variable: {} -> {}",
                    assignment.name, full_path
                ));
            }
        }

        // Pass 2: visit property values.
        for assignment in properties {
            self.scope_tracker.enter_scope(&assignment.name);

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "[TableProperty] Visiting: {}",
                    assignment.name
                ));
            }

            self.visit_value(&assignment.value);
            self.scope_tracker.exit_scope();
        }

        for _ in &path.segments {
            self.scope_tracker.exit_scope();
        }
    }

    fn visit_group_array(&mut self, path: &TablePath, items: &[Value]) {
        for segment in &path.segments {
            self.scope_tracker.enter_scope(segment);
        }

        for (i, item) in items.iter().enumerate() {
            let index_seg = format!("[{}]", i);
            self.scope_tracker.enter_scope(&index_seg);

            if self.debug_config.is_verbose {
                self.error_manager
                    .log_debug(&format!("[GroupArray] Item[{}]", i));
            }

            self.visit_value(item);
            self.scope_tracker.exit_scope();
        }

        for _ in &path.segments {
            self.scope_tracker.exit_scope();
        }
    }

    fn visit_object_property(&mut self, name: &str, object: &Value) {
        self.scope_tracker.enter_scope(name);

        let full_path = self.scope_tracker.get_current_path();
        self.scope_tracker.register_variable(name, &full_path);

        if self.debug_config.is_verbose {
            self.error_manager
                .log_debug(&format!("[ASTWalker]   [ObjectProperty] {}", name));
            self.error_manager
                .log_debug(&format!("[ASTWalker]     Full path: {}", full_path));
        }

        self.visit_value(object);
        self.scope_tracker.exit_scope();
    }

    fn visit_value(&mut self, value: &Value) {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "    [VisitValue] Type: {}",
                value_variant_name(value)
            ));
        }

        match value {
            Value::QuickFuncCall {
                function_name,
                arguments,
                position,
            } => {
                self.handle_value_quick_func_call(function_name, arguments, *position);
            }

            Value::Expression { expr, .. } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "      Found Expression wrapping: {}",
                        expr_variant_name(expr)
                    ));
                }
                self.visit_expression(expr);
            }

            Value::Array { values, .. } => {
                self.visit_array(values);
            }

            Value::NestedArray { values, .. } => {
                self.visit_array(values);
            }

            Value::Object { properties, .. } => {
                self.visit_object_literal(properties);
            }

            Value::InterpolatedString { expressions, .. } => {
                for expr in expressions {
                    self.visit_expression(expr);
                }
            }

            Value::PrefixedConstructor { prefix, arguments, .. } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "      Found PrefixedConstructor: {}",
                        prefix
                    ));
                }
                for (i, arg) in arguments.iter().enumerate() {
                    if self.debug_config.is_verbose {
                        self.error_manager.log_debug(&format!(
                            "        [PrefixedConstructor] Arg[{}]: {}",
                            i,
                            value_variant_name(arg)
                        ));
                    }
                    self.visit_value(arg);
                }
            }

            Value::Range { start, end, .. } => {
                self.visit_value(start);
                self.visit_value(end);
            }

            Value::Lambda { body, .. } => {
                self.visit_expression(body);
            }

            // Terminal values — no nested function calls possible.
            Value::Integer { .. }
            | Value::Float { .. }
            | Value::Double { .. }
            | Value::ScientificNotation { .. }
            | Value::String { .. }
            | Value::Boolean { .. }
            | Value::Null { .. }
            | Value::Date { .. }
            | Value::Timestamp { .. }
            | Value::HexColor { .. }
            | Value::EnumValue { .. }
            | Value::Identifier { .. }
            | Value::ParseError { .. }
            | Value::Error { .. }
            | Value::Unknown { .. } => {}
        }
    }

    fn handle_value_quick_func_call(
        &mut self,
        raw_name: &str,
        arguments: &[Expression],
        position: Position,
    ) {
        if self.debug_config.is_verbose {
            self.error_manager
                .log_debug(&format!("      Found QuickFuncCall: {}", raw_name));
        }

        let (func_name, namespace_name) = self.split_namespace(raw_name);

        if self.debug_config.is_verbose {
            if let Some(ref ns) = namespace_name {
                self.error_manager.log_debug(&format!(
                    "      Detected IMPORTED function: {}.{}",
                    ns, func_name
                ));
            }
        }

        self.found_calls.push(FunctionCallInfo {
            function_name: func_name,
            namespace_name,
            arguments: arguments.to_vec(),
            location: self.scope_tracker.get_current_path(),
            scope: self.scope_tracker.get_current_scope(),
            entry_path: self.current_entry_path.clone(),
            position,
            scope_context: self.scope_tracker.get_scope_variables_snapshot(),
        });

        for arg in arguments {
            self.visit_expression(arg);
        }
    }

    fn visit_array(&mut self, values: &[Value]) {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "        [VisitArray] Processing {} items",
                values.len()
            ));
        }

        for (i, value) in values.iter().enumerate() {
            let index_seg = format!("[{}]", i);
            self.scope_tracker.enter_scope(&index_seg);

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "        [VisitArray]   Item[{}]: {}",
                    i,
                    value_variant_name(value)
                ));
            }

            self.visit_value(value);
            self.scope_tracker.exit_scope();
        }
    }

    fn visit_object_literal(&mut self, properties: &[ObjectProperty]) {
        let current_path = self.scope_tracker.get_current_path();

        // Pass 1: register all properties.
        for prop in properties {
            let full_path = format!("{}.{}", current_path, prop.key);
            self.scope_tracker
                .register_variable(&prop.key, &full_path);
        }

        // Pass 2: visit values.
        for prop in properties {
            self.scope_tracker.enter_scope(&prop.key);
            self.visit_value(&prop.value);
            self.scope_tracker.exit_scope();
        }
    }

    fn visit_expression(&mut self, expr: &Expression) {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "      [VisitExpression] Type: {}",
                expr_variant_name(expr)
            ));
        }

        match expr {
            Expression::QuickFuncCall { name, arguments, position } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "        Found QuickFuncCall in expression: {}",
                        name
                    ));
                }

                let (func_name, namespace_name) = self.split_namespace(name);

                if self.debug_config.is_verbose {
                    if let Some(ref ns) = namespace_name {
                        self.error_manager.log_debug(&format!(
                            "        Detected IMPORTED function: {}.{}",
                            ns, func_name
                        ));
                    }
                }

                self.found_calls.push(FunctionCallInfo {
                    function_name: func_name,
                    namespace_name,
                    arguments: arguments.clone(),
                    location: self.scope_tracker.get_current_path(),
                    scope: self.scope_tracker.get_current_scope(),
                    entry_path: self.current_entry_path.clone(),
                    position: *position,
                    scope_context: self.scope_tracker.get_scope_variables_snapshot(),
                });
                // NOTE: arguments are not recursed here — preserved from C# original.
                // ImportedFunctionCall below does recurse; the asymmetry is intentional.
            }

            Expression::ImportedFunctionCall {
                namespace_name,
                function_name,
                arguments,
                position,
            } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "        Found ImportedFunctionCall: {}.{}",
                        namespace_name, function_name
                    ));
                }

                self.found_calls.push(FunctionCallInfo {
                    function_name: function_name.clone(),
                    namespace_name: Some(namespace_name.clone()),
                    arguments: arguments.clone(),
                    location: self.scope_tracker.get_current_path(),
                    scope: self.scope_tracker.get_current_scope(),
                    entry_path: self.current_entry_path.clone(),
                    position: *position,
                    scope_context: self.scope_tracker.get_scope_variables_snapshot(),
                });

                for arg in arguments {
                    self.visit_expression(arg);
                }
            }

            Expression::ArithmeticOp { left, right, .. } => {
                self.visit_expression(left);
                self.visit_expression(right);
            }

            Expression::ComparisonOp { left, right, .. } => {
                self.visit_expression(left);
                self.visit_expression(right);
            }

            Expression::LogicalOp { left, right, .. } => {
                self.visit_expression(left);
                self.visit_expression(right);
            }

            Expression::UnaryOp { operand, .. } => {
                self.visit_expression(operand);
            }

            Expression::Conditional { condition, true_value, false_value, .. } => {
                self.visit_expression(condition);
                self.visit_expression(true_value);
                self.visit_expression(false_value);
            }

            Expression::PropertyAccess { object, .. } => {
                self.visit_expression(object);
            }

            Expression::IndexAccess { object, index, .. } => {
                self.visit_expression(object);
                self.visit_expression(index);
            }

            Expression::StaticMethodCall { arguments, .. } => {
                for arg in arguments {
                    self.visit_expression(arg);
                }
            }

            Expression::InstanceMethodCall { instance, arguments, .. } => {
                self.visit_expression(instance);
                for arg in arguments {
                    self.visit_expression(arg);
                }
            }

            Expression::Identifier { .. }
            | Expression::Value { .. }
            | Expression::EnumAccess { .. }
            | Expression::ConfigAccess { .. } => {}

            other => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "        Unhandled Expression type: {}",
                        expr_variant_name(other)
                    ));
                }
            }
        }
    }

    /// If `raw_name` is `"ns.func"` and `ns` is a known imported namespace,
    /// return `("func", Some("ns"))`. Otherwise return `(raw_name, None)`.
    /// Uses a single `find('.')` rather than allocating a split array.
    fn split_namespace(&self, raw_name: &str) -> (String, Option<String>) {
        if let Some(dot_pos) = raw_name.find('.') {
            let prefix = &raw_name[..dot_pos];
            let suffix = &raw_name[dot_pos + 1..];

            if !suffix.contains('.')
                && self.symbol_table.is_imported_namespace(prefix)
            {
                return (suffix.to_string(), Some(prefix.to_string()));
            }
        }

        (raw_name.to_string(), None)
    }
}

fn get_entry_path(entry: &DataEntry) -> String {
    match entry {
        DataEntry::SimpleProperty { name, .. } => name.clone(),
        DataEntry::TableProperty { path, .. } => path.to_string(),
        DataEntry::GroupArray { path, .. } => path.to_string(),
        DataEntry::ObjectProperty { name, .. } => name.clone(),
    }
}

/// Human-readable variant name for a Value — used exclusively in debug logs.
/// The compiler turns this into a jump table; zero runtime allocation.
fn value_variant_name(value: &Value) -> &'static str {
    match value {
        Value::Integer { .. } => "Integer",
        Value::Float { .. } => "Float",
        Value::Double { .. } => "Double",
        Value::ScientificNotation { .. } => "ScientificNotation",
        Value::String { .. } => "String",
        Value::Boolean { .. } => "Boolean",
        Value::InterpolatedString { .. } => "InterpolatedString",
        Value::HexColor { .. } => "HexColor",
        Value::Date { .. } => "Date",
        Value::Timestamp { .. } => "Timestamp",
        Value::Null { .. } => "Null",
        Value::Array { .. } => "Array",
        Value::NestedArray { .. } => "NestedArray",
        Value::Object { .. } => "Object",
        Value::PrefixedConstructor { .. } => "PrefixedConstructor",
        Value::EnumValue { .. } => "EnumValue",
        Value::Identifier { .. } => "Identifier",
        Value::QuickFuncCall { .. } => "QuickFuncCall",
        Value::Expression { .. } => "Expression",
        Value::Range { .. } => "Range",
        Value::Lambda { .. } => "Lambda",
        Value::ParseError { .. } => "ParseError",
        Value::Error { .. } => "Error",
        Value::Unknown { .. } => "Unknown",
    }
}

/// Human-readable variant name for an Expression — used exclusively in debug logs.
fn expr_variant_name(expr: &Expression) -> &'static str {
    match expr {
        Expression::Identifier { .. } => "Identifier",
        Expression::QualifiedIdentifier { .. } => "QualifiedIdentifier",
        Expression::FunctionCall { .. } => "FunctionCall",
        Expression::QuickFuncCall { .. } => "QuickFuncCall",
        Expression::DixFunctionCall { .. } => "DixFunctionCall",
        Expression::StaticMethodCall { .. } => "StaticMethodCall",
        Expression::InstanceMethodCall { .. } => "InstanceMethodCall",
        Expression::BuiltinFunction { .. } => "BuiltinFunction",
        Expression::StaticFunction { .. } => "StaticFunction",
        Expression::ImportedFunctionCall { .. } => "ImportedFunctionCall",
        Expression::ArithmeticOp { .. } => "ArithmeticOp",
        Expression::BitwiseOp { .. } => "BitwiseOp",
        Expression::ComparisonOp { .. } => "ComparisonOp",
        Expression::LogicalOp { .. } => "LogicalOp",
        Expression::UnaryOp { .. } => "UnaryOp",
        Expression::ConfigAccess { .. } => "ConfigAccess",
        Expression::EnumAccess { .. } => "EnumAccess",
        Expression::ObjectAccess { .. } => "ObjectAccess",
        Expression::PropertyAccess { .. } => "PropertyAccess",
        Expression::IndexAccess { .. } => "IndexAccess",
        Expression::Value { .. } => "Value",
        Expression::Parenthesized { .. } => "Parenthesized",
        Expression::Conditional { .. } => "Conditional",
        Expression::TypeCast { .. } => "TypeCast",
        Expression::BitwiseOp { .. } => "BitwiseOp",
    }
}
