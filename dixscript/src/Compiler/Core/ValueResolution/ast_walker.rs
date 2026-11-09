// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/ValueResolution/ast_walker.rs"
// ============================================================================
//! ASTWalker: discovers every QuickFunction call inside @DATA.
//!
//! Responsibilities:
//!   - Recursively visits all Values and Expressions in every DATA entry.
//!   - Detects `namespace.function` patterns and splits them using the
//!     SymbolTable's imported-namespace registry.
//!   - Maintains accurate scope context via ScopeTracker.
//!   - Resets scope between top-level entries to prevent state bleeding.
//!
//! The walk is over the arena AST: [`ASTWalker::find_all_in`] reads a [`DocData`] and its
//! [`Arena`], and [`ASTWalker::find_all`] copies an owned `DataSection` into a scratch arena
//! and calls it. Only the arguments of a found call are copied out of the arena, once each, into
//! the owned `Expression`s of its [`FunctionCallInfo`].
//!
//! Improvements over C# version:
//!   - All debug logging is gated by a cached `DebugConfig`; `format!(…)`
//!     is never evaluated when debug mode is off.
//!   - Handles Value variants the C# walker skipped: NestedArray, Range, Lambda.
//!   - `ParentEntry` reference removed; `entry_path` is the sole stable
//!     identifier (the C# comment already flagged it as the reliable key).
//!   - Namespace splitting uses a single `find('.')` + length check instead
//!     of allocating a split array.

use crate::Utilities::RustcHash::FxHashMap;

