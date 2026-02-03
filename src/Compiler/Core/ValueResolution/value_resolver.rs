// src/Compiler/Core/ValueResolution/value_resolver.rs
//!
//! ValueResolver — Orchestrates compile-time value resolution
//!
//! ## Resolution Pipeline (5 phases):
//! 1. **Enum Pre-Resolution**   — Replace all EnumValue/EnumAccess with integers
//! 2. **Data Context Build**    — Populate data_context with all literal values
//! 3. **Function Call Discovery** — Walk AST to find all QuickFunction calls
//! 4. **Iterative Resolution**  — Execute calls, replace in AST, update context
//! 5. **Identifier Resolution** — Resolve remaining IdentifierValue references
//!
//! ## Key Improvements over C#:
//! - AST entry replacement via direct Vec index write (no ImmutableArray rebuild)
//! - DebugConfig cached — format!() never called when debug is off
//! - ResolutionRecord args formatted lazily (only when dump is generated)
//! - std::time::Instant for precise duration measurement
//! - PathBuilder used consistently (no manual string concatenation)
//! - Typed ResolverError variants (no string throws)

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use crate::Builtins::Core::DixValue;
use crate::Builtins::Resolver::BuiltinCallResolver;
use crate::Compiler::AST::{
    DataEntry, DataSection, DixScript, Expression, ObjectProperty, Position,
    PropertyAssignment, QuickFunction, SwitchCase, TablePath, Value,
};
use crate::Compiler::Core::DebugMode;
use crate::Compiler::Utilities::{PathBuilder, SymbolTable};
use crate::ErrorManager::ErrorManager;

