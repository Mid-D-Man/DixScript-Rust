// src/Compiler/Core/ValueResolution/value_resolver.rs
//!
//! ValueResolver — Orchestrates compile-time value resolution
//!
//! ## Resolution Pipeline (5 phases):
//! 1. **Enum Pre-Resolution**   — Replace all EnumValue/EnumAccess with integers
//! 2. **Data Context Build**    — Populate data_context with all literal values
//! 3. **Function Call Discovery** — Walk AST to find all QuickFunction calls
//! 4. **Iterative Resolution**  — Execute calls, replace in AST, update context
//! 5. **Identifier Resolution** — Resolve remaining Identifier references

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use chrono::Utc;

use crate::Builtins::Core::DixValue;
use crate::Builtins::Resolver;
use crate::Compiler::AST::{
    DataEntry, DataSection, DixScript, Expression, ObjectProperty, Position,
    PropertyAssignment, Value,
};
use crate::Compiler::Core::DebugMode;
use crate::Compiler::Utilities::{PathBuilder, SymbolTable};
use crate::ErrorManager::ErrorManager;

use super::ast_walker::ASTWalker;
use super::execution_context::ExecutionContext;
use super::function_interpreter::{FunctionInterpreter, InterpreterError};
use super::supporting_classes::{
    DebugConfig, FunctionCallInfo, ResolutionRecord,
    ValueResolutionResult,
};

// ==================== RESOLVER ERROR ====================

/// Typed errors from value resolution orchestration
#[derive(Debug, Clone)]
pub enum ResolverError {
    FunctionNotFound {
        name: String,
        location: String,
        position: Position,
    },
    NamespaceNotFound {
        name: String,
        location: String,
        position: Position,
    },
    FunctionNotInNamespace {
        namespace: String,
        function: String,
        location: String,
        position: Position,
    },
    InvalidEnumAccess {
        location: String,
        message: String,
        position: Position,
    },
    InvalidFunctionScope {
        function: String,
        call_scope: String,
        allowed_scopes: Vec<String>,
        position: Position,
    },
    CircularDependency {
        stuck_calls: Vec<String>,
    },
    ExecutionFailed {
        function: String,
        location: String,
        inner: InterpreterError,
    },
    Fatal {
        message: String,
    },
}

impl std::fmt::Display for ResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolverError::FunctionNotFound { name, location, position } => {
                write!(f, "Function '{}' not found at {} ({})", name, location, position)
            }
            ResolverError::InvalidFunctionScope { function, call_scope, allowed_scopes, position } => {
                write!(
                    f,
                    "Function '{}' not accessible from scope '{}' at {} (allowed: {})",
                    function,
                    call_scope,
                    position,
                    allowed_scopes.join(", ")
                )
            }
            ResolverError::CircularDependency { stuck_calls } => {
                write!(
                    f,
                    "Circular dependency detected: {} calls cannot be resolved",
                    stuck_calls.len()
                )
            }
            ResolverError::ExecutionFailed { function, location, inner } => {
                write!(f, "Execution failed for {} at {}: {}", function, location, inner)
            }
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::error::Error for ResolverError {}

// ==================== VALUE RESOLVER ====================

/// Orchestrates the full compile-time resolution pass
pub struct ValueResolver<'a> {
    /// Owned AST — mutated in place during resolution
    ast: DixScript,
    /// Borrowed symbol table (read-only)
    symbol_table: &'a SymbolTable,
    /// Function interpreter (owns cloned quick_functions)
    interpreter: FunctionInterpreter<'a>,
    /// Shared data context (Rc for shared ownership with interpreter)
    data_context: Rc<RefCell<HashMap<String, DixValue>>>,
    /// Cached debug flags
    debug_config: DebugConfig,
    /// Successfully resolved values (for dump)
    resolved_values: HashMap<String, DixValue>,
    /// Accumulated log statements from interpreter
    log_statements: Vec<String>,
    /// Resolution history (for dump)
    resolution_history: Vec<ResolutionRecord>,
    /// Wall-clock start time
    start_time: Instant,
    /// Error manager
    error_manager: ErrorManager,
}

impl<'a> ValueResolver<'a> {
    // ==================== CONSTRUCTOR ====================

    pub fn new(
        ast: DixScript,
        symbol_table: &'a SymbolTable,
        debug_mode: DebugMode,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();

        let quick_functions = ast.quick_functions
            .as_ref()
            .map(|qf| qf.functions.to_vec())
            .unwrap_or_default();

        let data_context = Rc::new(RefCell::new(HashMap::new()));

        let interpreter = FunctionInterpreter::new(
            symbol_table,
            quick_functions,
            Rc::clone(&data_context),
            debug_mode,
        );

        Resolver::initialize();

        let debug_config = DebugConfig::from_mode(debug_mode);

        if debug_config.is_enabled {
            error_manager.log_info("ValueResolver initialized");
        }

        ValueResolver {
            ast,
            symbol_table,
            interpreter,
            data_context,
            debug_config,
            resolved_values: HashMap::new(),
            log_statements: Vec::new(),
            resolution_history: Vec::new(),
            start_time: Instant::now(),
            error_manager,
        }
    }

    // ==================== MAIN ORCHESTRATION ====================

    /// Execute the full resolution pass (5 phases)
    pub fn resolve(mut self) -> ValueResolutionResult {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("[Phase 4.1] Starting value resolution");
        }

        // PHASE 1: Enum Pre-Resolution
        if let Err(e) = self.resolve_all_enum_values() {
            return self.create_failed_result(vec![e.to_string()]);
        }

        // PHASE 2: Initial Data Context Build
        self.build_initial_data_context();

        if self.debug_config.is_enabled {
            self.dump_data_context();
        }

