//! Orchestrates compile-time value resolution across five phases.
//!
//! Phase 1: Enum pre-resolution — EnumValue/EnumAccess → Integer.
//! Phase 2: Initial data context build from literal DATA entries.
//! Phase 3: QuickFunction call discovery via ASTWalker.
//! Phase 4: Iterative execution and AST replacement.
//! Phase 5: Remaining Identifier reference resolution.

use std::cell::RefCell;
use std::rc::Rc;
use web_time::Instant;

use chrono::Utc;
use rustc_hash::FxHashMap;

use crate::Builtins::Core::{DixType, DixValue};
use crate::Builtins::Resolver;
use crate::Compiler::AST::{
    DataEntry, DataSection, DixScript, Expression, ObjectProperty, Position,
    PropertyAssignment, Value,
};
use crate::Compiler::Core::DebugMode;
use crate::Compiler::Utilities::{PathBuilder, SymbolTable, ImportedNamespace};
use crate::ErrorManager::{DebugConfig, ErrorManager};

use super::ast_walker::ASTWalker;
use super::execution_context::ExecutionContext;
use super::function_interpreter::{FunctionInterpreter, InterpreterError};
use super::supporting_classes::{FunctionCallInfo, ResolutionRecord, ValueResolutionResult};

const MAX_RESOLUTION_ITERATIONS: usize = 10_000;
const MIN_CAPACITY: usize = 8;

// ==================== RESOLVER ERROR ====================

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
            ResolverError::NamespaceNotFound { name, location, position } => {
                write!(f, "Namespace '{}' not found at {} ({})", name, location, position)
            }
            ResolverError::FunctionNotInNamespace { namespace, function, location, position } => {
                write!(
                    f,
                    "Function '{}' not found in namespace '{}' at {} ({})",
                    function, namespace, location, position
                )
            }
            ResolverError::InvalidEnumAccess { location, message, .. } => {
                write!(f, "Invalid enum access at {}: {}", location, message)
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
                write!(f, "Circular dependency: {} calls unresolvable", stuck_calls.len())
            }
            ResolverError::ExecutionFailed { function, location, inner } => {
                write!(f, "Execution of '{}' at {} failed: {}", function, location, inner)
            }
            ResolverError::Fatal { message } => write!(f, "Fatal resolver error: {}", message),
        }
    }
}

impl std::error::Error for ResolverError {}

// ==================== VALUE RESOLVER ====================

pub struct ValueResolver<'a> {
    ast: DixScript,
    symbol_table: &'a SymbolTable,
    interpreter: FunctionInterpreter<'a>,
    data_context: Rc<RefCell<FxHashMap<String, DixValue>>>,
    debug_config: DebugConfig,
    resolved_values: FxHashMap<String, DixValue>,
    log_statements: Vec<String>,
    resolution_history: Vec<ResolutionRecord>,
    start_time: Instant,
    error_manager: ErrorManager,
}

impl<'a> ValueResolver<'a> {
    pub fn new(
        ast: DixScript,
        symbol_table: &'a SymbolTable,
        debug_mode: DebugMode,
    ) -> Self {
        let error_manager = ErrorManager::get_shared_instance();
Self::new_with_error_manager(ast,symbol_table,debug_mode,error_manager)
    }

    pub fn new_with_error_manager(
        ast: DixScript,
        symbol_table: &'a SymbolTable,
        debug_mode: DebugMode,
        error_manager:ErrorManager
    ) -> Self {

        let debug_config = DebugConfig::from_debug_mode(debug_mode);

        let func_count = ast
            .quick_functions
            .as_ref()
            .map(|qf| qf.functions.len())
            .unwrap_or(0);

        let quick_functions = ast
            .quick_functions
            .as_ref()
            .map(|qf| qf.functions.to_vec())
            .unwrap_or_default();

        let data_entry_count = ast
            .data
            .as_ref()
            .map(|d| d.entries.len())
            .unwrap_or(0);

        let data_context: Rc<RefCell<FxHashMap<String, DixValue>>> = Rc::new(RefCell::new(
            FxHashMap::with_capacity_and_hasher(
                (data_entry_count * 4).max(MIN_CAPACITY),
                Default::default(),
            ),
        ));

        let interpreter = FunctionInterpreter::new_with_error_manager(
            symbol_table,
            quick_functions,
            Rc::clone(&data_context),
            debug_mode,
           error_manager.clone()
        );

        Resolver::initialize();

        if debug_config.is_enabled {
            error_manager.log_info("ValueResolver initialized");
        }

        ValueResolver {
            ast,
            symbol_table,
            interpreter,
            data_context,
            debug_config,
            resolved_values: FxHashMap::with_capacity_and_hasher(
                func_count.max(MIN_CAPACITY),
                Default::default(),
            ),
            log_statements: Vec::new(),
            resolution_history: Vec::new(),
            start_time: Instant::now(),
            error_manager,
        }
    }
    // ==================== MAIN ORCHESTRATION ====================

    pub fn resolve(mut self) -> ValueResolutionResult {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("[Phase 4.1] Starting value resolution");
        }

        let original_ast = self.ast.clone();

        if let Err(e) = self.resolve_all_enum_values() {
            return self.create_failed_result(vec![e.to_string()], original_ast);
        }

        self.build_initial_data_context();

        if self.debug_config.is_enabled {
            self.dump_data_context();
        }