use crate::Compiler::AST::arena::{
    Arena, DocData, DocDataEntry, DocPropertyAssignment, ExprId, ExprList, ExprNode, PropList,
    ValueId, ValueList, ValueNode,
};
use crate::Compiler::AST::{DataSection, Expression, Position, TablePath};
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

    /// Walk an owned DATA section and return all discovered function calls.
    ///
    /// The section is copied into a scratch arena and walked by [`find_all_in`](Self::find_all_in),
    /// so a caller that already holds the arena form should call that instead.
    pub fn find_all(&mut self, data_section: &DataSection) -> Vec<FunctionCallInfo> {
        let mut arena = Arena::new();
        let data = DocData::from_owned(&mut arena, data_section);
        self.find_all_in(&arena, &data)
    }

    /// Walk the entire DATA section of an arena and return all discovered function calls.
    ///
    /// `data` must be a section of `arena`. The result vec is moved out via `mem::take`,
    /// zero-copy; `found_calls` is left empty and ready for a subsequent call if needed.
    ///
    /// # Panics
    ///
    /// Panics if an id in `data` is out of range for `arena`.
    pub fn find_all_in(&mut self, arena: &Arena, data: &DocData) -> Vec<FunctionCallInfo> {
        self.found_calls.clear();

        if self.debug_config.is_enabled {
            self.error_manager.log_debug(&format!(
                "Walking DATA section with {} entries",
                data.entries.len()
            ));
        }

        for (i, entry) in data.entries.iter().enumerate() {
            self.scope_tracker.reset_to_root();

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "[ASTWalker] Processing entry {} (scope reset to ROOT)",
                    i
                ));
            }

            self.visit_data_entry(arena, entry);
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

    fn visit_data_entry(&mut self, arena: &Arena, entry: &DocDataEntry) {
        self.current_entry_path = get_entry_path(entry);

        match entry {
            DocDataEntry::SimpleProperty { name, value, .. } => {
                self.visit_simple_property(arena, name, *value);
            }
            DocDataEntry::TableProperty { path, properties, .. } => {
                self.visit_table_property(arena, path, properties);
            }
            DocDataEntry::GroupArray { path, items, .. } => {
                self.visit_group_array(arena, path, *items);
            }
            DocDataEntry::ObjectProperty { name, object, .. } => {
                self.visit_object_property(arena, name, *object);
            }
        }
    }

    fn visit_simple_property(&mut self, arena: &Arena, name: &str, value: ValueId) {
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
                value_variant_name(arena.value(value))
            ));
        }

        self.visit_value(arena, value);
        self.scope_tracker.exit_scope();
    }

    fn visit_table_property(
        &mut self,
        arena: &Arena,
        path: &TablePath,
        properties: &[DocPropertyAssignment],
    ) {
        for segment in &path.segments {
            self.scope_tracker.enter_scope(segment);
        }

        // The scope variables were cleared when the entry began (`reset_to_root`), so the properties
        // registered below are the only ones in scope.
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

            self.visit_value(arena, assignment.value);
            self.scope_tracker.exit_scope();
        }

        for _ in &path.segments {
            self.scope_tracker.exit_scope();
        }
    }

    fn visit_group_array(&mut self, arena: &Arena, path: &TablePath, items: ValueList) {
        for segment in &path.segments {
            self.scope_tracker.enter_scope(segment);
        }

        for (i, &item) in arena.value_list(items).iter().enumerate() {
            let index_seg = format!("[{}]", i);
            self.scope_tracker.enter_scope(&index_seg);

            if self.debug_config.is_verbose {
                self.error_manager
                    .log_debug(&format!("[GroupArray] Item[{}]", i));
            }

            self.visit_value(arena, item);
            self.scope_tracker.exit_scope();
        }

        for _ in &path.segments {
            self.scope_tracker.exit_scope();
        }
    }

    fn visit_object_property(&mut self, arena: &Arena, name: &str, object: ValueId) {
        self.scope_tracker.enter_scope(name);

        let full_path = self.scope_tracker.get_current_path();
        self.scope_tracker.register_variable(name, &full_path);

        if self.debug_config.is_verbose {
            self.error_manager
                .log_debug(&format!("[ASTWalker]   [ObjectProperty] {}", name));
            self.error_manager
                .log_debug(&format!("[ASTWalker]     Full path: {}", full_path));
        }

        self.visit_value(arena, object);
        self.scope_tracker.exit_scope();
    }

    fn visit_value(&mut self, arena: &Arena, id: ValueId) {
        let node = *arena.value(id);

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "    [VisitValue] Type: {}",
                value_variant_name(&node)
            ));
        }

        match node {
            ValueNode::QuickFuncCall { function_name, arguments, position } => {
                self.handle_value_quick_func_call(
                    arena,
                    arena.text(function_name),
                    arguments,
                    position,
                );
            }

            ValueNode::Expression { expr, .. } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "      Found Expression wrapping: {}",
                        expr_variant_name(arena.expr(expr))
                    ));
                }
                self.visit_expression(arena, expr);
            }

            ValueNode::Array { values, .. } => {
                self.visit_array(arena, values);
            }

            ValueNode::NestedArray { values, .. } => {
                self.visit_array(arena, values);
            }

            ValueNode::Object { properties, .. } => {
                self.visit_object_literal(arena, properties);
            }

            ValueNode::InterpolatedString { expressions, .. } => {
                for &expr in arena.expr_list(expressions) {
                    self.visit_expression(arena, expr);
                }
            }

            ValueNode::PrefixedConstructor { prefix, arguments, .. } => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "      Found PrefixedConstructor: {}",
                        arena.text(prefix)
                    ));
                }
                for (i, &arg) in arena.value_list(arguments).iter().enumerate() {
                    if self.debug_config.is_verbose {
                        self.error_manager.log_debug(&format!(
                            "        [PrefixedConstructor] Arg[{}]: {}",
                            i,
                            value_variant_name(arena.value(arg))
                        ));
                    }
                    self.visit_value(arena, arg);
                }
            }

            ValueNode::Range { start, end, .. } => {
                self.visit_value(arena, start);
                self.visit_value(arena, end);
            }

            ValueNode::Lambda { body, .. } => {
                self.visit_expression(arena, body);
            }

            // Terminal values: no nested function calls possible.
            ValueNode::Integer { .. }
            | ValueNode::Long { .. }
            | ValueNode::Float { .. }
            | ValueNode::Double { .. }
            | ValueNode::ScientificNotation { .. }
            | ValueNode::String { .. }
            | ValueNode::Boolean { .. }
            | ValueNode::Null { .. }
            | ValueNode::Date { .. }
            | ValueNode::Timestamp { .. }
            | ValueNode::HexColor { .. }
            | ValueNode::EnumValue { .. }
            | ValueNode::Identifier { .. }
            | ValueNode::ParseError { .. }
            | ValueNode::Error { .. }
            | ValueNode::Unknown { .. } => {}
        }
    }

    /// Records a call that is a value of its own (`x = f(1)`) and visits its arguments, so a call
    /// nested in an argument is found too.
    fn handle_value_quick_func_call(
        &mut self,
        arena: &Arena,
        raw_name: &str,
        arguments: ExprList,
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

        self.push_call(arena, func_name, namespace_name, arguments, position);

        for &arg in arena.expr_list(arguments) {
            self.visit_expression(arena, arg);
        }
    }

    fn visit_array(&mut self, arena: &Arena, values: ValueList) {
        let values = arena.value_list(values);

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "        [VisitArray] Processing {} items",
                values.len()
            ));
        }

        for (i, &value) in values.iter().enumerate() {
            let index_seg = format!("[{}]", i);
            self.scope_tracker.enter_scope(&index_seg);

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "        [VisitArray]   Item[{}]: {}",
                    i,
                    value_variant_name(arena.value(value))
                ));
            }

            self.visit_value(arena, value);
            self.scope_tracker.exit_scope();
        }
    }

    fn visit_object_literal(&mut self, arena: &Arena, properties: PropList) {
        let properties = arena.prop_list(properties);
        let current_path = self.scope_tracker.get_current_path();

        // Register every key first so a value can name its siblings.
        for prop in properties {
            let key = arena.text(prop.key);
            let full_path = format!("{}.{}", current_path, key);
            self.scope_tracker.register_variable(key, &full_path);
        }

        for prop in properties {
            self.scope_tracker.enter_scope(arena.text(prop.key));
            self.visit_value(arena, prop.value);
            self.scope_tracker.exit_scope();
        }
    }

    fn visit_expression(&mut self, arena: &Arena, id: ExprId) {
        let node = *arena.expr(id);

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "      [VisitExpression] Type: {}",
                expr_variant_name(&node)
            ));
        }

        match node {
            // The arguments of a call inside an expression are not visited: a call nested
            // there is not found, as it never was.
            ExprNode::QuickFuncCall { name, arguments, position } => {
                let name = arena.text(name);
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

                self.push_call(arena, func_name, namespace_name, arguments, position);
            }

            ExprNode::ImportedFunctionCall { namespace_name, function_name, arguments, position } => {
                let (namespace_name, function_name) =
                    (arena.text(namespace_name), arena.text(function_name));
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "        Found ImportedFunctionCall: {}.{}",
                        namespace_name, function_name
                    ));
                }

                self.push_call(
                    arena,
                    function_name.to_string(),
                    Some(namespace_name.to_string()),
                    arguments,
                    position,
                );

                for &arg in arena.expr_list(arguments) {
                    self.visit_expression(arena, arg);
                }
            }

            ExprNode::ArithmeticOp { left, right, .. }
            | ExprNode::ComparisonOp { left, right, .. }
            | ExprNode::LogicalOp { left, right, .. } => {
                self.visit_expression(arena, left);
                self.visit_expression(arena, right);
            }

            ExprNode::UnaryOp { operand, .. } => {
                self.visit_expression(arena, operand);
            }

            ExprNode::Conditional { condition, true_value, false_value, .. } => {
                self.visit_expression(arena, condition);
                self.visit_expression(arena, true_value);
                self.visit_expression(arena, false_value);
            }

            ExprNode::PropertyAccess { object, .. } => {
                self.visit_expression(arena, object);
            }

            ExprNode::IndexAccess { object, index, .. } => {
                self.visit_expression(arena, object);
                self.visit_expression(arena, index);
            }

            ExprNode::StaticMethodCall { arguments, .. } => {
                for &arg in arena.expr_list(arguments) {
                    self.visit_expression(arena, arg);
                }
            }

            ExprNode::InstanceMethodCall { instance, arguments, .. } => {
                self.visit_expression(arena, instance);
                for &arg in arena.expr_list(arguments) {
                    self.visit_expression(arena, arg);
                }
            }

            ExprNode::Identifier { .. }
            | ExprNode::Value { .. }
            | ExprNode::EnumAccess { .. }
            | ExprNode::ConfigAccess { .. } => {}

            other => {
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "        Unhandled Expression type: {}",
                        expr_variant_name(&other)
                    ));
                }
            }
        }
    }

    /// Records a found call at the current scope, with its arguments copied out of the arena.
    fn push_call(
        &mut self,
        arena: &Arena,
        function_name: String,
        namespace_name: Option<String>,
        arguments: ExprList,
        position: Position,
    ) {
        let arguments: Vec<Expression> =
            arena.expr_list(arguments).iter().map(|&a| arena.export_expr(a)).collect();
        self.found_calls.push(FunctionCallInfo {
            function_name,
            namespace_name,
            arguments,
            location: self.scope_tracker.get_current_path(),
            scope: self.scope_tracker.get_current_scope(),
            entry_path: self.current_entry_path.clone(),
            position,
            scope_context: self.scope_tracker.get_scope_variables_snapshot(),
        });
    }

    // ── Name splitting ────────────────────────────────────────────────────

    /// Split `raw_name` into `(function, Some(namespace))` when it is exactly `namespace.function`
    /// and `namespace` is an imported one; otherwise the name is returned whole.
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