use super::ast_walker::ASTWalker;
use super::execution_context::ExecutionContext;
use super::function_interpreter::{FunctionInterpreter, InterpreterError};
use super::supporting_classes::{
    DebugConfig, FunctionCallInfo, ImportedNamespace, ResolutionRecord,
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
        
        // Clone quick_functions for interpreter (decouples from AST mutations)
        let quick_functions = ast.quick_functions
            .as_ref()
            .map(|qf| qf.functions.to_vec())
            .unwrap_or_default();
        
        // Shared data context
        let data_context = Rc::new(RefCell::new(HashMap::new()));
        
        // Create interpreter with cloned functions
        let interpreter = FunctionInterpreter::new(
            symbol_table,
            quick_functions,
            Rc::clone(&data_context),
            debug_mode,
        );
        
        // Initialize builtin resolver
        BuiltinCallResolver::initialize();
        
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
        self.error_manager.create_scope("ValueResolver.Resolve");
        
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
        
        // Final summary
        if self.debug_config.is_enabled {
            self.error_manager.log_info("==========================================");
            self.error_manager.log_info("[Phase 4.1] Resolution Complete");
            self.error_manager.log_info(&format!("  Resolved:             {}", success_count));
            self.error_manager.log_info(&format!("  Failed:               {}", errors.len()));
            self.error_manager.log_info(&format!("  Duration:             {:.2}ms", duration.as_secs_f64() * 1000.0));
            self.error_manager.log_info("==========================================");
        }
        
        self.error_manager.exit_scope();
        
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
    
    /// CRITICAL: Pre-process DATA section to resolve ALL enum values to integers
    /// This must happen BEFORE function resolution so enums are available as integers
    fn resolve_all_enum_values(&mut self) -> Result<(), ResolverError> {
        let data_section = match &mut self.ast.data {
            Some(data) => data,
            None => {
                if self.debug_config.is_enabled {
                    self.error_manager.log_debug("No DATA section - skipping enum resolution");
                }
                return Ok(());
            }
        };
        
        self.error_manager.create_scope("ResolveAllEnumValues");
        
        if self.debug_config.is_enabled {
            self.error_manager.log_info("Pre-processing: Resolving all enum values to integers");
        }
        
        let mut local_enum_count = 0;
        let mut imported_enum_count = 0;
        let mut new_entries = Vec::with_capacity(data_section.entries.len());
        
        for entry in &data_section.entries {
            let (new_entry, local_count, imported_count) = self.resolve_enums_in_entry(entry)?;
            new_entries.push(new_entry);
            local_enum_count += local_count;
            imported_enum_count += imported_count;
        }
        
        // Update AST with resolved entries
        self.ast.data = Some(DataSection {
            entries: new_entries,
            position: data_section.position,
        });
        
        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!("✓ Resolved {} local enum values to integers", local_enum_count));
            self.error_manager.log_info(&format!("✓ Resolved {} imported enum values to integers", imported_enum_count));
        }
        
        if local_enum_count + imported_enum_count == 0 && self.debug_config.is_verbose {
            self.error_manager.log_debug("No enum values found in DATA section");
        }
        
        self.error_manager.exit_scope();
        Ok(())
    }
    
    /// Resolve all enums in a single DATA entry
    /// Returns (new entry, local enum count, imported enum count)
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
                            object: new_obj,
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
    /// Returns (new value, local enum count, imported enum count)
    fn resolve_enums_in_value(
        &self,
        value: &Value,
    ) -> Result<(Value, usize, usize), ResolverError> {
        let mut local_count = 0;
        let mut imported_count = 0;
        
        match value {
            Value::EnumValue { enum_name, value: enum_value, position } => {
                // Check if it's an imported enum (dotted name like "utils.Status")
                if enum_name.contains('.') {
                    let parts: Vec<&str> = enum_name.split('.').collect();
                    if parts.len() == 2 {
                        let namespace_name = parts[0];
                        let actual_enum_name = parts[1];
                        
                        if self.debug_config.is_verbose {
                            self.error_manager.log_debug(&format!(
                                "Resolving imported enum: {}.{}.{}",
                                namespace_name, actual_enum_name, enum_value
                            ));
                        }
                        
                        let ns = self.symbol_table.try_get_namespace(namespace_name)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", namespace_name, actual_enum_name, enum_value),
                                message: format!("Namespace '{}' not found", namespace_name),
                                position: *position,
                            })?;
                        
                        let enum_fields = ns.enums.get(actual_enum_name)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", namespace_name, actual_enum_name, enum_value),
                                message: format!("Enum '{}' not found in namespace '{}'", actual_enum_name, namespace_name),
                                position: *position,
                            })?;
                        
                        let field_value = enum_fields.get(enum_value)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", namespace_name, actual_enum_name, enum_value),
                                message: format!("Enum value '{}.{}' not found", actual_enum_name, enum_value),
                                position: *position,
                            })?;
                        
                        if self.debug_config.is_verbose {
                            self.error_manager.log_debug(&format!(
                                "Resolved imported enum: {}.{}.{} = {}",
                                namespace_name, actual_enum_name, enum_value, field_value
                            ));
                        }
                        
                        return Ok((
                            Value::Integer {
                                value: *field_value,
                                position: *position,
                            },
                            0,
                            1,
                        ));
                    }
                }
                
                // LOCAL ENUM: Convert to integer
                let enum_int_value = self.symbol_table.try_get_enum_field_value(enum_name, enum_value)
                    .ok_or_else(|| ResolverError::InvalidEnumAccess {
                        location: format!("{}.{}", enum_name, enum_value),
                        message: format!("Enum value {}.{} not found in symbol table", enum_name, enum_value),
                        position: *position,
                    })?;
                
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "Resolved local enum: {}.{} = {}",
                        enum_name, enum_value, enum_int_value
                    ));
                }
                
                Ok((
                    Value::Integer {
                        value: enum_int_value,
                        position: *position,
                    },
                    1,
                    0,
                ))
            }
            
            Value::Expression { expr, position: expr_pos } => {
                // Check if the expression is an enum access
                if let Expression::EnumAccess { namespace_name, enum_name, value: enum_value, position } = expr.as_ref() {
                    if self.debug_config.is_verbose {
                        self.error_manager.log_debug("[ResolveEnumsInValue] Found EnumAccess in Expression");
                    }
                    
                    if let Some(ns_name) = namespace_name {
                        // Imported enum
                        if self.debug_config.is_verbose {
                            self.error_manager.log_debug(&format!(
                                "[ResolveEnumsInValue] Imported enum: {}.{}.{}",
                                ns_name, enum_name, enum_value
                            ));
                        }
                        
                        let ns = self.symbol_table.try_get_namespace(ns_name)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", ns_name, enum_name, enum_value),
                                message: format!("Namespace '{}' not found", ns_name),
                                position: *position,
                            })?;
                        
                        let enum_fields = ns.enums.get(enum_name)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", ns_name, enum_name, enum_value),
                                message: format!("Enum '{}' not found in namespace '{}'", enum_name, ns_name),
                                position: *position,
                            })?;
                        
                        let field_value = enum_fields.get(enum_value)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}.{}", ns_name, enum_name, enum_value),
                                message: format!("Enum value '{}.{}' not found", enum_name, enum_value),
                                position: *position,
                            })?;
                        
                        if self.debug_config.is_verbose {
                            self.error_manager.log_debug(&format!(
                                "[ResolveEnumsInValue] Resolved imported enum: {}.{}.{} = {}",
                                ns_name, enum_name, enum_value, field_value
                            ));
                        }
                        
                        return Ok((
                            Value::Integer {
                                value: *field_value,
                                position: *position,
                            },
                            0,
                            1,
                        ));
                    } else {
                        // LOCAL ENUM ACCESS
                        if self.debug_config.is_verbose {
                            self.error_manager.log_debug(&format!(
                                "[ResolveEnumsInValue] Local enum: {}.{}",
                                enum_name, enum_value
                            ));
                        }
                        
                        let local_enum_value = self.symbol_table.try_get_enum_field_value(enum_name, enum_value)
                            .ok_or_else(|| ResolverError::InvalidEnumAccess {
                                location: format!("{}.{}", enum_name, enum_value),
                                message: format!("Enum value {}.{} not found", enum_name, enum_value),
                                position: *position,
                            })?;
                        
                        if self.debug_config.is_verbose {
                            self.error_manager.log_debug(&format!(
                                "[ResolveEnumsInValue] Resolved local enum: {}.{} = {}",
                                enum_name, enum_value, local_enum_value
                            ));
                        }
                        
                        return Ok((
                            Value::Integer {
                                value: local_enum_value,
                                position: *position,
                            },
                            1,
                            0,
                        ));
                    }
                }
                
                // Not an enum access - will be resolved later
                if self.debug_config.is_verbose {
                    self.error_manager.log_debug(&format!(
                        "[ResolveEnumsInValue] Expression contains: {:?} - skipping (will resolve later)",
                        std::any::type_name_of_val(expr.as_ref())
                    ));
                }
                
                Ok((value.clone(), 0, 0))
            }
            
            Value::Array { values, position } => {
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
                        Value::Array {
                            values: new_values,
                            position: *position,
                        },
                        local_count,
                        imported_count,
                    ))
                } else {
                    Ok((value.clone(), local_count, imported_count))
                }
            }
            
            Value::Object { properties, position } => {
                let (new_obj, local, imported) = self.resolve_enums_in_object_literal_from_value(properties, *position)?;
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
            
            _ => {
                // Other value types don't contain enums
                Ok((value.clone(), 0, 0))
            }
        }
    }
    
    /// Resolve all enums in an object literal (when it's a standalone Value::Object)
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
                Value::Object {
                    properties: new_properties,
                    position,
                },
                local_count,
                imported_count,
            ))
        } else {
            Ok((
                Value::Object {
                    properties: properties.to_vec(),
                    position,
                },
                local_count,
                imported_count,
            ))
        }
    }
    
    /// Resolve all enums in an object literal (standalone helper)
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

    /// Populate data_context with every literal value currently in DATA.
    /// Must run AFTER enum pre-resolution so enum slots are already integers.
    /// Identifiers backed by literals become immediately available for Phase 4
    /// argument resolution — no interpreter round-trip needed.
    fn build_initial_data_context(&mut self) {
        let data_section = match &self.ast.data {
            Some(d) => d,
            None => return,
        };

        self.error_manager.create_scope("BuildInitialDataContext");

        let mut context = self.data_context.borrow_mut();
        let mut total_inserted: usize = 0;

        for entry in &data_section.entries {
            total_inserted += Self::populate_context_from_entry(entry, &mut context);
        }

        drop(context); // release borrow before any further method calls

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "✓ Data context populated: {} literal entries",
                total_inserted
            ));
        }

        self.error_manager.exit_scope();
    }

    /// Insert every resolvable literal in a single DataEntry into `context`.
    /// Returns the number of keys written.
    fn populate_context_from_entry(
        entry: &DataEntry,
        context: &mut HashMap<String, DixValue>,
    ) -> usize {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let path = PathBuilder::new("DATA").push(name).build();
                Self::insert_value_recursive(value, &path, context)
            }

            DataEntry::TableProperty { path: table_path, properties, .. } => {
                let base = PathBuilder::from_table_path(table_path);
                let mut count = 0usize;
                for prop in properties {
                    let full = base.push(&prop.name).build();
                    count += Self::insert_value_recursive(&prop.value, &full, context);
                }
                count
            }

            DataEntry::GroupArray { path: group_path, items, .. } => {
                let base = PathBuilder::from_table_path(group_path);
                let mut count = 0usize;
                for (i, item) in items.iter().enumerate() {
                    let indexed = base.index(i).build();
                    count += Self::insert_value_recursive(item, &indexed, context);
                }
                count
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let base = PathBuilder::new("DATA").push(name).build();
                Self::insert_value_recursive(object, &base, context)
            }
        }
    }

    /// Recursively walk `value`, inserting every leaf (and every fully-literal
    /// container) into `context` under `path`.  Function-call expressions are
    /// skipped — they are not yet resolved.  Returns keys written.
    fn insert_value_recursive(
        value: &Value,
        path: &str,
        context: &mut HashMap<String, DixValue>,
    ) -> usize {
        match value {
            // ── containers: recurse first, then try to insert the whole thing ──
            Value::Object { properties, .. } => {
                let mut count = 0usize;
                for prop in properties {
                    let child = format!("{}.{}", path, prop.key);
                    count += Self::insert_value_recursive(&prop.value, &child, context);
                }
                // Insert the full object only if every child converted
                if count == properties.len() {
                    if let Some(dix) = Self::try_value_to_dix(value) {
                        context.insert(path.to_string(), dix);
                        count += 1;
                    }
                }
                count
            }

            Value::Array { values, .. } => {
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
                // Full array only if every element resolved
                if all_leaves_ok && !values.is_empty() {
                    if let Some(dix) = Self::try_value_to_dix(value) {
                        context.insert(path.to_string(), dix);
                        count += 1;
                    }
                }
                count
            }

            // ── leaves ─────────────────────────────────────────────────────────
            _ => {
                if let Some(dix) = Self::try_value_to_dix(value) {
                    context.insert(path.to_string(), dix);
                    1
                } else {
                    0 // expression / identifier — not yet available
                }
            }
        }
    }

    // ==================== PHASE 3: FUNCTION CALL DISCOVERY ====================

    /// Walk the entire DATA section and collect every QuickFunction call site.
    /// The returned vec is ordered by appearance — iteration order in Phase 4
    /// preserves declaration order, which is the expected dependency direction.
    fn find_all_function_calls(&self) -> Vec<FunctionCallInfo> {
        let data_section = match &self.ast.data {
            Some(d) => d,
            None => return Vec::new(),
        };

        self.error_manager.create_scope("FindAllFunctionCalls");

        // Heuristic pre-alloc: assume ~15 % of entries contain a call
        let mut calls = Vec::with_capacity((data_section.entries.len() / 6).max(8));

        for (entry_idx, entry) in data_section.entries.iter().enumerate() {
            Self::discover_calls_in_entry(entry, entry_idx, &mut calls);
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[FindAllFunctionCalls] discovered {} call sites across {} entries",
                calls.len(),
                data_section.entries.len()
            ));
        }

        self.error_manager.exit_scope();
        calls
    }

    fn discover_calls_in_entry(
        entry: &DataEntry,
        entry_idx: usize,
        calls: &mut Vec<FunctionCallInfo>,
    ) {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let path = PathBuilder::new("DATA").push(name).build();
                Self::discover_calls_in_value(value, &path, entry_idx, calls);
            }

            DataEntry::TableProperty { path: table_path, properties, .. } => {
                let base = PathBuilder::from_table_path(table_path);
                for prop in properties {
                    let full = base.push(&prop.name).build();
                    Self::discover_calls_in_value(&prop.value, &full, entry_idx, calls);
                }
            }

            DataEntry::GroupArray { path: group_path, items, .. } => {
                let base = PathBuilder::from_table_path(group_path);
                for (i, item) in items.iter().enumerate() {
                    let indexed = base.index(i).build();
                    Self::discover_calls_in_value(item, &indexed, entry_idx, calls);
                }
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let path = PathBuilder::new("DATA").push(name).build();
                Self::discover_calls_in_value(object, &path, entry_idx, calls);
            }
        }
    }

    /// Recurse through a value tree.  When a FunctionCall expression is found
    /// it is recorded; recursion continues so that calls nested inside arrays /
    /// objects / constructor arguments are all captured.
    fn discover_calls_in_value(
        value: &Value,
        path: &str,
        entry_idx: usize,
        calls: &mut Vec<FunctionCallInfo>,
    ) {
        match value {
            Value::Expression { expr, position } => {
                if let Expression::FunctionCall {
                    name,
                    arguments,
                    namespace,
                    position: call_pos,
                } = expr.as_ref()
                {
                    calls.push(FunctionCallInfo {
                        function_name: name.clone(),
                        namespace:     namespace.clone(),
                        arguments:     arguments.clone(),
                        entry_index:   entry_idx,
                        path:          path.to_string(),
                        position:      *call_pos,
                        resolved:      false,
                    });
                }
                // Expressions that are NOT calls (e.g. arithmetic) may still
                // contain nested call sub-expressions — handled by the
                // interpreter itself, so we do not recurse here.
            }

            Value::Array { values, .. } => {
                for (i, item) in values.iter().enumerate() {
                    let indexed = format!("{}[{}]", path, i);
                    Self::discover_calls_in_value(item, &indexed, entry_idx, calls);
                }
            }

            Value::Object { properties, .. } => {
                for prop in properties {
                    let child = format!("{}.{}", path, prop.key);
                    Self::discover_calls_in_value(&prop.value, &child, entry_idx, calls);
                }
            }

            Value::PrefixedConstructor { arguments, .. } => {
                for (i, arg) in arguments.iter().enumerate() {
                    let arg_path = format!("{}.__ctor_arg{}", path, i);
                    Self::discover_calls_in_value(arg, &arg_path, entry_idx, calls);
                }
            }

            _ => {} // literals, identifiers — nothing to discover
        }
    }

    // ==================== PHASE 4: ITERATIVE RESOLUTION ====================

    /// Core resolution loop.  Each pass attempts every pending call.  A call
    /// is skipped when its arguments still reference unresolved identifiers
    /// (they may be produced by a sibling call that hasn't executed yet).
    ///
    /// ### Iteration limits
    /// * **dynamic_limit** — `max(50, pending_count × 3)`.  Scales with the
    ///   number of inter-dependent calls; a DAG of N nodes needs at most N
    ///   passes, so 3× gives comfortable headroom for sparse graphs.
    /// * **absolute_limit** — hard-coded 10 000.  Prevents runaway loops even
    ///   if the dynamic formula is somehow wrong.
    ///
    /// If a full pass produces zero resolutions and calls remain, the set is
    /// declared a circular dependency and an error is returned immediately
    /// (no need to burn iterations up to the limit).
    fn execute_iterative_resolution(
        &mut self,
        mut pending: Vec<FunctionCallInfo>,
    ) -> (usize, Vec<String>) {
        self.error_manager.create_scope("ExecuteIterativeResolution");

        let total            = pending.len();
        let dynamic_limit    = (total * 3).max(50);
        let absolute_limit   = 10_000usize;
        let max_iterations   = dynamic_limit.min(absolute_limit);

        let mut resolved_count = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut iteration      = 0usize;

        loop {
            // ── termination: nothing left to do ──────────────────────────
            if !pending.iter().any(|c| !c.resolved) {
                break;
            }

            iteration += 1;

            // ── absolute iteration guard ─────────────────────────────────
            if iteration > max_iterations {
                let stuck: Vec<String> = pending.iter()
                    .filter(|c| !c.resolved)
                    .map(|c| c.path.clone())
                    .collect();
                errors.push(
                    ResolverError::CircularDependency { stuck_calls: stuck }.to_string()
                );
                break;
            }

            let mut resolved_this_pass = 0usize;

            for i in 0..pending.len() {
                if pending[i].resolved {
                    continue;
                }

                // ── skip: arguments not yet available ────────────────────
                if self.has_unresolved_dependencies(&pending[i]) {
                    if self.debug_config.is_verbose {
                        self.error_manager.log_debug(&format!(
                            "[iter {}] skip '{}' at {} — deps pending",
                            iteration, pending[i].function_name, pending[i].path
                        ));
                    }
                    continue;
                }

                // ── scope / existence validation ─────────────────────────
                if let Err(e) = self.validate_function_scope(&pending[i]) {
                    errors.push(e.to_string());
                    pending[i].resolved = true;
                    continue;
                }

                // ── resolve arguments from data_context ──────────────────
                let resolved_args = match self.resolve_call_arguments(&pending[i]) {
                    Ok(a)  => a,
                    Err(e) => {
                        errors.push(e.to_string());
                        pending[i].resolved = true;
                        continue;
                    }
                };

                let call_start = Instant::now();

                // ── dispatch to interpreter ──────────────────────────────
                let result = match &pending[i].namespace {
                    Some(ns) => self.execute_namespaced_call(
                        ns, &pending[i].function_name, resolved_args, pending[i].position,
                    ),
                    None => self.execute_local_call(
                        &pending[i].function_name, resolved_args, pending[i].position,
                    ),
                };

                let call_duration = call_start.elapsed();

                // ── handle outcome ───────────────────────────────────────
                match result {
                    Ok(dix_value) => {
                        // Snapshot fields we need while `pending` is not borrowed mutably
                        let path     = pending[i].path.clone();
                        let entry_idx = pending[i].entry_index;
                        let fn_name  = pending[i].function_name.clone();
                        let pos      = pending[i].position;

                        let new_value =
                            Self::convert_dix_value_to_value(&dix_value, pos);

                        // ── mutate AST in-place (O(1) entry lookup + tree walk) ──
                        self.replace_value_in_ast(entry_idx, pos, new_value.clone());

                        // ── update shared context ────────────────────────
                        self.data_context.borrow_mut()
                            .insert(path.clone(), dix_value.clone());
                        self.resolved_values.insert(path.clone(), dix_value);

                        // ── resolution history ───────────────────────────
                        self.resolution_history.push(ResolutionRecord {
                            function_name: fn_name,
                            path:          path.clone(),
                            result:        Some(new_value),
                            duration:      call_duration,
                            iteration,
                            error:         None,
                        });

                        if self.debug_config.is_enabled {
                            self.error_manager.log_info(&format!(
                                "✓ [iter {}] {} → {:?}  ({:.3}ms)",
                                iteration, path,
                                self.resolved_values.get(&path),
                                call_duration.as_secs_f64() * 1000.0
                            ));
                        }

                        pending[i].resolved = true;
                        resolved_count     += 1;
                        resolved_this_pass += 1;
                    }

                    Err(interp_err) => {
                        let path    = pending[i].path.clone();
                        let fn_name = pending[i].function_name.clone();

                        let resolver_err = ResolverError::ExecutionFailed {
                            function: fn_name.clone(),
                            location: path.clone(),
                            inner:    interp_err,
                        };

                        self.resolution_history.push(ResolutionRecord {
                            function_name: fn_name,
                            path,
                            result:        None,
                            duration:      call_duration,
                            iteration,
                            error:         Some(resolver_err.to_string()),
                        });

                        errors.push(resolver_err.to_string());
                        pending[i].resolved = true;
                    }
                }
            }

            // ── zero-progress early exit (circular dep) ──────────────────
            if resolved_this_pass == 0 && pending.iter().any(|c| !c.resolved) {
                let stuck: Vec<String> = pending.iter()
                    .filter(|c| !c.resolved)
                    .map(|c| c.path.clone())
                    .collect();
                errors.push(
                    ResolverError::CircularDependency { stuck_calls: stuck }.to_string()
                );
                break;
            }
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "[Phase 4] done — {}/{} resolved, {} iterations",
                resolved_count, total, iteration
            ));
        }

        self.error_manager.exit_scope();
        (resolved_count, errors)
    }

    // ==================== EXECUTION HELPERS ====================

    /// Route a local (non-namespaced) call through the interpreter.
    fn execute_local_call(
        &mut self,
        function_name: &str,
        arguments: Vec<DixValue>,
        position: Position,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[execute_local_call] ~{}() at {}", function_name, position
            ));
        }

        let function = self.interpreter.find_function(function_name)
            .ok_or_else(|| InterpreterError::FunctionNotFound(function_name.to_string()))?;

        self.interpreter.execute(&function, arguments)
    }

    /// Route a namespaced call (e.g. `~utils.calculate()`) through the
    /// interpreter using the namespace's exported quick_functions.
    fn execute_namespaced_call(
        &mut self,
        namespace: &str,
        function_name: &str,
        arguments: Vec<DixValue>,
        position: Position,
    ) -> Result<DixValue, InterpreterError> {
        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[execute_namespaced_call] ~{}.{}() at {}",
                namespace, function_name, position
            ));
        }

        let ns = self.symbol_table.try_get_namespace(namespace)
            .ok_or_else(|| InterpreterError::NamespaceNotFound(namespace.to_string()))?;

        let function = ns.quick_functions.get(function_name)
            .ok_or_else(|| InterpreterError::FunctionNotFound(
                format!("{}.{}", namespace, function_name)
            ))?;

        self.interpreter.execute_namespaced(function, arguments, namespace)
    }

    // ==================== SCOPE VALIDATION ====================

    /// Verify that the target function exists and is reachable before we burn
    /// CPU on argument resolution.  Returns a typed error on failure so the
    /// caller can record it and continue with remaining calls.
    fn validate_function_scope(&self, call: &FunctionCallInfo) -> Result<(), ResolverError> {
        match &call.namespace {
            Some(ns_name) => {
                let ns = self.symbol_table.try_get_namespace(ns_name)
                    .ok_or_else(|| ResolverError::NamespaceNotFound {
                        name:     ns_name.clone(),
                        location: call.path.clone(),
                        position: call.position,
                    })?;

                if !ns.quick_functions.contains_key(&call.function_name) {
                    return Err(ResolverError::FunctionNotInNamespace {
                        namespace: ns_name.clone(),
                        function:  call.function_name.clone(),
                        location:  call.path.clone(),
                        position:  call.position,
                    });
                }

                // Scope-access check: is this namespace imported in current file?
                if !self.symbol_table.is_namespace_imported(ns_name) {
                    return Err(ResolverError::InvalidFunctionScope {
                        function:       call.function_name.clone(),
                        call_scope:     ns_name.clone(),
                        allowed_scopes: self.symbol_table.imported_namespace_names(),
                        position:       call.position,
                    });
                }

                Ok(())
            }

            None => {
                if self.interpreter.find_function(&call.function_name).is_none() {
                    return Err(ResolverError::FunctionNotFound {
                        name:     call.function_name.clone(),
                        location: call.path.clone(),
                        position: call.position,
                    });
                }
                Ok(())
            }
        }
    }

    // ==================== ARGUMENT RESOLUTION ====================

    /// Returns `true` when at least one argument references an identifier that
    /// is not yet in `data_context`.  The call must be deferred to a later
    /// iteration — its dependency may resolve in this same pass.
    fn has_unresolved_dependencies(&self, call: &FunctionCallInfo) -> bool {
        let ctx = self.data_context.borrow();
        call.arguments.iter().any(|arg| Self::value_has_unresolved_ref(arg, &ctx))
    }

    fn value_has_unresolved_ref(value: &Value, ctx: &HashMap<String, DixValue>) -> bool {
        match value {
            Value::IdentifierValue { path, .. } => !ctx.contains_key(path),

            // A nested function call inside an argument is itself an
            // unresolved dependency — it will be discovered as its own
            // FunctionCallInfo and resolved independently first.
            Value::Expression { expr, .. } => {
                matches!(expr.as_ref(), Expression::FunctionCall { .. })
            }

            Value::Array { values, .. } => {
                values.iter().any(|v| Self::value_has_unresolved_ref(v, ctx))
            }

            Value::Object { properties, .. } => {
                properties.iter().any(|p| Self::value_has_unresolved_ref(&p.value, ctx))
            }

            Value::PrefixedConstructor { arguments, .. } => {
                arguments.iter().any(|a| Self::value_has_unresolved_ref(a, ctx))
            }

            _ => false, // literals are never unresolved
        }
    }

    /// Eagerly convert every argument Value → DixValue, pulling identifier
    /// values out of `data_context`.  By the time we reach here,
    /// `has_unresolved_dependencies` has already confirmed everything is
    /// available, so the only errors are logic bugs.
    fn resolve_call_arguments(
        &self,
        call: &FunctionCallInfo,
    ) -> Result<Vec<DixValue>, ResolverError> {
        let ctx = self.data_context.borrow();
        let mut out = Vec::with_capacity(call.arguments.len());

        for arg in &call.arguments {
            out.push(Self::resolve_argument_value(arg, &ctx, call.position)?);
        }

        Ok(out)
    }

    /// Recursively convert a single argument Value into a DixValue.
    fn resolve_argument_value(
        value:    &Value,
        ctx:      &HashMap<String, DixValue>,
        call_pos: Position,
    ) -> Result<DixValue, ResolverError> {
        match value {
            Value::Integer { value, .. }  => Ok(DixValue::Int(*value)),
            Value::Float   { value, .. }  => Ok(DixValue::Float(*value)),
            Value::String  { value, .. }  => Ok(DixValue::String(value.clone())),
            Value::Boolean { value, .. }  => Ok(DixValue::Bool(*value)),
            Value::Null    { .. }         => Ok(DixValue::Null),

            Value::IdentifierValue { path, .. } => {
                ctx.get(path).cloned().ok_or_else(|| ResolverError::Fatal {
                    message: format!(
                        "identifier '{}' missing from data_context at call site {}",
                        path, call_pos
                    ),
                })
            }

            Value::Array { values, .. } => {
                let items: Result<Vec<DixValue>, ResolverError> = values.iter()
                    .map(|v| Self::resolve_argument_value(v, ctx, call_pos))
                    .collect();
                Ok(DixValue::Array(items?))
            }

            Value::Object { properties, .. } => {
                let mut map = HashMap::with_capacity(properties.len());
                for prop in properties {
                    let val = Self::resolve_argument_value(&prop.value, ctx, call_pos)?;
                    map.insert(prop.key.clone(), val);
                }
                Ok(DixValue::Object(map))
            }

            _ => Err(ResolverError::Fatal {
                message: format!(
                    "cannot convert argument variant to DixValue at {}",
                    call_pos
                ),
            }),
        }
    }

    // ==================== AST MUTATION ====================

    /// Replace the expression at `target_position` inside `entries[entry_idx]`
    /// with `new_value`.  Uses direct Vec indexing for the entry lookup (O(1))
    /// then a shallow tree walk to locate the exact Value node by its unique
    /// Position — no entry rebuild, no clone of the surrounding structure.
    fn replace_value_in_ast(
        &mut self,
        entry_idx:       usize,
        target_position: Position,
        new_value:       Value,
    ) {
        let data = self.ast.data.as_mut()
            .expect("replace_value_in_ast called but ast.data is None");

        let entry = &mut data.entries[entry_idx]; // O(1) index

        let replaced = Self::replace_in_entry(entry, target_position, &new_value);

        debug_assert!(replaced, "replace_value_in_ast: target position {} not found in entry {}", target_position, entry_idx);
    }

    fn replace_in_entry(
        entry:  &mut DataEntry,
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

    /// Walk a Value tree in-place.  When the node at `target` is found it is
    /// overwritten with `new_value` and `true` is returned.  The walk short-
    /// circuits on first match — Position is unique across the AST.
    fn replace_in_value(
        value:     &mut Value,
        target:    Position,
        new_value: &Value,
    ) -> bool {
        // Check: does THIS node sit at the target position?
        if Self::value_position(value) == Some(target) {
            *value = new_value.clone();
            return true;
        }

        // Recurse into containers
        match value {
            Value::Array { values, .. } => {
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

    /// Extract the Position from any Value variant.  Every variant carries one.
    fn value_position(value: &Value) -> Option<Position> {
        match value {
            Value::Integer        { position, .. }
            | Value::Float        { position, .. }
            | Value::String       { position, .. }
            | Value::Boolean      { position, .. }
            | Value::Null         { position, .. }
            | Value::Array        { position, .. }
            | Value::Object       { position, .. }
            | Value::Expression   { position, .. }
            | Value::IdentifierValue { position, .. }
            | Value::EnumValue    { position, .. }
            | Value::PrefixedConstructor { position, .. } => Some(*position),

            // If new variants are added, this exhaustiveness check will
            // force a compile error — intentional.
        }
    }

    // ==================== VALUE ↔ DIX CONVERSIONS ====================

    /// Attempt to convert a literal Value to a DixValue.  Returns `None` for
    /// expressions / identifiers that have not yet been resolved.
    fn try_value_to_dix(value: &Value) -> Option<DixValue> {
        match value {
            Value::Integer { value, .. }  => Some(DixValue::Int(*value)),
            Value::Float   { value, .. }  => Some(DixValue::Float(*value)),
            Value::String  { value, .. }  => Some(DixValue::String(value.clone())),
            Value::Boolean { value, .. }  => Some(DixValue::Bool(*value)),
            Value::Null    { .. }         => Some(DixValue::Null),

            Value::Array { values, .. } => {
                let items: Option<Vec<DixValue>> = values.iter()
                    .map(|v| Self::try_value_to_dix(v))
                    .collect();                        // short-circuits on first None
                items.map(DixValue::Array)
            }

            Value::Object { properties, .. } => {
                let mut map = HashMap::with_capacity(properties.len());
                for prop in properties {
                    map.insert(prop.key.clone(), Self::try_value_to_dix(&prop.value)?);
                }
                Some(DixValue::Object(map))
            }

            _ => None, // Expression, Identifier, Enum — not a plain literal
        }
    }

    /// Convert an interpreter result back into an AST Value node.
    /// `position` is carried forward from the original call-site so the new
    /// node occupies the same source location in diagnostics.
    fn convert_dix_value_to_value(dix: &DixValue, position: Position) -> Value {
        match dix {
            DixValue::Int(n)    => Value::Integer { value: *n,            position },
            DixValue::Float(f)  => Value::Float   { value: *f,            position },
            DixValue::String(s) => Value::String  { value: s.clone(),     position },
            DixValue::Bool(b)   => Value::Boolean { value: *b,            position },
            DixValue::Null      => Value::Null    { position },

            DixValue::Array(items) => {
                let values: Vec<Value> = items.iter()
                    .map(|item| Self::convert_dix_value_to_value(item, position))
                    .collect();
                Value::Array { values, position }
            }

            DixValue::Object(map) => {
                let properties: Vec<ObjectProperty> = map.iter()
                    .map(|(key, val)| ObjectProperty {
                        key:      key.clone(),
                        value:    Self::convert_dix_value_to_value(val, position),
                        position,
                    })
                    .collect();
                Value::Object { properties, position }
            }
        }
    }

    // ==================== DIAGNOSTIC / DUMP UTILITIES ====================

    /// Pretty-print the current data_context.  Sorted by key for stable output.
    fn dump_data_context(&self) {
        let ctx = self.data_context.borrow();
        self.error_manager.log_info("[DIAGNOSTIC] ── data_context dump ──");
        self.error_manager.log_info(&format!("  entries: {}", ctx.len()));

        let mut keys: Vec<&String> = ctx.keys().collect();
        keys.sort_unstable();

        for key in keys {
            // Verbose: print full debug repr; normal: truncated single-line
            if self.debug_config.is_verbose {
                self.error_manager.log_debug(&format!("  {} = {:?}", key, ctx[key]));
            } else {
                let repr = format!("{:?}", ctx[key]);
                let truncated = if repr.len() > 80 {
                    format!("{}…", &repr[..77])
                } else {
                    repr
                };
                self.error_manager.log_info(&format!("  {} = {}", key, truncated));
            }
        }
    }

    /// Breakdown of discovered calls by category — logged once before the
    /// resolution loop begins.
    fn log_function_call_breakdown(&self, calls: &[FunctionCallInfo]) {
        let local_count = calls.iter().filter(|c| c.namespace.is_none()).count();
        let ns_count    = calls.iter().filter(|c| c.namespace.is_some()).count();

        self.error_manager.log_info(&format!(
            "[DIAGNOSTIC]   local calls:      {}", local_count
        ));
        self.error_manager.log_info(&format!(
            "[DIAGNOSTIC]   namespaced calls: {}", ns_count
        ));

        if self.debug_config.is_verbose {
            for call in calls {
                let prefix = call.namespace.as_ref()
                    .map(|ns| format!("{}.", ns))
                    .unwrap_or_default();
                self.error_manager.log_debug(&format!(
                    "[DIAGNOSTIC]     ~{}{}()  →  {}",
                    prefix, call.function_name, call.path
                ));
            }
        }
    }

    /// Build a failed result snapshot from the current resolver state.
    fn create_failed_result(&self, errors: Vec<String>) -> ValueResolutionResult {
        ValueResolutionResult {
            is_success:              false,
            original_ast:            Some(self.ast.clone()),
            resolved_ast:            Some(self.ast.clone()),
            function_calls_resolved: 0,
            errors,
            log_statements:          self.log_statements.clone(),
            resolution_duration:     self.start_time.elapsed(),
            resolution_history:      self.resolution_history.clone(),
        }
                }