        let function_calls = self.find_all_function_calls();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "[DIAGNOSTIC] Found {} function calls to resolve",
                function_calls.len()
            ));
            self.log_function_call_breakdown(&function_calls);
        }

        if function_calls.is_empty() {
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_warning("[DIAGNOSTIC] No function calls found in DATA section");
            }
            self.resolve_remaining_identifiers();
            let logs = self.interpreter.take_logs();
            self.log_statements.extend(logs);
            let duration = self.start_time.elapsed();
            return ValueResolutionResult {
                is_success: true,
                original_ast: Some(original_ast),
                resolved_ast: Some(self.ast),
                function_calls_resolved: 0,
                errors: Vec::new(),
                log_statements: self.log_statements,
                resolution_duration: duration,
                resolution_history: Vec::new(),
            };
        }

        let (success_count, errors) = self.execute_iterative_resolution(function_calls);

        let interpreter_logs = self.interpreter.take_logs();
        self.log_statements.extend(interpreter_logs);

        if errors.is_empty() && success_count > 0 {
            self.resolve_remaining_identifiers();
        }

        let duration = self.start_time.elapsed();

        if self.debug_config.is_enabled {
            self.error_manager.log_info("[Phase 4.1] Resolution complete");
            self.error_manager.log_info(&format!("  Resolved: {}", success_count));
            self.error_manager.log_info(&format!("  Failed:   {}", errors.len()));
            self.error_manager.log_info(&format!(
                "  Duration: {:.3}ms",
                duration.as_secs_f64() * 1000.0
            ));
        }

        ValueResolutionResult {
            is_success: errors.is_empty(),
            original_ast: Some(original_ast),
            resolved_ast: Some(self.ast),
            function_calls_resolved: success_count,
            errors,
            log_statements: self.log_statements,
            resolution_duration: duration,
            resolution_history: self.resolution_history,
        }
    }

    // ==================== PHASE 1: ENUM PRE-RESOLUTION ====================

    fn resolve_all_enum_values(&mut self) -> Result<(), ResolverError> {
        if self.ast.data.is_none() {
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_debug("No DATA section — skipping enum pre-resolution");
            }
            return Ok(());
        }

        if self.debug_config.is_enabled {
            self.error_manager
                .log_info("Pre-processing: resolving all enum values to integers");
        }

        let (entries_snapshot, section_position) = {
            let data = self.ast.data.as_ref().unwrap();
            (data.entries.clone(), data.position)
        };

        let cap = entries_snapshot.len().max(MIN_CAPACITY);
        let mut new_entries = Vec::with_capacity(cap);
        let mut local_count = 0usize;
        let mut imported_count = 0usize;

        for entry in &entries_snapshot {
            let (new_entry, lc, ic) = self.resolve_enums_in_entry(entry)?;
            new_entries.push(new_entry);
            local_count += lc;
            imported_count += ic;
        }

        self.ast.data = Some(DataSection {
            entries: new_entries,
            position: section_position,
        });

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Resolved {} local + {} imported enum values",
                local_count, imported_count
            ));
        }

        Ok(())
    }

    fn resolve_enums_in_entry(
        &self,
        entry: &DataEntry,
    ) -> Result<(DataEntry, usize, usize), ResolverError> {
        match entry {
            DataEntry::SimpleProperty { name, data_type, value, position } => {
                let (new_value, lc, ic) = self.resolve_enums_in_value(value)?;
                if lc + ic > 0 {
                    Ok((
                        DataEntry::SimpleProperty {
                            name: name.clone(),
                            data_type: *data_type,
                            value: new_value,
                            position: *position,
                        },
                        lc,
                        ic,
                    ))
                } else {
                    Ok((entry.clone(), 0, 0))
                }
            }

            DataEntry::TableProperty { path, properties, position } => {
                let mut new_properties = Vec::with_capacity(properties.len().max(MIN_CAPACITY));
                let mut any_changed = false;
                let mut total_lc = 0usize;
                let mut total_ic = 0usize;

                for prop in properties {
                    let (new_value, lc, ic) = self.resolve_enums_in_value(&prop.value)?;
                    total_lc += lc;
                    total_ic += ic;
                    if lc + ic > 0 {
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
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((entry.clone(), 0, 0))
                }
            }

            DataEntry::GroupArray { path, items, position } => {
                let mut new_items = Vec::with_capacity(items.len().max(MIN_CAPACITY));
                let mut any_changed = false;
                let mut total_lc = 0usize;
                let mut total_ic = 0usize;

                for item in items {
                    let (new_value, lc, ic) = self.resolve_enums_in_value(item)?;
                    total_lc += lc;
                    total_ic += ic;
                    if lc + ic > 0 {
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
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((entry.clone(), 0, 0))
                }
            }

            DataEntry::ObjectProperty { name, data_type, object, position } => {
                let (new_obj, lc, ic) = self.resolve_enums_in_object_literal(object)?;
                if lc + ic > 0 {
                    Ok((
                        DataEntry::ObjectProperty {
                            name: name.clone(),
                            data_type: *data_type,
                            object: Box::from(new_obj),
                            position: *position,
                        },
                        lc,
                        ic,
                    ))
                } else {
                    Ok((entry.clone(), 0, 0))
                }
            }
        }
    }

    fn resolve_enums_in_value(
        &self,
        value: &Value,
    ) -> Result<(Value, usize, usize), ResolverError> {
        match value {
            // Direct enum value reference at a leaf/data position (a bare
            // `Enum.FIELD` sitting as a field's value, table property, group
            // array item, or array element -- resolve_enums_in_entry calls
            // this function directly on those). Validate the reference
            // exactly as before (same lookups, same error paths for a bad
            // namespace/enum/field), but do NOT collapse to Value::Integer.
            //
            // BUGFIX: this used to always return Value::Integer here,
            // discarding enum_name/field_name. That's correct for enums used
            // in actual *computation* (arithmetic, QuickFunc call arguments)
            // where a concrete int is genuinely needed -- but those go
            // through the separate Expression::EnumAccess arms in
            // resolve_enums_in_expr below, not this one. This arm only ever
            // fires for enums sitting as plain data, where collapsing to a
            // bare Integer meant every downstream consumer that cares about
            // enum identity (DixData::from_ast's DixValue::Enum
            // construction, the mdix-ffi mdix_get_enum_name/
            // mdix_get_enum_field FFI exports, Runtime/schema.rs's
            // ExpectedValueType::Enum validation) silently stopped working
            // for any file whose Stage 7 (Runtime/loader.rs) happened to run
            // -- which is any file with a QuickFunc anywhere in scope,
            // whether or not that QuickFunc has anything to do with the enum
            // field in question. Leaving the node intact here means all of
            // those consumers see real enum identity again, and
            // Compiler/Core/BinarySerialization/value_encoder.rs's
            // ValueTypeTag::Enum wire format (see encode_enum) can actually
            // tag it correctly in binary output too, instead of it having
            // already been silently flattened to a plain int before the
            // encoder ever saw it.
            Value::EnumValue { enum_name, value: enum_field, position } => {
                if let Some(dot) = enum_name.find('.') {
                    let ns_name = &enum_name[..dot];
                    let actual_enum = &enum_name[dot + 1..];
                    let ns = self
                        .symbol_table
                        .try_get_namespace(ns_name)
                        .ok_or_else(|| ResolverError::InvalidEnumAccess {
                            location: format!("{}.{}.{}", ns_name, actual_enum, enum_field),
                            message: format!("Namespace '{}' not found", ns_name),
                            position: *position,
                        })?;
                    let fields = ns.enums.get(actual_enum).ok_or_else(|| {
                        ResolverError::InvalidEnumAccess {
                            location: format!("{}.{}.{}", ns_name, actual_enum, enum_field),
                            message: format!("Enum '{}' not found", actual_enum),
                            position: *position,
                        }
                    })?;
                    fields.get(enum_field.as_str()).ok_or_else(|| {
                        ResolverError::InvalidEnumAccess {
                            location: format!("{}.{}.{}", ns_name, actual_enum, enum_field),
                            message: format!("Field '{}' not found", enum_field),
                            position: *position,
                        }
                    })?;
                    return Ok((value.clone(), 0, 1));
                }

                self.symbol_table
                    .try_get_enum_field_value(enum_name, enum_field)
                    .ok_or_else(|| ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}", enum_name, enum_field),
                        message: format!("Enum field '{}' not found", enum_field),
                        position: *position,
                    })?;
                Ok((value.clone(), 1, 0))
            }

            // Expression wrapper — delegate to resolve_enums_in_expr for full recursion.
            Value::Expression { expr, position } => {
                let (new_expr, lc, ic) = self.resolve_enums_in_expr(expr.as_ref())?;
                if lc + ic > 0 {
                    Ok((
                        Value::Expression {
                            expr: Box::new(new_expr),
                            position: *position,
                        },
                        lc,
                        ic,
                    ))
                } else {
                    Ok((value.clone(), 0, 0))
                }
            }

            // QuickFunction call — resolve enums in every argument expression.
            // This is the primary fix: enums passed as QuickFunc args in @DATA are
            // now resolved to integers before the ASTWalker collects call sites.
            Value::QuickFuncCall { function_name, arguments, position } => {
                let mut new_args = Vec::with_capacity(arguments.len().max(MIN_CAPACITY));
                let mut any_changed = false;
                let mut total_lc = 0usize;
                let mut total_ic = 0usize;

                for arg in arguments {
                    let (new_arg, lc, ic) = self.resolve_enums_in_expr(arg)?;
                    total_lc += lc;
                    total_ic += ic;
                    if lc + ic > 0 {
                        new_args.push(new_arg);
                        any_changed = true;
                    } else {
                        new_args.push(arg.clone());
                    }
                }

                if any_changed {
                    Ok((
                        Value::QuickFuncCall {
                            function_name: function_name.clone(),
                            arguments: new_args,
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((value.clone(), 0, 0))
                }
            }

            Value::Array { values, position }
            | Value::NestedArray { values, position, .. } => {
                let mut new_values = Vec::with_capacity(values.len().max(MIN_CAPACITY));
                let mut any_changed = false;
                let mut total_lc = 0usize;
                let mut total_ic = 0usize;

                for item in values {
                    let (nv, lc, ic) = self.resolve_enums_in_value(item)?;
                    total_lc += lc;
                    total_ic += ic;
                    if lc + ic > 0 {
                        new_values.push(nv);
                        any_changed = true;
                    } else {
                        new_values.push(item.clone());
                    }
                }

                if any_changed {
                    Ok((Value::Array { values: new_values, position: *position }, total_lc, total_ic))
                } else {
                    Ok((value.clone(), 0, 0))
                }
            }

            Value::Object { properties, position } => {
                let (new_obj, lc, ic) =
                    self.resolve_enums_in_properties(properties, *position)?;
                if lc + ic > 0 {
                    Ok((new_obj, lc, ic))
                } else {
                    Ok((value.clone(), 0, 0))
                }
            }

            Value::PrefixedConstructor { prefix, arguments, position } => {
                let mut new_args = Vec::with_capacity(arguments.len().max(MIN_CAPACITY));
                let mut any_changed = false;
                let mut total_lc = 0usize;
                let mut total_ic = 0usize;

                for arg in arguments {
                    let (nv, lc, ic) = self.resolve_enums_in_value(arg)?;
                    total_lc += lc;
                    total_ic += ic;
                    if lc + ic > 0 {
                        new_args.push(nv);
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
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((value.clone(), 0, 0))
                }
            }

            // Interpolated strings may contain enum accesses in their expressions.
            Value::InterpolatedString { template, expressions, position } => {
                let mut new_exprs = Vec::with_capacity(expressions.len().max(MIN_CAPACITY));
                let mut any_changed = false;
                let mut total_lc = 0usize;
                let mut total_ic = 0usize;

                for expr in expressions {
                    let (ne, lc, ic) = self.resolve_enums_in_expr(expr)?;
                    total_lc += lc;
                    total_ic += ic;
                    if lc + ic > 0 {
                        new_exprs.push(ne);
                        any_changed = true;
                    } else {
                        new_exprs.push(expr.clone());
                    }
                }

                if any_changed {
                    Ok((
                        Value::InterpolatedString {
                            template: template.clone(),
                            expressions: new_exprs,
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((value.clone(), 0, 0))
                }
            }

            _ => Ok((value.clone(), 0, 0)),
        }
    }

    fn resolve_enums_in_properties(
        &self,
        properties: &[ObjectProperty],
        position: Position,
    ) -> Result<(Value, usize, usize), ResolverError> {
        let mut new_props = Vec::with_capacity(properties.len().max(MIN_CAPACITY));
        let mut any_changed = false;
        let mut total_lc = 0usize;
        let mut total_ic = 0usize;

        for prop in properties {
            let (nv, lc, ic) = self.resolve_enums_in_value(&prop.value)?;
            total_lc += lc;
            total_ic += ic;
            if lc + ic > 0 {
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

        Ok((
            Value::Object { properties: new_props, position },
            total_lc,
            total_ic,
        ))
    }

    fn resolve_enums_in_object_literal(
        &self,
        obj: &Value,
    ) -> Result<(Value, usize, usize), ResolverError> {
        match obj {
            Value::Object { properties, position } => {
                self.resolve_enums_in_properties(properties, *position)
            }
            _ => Ok((obj.clone(), 0, 0)),
        }
    }

    /// Recursively resolves enum accesses inside any expression node.
    ///
    /// Handles:
    /// - `EnumAccess` (local and imported) → `Value::Integer`
    /// - `QuickFuncCall` args (covers enums inside calls inside calls)
    /// - `ImportedFunctionCall` args
    /// - `Value` wrapper (delegates back to `resolve_enums_in_value`)
    /// - All binary/unary/conditional expression variants
    fn resolve_enums_in_expr(
        &self,
        expr: &Expression,
    ) -> Result<(Expression, usize, usize), ResolverError> {
        match expr {
            // Local enum: AIType.AGGRESSIVE → Integer(value)
            Expression::EnumAccess {
                namespace_name: None,
                enum_name,
                value: enum_field,
                position,
            } => {
                let int_val = self
                    .symbol_table
                    .try_get_enum_field_value(enum_name, enum_field)
                    .ok_or_else(|| ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}", enum_name, enum_field),
                        message: format!("Enum '{}' field '{}' not found", enum_name, enum_field),
                        position: *position,
                    })?;
                Ok((
                    Expression::Value {
                        value: Value::Integer { value: int_val, position: *position },
                        position: *position,
                    },
                    1,
                    0,
                ))
            }

            // Imported enum: Namespace.EnumName.Field → Integer(value)
            Expression::EnumAccess {
                namespace_name: Some(ns_name),
                enum_name,
                value: enum_field,
                position,
            } => {
                let ns = self
                    .symbol_table
                    .try_get_namespace(ns_name)
                    .ok_or_else(|| ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}.{}", ns_name, enum_name, enum_field),
                        message: format!("Namespace '{}' not found", ns_name),
                        position: *position,
                    })?;
                let fields = ns.enums.get(enum_name.as_str()).ok_or_else(|| {
                    ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}.{}", ns_name, enum_name, enum_field),
                        message: format!("Enum '{}' not found in namespace '{}'", enum_name, ns_name),
                        position: *position,
                    }
                })?;
                let int_val = fields.get(enum_field.as_str()).ok_or_else(|| {
                    ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}.{}", ns_name, enum_name, enum_field),
                        message: format!("Field '{}' not found", enum_field),
                        position: *position,
                    }
                })?;
                Ok((
                    Expression::Value {
                        value: Value::Integer { value: *int_val, position: *position },
                        position: *position,
                    },
                    0,
                    1,
                ))
            }

            // Value wrapper — recurse into the inner value.
            Expression::Value { value, position } => {
                let (new_val, lc, ic) = self.resolve_enums_in_value(value)?;
                if lc + ic > 0 {
                    Ok((Expression::Value { value: new_val, position: *position }, lc, ic))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            // QuickFunction call — recurse into every argument.
            // This covers enums inside calls inside calls (arbitrary depth).
            Expression::QuickFuncCall { name, arguments, position } => {
                let mut new_args = Vec::with_capacity(arguments.len().max(MIN_CAPACITY));
                let mut any_changed = false;
                let mut total_lc = 0usize;
                let mut total_ic = 0usize;

                for arg in arguments {
                    let (na, lc, ic) = self.resolve_enums_in_expr(arg)?;
                    total_lc += lc;
                    total_ic += ic;
                    if lc + ic > 0 {
                        new_args.push(na);
                        any_changed = true;
                    } else {
                        new_args.push(arg.clone());
                    }
                }

                if any_changed {
                    Ok((
                        Expression::QuickFuncCall {
                            name: name.clone(),
                            arguments: new_args,
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            // Imported function call — recurse into every argument.
            Expression::ImportedFunctionCall {
                namespace_name,
                function_name,
                arguments,
                position,
            } => {
                let mut new_args = Vec::with_capacity(arguments.len().max(MIN_CAPACITY));
                let mut any_changed = false;
                let mut total_lc = 0usize;
                let mut total_ic = 0usize;

                for arg in arguments {
                    let (na, lc, ic) = self.resolve_enums_in_expr(arg)?;
                    total_lc += lc;
                    total_ic += ic;
                    if lc + ic > 0 {
                        new_args.push(na);
                        any_changed = true;
                    } else {
                        new_args.push(arg.clone());
                    }
                }

                if any_changed {
                    Ok((
                        Expression::ImportedFunctionCall {
                            namespace_name: namespace_name.clone(),
                            function_name: function_name.clone(),
                            arguments: new_args,
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            // Static method call — recurse into arguments.
            Expression::StaticMethodCall {
                object_name,
                method_name,
                arguments,
                position,
            } => {
                let mut new_args = Vec::with_capacity(arguments.len().max(MIN_CAPACITY));
                let mut any_changed = false;
                let mut total_lc = 0usize;
                let mut total_ic = 0usize;

                for arg in arguments {
                    let (na, lc, ic) = self.resolve_enums_in_expr(arg)?;
                    total_lc += lc;
                    total_ic += ic;
                    if lc + ic > 0 {
                        new_args.push(na);
                        any_changed = true;
                    } else {
                        new_args.push(arg.clone());
                    }
                }

                if any_changed {
                    Ok((
                        Expression::StaticMethodCall {
                            object_name: object_name.clone(),
                            method_name: method_name.clone(),
                            arguments: new_args,
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            // Binary arithmetic — recurse both sides.
            Expression::ArithmeticOp { left, operator, right, position } => {
                let (nl, lc1, ic1) = self.resolve_enums_in_expr(left)?;
                let (nr, lc2, ic2) = self.resolve_enums_in_expr(right)?;
                let total_lc = lc1 + lc2;
                let total_ic = ic1 + ic2;
                if total_lc + total_ic > 0 {
                    Ok((
                        Expression::ArithmeticOp {
                            left: Box::new(nl),
                            operator: operator.clone(),
                            right: Box::new(nr),
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            Expression::BitwiseOp { left, operator, right, position } => {
                let (nl, lc1, ic1) = self.resolve_enums_in_expr(left)?;
                let (nr, lc2, ic2) = self.resolve_enums_in_expr(right)?;
                let total_lc = lc1 + lc2;
                let total_ic = ic1 + ic2;
                if total_lc + total_ic > 0 {
                    Ok((
                        Expression::BitwiseOp {
                            left: Box::new(nl),
                            operator: operator.clone(),
                            right: Box::new(nr),
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            Expression::ComparisonOp { left, operator, right, position } => {
                let (nl, lc1, ic1) = self.resolve_enums_in_expr(left)?;
                let (nr, lc2, ic2) = self.resolve_enums_in_expr(right)?;
                let total_lc = lc1 + lc2;
                let total_ic = ic1 + ic2;
                if total_lc + total_ic > 0 {
                    Ok((
                        Expression::ComparisonOp {
                            left: Box::new(nl),
                            operator: operator.clone(),
                            right: Box::new(nr),
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            Expression::LogicalOp { left, operator, right, position } => {
                let (nl, lc1, ic1) = self.resolve_enums_in_expr(left)?;
                let (nr, lc2, ic2) = self.resolve_enums_in_expr(right)?;
                let total_lc = lc1 + lc2;
                let total_ic = ic1 + ic2;
                if total_lc + total_ic > 0 {
                    Ok((
                        Expression::LogicalOp {
                            left: Box::new(nl),
                            operator: operator.clone(),
                            right: Box::new(nr),
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            Expression::UnaryOp { operator, operand, position } => {
                let (no, lc, ic) = self.resolve_enums_in_expr(operand)?;
                if lc + ic > 0 {
                    Ok((
                        Expression::UnaryOp {
                            operator: operator.clone(),
                            operand: Box::new(no),
                            position: *position,
                        },
                        lc,
                        ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            Expression::Conditional { condition, true_value, false_value, position } => {
                let (nc, lc1, ic1) = self.resolve_enums_in_expr(condition)?;
                let (nt, lc2, ic2) = self.resolve_enums_in_expr(true_value)?;
                let (nf, lc3, ic3) = self.resolve_enums_in_expr(false_value)?;
                let total_lc = lc1 + lc2 + lc3;
                let total_ic = ic1 + ic2 + ic3;
                if total_lc + total_ic > 0 {
                    Ok((
                        Expression::Conditional {
                            condition: Box::new(nc),
                            true_value: Box::new(nt),
                            false_value: Box::new(nf),
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            Expression::Parenthesized { expression, position } => {
                let (ne, lc, ic) = self.resolve_enums_in_expr(expression)?;
                if lc + ic > 0 {
                    Ok((
                        Expression::Parenthesized {
                            expression: Box::new(ne),
                            position: *position,
                        },
                        lc,
                        ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            Expression::PropertyAccess { object, property, position } => {
                let (no, lc, ic) = self.resolve_enums_in_expr(object)?;
                if lc + ic > 0 {
                    Ok((
                        Expression::PropertyAccess {
                            object: Box::new(no),
                            property: property.clone(),
                            position: *position,
                        },
                        lc,
                        ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            Expression::IndexAccess { object, index, position } => {
                let (no, lc1, ic1) = self.resolve_enums_in_expr(object)?;
                let (ni, lc2, ic2) = self.resolve_enums_in_expr(index)?;
                let total_lc = lc1 + lc2;
                let total_ic = ic1 + ic2;
                if total_lc + total_ic > 0 {
                    Ok((
                        Expression::IndexAccess {
                            object: Box::new(no),
                            index: Box::new(ni),
                            position: *position,
                        },
                        total_lc,
                        total_ic,
                    ))
                } else {
                    Ok((expr.clone(), 0, 0))
                }
            }

            // Terminal nodes that cannot contain enum accesses.
            _ => Ok((expr.clone(), 0, 0)),
        }
    }

    // ==================== PHASE 2: DATA CONTEXT BUILD ====================

    fn build_initial_data_context(&mut self) {
        let data_section = match &self.ast.data {
            Some(d) => d,
            None    => return,
        };

        let estimated = (data_section.entries.len() * 4).max(MIN_CAPACITY);
        self.data_context.borrow_mut().reserve(estimated);

        let mut context = self.data_context.borrow_mut();
        let mut total_inserted = 0usize;

        for entry in &data_section.entries {
            total_inserted += Self::populate_context_from_entry(entry, &mut context);
        }

        drop(context);

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Data context populated: {} literal entries",
                total_inserted
            ));
        }

        // Multi-pass: resolve simple variable-alias assignments (a = b style).
        // These are properties whose value is a plain Identifier referencing a
        // sibling or previously-defined variable.  They are not function calls so
        // Phase 4 never touches them, yet function calls may depend on them.
        // Example: `tax_value = base_price` inside a table must be resolved so
        // that `calculateTotal(base_price, tax_value, discount_value)` can proceed.
        self.resolve_identifier_aliases_in_context();
    }

    fn resolve_identifier_aliases_in_context(&mut self) {
        let entries = match &self.ast.data {
            Some(d) => d.entries.clone(),
            None    => return,
        };

        // Up to 8 passes handles chains like a=b, b=c, c=1 that resolve in order.
        for pass in 0..8usize {
            let mut changed = false;
            let mut ctx = self.data_context.borrow_mut();

            for entry in &entries {
                changed |= Self::try_resolve_identifier_aliases_in_entry(entry, &mut ctx);
            }

            drop(ctx);

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "[IdentifierAliasPass {}] changed={}",
                    pass + 1,
                    changed
                ));
            }

            if !changed {
                break;
            }
        }
    }
    fn try_resolve_identifier_aliases_in_entry(
        entry:   &DataEntry,
        context: &mut FxHashMap<String, DixValue>,
    ) -> bool {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let path = PathBuilder::build(&[name.as_str()]);
                if context.contains_key(&path) {
                    return false; // already resolved
                }
                if let Value::Identifier { value: id, .. } = value {
                    if let Some(dix) = Self::lookup_by_name_or_suffix(id, context) {
                        context.insert(path, dix);
                        return true;
                    }
                }
                false
            }

            DataEntry::TableProperty { path: tp, properties, .. } => {
                let segments: Vec<&str> = tp.segments.iter().map(|s| s.as_str()).collect();
                let mut changed = false;

                for prop in properties {
                    let mut segs = segments.clone();
                    segs.push(prop.name.as_str());
                    let full = PathBuilder::build(&segs);

                    if context.contains_key(&full) {
                        continue; // already resolved
                    }

                    if let Value::Identifier { value: id, .. } = &prop.value {
                        if let Some(dix) = Self::lookup_by_name_or_suffix(id, context) {
                            context.insert(full, dix);
                            changed = true;
                        }
                    }
                }

                changed
            }

            DataEntry::GroupArray { path: gp, items, .. } => {
                let segments: Vec<&str> = gp.segments.iter().map(|s| s.as_str()).collect();
                let base = PathBuilder::build(&segments);
                let mut changed = false;

                for (i, item) in items.iter().enumerate() {
                    let indexed = format!("{}[{}]", base, i);
                    if context.contains_key(&indexed) {
                        continue;
                    }
                    if let Value::Identifier { value: id, .. } = item {
                        if let Some(dix) = Self::lookup_by_name_or_suffix(id, context) {
                            context.insert(indexed, dix);
                            changed = true;
                        }
                    }
                }

                changed
            }

            DataEntry::ObjectProperty { .. } => false,
        }
    }
    /// Look up `name` in `context` by exact key first, then by path-suffix
    /// (e.g. `"port"` matches `"DATA.server.config.port"`).
    /// Returns a clone of the first match, or `None`.
    fn lookup_by_name_or_suffix(
        name:    &str,
        context: &FxHashMap<String, DixValue>,
    ) -> Option<DixValue> {
        // Exact match (flat property at DATA root)
        if let Some(v) = context.get(name) {
            return Some(v.clone());
        }
        // Suffix match (nested property)
        let suffix = format!(".{}", name);
        context
            .iter()
            .find(|(k, _)| k.ends_with(&suffix))
            .map(|(_, v)| v.clone())
    }
    fn populate_context_from_entry(
        entry: &DataEntry,
        context: &mut FxHashMap<String, DixValue>,
    ) -> usize {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let path = PathBuilder::build(&[name.as_str()]);
                Self::insert_value_recursive(value, &path, context)
            }

            DataEntry::TableProperty { path: tp, properties, .. } => {
                let segments: Vec<&str> = tp.segments.iter().map(|s| s.as_str()).collect();
                let mut count = 0usize;
                for prop in properties {
                    let mut segs = segments.clone();
                    segs.push(prop.name.as_str());
                    let full = PathBuilder::build(&segs);
                    count += Self::insert_value_recursive(&prop.value, &full, context);
                }
                count
            }

            DataEntry::GroupArray { path: gp, items, .. } => {
                let segments: Vec<&str> = gp.segments.iter().map(|s| s.as_str()).collect();
                let base = PathBuilder::build(&segments);
                let mut count = 0usize;
                for (i, item) in items.iter().enumerate() {
                    let indexed = format!("{}[{}]", base, i);
                    count += Self::insert_value_recursive(item, &indexed, context);
                }
                count
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let base = PathBuilder::build(&[name.as_str()]);
                Self::insert_value_recursive(object, &base, context)
            }
        }
    }

    fn insert_value_recursive(
        value: &Value,
        path: &str,
        context: &mut FxHashMap<String, DixValue>,
    ) -> usize {
        match value {
            Value::Object { properties, .. } => {
                let mut count = 0usize;
                for prop in properties {
                    let child = format!("{}.{}", path, prop.key);
                    count += Self::insert_value_recursive(&prop.value, &child, context);
                }
                if let Some(dix) = Self::try_value_to_dix(value) {
                    context.insert(path.to_string(), dix);
                    count += 1;
                }
                count
            }

            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                let mut count = 0usize;
                let mut all_ok = true;
                for (i, item) in values.iter().enumerate() {
                    let idx = format!("{}[{}]", path, i);
                    let inserted = Self::insert_value_recursive(item, &idx, context);
                    if inserted == 0 {
                        all_ok = false;
                    }
                    count += inserted;
                }
                if all_ok && !values.is_empty() {
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

    let dynamic_limit = (total * 3).max(MIN_CAPACITY * 4);
    let max_iterations = dynamic_limit.min(MAX_RESOLUTION_ITERATIONS);

    let mut resolved_count = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut iteration = 0usize;

    let mut pending: Vec<(FunctionCallInfo, bool)> =
        function_calls.into_iter().map(|c| (c, false)).collect();

    loop {
        if pending.iter().all(|(_, r)| *r) {
            break;
        }

        iteration += 1;

        if iteration > max_iterations {
            let stuck: Vec<String> = pending
                .iter()
                .filter(|(_, r)| !r)
                .map(|(c, _)| c.location.clone())
                .collect();
            errors.push(
                ResolverError::CircularDependency { stuck_calls: stuck }.to_string(),
            );
            break;
        }

        let mut resolved_this_pass = 0usize;

        for i in 0..pending.len() {
            if pending[i].1 {
                continue;
            }

            if self.has_unresolved_dependencies(&pending[i].0) {
                continue;
            }

            if let Err(e) = self.validate_function_scope(&pending[i].0) {
                errors.push(e.to_string());
                pending[i].1 = true;
                continue;
            }

            let call_start = Instant::now();
            let result = self.execute_call_raw(&pending[i].0);
            let call_dur = call_start.elapsed();

            match result {
                Ok(dix_value) => {
                    let location  = pending[i].0.location.clone();
                    let fn_name   = pending[i].0.function_name.clone();
                    let ns_name   = pending[i].0.namespace_name.clone();
                    let scope     = pending[i].0.scope.clone();
                    let pos       = pending[i].0.position;
                    let arg_strs: Vec<String> = pending[i]
                        .0
                        .arguments
                        .iter()
                        .map(|a| format!("{:?}", a))
                        .collect();

                    let new_value = Self::convert_dix_value_to_value(&dix_value, pos);
                    self.replace_value_in_ast_by_location(&location, pos, new_value);
                    self.data_context
                        .borrow_mut()
                        .insert(location.clone(), dix_value.clone());
                    self.resolved_values
                        .insert(location.clone(), dix_value.clone());

                    if self.debug_config.is_enabled {
                        self.error_manager.log_info(&format!(
                            "[iter {}] resolved {} ({:.3}ms)",
                            iteration,
                            location,
                            call_dur.as_secs_f64() * 1000.0
                        ));
                    }

                    self.resolution_history.push(ResolutionRecord {
                        function_name:  fn_name,
                        namespace_name: ns_name,
                        location,
                        scope,
                        arguments:      arg_strs,
                        result:         Some(dix_value),
                        success:        true,
                        error_message:  String::new(),
                        timestamp:      Utc::now(),
                    });

                    pending[i].1 = true;
                    resolved_count += 1;
                    resolved_this_pass += 1;
                }

                Err(interp_err) => {
                    let location = pending[i].0.location.clone();
                    let fn_name  = pending[i].0.function_name.clone();
                    let ns_name  = pending[i].0.namespace_name.clone();
                    let scope    = pending[i].0.scope.clone();
                    let arg_strs: Vec<String> = pending[i]
                        .0
                        .arguments
                        .iter()
                        .map(|a| format!("{:?}", a))
                        .collect();

                    let resolver_err = ResolverError::ExecutionFailed {
                        function: fn_name.clone(),
                        location: location.clone(),
                        inner:    interp_err,
                    };

                    self.resolution_history.push(ResolutionRecord {
                        function_name:  fn_name,
                        namespace_name: ns_name,
                        location,
                        scope,
                        arguments:      arg_strs,
                        result:         None,
                        success:        false,
                        error_message:  resolver_err.to_string(),
                        timestamp:      Utc::now(),
                    });

                    errors.push(resolver_err.to_string());
                    pending[i].1 = true;
                }
            }
        }

        if resolved_this_pass == 0 && pending.iter().any(|(_, r)| !r) {
            let stuck: Vec<String> = pending
                .iter()
                .filter(|(_, r)| !r)
                .map(|(c, _)| c.location.clone())
                .collect();
            errors.push(
                ResolverError::CircularDependency { stuck_calls: stuck }.to_string(),
            );
            break;
        }
    }

    if self.debug_config.is_enabled {
        self.error_manager.log_info(&format!(
            "[Phase 4] done — {}/{} resolved, {} iterations used (limit: {})",
            resolved_count, total, iteration, max_iterations
        ));
    }

    (resolved_count, errors)
}

    fn execute_call_raw(
    &mut self,
    call: &FunctionCallInfo,
) -> Result<DixValue, InterpreterError> {
    // At DATA level there are no local variables — create an empty top-level
    // context. The interpreter resolves identifiers via data_context.
    let mut top_level_ctx = ExecutionContext::new("<data>", None);

    // Evaluate every argument in the top-level context. This correctly handles
    // nested ImportedFunctionCalls, QuickFuncCalls, arithmetic, enum accesses —
    // anything the interpreter knows how to evaluate.
    let evaluated_args = self.interpreter.evaluate_arguments_in_caller_context(
        &call.arguments,
        call.position,
        &mut top_level_ctx,
        &call.scope_context,
        None,
    )?;

    // Wrap resolved DixValues as literal Expression::Value nodes so that
    // interpreter.execute → bind_parameters → evaluate_expression returns
    // them immediately without any further lookup.
    let expr_args: Vec<Expression> = evaluated_args
        .iter()
        .map(|dv| Expression::Value {
            value: Self::convert_dix_value_to_value(dv, call.position),
            position: call.position,
        })
        .collect();

    match &call.namespace_name {
        Some(ns_name) => {
            let (func_ast, target_namespace) = {
                let ns = self
                    .symbol_table
                    .try_get_namespace(ns_name)
                    .ok_or_else(|| InterpreterError::UndefinedFunction {
                        name: format!("{}.{}", ns_name, call.function_name),
                        position: call.position,
                    })?;
                let func_ast = ns
                    .functions
                    .get(&call.function_name)
                    .ok_or_else(|| InterpreterError::UndefinedFunction {
                        name: format!("{}.{}", ns_name, call.function_name),
                        position: call.position,
                    })?
                    .ast
                    .clone();
                (func_ast, ns as *const ImportedNamespace)
            };

            // SAFETY: symbol_table lives for 'a which outlives this call.
            let ns_ref: &ImportedNamespace = unsafe { &*target_namespace };
            let fqn = format!("{}.{}", ns_name, call.function_name);
            let mut ctx = ExecutionContext::new(&fqn, None);
            self.interpreter.execute(
                &func_ast,
                &expr_args,
                &mut ctx,
                &call.scope_context,
                Some(ns_ref),
            )
        }

        None => {
            let function_clone = self
                .interpreter
                .find_function(&call.function_name)
                .ok_or_else(|| InterpreterError::UndefinedFunction {
                    name: call.function_name.clone(),
                    position: call.position,
                })?
                .clone();

            let mut ctx = ExecutionContext::new(&call.function_name, None);
            self.interpreter.execute(
                &function_clone,
                &expr_args,
                &mut ctx,
                &call.scope_context,
                None,
            )
        }
    }
}

    // ==================== PHASE 4 HELPERS ====================

    fn has_unresolved_dependencies(&self, call: &FunctionCallInfo) -> bool {
        let ctx = self.data_context.borrow();
        call.arguments
            .iter()
            .any(|arg| Self::expr_has_unresolved_ref(arg, &ctx))
    }

    fn expr_has_unresolved_ref(
        expr: &Expression,
        ctx: &FxHashMap<String, DixValue>,
    ) -> bool {
        match expr {
            // An Identifier is only truly unresolved if it cannot be found anywhere
            // in the data context — either by exact key or by path-suffix match.
            // Previously this was just `!ctx.contains_key(name)`, which caused false
            // positives for results stored at nested paths like "db.primary.total"
            // when the argument was just Identifier("total").
            Expression::Identifier { name, .. } => {
                if ctx.contains_key(name.as_str()) {
                    return false;
                }
                let suffix = format!(".{}", name);
                !ctx.keys().any(|k| k.ends_with(&suffix))
            }

            // After Phase 1 enum pre-resolution these are already Integer literals.
            Expression::EnumAccess { .. } => false,

            // A nested call is only "unresolved" if its own arguments contain
            // something we cannot evaluate yet.  The interpreter will execute it
            // inline, so the call itself is not a dependency gate.
            Expression::QuickFuncCall { arguments, .. }
            | Expression::ImportedFunctionCall { arguments, .. } => {
                arguments.iter().any(|a| Self::expr_has_unresolved_ref(a, ctx))
            }

            Expression::ArithmeticOp { left, right, .. }
            | Expression::BitwiseOp { left, right, .. }
            | Expression::ComparisonOp { left, right, .. }
            | Expression::LogicalOp { left, right, .. } => {
                Self::expr_has_unresolved_ref(left, ctx)
                    || Self::expr_has_unresolved_ref(right, ctx)
            }

            Expression::Conditional { condition, true_value, false_value, .. } => {
                Self::expr_has_unresolved_ref(condition, ctx)
                    || Self::expr_has_unresolved_ref(true_value, ctx)
                    || Self::expr_has_unresolved_ref(false_value, ctx)
            }

            Expression::Value { value, .. } => Self::value_has_unresolved_ref(value, ctx),

            // For property access (e.g. `server.host`) reconstruct the full dotted
            // path and look it up directly rather than only checking the root object.
            Expression::PropertyAccess { object, property, .. } => {
                if let Some(full_path) = Self::reconstruct_access_path(object, property) {
                    if ctx.contains_key(full_path.as_str()) {
                        return false;
                    }
                    let suffix = format!(".{}", full_path);
                    !ctx.keys().any(|k| k.ends_with(&suffix))
                } else {
                    // Cannot reconstruct — fall back to checking the base object.
                    Self::expr_has_unresolved_ref(object, ctx)
                }
            }

            Expression::IndexAccess { object, index, .. } => {
                Self::expr_has_unresolved_ref(object, ctx)
                    || Self::expr_has_unresolved_ref(index, ctx)
            }

            // All other expression variants (literals, static calls, etc.)
            // are either self-contained or handled by the interpreter directly.
            _ => false,
        }
    }
    /// Reconstruct a fully-qualified dotted path from a chain of PropertyAccess
    /// nodes.  Returns `None` if the chain contains a non-identifier base.
    fn reconstruct_access_path(object: &Expression, property: &str) -> Option<String> {
        match object {
            Expression::Identifier { name, .. } => Some(format!("{}.{}", name, property)),
            Expression::PropertyAccess {
                object: inner,
                property: inner_prop,
                ..
            } => Self::reconstruct_access_path(inner, inner_prop)
                .map(|base| format!("{}.{}", base, property)),
            _ => None,
        }
    }
    fn value_has_unresolved_ref(value: &Value, ctx: &FxHashMap<String, DixValue>) -> bool {
        match value {
            Value::Identifier { value: id, .. } => {
                if ctx.contains_key(id.as_str()) {
                    return false;
                }
                let suffix = format!(".{}", id);
                !ctx.keys().any(|k| k.ends_with(&suffix))
            }
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
                let ns =
                    self.symbol_table
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
        let mut out = Vec::with_capacity(call.arguments.len().max(MIN_CAPACITY));
        for arg in &call.arguments {
            out.push(self.resolve_expr_to_dix(arg, &ctx, call.position)?);
        }
        Ok(out)
    }

    /// Resolve a single expression to a DixValue using the data context.
    ///
    /// Now handles `Expression::EnumAccess` as a safety net for any enum that
    /// Phase 1 missed (e.g. in an expression position not yet covered), so
    /// resolution never falls through to a Fatal error on a valid enum.
    fn resolve_expr_to_dix(
        &self,
        expr: &Expression,
        ctx: &FxHashMap<String, DixValue>,
        call_pos: Position,
    ) -> Result<DixValue, ResolverError> {
        match expr {
            Expression::Value { value, .. } => {
                Self::resolve_value_to_dix(value, ctx, call_pos)
            }

            Expression::Identifier { name, .. } => {
                ctx.get(name.as_str()).cloned().ok_or_else(|| ResolverError::Fatal {
                    message: format!(
                        "identifier '{}' missing from context at {}",
                        name, call_pos
                    ),
                })
            }

            // Safety net: any EnumAccess that survived Phase 1 unresolved is
            // resolved here before the interpreter sees it.
            Expression::EnumAccess {
                namespace_name: None,
                enum_name,
                value: enum_field,
                position,
            } => {
                let int_val = self
                    .symbol_table
                    .try_get_enum_field_value(enum_name, enum_field)
                    .ok_or_else(|| ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}", enum_name, enum_field),
                        message: format!(
                            "Enum '{}' field '{}' not found (late resolution)",
                            enum_name, enum_field
                        ),
                        position: *position,
                    })?;
                Ok(DixValue::from_int(int_val))
            }

            Expression::EnumAccess {
                namespace_name: Some(ns_name),
                enum_name,
                value: enum_field,
                position,
            } => {
                let ns = self
                    .symbol_table
                    .try_get_namespace(ns_name)
                    .ok_or_else(|| ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}.{}", ns_name, enum_name, enum_field),
                        message: format!("Namespace '{}' not found (late resolution)", ns_name),
                        position: *position,
                    })?;
                let fields = ns.enums.get(enum_name.as_str()).ok_or_else(|| {
                    ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}.{}", ns_name, enum_name, enum_field),
                        message: format!(
                            "Enum '{}' not found in namespace '{}' (late resolution)",
                            enum_name, ns_name
                        ),
                        position: *position,
                    }
                })?;
                let int_val = fields.get(enum_field.as_str()).ok_or_else(|| {
                    ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}.{}", ns_name, enum_name, enum_field),
                        message: format!("Field '{}' not found (late resolution)", enum_field),
                        position: *position,
                    }
                })?;
                Ok(DixValue::from_int(*int_val))
            }

            _ => Err(ResolverError::Fatal {
                message: format!("cannot resolve expr to DixValue at {}", call_pos),
            }),
        }
    }
// value_resolver.rs — fn resolve_value_to_dix
// (Builtins::Core::DixValue, used in Phase 4 argument resolution)

fn resolve_value_to_dix(
    value: &Value,
    ctx: &FxHashMap<String, DixValue>,
    call_pos: Position,
) -> Result<DixValue, ResolverError> {
    match value {
        // ── Primitives ────────────────────────────────────────────────────────
        Value::Integer { value, .. }  => Ok(DixValue::from_int(*value)),
        Value::Long { value, .. }     => Ok(DixValue::from_long(*value)),
        Value::Float { value, .. }    => Ok(DixValue::from_float(*value)),
        Value::Double { value, .. }   => Ok(DixValue::from_double(*value)),
        // FIX: was falling through to Fatal error — causes every chemistry-DB
        // constant (6.62607015e-34, 6.02214076e23, …) to abort resolution.
        Value::ScientificNotation { value, .. } => Ok(DixValue::from_double(*value)),
        Value::String { value, .. }   => Ok(DixValue::from_string(value.clone())),
        Value::Boolean { value, .. }  => Ok(DixValue::from_bool(*value)),
        Value::Null { .. }            => Ok(DixValue::null()),
        Value::HexColor { value, .. } => Ok(DixValue::from_hex(value.clone())),

        // Date/Timestamp: parse string → DateTime<Utc>.
        Value::Date { value: d, .. } => {
            let dt = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .ok()
                .and_then(|nd| nd.and_hms_opt(0, 0, 0))
                .map(|ndt| ndt.and_utc())
                .or_else(|| d.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .unwrap_or_else(chrono::Utc::now);
            Ok(DixValue::from_date(dt))
        }
        Value::Timestamp { value: t, .. } => {
            let dt = t.parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap_or_else(|_| chrono::Utc::now());
            Ok(DixValue::from_timestamp(dt))
        }

        // InterpolatedString: use template text.
        Value::InterpolatedString { template, .. } => {
            Ok(DixValue::from_string(template.clone()))
        }

        // EnumValue: should be Integer after Phase 1; this is a safety net so
        // a missed enum doesn't abort the entire resolution pass.
        Value::EnumValue { .. } => Ok(DixValue::from_int(0)),

        // Identifier: resolve from data context.
        Value::Identifier { value: id, .. } => {
            ctx.get(id.as_str()).cloned().ok_or_else(|| ResolverError::Fatal {
                message: format!(
                    "identifier '{}' missing from context at {}",
                    id, call_pos
                ),
            })
        }

        // ── Collections ───────────────────────────────────────────────────────
        Value::Array { values, .. } => {
            let items: Result<Vec<DixValue>, ResolverError> = values
                .iter()
                .map(|v| Self::resolve_value_to_dix(v, ctx, call_pos))
                .collect();
            Ok(DixValue::from_array(items?))
        }

        // NestedArray: treat identically to Array.
        Value::NestedArray { values, .. } => {
            let items: Result<Vec<DixValue>, ResolverError> = values
                .iter()
                .map(|v| Self::resolve_value_to_dix(v, ctx, call_pos))
                .collect();
            Ok(DixValue::from_array(items?))
        }

        Value::Object { properties, .. } => {
            let mut map = FxHashMap::with_capacity_and_hasher(
                properties.len().max(MIN_CAPACITY),
                Default::default(),
            );
            for prop in properties {
                map.insert(
                    prop.key.clone(),
                    Self::resolve_value_to_dix(&prop.value, ctx, call_pos)?,
                );
            }
            Ok(DixValue::from_object(map.into_iter().collect()))
        }

        Value::PrefixedConstructor { prefix, arguments, .. } => {
            match prefix.to_lowercase().as_str() {
                "b" => {
                    let data = arguments
                        .first()
                        .and_then(|a| {
                            if let Value::String { value, .. } = a {
                                Some(value.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    DixValue::from_blob(data)
                        .map_err(|e| ResolverError::Fatal { message: e })
                }
                "r" => {
                    let pattern = arguments
                        .first()
                        .and_then(|a| {
                            if let Value::String { value, .. } = a {
                                Some(value.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| ".*".to_string());
                    DixValue::from_regex(pattern)
                        .map_err(|e| ResolverError::Fatal { message: e })
                }
                // "t" recurses — nested tuples like t:(t:(1,2), t:(3,4)) resolve correctly.
                "t" => {
                    let items: Result<Vec<DixValue>, ResolverError> = arguments
                        .iter()
                        .take(6)
                        .map(|a| Self::resolve_value_to_dix(a, ctx, call_pos))
                        .collect();
                    Ok(DixValue::from_tuple(items?))
                }
                _ => Err(ResolverError::Fatal {
                    message: format!("unknown prefix constructor: {}", prefix),
                }),
            }
        }

        _ => Err(ResolverError::Fatal {
            message: format!(
                "cannot convert value variant {:?} to DixValue at {}",
                std::mem::discriminant(value),
                call_pos
            ),
        }),
    }
}
    // ==================== AST REPLACEMENT ====================

    fn replace_value_in_ast_by_location(
        &mut self,
        _location: &str,
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

    fn replace_in_value(value: &mut Value, target: Position, new_value: &Value) -> bool {
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

    #[inline]
    fn value_position(value: &Value) -> Option<Position> {
        Some(match value {
            Value::Integer { position, .. }
            | Value::Long { position, .. }
            | Value::Float { position, .. }
            | Value::Double { position, .. }
            | Value::ScientificNotation { position, .. }
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
            | Value::Unknown { position, .. } => *position,
        })
    }

    // ==================== PHASE 5: IDENTIFIER RESOLUTION ====================

    fn resolve_remaining_identifiers(&mut self) {
        let node_estimate = self
            .ast
            .data
            .as_ref()
            .map(|d| d.entries.len() * 6)
            .unwrap_or(0);
        let max_passes = (node_estimate / 4).max(8).min(64);

        let mut total_resolved = 0usize;
        let mut final_skipped = 0usize;

        for pass in 1..=max_passes {
            let data = match self.ast.data.as_ref() {
                Some(d) => d,
                None => break,
            };
            let ctx = self.data_context.borrow();

            let mut newly_resolved: Vec<(String, DixValue)> = Vec::new();
            let mut resolved_this_pass = 0usize;
            let mut skipped_this_pass = 0usize;
            let mut new_entries = Vec::with_capacity(data.entries.len());

            for entry in &data.entries {
                let (new_entry, res, skip) =
                    Self::resolve_identifiers_in_entry(entry, &ctx, &mut newly_resolved);
                new_entries.push(new_entry);
                resolved_this_pass += res;
                skipped_this_pass += skip;
            }

            drop(ctx);

            if resolved_this_pass > 0 {
                let pos = self.ast.data.as_ref().unwrap().position;
                self.ast.data = Some(DataSection { entries: new_entries, position: pos });
                let mut ctx = self.data_context.borrow_mut();
                for (path, dix) in newly_resolved {
                    ctx.insert(path, dix);
                }
            }

            total_resolved += resolved_this_pass;
            final_skipped = skipped_this_pass;

            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!(
                    "[Phase 5, pass {}/{}] resolved {}, {} pending",
                    pass, max_passes, resolved_this_pass, skipped_this_pass
                ));
            }

            if resolved_this_pass == 0 {
                break;
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Identifier resolution: {} resolved, {} external/runtime",
                total_resolved, final_skipped
            ));
        }
    }

    fn resolve_identifiers_in_entry(
        entry: &DataEntry,
        ctx: &FxHashMap<String, DixValue>,
        newly_resolved: &mut Vec<(String, DixValue)>,
    ) -> (DataEntry, usize, usize) {
        match entry {
            DataEntry::SimpleProperty { name, data_type, value, position } => {
                let base = PathBuilder::build(&[name.as_str()]);
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

            DataEntry::TableProperty { path: tp, properties, position } => {
                let segs: Vec<&str> = tp.segments.iter().map(|s| s.as_str()).collect();
                let mut new_props = Vec::with_capacity(properties.len().max(MIN_CAPACITY));
                let mut total_res = 0usize;
                let mut total_skip = 0usize;
                let mut any_changed = false;

                for prop in properties {
                    let mut full_segs = segs.clone();
                    full_segs.push(prop.name.as_str());
                    let full = PathBuilder::build(&full_segs);
                    let (nv, res, skip) = Self::resolve_identifiers_in_value(
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
                        DataEntry::TableProperty {
                            path: tp.clone(),
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

            DataEntry::GroupArray { path: gp, items, position } => {
                let segs: Vec<&str> = gp.segments.iter().map(|s| s.as_str()).collect();
                let base = PathBuilder::build(&segs);
                let mut new_items = Vec::with_capacity(items.len().max(MIN_CAPACITY));
                let mut total_res = 0usize;
                let mut total_skip = 0usize;
                let mut any_changed = false;

                for (i, item) in items.iter().enumerate() {
                    let indexed = format!("{}[{}]", base, i);
                    let (nv, res, skip) = Self::resolve_identifiers_in_value(
                        item,
                        &indexed,
                        ctx,
                        newly_resolved,
                    );
                    total_res += res;
                    total_skip += skip;
                    if res > 0 {
                        new_items.push(nv);
                        any_changed = true;
                    } else {
                        new_items.push(item.clone());
                    }
                }

                if any_changed {
                    (
                        DataEntry::GroupArray {
                            path: gp.clone(),
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
                let base = PathBuilder::build(&[name.as_str()]);
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
        ctx: &FxHashMap<String, DixValue>,
        newly_resolved: &mut Vec<(String, DixValue)>,
    ) -> (Value, usize, usize) {
        match value {
            Value::Identifier { value: id, position } => {
                if let Some(dix) = ctx.get(id.as_str()) {
                    let new_val = Self::convert_dix_value_to_value(dix, *position);
                    newly_resolved.push((path.to_string(), dix.clone()));
                    (new_val, 1, 0)
                } else {
                    (value.clone(), 0, 1)
                }
            }

            Value::Array { values, position } | Value::NestedArray { values, position, .. } => {
                let mut new_values = Vec::with_capacity(values.len().max(MIN_CAPACITY));
                let mut total_res = 0usize;
                let mut total_skip = 0usize;
                let mut any_changed = false;

                for (i, item) in values.iter().enumerate() {
                    let idx = format!("{}[{}]", path, i);
                    let (nv, res, skip) =
                        Self::resolve_identifiers_in_value(item, &idx, ctx, newly_resolved);
                    total_res += res;
                    total_skip += skip;
                    if res > 0 {
                        any_changed = true;
                        new_values.push(nv);
                    } else {
                        new_values.push(item.clone());
                    }
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
                let mut new_props = Vec::with_capacity(properties.len().max(MIN_CAPACITY));
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
                let mut new_args = Vec::with_capacity(arguments.len().max(MIN_CAPACITY));
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
                        new_args.push(nv);
                    } else {
                        new_args.push(arg.clone());
                    }
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
// value_resolver.rs — fn try_value_to_dix
// (Builtins::Core::DixValue, used in Phase 2 data-context build)

fn try_value_to_dix(value: &Value) -> Option<DixValue> {
    match value {
        // ── Primitives ────────────────────────────────────────────────────────
        Value::Integer { value, .. }            => Some(DixValue::from_int(*value)),
        Value::Long { value, .. }               => Some(DixValue::from_long(*value)),
        Value::Float { value, .. }              => Some(DixValue::from_float(*value)),
        Value::Double { value, .. }             => Some(DixValue::from_double(*value)),
        Value::ScientificNotation { value, .. } => Some(DixValue::from_double(*value)),
        Value::String { value, .. }             => Some(DixValue::from_string(value.clone())),
        Value::Boolean { value, .. }            => Some(DixValue::from_bool(*value)),
        Value::Null { .. }                      => Some(DixValue::null()),
        Value::HexColor { value, .. }           => Some(DixValue::from_hex(value.clone())),

        // Date/Timestamp: parse string → DateTime<Utc> for Builtins::DixValue.
        Value::Date { value: d, .. } => {
            let dt = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .ok()
                .and_then(|nd| nd.and_hms_opt(0, 0, 0))
                .map(|ndt| ndt.and_utc())
                .or_else(|| d.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .unwrap_or_else(chrono::Utc::now);
            Some(DixValue::from_date(dt))
        }
        Value::Timestamp { value: t, .. } => {
            let dt = t.parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap_or_else(|_| chrono::Utc::now());
            Some(DixValue::from_timestamp(dt))
        }

        // InterpolatedString: expressions are compile-time; use template text.
        Value::InterpolatedString { template, .. } => {
            Some(DixValue::from_string(template.clone()))
        }

        // EnumValue: Phase 1 should have converted all enums to Integer.
        // This arm is a safety-net for any that slip through.
        Value::EnumValue { .. } => Some(DixValue::from_int(0)),

        // ── Collections ───────────────────────────────────────────────────────
        Value::PrefixedConstructor { prefix, arguments, .. } => {
            match prefix.to_lowercase().as_str() {
                "b" => {
                    let data = arguments
                        .first()
                        .and_then(|a| {
                            if let Value::String { value, .. } = a {
                                Some(value.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    DixValue::from_blob(data).ok()
                }
                "r" => {
                    let pattern = arguments
                        .first()
                        .and_then(|a| {
                            if let Value::String { value, .. } = a {
                                Some(value.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| ".*".to_string());
                    DixValue::from_regex(pattern).ok()
                }
                // "t" recurses, so nested tuples like t:(t:(1,2), t:(3,4)) work correctly.
                "t" => {
                    let items: Option<Vec<DixValue>> = arguments
                        .iter()
                        .take(6)
                        .map(Self::try_value_to_dix)
                        .collect();
                    items.map(DixValue::from_tuple)
                }
                _ => None,
            }
        }

        Value::Array { values, .. } => {
            let items: Option<Vec<DixValue>> =
                values.iter().map(Self::try_value_to_dix).collect();
            items.map(DixValue::from_array)
        }

        // NestedArray ([[1,2],[3,4]]) — same treatment as Array.
        Value::NestedArray { values, .. } => {
            let items: Option<Vec<DixValue>> =
                values.iter().map(Self::try_value_to_dix).collect();
            items.map(DixValue::from_array)
        }

        Value::Object { properties, .. } => {
            let mut map = std::collections::HashMap::with_capacity(
                properties.len().max(MIN_CAPACITY),
            );
            for prop in properties {
                map.insert(prop.key.clone(), Self::try_value_to_dix(&prop.value)?);
            }
            Some(DixValue::from_object(map.into_iter().collect()))
        }

        // Everything else (Identifier, Expression, Lambda, Range, errors) is
        // not representable as a static data-context value.
        _ => None,
    }
}

 // value_resolver.rs — fn convert_dix_value_to_value
// (Builtins::Core::DixValue → AST Value, used after Phase 4 to write results back)

pub fn convert_dix_value_to_value(dix: &DixValue, position: Position) -> Value {
    match dix.get_type() {
        DixType::Int    => Value::Integer { value: dix.as_int(),    position },
        DixType::Long   => Value::Long    { value: dix.as_long(),   position },
        DixType::Float  => Value::Float   { value: dix.as_float(),  position },
        DixType::Double => Value::Double  { value: dix.as_double(), position },
        DixType::String => Value::String  { value: dix.as_string(), position },
        DixType::Bool   => Value::Boolean { value: dix.as_bool(),   position },
        DixType::Null   => Value::Null    { position },
        DixType::Hex    => Value::HexColor { value: dix.as_string(), position },

        DixType::Blob => Value::PrefixedConstructor {
            prefix: "b".to_string(),
            arguments: vec![Value::String {
                value: dix.as_blob_base64().unwrap_or_default(),
                position,
            }],
            position,
        },

        DixType::Regex => Value::PrefixedConstructor {
            prefix: "r".to_string(),
            arguments: vec![Value::String {
                value: dix.as_string(),
                position,
            }],
            position,
        },

        //  Tuple must round-trip as t:(...) not as [...].
        // Previously both Array and Tuple produced Value::Array, which broke
        // subsequent resolution passes that needed to re-read tuple elements
        // (e.g. createLennardJones returns t:(epsilon, sigma), then the caller
        // does lj_parameters.first()). Now Tuple → PrefixedConstructor{"t"}
        // which try_value_to_dix handles correctly via the "t" branch,
        // and Display renders as `t:(...)` in the .resolved.mdix output.
        DixType::Array => {
            let values: Vec<Value> = dix
                .as_array()
                .iter()
                .map(|item| Self::convert_dix_value_to_value(item, position))
                .collect();
            Value::Array { values, position }
        }

        DixType::Tuple => {
            // Recursion preserves nested tuples: t:(t:(1,2), t:(3,4)) → correct.
            let arguments: Vec<Value> = dix
                .as_array()
                .iter()
                .map(|item| Self::convert_dix_value_to_value(item, position))
                .collect();
            Value::PrefixedConstructor {
                prefix: "t".to_string(),
                arguments,
                position,
            }
        }

        DixType::Object => {
            let properties: Vec<ObjectProperty> = dix
                .as_object()
                .iter()
                .map(|(key, val)| ObjectProperty {
                    key:      key.clone(),
                    value:    Self::convert_dix_value_to_value(val, position),
                    position,
                })
                .collect();
            Value::Object { properties, position }
        }

        DixType::Date      => Value::Date      { value: dix.as_string(), position },
        DixType::Timestamp => Value::Timestamp { value: dix.as_string(), position },

        // Enum: already converted to Integer in Phase 1; unreachable in practice.
        _ => Value::String { value: dix.as_string(), position },
    }
}

    // ==================== DIAGNOSTIC UTILITIES ====================

    fn dump_data_context(&self) {
        let ctx = self.data_context.borrow();
        self.error_manager.log_info("[DIAGNOSTIC] data_context dump");
        self.error_manager
            .log_info(&format!("  entries: {}", ctx.len()));

        if self.debug_config.is_verbose {
            let mut keys: Vec<&String> = ctx.keys().collect();
            keys.sort_unstable();
            for key in keys {
                self.error_manager
                    .log_debug(&format!("  {} = {:?}", key, ctx[key]));
            }
        }
    }

    fn log_function_call_breakdown(&self, calls: &[FunctionCallInfo]) {
        let local_count = calls.iter().filter(|c| c.namespace_name.is_none()).count();
        let ns_count = calls.iter().filter(|c| c.namespace_name.is_some()).count();
        self.error_manager
            .log_info(&format!("  local: {}, namespaced: {}", local_count, ns_count));

        if self.debug_config.is_verbose {
            for call in calls {
                let prefix = call
                    .namespace_name
                    .as_ref()
                    .map(|ns| format!("{}.", ns))
                    .unwrap_or_default();
                self.error_manager.log_debug(&format!(
                    "  ~{}{}() → {}",
                    prefix, call.function_name, call.location
                ));
            }
        }
    }

    fn create_failed_result(
        &self,
        errors: Vec<String>,
        original_ast: DixScript,
    ) -> ValueResolutionResult {
        ValueResolutionResult {
            is_success: false,
            original_ast: Some(original_ast),
            resolved_ast: Some(self.ast.clone()),
            function_calls_resolved: 0,
            errors,
            log_statements: self.log_statements.clone(),
            resolution_duration: self.start_time.elapsed(),
            resolution_history: self.resolution_history.clone(),
        }
    }
}