fn get_entry_path(entry: &DocDataEntry) -> String {
    match entry {
        DocDataEntry::SimpleProperty { name, .. } => name.clone(),
        DocDataEntry::TableProperty { path, .. } => path.to_string(),
        DocDataEntry::GroupArray { path, .. } => path.to_string(),
        DocDataEntry::ObjectProperty { name, .. } => name.clone(),
    }
}

// ── Debug helpers ─────────────────────────────────────────────────────────

fn value_variant_name(value: &ValueNode) -> &'static str {
    match value {
        ValueNode::Integer { .. } => "Integer",
        ValueNode::Long { .. } => "Long",
        ValueNode::Float { .. } => "Float",
        ValueNode::Double { .. } => "Double",
        ValueNode::ScientificNotation { .. } => "ScientificNotation",
        ValueNode::String { .. } => "String",
        ValueNode::Boolean { .. } => "Boolean",
        ValueNode::InterpolatedString { .. } => "InterpolatedString",
        ValueNode::HexColor { .. } => "HexColor",
        ValueNode::Date { .. } => "Date",
        ValueNode::Timestamp { .. } => "Timestamp",
        ValueNode::Null { .. } => "Null",
        ValueNode::Array { .. } => "Array",
        ValueNode::NestedArray { .. } => "NestedArray",
        ValueNode::Object { .. } => "Object",
        ValueNode::PrefixedConstructor { .. } => "PrefixedConstructor",
        ValueNode::EnumValue { .. } => "EnumValue",
        ValueNode::Identifier { .. } => "Identifier",
        ValueNode::QuickFuncCall { .. } => "QuickFuncCall",
        ValueNode::Expression { .. } => "Expression",
        ValueNode::Range { .. } => "Range",
        ValueNode::Lambda { .. } => "Lambda",
        ValueNode::ParseError { .. } => "ParseError",
        ValueNode::Error { .. } => "Error",
        ValueNode::Unknown { .. } => "Unknown",
    }
}