        // PHASE 3: Function Call Discovery
        let function_calls = self.find_all_function_calls();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "[DIAGNOSTIC] Found {} total function calls to resolve",
                function_calls.len()
            ));
            self.log_function_call_breakdown(&function_calls);
        }

        if function_calls.is_empty() {
            if self.debug_config.is_enabled {
                self.error_manager.log_warning("[DIAGNOSTIC] No function calls found in DATA section!");
            }

            // Phase 5 still runs: identifiers may reference sibling literals
            self.resolve_remaining_identifiers();

            return ValueResolutionResult {
                is_success: true,
                original_ast: Some(self.ast.clone()),
                resolved_ast: Some(self.ast),
                function_calls_resolved: 0,
                errors: Vec::new(),
                log_statements: self.log_statements,
                resolution_duration: self.start_time.elapsed(),
                resolution_history: Vec::new(),
            };
        }

        // PHASE 4: Iterative Resolution Loop
        let (success_count, errors) = self.execute_iterative_resolution(function_calls);

        // PHASE 5: Resolve Remaining Identifiers (only if all functions resolved)
        if errors.is_empty() && success_count > 0 {
            self.resolve_remaining_identifiers();
        }

        let duration = self.start_time.elapsed();

        if self.debug_config.is_enabled {
            self.error_manager.log_info("==========================================");
            self.error_manager.log_info("[Phase 4.1] Resolution Complete");
            self.error_manager.log_info(&format!("  Resolved:             {}", success_count));
            self.error_manager.log_info(&format!("  Failed:               {}", errors.len()));
            self.error_manager.log_info(&format!("  Duration:             {:.2}ms", duration.as_secs_f64() * 1000.0));
            self.error_manager.log_info("==========================================");
        }

        ValueResolutionResult {
            is_success: errors.is_empty(),
            original_ast: Some(self.ast.clone()),
            resolved_ast: Some(self.ast),
            function_calls_resolved: success_count,
            errors,
            log_statements: self.log_statements,
            resolution_duration: duration,
            resolution_history: self.resolution_history,
        }
    }

    // ==================== PHASE 1: ENUM PRE-RESOLUTION ====================

    /// CRITICAL: Pre-process DATA section to resolve ALL enum values to integers.
    ///
    /// FIX: Clone the entries first so the mutable borrow of `self.ast.data`
    /// is dropped before `self.resolve_enums_in_entry()` needs `&self`.
    fn resolve_all_enum_values(&mut self) -> Result<(), ResolverError> {
        // Early-out: nothing to do without a DATA section
        if self.ast.data.is_none() {
            if self.debug_config.is_enabled {
                self.error_manager.log_debug("No DATA section - skipping enum resolution");
            }
            return Ok(());
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info("Pre-processing: Resolving all enum values to integers");
        }

        // ── BORROW FIX ────────────────────────────────────────────────────────
        // Clone the minimal state we need from `self.ast.data` so that the
        // mutable borrow ends here.  `resolve_enums_in_entry` only needs &self,
        // so there must be no outstanding &mut when we call it.
        let (entries_snapshot, section_position) = {
            let data = self.ast.data.as_ref().unwrap();
            (data.entries.clone(), data.position)
        };
        // ─────────────────────────────────────────────────────────────────────

        let mut local_enum_count = 0usize;
        let mut imported_enum_count = 0usize;
        let mut new_entries = Vec::with_capacity(entries_snapshot.len());

        for entry in &entries_snapshot {
            let (new_entry, local_count, imported_count) = self.resolve_enums_in_entry(entry)?;
            new_entries.push(new_entry);
            local_enum_count += local_count;
            imported_enum_count += imported_count;
        }

        // Write resolved entries back — no existing borrows at this point
        self.ast.data = Some(DataSection {
            entries: new_entries,
            position: section_position,
        });

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "✓ Resolved {} local enum values to integers",
                local_enum_count
            ));
            self.error_manager.log_info(&format!(
                "✓ Resolved {} imported enum values to integers",
                imported_enum_count
            ));
        }

        Ok(())
    }

    /// Resolve all enums in a single DATA entry
    fn resolve_enums_in_entry(
        &self,
        entry: &DataEntry,
    ) -> Result<(DataEntry, usize, usize), ResolverError> {
        let mut local_count = 0;
        let mut imported_count = 0;

        match entry {
            DataEntry::SimpleProperty { name, data_type, value, position } => {
                let (new_value, local, imported) = self.resolve_enums_in_value(value)?;
                local_count += local;
                imported_count += imported;

                if local + imported > 0 {
                    Ok((
                        DataEntry::SimpleProperty {
                            name: name.clone(),
                            data_type: *data_type,
                            value: new_value,
                            position: *position,
                        },
                        local_count,
                        imported_count,
                    ))
                } else {
                    Ok((entry.clone(), local_count, imported_count))
                }
            }

            DataEntry::TableProperty { path, properties, position } => {
                let mut new_properties = Vec::with_capacity(properties.len());
                let mut any_changed = false;

                for prop in properties {
                    let (new_value, local, imported) = self.resolve_enums_in_value(&prop.value)?;
                    local_count += local;
                    imported_count += imported;

                    if local + imported > 0 {
                        new_properties.push(PropertyAssignment {
                            name: prop.name.clone(),
                            data_type: prop.data_type,
                            value: new_value,
                            position: prop.position,
                        });
                        any_changed = true;
                    } else {
                        new_properties.push(prop.clone());
                    }
                }

                if any_changed {
                    Ok((
                        DataEntry::TableProperty {
                            path: path.clone(),
                            properties: new_properties,
                            position: *position,
                        },
                        local_count,
                        imported_count,
                    ))
                } else {
                    Ok((entry.clone(), local_count, imported_count))
                }
            }

            DataEntry::GroupArray { path, items, position } => {
                let mut new_items = Vec::with_capacity(items.len());
                let mut any_changed = false;

                for item in items {
                    let (new_value, local, imported) = self.resolve_enums_in_value(item)?;
                    local_count += local;
                    imported_count += imported;

                    if local + imported > 0 {
                        new_items.push(new_value);
                        any_changed = true;
                    } else {
                        new_items.push(item.clone());
                    }
                }

                if any_changed {
                    Ok((
                        DataEntry::GroupArray {
                            path: path.clone(),
                            items: new_items,
                            position: *position,
                        },
                        local_count,
                        imported_count,
                    ))
                } else {
                    Ok((entry.clone(), local_count, imported_count))
                }
            }

            DataEntry::ObjectProperty { name, data_type, object, position } => {
                let (new_obj, local, imported) = self.resolve_enums_in_object_literal(object)?;
                local_count += local;
                imported_count += imported;

                if local + imported > 0 {
                    Ok((
                        DataEntry::ObjectProperty {
                            name: name.clone(),
                            data_type: *data_type,
                            object: Box::from(new_obj),
                            position: *position,
                        },
                        local_count,
                        imported_count,
                    ))
                } else {
                    Ok((entry.clone(), local_count, imported_count))
                }
            }
        }
    }

    /// Recursively resolve all enums in a value
    fn resolve_enums_in_value(
        &self,
        value: &Value,
    ) -> Result<(Value, usize, usize), ResolverError> {
        let mut local_count = 0;
        let mut imported_count = 0;

        match value {
            Value::EnumValue { enum_name, value: enum_value, position } => {
                if enum_name.contains('.') {
                    let parts: Vec<&str> = enum_name.split('.').collect();
                    if parts.len() == 2 {
                        let namespace_name = parts[0];
                        let actual_enum_name = parts[1];

                        let ns = self.symbol_table.try_get_namespace(namespace_name)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", namespace_name, actual_enum_name, enum_value),
                                message: format!("Namespace '{}' not found", namespace_name),
                                position: *position,
                            })?;

                        let enum_fields = ns.enums.get(actual_enum_name)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", namespace_name, actual_enum_name, enum_value),
                                message: format!("Enum '{}' not found", actual_enum_name),
                                position: *position,
                            })?;

                        let field_value = enum_fields.get(enum_value)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", namespace_name, actual_enum_name, enum_value),
                                message: "Enum value not found".to_string(),
                                position: *position,
                            })?;

                        return Ok((
                            Value::Integer { value: *field_value, position: *position },
                            0,
                            1,
                        ));
                    }
                }

                // Local enum
                let enum_int_value = self.symbol_table.try_get_enum_field_value(enum_name, enum_value)
                    .ok_or_else(|| ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}", enum_name, enum_value),
                        message: "Enum value not found".to_string(),
                        position: *position,
                    })?;

                Ok((
                    Value::Integer { value: enum_int_value, position: *position },
                    1,
                    0,
                ))
            }

            Value::Expression { expr, position: _ } => {
                if let Expression::EnumAccess { namespace_name, enum_name, value: enum_value, position } = expr.as_ref() {
                    if let Some(ns_name) = namespace_name {
                        let ns = self.symbol_table.try_get_namespace(ns_name)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", ns_name, enum_name, enum_value),
                                message: format!("Namespace '{}' not found", ns_name),
                                position: *position,
                            })?;

                        let enum_fields = ns.enums.get(enum_name)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", ns_name, enum_name, enum_value),
                                message: format!("Enum '{}' not found", enum_name),
                                position: *position,
                            })?;

                        let field_value = enum_fields.get(enum_value)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", ns_name, enum_name, enum_value),
                                message: "Enum value not found".to_string(),
                                position: *position,
                            })?;

                        return Ok((
                            Value::Integer { value: *field_value, position: *position },
                            0,
                            1,
                        ));
                    } else {
                        let local_enum_value = self.symbol_table.try_get_enum_field_value(enum_name, enum_value)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}", enum_name, enum_value),
                                message: "Enum value not found".to_string(),
                                position: *position,
                            })?;

                        return Ok((
                            Value::Integer { value: local_enum_value, position: *position },
                            1,
                            0,
                        ));
                    }
                }

                Ok((value.clone(), 0, 0))
            }

            Value::Array { values, position } | Value::NestedArray { values, position, .. } => {
                let mut new_values = Vec::with_capacity(values.len());
                let mut any_changed = false;

                for item in values {
                    let (new_value, local, imported) = self.resolve_enums_in_value(item)?;
                    local_count += local;
                    imported_count += imported;

                    if local + imported > 0 {
                        new_values.push(new_value);
                        any_changed = true;
                    } else {
                        new_values.push(item.clone());
                    }
                }

                if any_changed {
                    Ok((
                        Value::Array { values: new_values, position: *position },
                        local_count,
                        imported_count,
                    ))
                } else {
                    Ok((value.clone(), local_count, imported_count))
                }
            }

            Value::Object { properties, position } => {
                let (new_obj, local, imported) =
                    self.resolve_enums_in_object_literal_from_value(properties, *position)?;
                local_count += local;
                imported_count += imported;

                if local + imported > 0 {
                    Ok((new_obj, local_count, imported_count))
                } else {
                    Ok((value.clone(), local_count, imported_count))
                }
            }

            Value::PrefixedConstructor { prefix, arguments, position } => {
                let mut new_args = Vec::with_capacity(arguments.len());
                let mut any_changed = false;

                for arg in arguments {
                    let (new_value, local, imported) = self.resolve_enums_in_value(arg)?;
                    local_count += local;
                    imported_count += imported;

                    if local + imported > 0 {
                        new_args.push(new_value);
                        any_changed = true;
                    } else {
                        new_args.push(arg.clone());
                    }
                }

                if any_changed {
                    Ok((
                        Value::PrefixedConstructor {
                            prefix: prefix.clone(),
                            arguments: new_args,
                            position: *position,
                        },
                        local_count,
                        imported_count,
                    ))
                } else {
                    Ok((value.clone(), local_count, imported_count))
                }
            }

            _ => Ok((value.clone(), 0, 0)),
        }
    }

    fn resolve_enums_in_object_literal_from_value(
        &self,
        properties: &[ObjectProperty],
        position: Position,
    ) -> Result<(Value, usize, usize), ResolverError> {
        let mut local_count = 0;
        let mut imported_count = 0;
        let mut new_properties = Vec::with_capacity(properties.len());
        let mut any_changed = false;

        for prop in properties {
            let (new_value, local, imported) = self.resolve_enums_in_value(&prop.value)?;
            local_count += local;
            imported_count += imported;

            if local + imported > 0 {
                new_properties.push(ObjectProperty {
                    key: prop.key.clone(),
                    value: new_value,
                    position: prop.position,
                });
                any_changed = true;
            } else {
                new_properties.push(prop.clone());
            }
        }

        if any_changed {
            Ok((
                Value::Object { properties: new_properties, position },
                local_count,
                imported_count,
            ))
        } else {
            Ok((
                Value::Object { properties: properties.to_vec(), position },
                local_count,
                imported_count,
            ))
        }
    }

    fn resolve_enums_in_object_literal(
        &self,
        obj: &Value,
    ) -> Result<(Value, usize, usize), ResolverError> {
        match obj {
            Value::Object { properties, position } => {
                self.resolve_enums_in_object_literal_from_value(properties, *position)
            }
            _ => Ok((obj.clone(), 0, 0)),
        }
    }

    // ==================== PHASE 2: DATA CONTEXT BUILD ====================

    fn build_initial_data_context(&mut self) {
        let data_section = match &self.ast.data {
            Some(d) => d,
            None => return,
        };

        let mut context = self.data_context.borrow_mut();
        let mut total_inserted: usize = 0;

        for entry in &data_section.entries {
            total_inserted += Self::populate_context_from_entry(entry, &mut context);
        }

        drop(context);

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "✓ Data context populated: {} literal entries",
                total_inserted
            ));
        }
    }

    fn populate_context_from_entry(
        entry: &DataEntry,
        context: &mut HashMap<String, DixValue>,
    ) -> usize {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let path = PathBuilder::build(&[name]);
                Self::insert_value_recursive(value, &path, context)
            }

            DataEntry::TableProperty { path: table_path, properties, .. } => {
                let segments: Vec<&str> = table_path.segments.iter().map(|s| s.as_str()).collect();
                let mut count = 0usize;
                for prop in properties {
                    let mut full_segments = segments.clone();
                    full_segments.push(&prop.name);
                    let full = PathBuilder::build(&full_segments);
                    count += Self::insert_value_recursive(&prop.value, &full, context);
                }
                count
            }

            DataEntry::GroupArray { path: group_path, items, .. } => {
                let segments: Vec<&str> = group_path.segments.iter().map(|s| s.as_str()).collect();
                let base = PathBuilder::build(&segments);
                let mut count = 0usize;
                for (i, item) in items.iter().enumerate() {
                    let indexed = format!("{}[{}]", base, i);
                    count += Self::insert_value_recursive(item, &indexed, context);
                }
                count
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let base = PathBuilder::build(&[name]);
                Self::insert_value_recursive(object, &base, context)
            }
        }
    }

    fn insert_value_recursive(
        value: &Value,
        path: &str,
        context: &mut HashMap<String, DixValue>,
    ) -> usize {
        match value {
            Value::Object { properties, .. } => {
                let mut count = 0usize;
                for prop in properties {
                    let child = format!("{}.{}", path, prop.key);
                    count += Self::insert_value_recursive(&prop.value, &child, context);
                }
                if count == properties.len() {
                    if let Some(dix) = Self::try_value_to_dix(value) {
                        context.insert(path.to_string(), dix);
                        count += 1;
                    }
                }
                count
            }

            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                let mut count = 0usize;
                let mut all_leaves_ok = true;

                for (i, item) in values.iter().enumerate() {
                    let indexed = format!("{}[{}]", path, i);
                    let inserted = Self::insert_value_recursive(item, &indexed, context);
                    if inserted == 0 {
                        all_leaves_ok = false;
                    }
                    count += inserted;
                }
                if all_leaves_ok && !values.is_empty() {
                    if let Some(dix) = Self::try_value_to_dix(value) {
                        context.insert(path.to_string(), dix);
                        count += 1;
                    }
                }
                count
            }

            _ => {
                if let Some(dix) = Self::try_value_to_dix(value) {
                    context.insert(path.to_string(), dix);
                    1
                } else {
                    0
                }
            }
        }
    }

    // ==================== PHASE 3: FUNCTION CALL DISCOVERY ====================

    fn find_all_function_calls(&self) -> Vec<FunctionCallInfo> {
        let data_section = match &self.ast.data {
            Some(d) => d,
            None => return Vec::new(),
        };

        let debug_mode = if self.debug_config.is_verbose {
            DebugMode::Verbose
        } else if self.debug_config.is_enabled {
            DebugMode::Regular
        } else {
            DebugMode::Off
        };

        let mut walker = ASTWalker::new(
            self.error_manager.clone(),
            self.symbol_table,
            debug_mode,
        );

        walker.find_all(data_section)
    }

    // ==================== PHASE 4: ITERATIVE RESOLUTION ====================

    fn execute_iterative_resolution(
        &mut self,
        function_calls: Vec<FunctionCallInfo>,
    ) -> (usize, Vec<String>) {
        let total = function_calls.len();
        let dynamic_limit = (total * 3).max(50);
        let absolute_limit = 10_000usize;
        let max_iterations = dynamic_limit.min(absolute_limit);

        let mut resolved_count = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut iteration = 0usize;

        let mut pending: Vec<(FunctionCallInfo, bool)> = function_calls
            .into_iter()
            .map(|call| (call, false))
            .collect();

        loop {
            if !pending.iter().any(|(_, resolved)| !resolved) {
                break;
            }

            iteration += 1;

            if iteration > max_iterations {
                let stuck: Vec<String> = pending.iter()
                    .filter(|(_, resolved)| !resolved)
                    .map(|(call, _)| call.location.clone())
                    .collect();
                errors.push(ResolverError::CircularDependency { stuck_calls: stuck }.to_string());
                break;
            }

            let mut resolved_this_pass = 0usize;

            for i in 0..pending.len() {
                let (ref call, resolved) = pending[i];
                if resolved {
                    continue;
                }

                if self.has_unresolved_dependencies(call) {
                    continue;
                }

                if let Err(e) = self.validate_function_scope(call) {
                    errors.push(e.to_string());
                    pending[i].1 = true;
                    continue;
                }

                let resolved_args = match self.resolve_call_arguments(call) {
                    Ok(a) => a,
                    Err(e) => {
                        errors.push(e.to_string());
                        pending[i].1 = true;
                        continue;
                    }
                };

                let call_start = Instant::now();
                let result = self.execute_call(call, resolved_args);
                let call_duration = call_start.elapsed();

                match result {
                    Ok(dix_value) => {
                        let location = call.location.clone();
                        let fn_name = call.function_name.clone();
                        let pos = call.position;

                        let new_value = Self::convert_dix_value_to_value(&dix_value, pos);

                        self.replace_value_in_ast_by_location(&location, pos, new_value);
                        self.data_context.borrow_mut().insert(location.clone(), dix_value.clone());
                        self.resolved_values.insert(location.clone(), dix_value.clone());

                        self.resolution_history.push(ResolutionRecord {
                            function_name: fn_name,
                            namespace_name: call.namespace_name.clone(),
                            location: location.clone(),
                            scope: call.scope.clone(),
                            arguments: call.arguments.iter().map(|a| format!("{:?}", a)).collect(),
                            result: Some(dix_value),
                            success: true,
                            error_message: String::new(),
                            timestamp: Utc::now(),
                        });

                        if self.debug_config.is_enabled {
                            self.error_manager.log_info(&format!(
                                "✓ [iter {}] {} ({:.3}ms)",
                                iteration, location,
                                call_duration.as_secs_f64() * 1000.0
                            ));
                        }

                        pending[i].1 = true;
                        resolved_count += 1;
                        resolved_this_pass += 1;
                    }

                    Err(interp_err) => {
                        let location = call.location.clone();
                        let fn_name = call.function_name.clone();

                        let resolver_err = ResolverError::ExecutionFailed {
                            function: fn_name.clone(),
                            location: location.clone(),
                            inner: interp_err,
                        };

                        self.resolution_history.push(ResolutionRecord {
                            function_name: fn_name,
                            namespace_name: call.namespace_name.clone(),
                            location,
                            scope: call.scope.clone(),
                            arguments: call.arguments.iter().map(|a| format!("{:?}", a)).collect(),
                            result: None,
                            success: false,
                            error_message: resolver_err.to_string(),
                            timestamp: Utc::now(),
                        });

                        errors.push(resolver_err.to_string());
                        pending[i].1 = true;
                    }
                }
            }

            if resolved_this_pass == 0 && pending.iter().any(|(_, resolved)| !resolved) {
                let stuck: Vec<String> = pending.iter()
                    .filter(|(_, resolved)| !resolved)
                    .map(|(call, _)| call.location.clone())
                    .collect();
                errors.push(ResolverError::CircularDependency { stuck_calls: stuck }.to_string());
                break;
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "[Phase 4] done — {}/{} resolved, {} iterations",
                resolved_count, total, iteration
            ));
        }

        (resolved_count, errors)
    }

    /// Execute a single function call — handles both local and imported (namespaced) functions.
    ///
    /// FIX: For local calls `find_function` returns `&QuickFunction` which borrows
    /// `self.interpreter` immutably.  We then need `&mut self.interpreter` for
    /// `execute()`.  Rust forbids having both at once.
    ///
    /// Solution: clone the `QuickFunction` out of the registry so the immutable
    /// borrow is dropped before `execute()` takes the mutable borrow.
    /// `QuickFunction` derives `Clone`, so this is a one-time, call-site clone
    /// (cold path — only on errors is cloning expensive, and function bodies are
    /// small relative to the total resolution work).
    fn execute_call(
        &mut self,
        call: &FunctionCallInfo,
        arguments: Vec<DixValue>,
    ) -> Result<DixValue, InterpreterError> {
        // Convert DixValue args back to Expression wrappers expected by the interpreter
        let expr_args: Vec<Expression> = arguments.iter().map(|dv| {
            Expression::Value {
                value: Self::convert_dix_value_to_value(dv, call.position),
                position: call.position,
            }
        }).collect();

        match &call.namespace_name {
            // ── IMPORTED (namespaced) function ────────────────────────────────
            Some(ns_name) => {
                // Look up namespace + function; clone the AST to free the borrow.
                let func_ast = {
                    let ns = self.symbol_table.try_get_namespace(ns_name)
                        .ok_or_else(|| InterpreterError::UndefinedFunction {
                            name: format!("{}.{}", ns_name, call.function_name),
                            position: call.position,
                        })?;

                    ns.functions.get(&call.function_name)
                        .ok_or_else(|| InterpreterError::UndefinedFunction {
                            name: format!("{}.{}", ns_name, call.function_name),
                            position: call.position,
                        })?
                        .ast
                        .clone() // ← drop the borrow on symbol_table
                };

                // Also grab the namespace ref separately for passing to execute()
                let ns = self.symbol_table.try_get_namespace(ns_name).unwrap();

                let mut context = ExecutionContext::new(&call.function_name, None);
                self.interpreter.execute(
                    &func_ast,
                    &expr_args,
                    &mut context,
                    &call.scope_context,
                    Some(ns),
                )
            }

            // ── LOCAL function ────────────────────────────────────────────────
            None => {
                // FIX: clone the QuickFunction so the immutable borrow of
                // `self.interpreter` from `find_function` is dropped before
                // `execute()` takes a mutable borrow.
                let function_clone = self.interpreter
                    .find_function(&call.function_name)
                    .ok_or_else(|| InterpreterError::UndefinedFunction {
                        name: call.function_name.clone(),
                        position: call.position,
                    })?
                    .clone(); // ← releases the &self.interpreter borrow

                let mut context = ExecutionContext::new(&call.function_name, None);
                self.interpreter.execute(
                    &function_clone,
                    &expr_args,
                    &mut context,
                    &call.scope_context,
                    None,
                )
            }
        }
    }

    // ==================== PHASE 4 HELPERS ====================

    fn has_unresolved_dependencies(&self, call: &FunctionCallInfo) -> bool {
        let ctx = self.data_context.borrow();
        call.arguments.iter().any(|arg| Self::expr_has_unresolved_ref(arg, &ctx))
    }

    fn expr_has_unresolved_ref(expr: &Expression, ctx: &HashMap<String, DixValue>) -> bool {
        match expr {
            Expression::Identifier { name, .. } => !ctx.contains_key(name),
            Expression::QuickFuncCall { .. } => true,
            Expression::ImportedFunctionCall { .. } => true,
            Expression::ArithmeticOp { left, right, .. } => {
                Self::expr_has_unresolved_ref(left, ctx)
                    || Self::expr_has_unresolved_ref(right, ctx)
            }
            Expression::Value { value, .. } => Self::value_has_unresolved_ref(value, ctx),
            Expression::PropertyAccess { object, .. } => {
                Self::expr_has_unresolved_ref(object, ctx)
            }
            Expression::IndexAccess { object, index, .. } => {
                Self::expr_has_unresolved_ref(object, ctx)
                    || Self::expr_has_unresolved_ref(index, ctx)
            }
            _ => false,
        }
    }

    fn value_has_unresolved_ref(value: &Value, ctx: &HashMap<String, DixValue>) -> bool {
        match value {
            Value::Identifier { value: id, .. } => !ctx.contains_key(id),
            Value::Expression { expr, .. } => Self::expr_has_unresolved_ref(expr, ctx),
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                values.iter().any(|v| Self::value_has_unresolved_ref(v, ctx))
            }
            Value::Object { properties, .. } => properties
                .iter()
                .any(|p| Self::value_has_unresolved_ref(&p.value, ctx)),
            Value::PrefixedConstructor { arguments, .. } => {
                arguments.iter().any(|a| Self::value_has_unresolved_ref(a, ctx))
            }
            _ => false,
        }
    }

    fn validate_function_scope(&self, call: &FunctionCallInfo) -> Result<(), ResolverError> {
        match &call.namespace_name {
            Some(ns_name) => {
                let ns = self.symbol_table
                    .try_get_namespace(ns_name)
                    .ok_or_else(|| ResolverError::NamespaceNotFound {
                        name: ns_name.clone(),
                        location: call.location.clone(),
                        position: call.position,
                    })?;

                if !ns.functions.contains_key(&call.function_name) {
                    return Err(ResolverError::FunctionNotInNamespace {
                        namespace: ns_name.clone(),
                        function: call.function_name.clone(),
                        location: call.location.clone(),
                        position: call.position,
                    });
                }

                Ok(())
            }
            None => {
                if self.interpreter.find_function(&call.function_name).is_none() {
                    return Err(ResolverError::FunctionNotFound {
                        name: call.function_name.clone(),
                        location: call.location.clone(),
                        position: call.position,
                    });
                }
                Ok(())
            }
        }
    }

    fn resolve_call_arguments(
        &self,
        call: &FunctionCallInfo,
    ) -> Result<Vec<DixValue>, ResolverError> {
        let ctx = self.data_context.borrow();
        let mut out = Vec::with_capacity(call.arguments.len());

        for arg in &call.arguments {
            out.push(Self::resolve_expr_to_dix(arg, &ctx, call.position)?);
        }

        Ok(out)
    }

    fn resolve_expr_to_dix(
        expr: &Expression,
        ctx: &HashMap<String, DixValue>,
        call_pos: Position,
    ) -> Result<DixValue, ResolverError> {
        match expr {
            Expression::Value { value, .. } => {
                Self::resolve_value_to_dix(value, ctx, call_pos)
            }
            Expression::Identifier { name, .. } => {
                ctx.get(name).cloned().ok_or_else(|| ResolverError::Fatal {
                    message: format!(
                        "identifier '{}' missing from context at {}",
                        name, call_pos
                    ),
                })
            }
            _ => Err(ResolverError::Fatal {
                message: format!("cannot resolve expr to DixValue at {}", call_pos),
            }),
        }
    }

    fn resolve_value_to_dix(
        value: &Value,
        ctx: &HashMap<String, DixValue>,
        call_pos: Position,
    ) -> Result<DixValue, ResolverError> {
        match value {
            Value::Integer { value, .. } => Ok(DixValue::from_int(*value)),
            Value::Float { value, .. } => Ok(DixValue::from_float(*value)),
            Value::Double { value, .. } => Ok(DixValue::from_double(*value)),
            Value::String { value, .. } => Ok(DixValue::from_string(value.clone())),
            Value::Boolean { value, .. } => Ok(DixValue::from_bool(*value)),
            Value::Null { .. } => Ok(DixValue::null()),
            Value::Identifier { value: id, .. } => {
                ctx.get(id).cloned().ok_or_else(|| ResolverError::Fatal {
                    message: format!("identifier '{}' missing at {}", id, call_pos),
                })
            }
            Value::Array { values, .. } => {
                let items: Result<Vec<DixValue>, ResolverError> = values
                    .iter()
                    .map(|v| Self::resolve_value_to_dix(v, ctx, call_pos))
                    .collect();
                Ok(DixValue::from_array(items?))
            }
            Value::Object { properties, .. } => {
                let mut map = HashMap::with_capacity(properties.len());
                for prop in properties {
                    let val = Self::resolve_value_to_dix(&prop.value, ctx, call_pos)?;
                    map.insert(prop.key.clone(), val);
                }
                Ok(DixValue::from_object(map))
            }
            _ => Err(ResolverError::Fatal {
                message: format!(
                    "cannot convert value variant to DixValue at {}",
                    call_pos
                ),
            }),
        }
    }

    fn replace_value_in_ast_by_location(
        &mut self,
        location: &str,
        target_position: Position,
        new_value: Value,
    ) {
        let data = match self.ast.data.as_mut() {
            Some(d) => d,
            None => return,
        };

        for entry in &mut data.entries {
            if Self::replace_in_entry(entry, target_position, &new_value) {
                return;
            }
        }
    }

    fn replace_in_entry(
        entry: &mut DataEntry,
        target: Position,
        new_value: &Value,
    ) -> bool {
        match entry {
            DataEntry::SimpleProperty { value, .. } => {
                Self::replace_in_value(value, target, new_value)
            }
            DataEntry::TableProperty { properties, .. } => {
                for prop in properties.iter_mut() {
                    if Self::replace_in_value(&mut prop.value, target, new_value) {
                        return true;
                    }
                }
                false
            }
            DataEntry::GroupArray { items, .. } => {
                for item in items.iter_mut() {
                    if Self::replace_in_value(item, target, new_value) {
                        return true;
                    }
                }
                false
            }
            DataEntry::ObjectProperty { object, .. } => {
                Self::replace_in_value(object, target, new_value)
            }
        }
    }

    fn replace_in_value(
        value: &mut Value,
        target: Position,
        new_value: &Value,
    ) -> bool {
        if Self::value_position(value) == Some(target) {
            *value = new_value.clone();
            return true;
        }

        match value {
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                for item in values.iter_mut() {
                    if Self::replace_in_value(item, target, new_value) {
                        return true;
                    }
                }
            }
            Value::Object { properties, .. } => {
                for prop in properties.iter_mut() {
                    if Self::replace_in_value(&mut prop.value, target, new_value) {
                        return true;
                    }
                }
            }
            Value::PrefixedConstructor { arguments, .. } => {
                for arg in arguments.iter_mut() {
                    if Self::replace_in_value(arg, target, new_value) {
                        return true;
                    }
                }
            }
            _ => {}
        }

        false
    }

    fn value_position(value: &Value) -> Option<Position> {
        match value {
            Value::Integer { position, .. }
            | Value::Float { position, .. }
            | Value::Double { position, .. }
            | Value::String { position, .. }
            | Value::Boolean { position, .. }
            | Value::Null { position, .. }
            | Value::Array { position, .. }
            | Value::NestedArray { position, .. }
            | Value::Object { position, .. }
            | Value::Expression { position, .. }
            | Value::Identifier { position, .. }
            | Value::EnumValue { position, .. }
            | Value::PrefixedConstructor { position, .. }
            | Value::HexColor { position, .. }
            | Value::Date { position, .. }
            | Value::Timestamp { position, .. }
            | Value::QuickFuncCall { position, .. }
            | Value::InterpolatedString { position, .. }
            | Value::Range { position, .. }
            | Value::Lambda { position, .. }
            | Value::ParseError { position, .. }
            | Value::Error { position, .. }
            | Value::ScientificNotation { position, .. }
            | Value::Unknown { position, .. } => Some(*position),
        }
    }

    // ==================== PHASE 5: IDENTIFIER RESOLUTION ====================

    fn resolve_remaining_identifiers(&mut self) {
        let max_passes = 64usize;
        let mut total_resolved = 0usize;
        let mut final_skipped = 0usize;

        for pass in 1..=max_passes {
            let (new_entries, resolved_count, skipped_count, newly_resolved) = {
                let data = match self.ast.data.as_ref() {
                    Some(d) => d,
                    None => break,
                };
                let ctx = self.data_context.borrow();

                let mut newly_resolved: Vec<(String, DixValue)> = Vec::new();
                let mut resolved_count = 0usize;
                let mut skipped_count = 0usize;
                let mut new_entries = Vec::with_capacity(data.entries.len());

                for entry in &data.entries {
                    let (new_entry, res, skip) = Self::resolve_identifiers_in_entry(
                        entry,
                        &ctx,
                        &mut newly_resolved,
                    );
                    new_entries.push(new_entry);
                    resolved_count += res;
                    skipped_count += skip;
                }

                (new_entries, resolved_count, skipped_count, newly_resolved)
            };

            if resolved_count > 0 {
                let position = self.ast.data.as_ref().unwrap().position;
                self.ast.data = Some(DataSection {
                    entries: new_entries,
                    position,
                });

                {
                    let mut ctx = self.data_context.borrow_mut();
                    for (path, dix) in newly_resolved {
                        ctx.insert(path, dix);
                    }
                }
            }

            total_resolved += resolved_count;
            final_skipped = skipped_count;

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "[Phase 5, pass {}] resolved {}, {} still pending",
                    pass, resolved_count, skipped_count
                ));
            }

            if resolved_count == 0 {
                break;
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "✓ Identifier resolution: {} resolved, {} left (external/runtime)",
                total_resolved, final_skipped
            ));
        }
    }

    fn resolve_identifiers_in_entry(
        entry: &DataEntry,
        ctx: &HashMap<String, DixValue>,
        newly_resolved: &mut Vec<(String, DixValue)>,
    ) -> (DataEntry, usize, usize) {
        match entry {
            DataEntry::SimpleProperty { name, data_type, value, position } => {
                let base = PathBuilder::build(&[name]);
                let (new_value, res, skip) =
                    Self::resolve_identifiers_in_value(value, &base, ctx, newly_resolved);

                if res > 0 {
                    (
                        DataEntry::SimpleProperty {
                            name: name.clone(),
                            data_type: *data_type,
                            value: new_value,
                            position: *position,
                        },
                        res,
                        skip,
                    )
                } else {
                    (entry.clone(), 0, skip)
                }
            }

            DataEntry::TableProperty { path: table_path, properties, position } => {
                let segments: Vec<&str> =
                    table_path.segments.iter().map(|s| s.as_str()).collect();
                let mut new_props = Vec::with_capacity(properties.len());
                let mut total_res = 0usize;
                let mut total_skip = 0usize;
                let mut any_changed = false;

                for prop in properties {
                    let mut full_segs = segments.clone();
                    full_segs.push(&prop.name);
                    let full = PathBuilder::build(&full_segs);
                    let (new_value, res, skip) = Self::resolve_identifiers_in_value(
                        &prop.value,
                        &full,
                        ctx,
                        newly_resolved,
                    );
                    total_res += res;
                    total_skip += skip;

                    if res > 0 {
                        new_props.push(PropertyAssignment {
                            name: prop.name.clone(),
                            data_type: prop.data_type,
                            value: new_value,
                            position: prop.position,
                        });
                        any_changed = true;
                    } else {
                        new_props.push(prop.clone());
                    }
                }

                if any_changed {
                    (
                        DataEntry::TableProperty {
                            path: table_path.clone(),
                            properties: new_props,
                            position: *position,
                        },
                        total_res,
                        total_skip,
                    )
                } else {
                    (entry.clone(), 0, total_skip)
                }
            }

            DataEntry::GroupArray { path: group_path, items, position } => {
                let segments: Vec<&str> =
                    group_path.segments.iter().map(|s| s.as_str()).collect();
                let base = PathBuilder::build(&segments);
                let mut new_items = Vec::with_capacity(items.len());
                let mut total_res = 0usize;
                let mut total_skip = 0usize;
                let mut any_changed = false;

                for (i, item) in items.iter().enumerate() {
                    let indexed = format!("{}[{}]", base, i);
                    let (new_value, res, skip) = Self::resolve_identifiers_in_value(
                        item,
                        &indexed,
                        ctx,
                        newly_resolved,
                    );
                    total_res += res;
                    total_skip += skip;

                    if res > 0 {
                        new_items.push(new_value);
                        any_changed = true;
                    } else {
                        new_items.push(item.clone());
                    }
                }

                if any_changed {
                    (
                        DataEntry::GroupArray {
                            path: group_path.clone(),
                            items: new_items,
                            position: *position,
                        },
                        total_res,
                        total_skip,
                    )
                } else {
                    (entry.clone(), 0, total_skip)
                }
            }

            DataEntry::ObjectProperty { name, data_type, object, position } => {
                let base = PathBuilder::build(&[name]);
                let (new_obj, res, skip) =
                    Self::resolve_identifiers_in_value(object, &base, ctx, newly_resolved);

                if res > 0 {
                    (
                        DataEntry::ObjectProperty {
                            name: name.clone(),
                            data_type: *data_type,
                            object: Box::from(new_obj),
                            position: *position,
                        },
                        res,
                        skip,
                    )
                } else {
                    (entry.clone(), 0, skip)
                }
            }
        }
    }

    fn resolve_identifiers_in_value(
        value: &Value,
        path: &str,
        ctx: &HashMap<String, DixValue>,
        newly_resolved: &mut Vec<(String, DixValue)>,
    ) -> (Value, usize, usize) {
        match value {
            Value::Identifier { value: id_value, position } => {
                match ctx.get(id_value.as_str()) {
                    Some(dix) => {
                        let new_val = Self::convert_dix_value_to_value(dix, *position);
                        newly_resolved.push((path.to_string(), dix.clone()));
                        (new_val, 1, 0)
                    }
                    None => (value.clone(), 0, 1),
                }
            }

            Value::Array { values, position } | Value::NestedArray { values, position, .. } => {
                let mut new_values = Vec::with_capacity(values.len());
                let mut total_res = 0usize;
                let mut total_skip = 0usize;
                let mut any_changed = false;

                for (i, item) in values.iter().enumerate() {
                    let indexed = format!("{}[{}]", path, i);
                    let (nv, res, skip) = Self::resolve_identifiers_in_value(
                        item,
                        &indexed,
                        ctx,
                        newly_resolved,
                    );
                    total_res += res;
                    total_skip += skip;
                    if res > 0 {
                        any_changed = true;
                    }
                    new_values.push(if res > 0 { nv } else { item.clone() });
                }

                if any_changed {
                    (
                        Value::Array { values: new_values, position: *position },
                        total_res,
                        total_skip,
                    )
                } else {
                    (value.clone(), 0, total_skip)
                }
            }

            Value::Object { properties, position } => {
                let mut new_props = Vec::with_capacity(properties.len());
                let mut total_res = 0usize;
                let mut total_skip = 0usize;
                let mut any_changed = false;

                for prop in properties {
                    let child = format!("{}.{}", path, prop.key);
                    let (nv, res, skip) = Self::resolve_identifiers_in_value(
                        &prop.value,
                        &child,
                        ctx,
                        newly_resolved,
                    );
                    total_res += res;
                    total_skip += skip;

                    if res > 0 {
                        new_props.push(ObjectProperty {
                            key: prop.key.clone(),
                            value: nv,
                            position: prop.position,
                        });
                        any_changed = true;
                    } else {
                        new_props.push(prop.clone());
                    }
                }

                if any_changed {
                    (
                        Value::Object { properties: new_props, position: *position },
                        total_res,
                        total_skip,
                    )
                } else {
                    (value.clone(), 0, total_skip)
                }
            }

            Value::PrefixedConstructor { prefix, arguments, position } => {
                let mut new_args = Vec::with_capacity(arguments.len());
                let mut total_res = 0usize;
                let mut total_skip = 0usize;
                let mut any_changed = false;

                for (i, arg) in arguments.iter().enumerate() {
                    let arg_path = format!("{}.__arg{}", path, i);
                    let (nv, res, skip) = Self::resolve_identifiers_in_value(
                        arg,
                        &arg_path,
                        ctx,
                        newly_resolved,
                    );
                    total_res += res;
                    total_skip += skip;
                    if res > 0 {
                        any_changed = true;
                    }
                    new_args.push(if res > 0 { nv } else { arg.clone() });
                }

                if any_changed {
                    (
                        Value::PrefixedConstructor {
                            prefix: prefix.clone(),
                            arguments: new_args,
                            position: *position,
                        },
                        total_res,
                        total_skip,
                    )
                } else {
                    (value.clone(), 0, total_skip)
                }
            }

            _ => (value.clone(), 0, 0),
        }
    }

    // ==================== VALUE ↔ DIX CONVERSIONS ====================

    fn try_value_to_dix(value: &Value) -> Option<DixValue> {
        match value {
            Value::Integer { value, .. } => Some(DixValue::from_int(*value)),
            Value::Float { value, .. } => Some(DixValue::from_float(*value)),
            Value::Double { value, .. } => Some(DixValue::from_double(*value)),
            Value::String { value, .. } => Some(DixValue::from_string(value.clone())),
            Value::Boolean { value, .. } => Some(DixValue::from_bool(*value)),
            Value::Null { .. } => Some(DixValue::null()),

            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                let items: Option<Vec<DixValue>> =
                    values.iter().map(|v| Self::try_value_to_dix(v)).collect();
                items.map(DixValue::from_array)
            }

            Value::Object { properties, .. } => {
                let mut map = HashMap::with_capacity(properties.len());
                for prop in properties {
                    map.insert(prop.key.clone(), Self::try_value_to_dix(&prop.value)?);
                }
                Some(DixValue::from_object(map))
            }

            _ => None,
        }
    }

    fn convert_dix_value_to_value(dix: &DixValue, position: Position) -> Value {
        match dix.get_type() {
            crate::Builtins::Core::DixType::Int => {
                Value::Integer { value: dix.as_int(), position }
            }
            crate::Builtins::Core::DixType::Float => {
                Value::Float { value: dix.as_float(), position }
            }
            crate::Builtins::Core::DixType::Double => {
                Value::Double { value: dix.as_double(), position }
            }
            crate::Builtins::Core::DixType::String => {
                Value::String { value: dix.as_string(), position }
            }
            crate::Builtins::Core::DixType::Bool => {
                Value::Boolean { value: dix.as_bool(), position }
            }
            crate::Builtins::Core::DixType::Null => Value::Null { position },
            crate::Builtins::Core::DixType::Array
            | crate::Builtins::Core::DixType::Tuple => {
                let values: Vec<Value> = dix
                    .as_array()
                    .iter()
                    .map(|item| Self::convert_dix_value_to_value(item, position))
                    .collect();
                Value::Array { values, position }
            }
            crate::Builtins::Core::DixType::Object => {
                let properties: Vec<ObjectProperty> = dix
                    .as_object()
                    .iter()
                    .map(|(key, val)| ObjectProperty {
                        key: key.clone(),
                        value: Self::convert_dix_value_to_value(val, position),
                        position,
                    })
                    .collect();
                Value::Object { properties, position }
            }
            _ => Value::String {
                value: format!("{:?}", dix),
                position,
            },
        }
    }

    // ==================== DIAGNOSTIC UTILITIES ====================

    fn dump_data_context(&self) {
        let ctx = self.data_context.borrow();
        self.error_manager.log_info("[DIAGNOSTIC] ── data_context dump ──");
        self.error_manager
            .log_info(&format!("  entries: {}", ctx.len()));

        let mut keys: Vec<&String> = ctx.keys().collect();
        keys.sort_unstable();

        for key in keys {
            if self.debug_config.is_verbose {
                self.error_manager
                    .log_debug(&format!("  {} = {:?}", key, ctx[key]));
            } else {
                let repr = format!("{:?}", ctx[key]);
                let truncated = if repr.len() > 80 {
                    format!("{}…", &repr[..77])
                } else {
                    repr
                };
                self.error_manager
                    .log_info(&format!("  {} = {}", key, truncated));
            }
        }
    }

    fn log_function_call_breakdown(&self, calls: &[FunctionCallInfo]) {
        let local_count = calls.iter().filter(|c| c.namespace_name.is_none()).count();
        let ns_count = calls.iter().filter(|c| c.namespace_name.is_some()).count();

        self.error_manager.log_info(&format!(
            "[DIAGNOSTIC]   local calls:      {}",
            local_count
        ));
        self.error_manager.log_info(&format!(
            "[DIAGNOSTIC]   namespaced calls: {}",
            ns_count
        ));

        if self.debug_config.is_verbose {
            for call in calls {
                let prefix = call
                    .namespace_name
                    .as_ref()
                    .map(|ns| format!("{}.", ns))
                    .unwrap_or_default();
                self.error_manager.log_debug(&format!(
                    "[DIAGNOSTIC]     ~{}{}()  →  {}",
                    prefix, call.function_name, call.location
                ));
            }
        }
    }

    fn create_failed_result(&self, errors: Vec<String>) -> ValueResolutionResult {
        ValueResolutionResult {
            is_success: false,
            original_ast: Some(self.ast.clone()),
            resolved_ast: Some(self.ast.clone()),
            function_calls_resolved: 0,
            errors,
            log_statements: self.log_statements.clone(),
            resolution_duration: self.start_time.elapsed(),
            resolution_history: self.resolution_history.clone(),
        }
    }
}