fn expr_variant_name(expr: &ExprNode) -> &'static str {
    match expr {
        ExprNode::Identifier { .. } => "Identifier",
        ExprNode::QualifiedIdentifier { .. } => "QualifiedIdentifier",
        ExprNode::FunctionCall { .. } => "FunctionCall",
        ExprNode::QuickFuncCall { .. } => "QuickFuncCall",
        ExprNode::DixFunctionCall { .. } => "DixFunctionCall",
        ExprNode::StaticMethodCall { .. } => "StaticMethodCall",
        ExprNode::InstanceMethodCall { .. } => "InstanceMethodCall",
        ExprNode::BuiltinFunction { .. } => "BuiltinFunction",
        ExprNode::StaticFunction { .. } => "StaticFunction",
        ExprNode::ImportedFunctionCall { .. } => "ImportedFunctionCall",
        ExprNode::ArithmeticOp { .. } => "ArithmeticOp",
        ExprNode::BitwiseOp { .. } => "BitwiseOp",
        ExprNode::ComparisonOp { .. } => "ComparisonOp",
        ExprNode::LogicalOp { .. } => "LogicalOp",
        ExprNode::UnaryOp { .. } => "UnaryOp",
        ExprNode::ConfigAccess { .. } => "ConfigAccess",
        ExprNode::EnumAccess { .. } => "EnumAccess",
        ExprNode::ObjectAccess { .. } => "ObjectAccess",
        ExprNode::PropertyAccess { .. } => "PropertyAccess",
        ExprNode::IndexAccess { .. } => "IndexAccess",
        ExprNode::Value { .. } => "Value",
        ExprNode::Parenthesized { .. } => "Parenthesized",
        ExprNode::Conditional { .. } => "Conditional",
        ExprNode::TypeCast { .. } => "TypeCast",
    }
}
