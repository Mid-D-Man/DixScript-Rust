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
    // ==================== PHASE 2: INITIAL DATA CONTEXT BUILD ====================

    /// Walk all DATA entries and populate data_context with resolvable literals.
    /// This gives the interpreter access to existing field values during execution.
    fn build_initial_data_context(&mut self) {
        let data_section = match &self.ast.data {
            Some(data) => data,
            None => return,
        };

        self.error_manager.create_scope("BuildInitialDataContext");

        let mut context = self.data_context.borrow_mut();
        let mut count = 0usize;

        for entry in &data_section.entries {
            count += Self::populate_context_from_entry(entry, &mut context);
        }

        drop(context); // Release borrow before any further self use

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "[Phase 4.2] Populated data_context with {} literal value(s)",
                count
            ));
        }

        self.error_manager.exit_scope();
    }

    /// Extract all statically-resolvable values from a single DataEntry.
    /// Returns the number of values inserted.
    fn populate_context_from_entry(
        entry: &DataEntry,
        context: &mut HashMap<String, DixValue>,
    ) -> usize {
        let mut count = 0usize;

        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let path = PathBuilder::data_root(name);
                if let Some(dix_val) = Self::value_to_dix_value(value) {
                    context.insert(path, dix_val);
                    count += 1;
                }
            }

            DataEntry::TableProperty { path: table_path, properties, .. } => {
                let base = PathBuilder::from_table_path(table_path);
                for prop in properties {
                    let full_path = PathBuilder::append(&base, &prop.name);
                    if let Some(dix_val) = Self::value_to_dix_value(&prop.value) {
                        context.insert(full_path, dix_val);
                        count += 1;
                    }
                }
            }

            DataEntry::GroupArray { path: group_path, items, .. } => {
                let base = PathBuilder::from_table_path(group_path);
                for (idx, item) in items.iter().enumerate() {
                    let indexed_path = PathBuilder::index(&base, idx);

                    // For object items, flatten each property into its own key
                    if let Value::Object { properties, .. } = item {
                        for prop in properties {
                            let prop_path = PathBuilder::append(&indexed_path, &prop.key);
                            if let Some(dix_val) = Self::value_to_dix_value(&prop.value) {
                                context.insert(prop_path, dix_val);
                                count += 1;
                            }
                        }
                    }

                    // Always store the item itself (object or scalar)
                    if let Some(dix_val) = Self::value_to_dix_value(item) {
                        context.insert(indexed_path, dix_val);
                        count += 1;
                    }
                }
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let base = PathBuilder::data_root(name);

                // Flatten nested properties
                if let Value::Object { properties, .. } = object {
                    for prop in properties {
                        let prop_path = PathBuilder::append(&base, &prop.key);
                        if let Some(dix_val) = Self::value_to_dix_value(&prop.value) {
                            context.insert(prop_path, dix_val);
                            count += 1;
                        }
                    }
                }

                // Store the top-level object itself
                if let Some(dix_val) = Self::value_to_dix_value(object) {
                    context.insert(base, dix_val);
                    count += 1;
                }
            }
        }

        count
    }

    /// Convert an AST Value → DixValue for context population.
    /// Returns None if the value contains unresolvable nodes (calls, unresolved identifiers).
    fn value_to_dix_value(value: &Value) -> Option<DixValue> {
        match value {
            Value::Integer { value: v, .. }  => Some(DixValue::Int(*v)),
            Value::Float   { value: v, .. }  => Some(DixValue::Float(*v)),
            Value::Str     { value: v, .. }  => Some(DixValue::Str(v.clone())),
            Value::Boolean { value: v, .. }  => Some(DixValue::Bool(*v)),
            Value::Null    { .. }            => Some(DixValue::Null),

            Value::Array { values, .. } => {
                let mut dix_values = Vec::with_capacity(values.len());
                for v in values {
                    dix_values.push(Self::value_to_dix_value(v)?); // Bail if any element unresolvable
                }
                Some(DixValue::Array(dix_values))
            }

            Value::Object { properties, .. } => {
                let mut map = HashMap::with_capacity(properties.len());
                for prop in properties {
                    let dv = Self::value_to_dix_value(&prop.value)?;
                    map.insert(prop.key.clone(), dv);
                }
                Some(DixValue::Object(map))
            }

            // Expressions, function calls, identifiers — not statically resolvable
            _ => None,
        }
    }

    // ==================== PHASE 3: FUNCTION CALL DISCOVERY ====================

    /// Walk the entire DATA section and collect all QuickFunction call sites.
    /// Calls are gathered depth-first; nested calls within arguments are NOT
    /// collected here — they surface when the outer call's args get resolved
    /// during iterative resolution.
    fn find_all_function_calls(&self) -> Vec<FunctionCallInfo> {
        let data_section = match &self.ast.data {
            Some(data) => data,
            None => return Vec::new(),
        };

        self.error_manager.create_scope("FindAllFunctionCalls");

        let mut calls: Vec<FunctionCallInfo> = Vec::new();

        for (entry_idx, entry) in data_section.entries.iter().enumerate() {
            self.collect_calls_from_entry(entry, entry_idx, &mut calls);
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "[Phase 4.3] Discovered {} function call(s) in DATA section",
                calls.len()
            ));
        }

        self.error_manager.exit_scope();
        calls
    }

    /// Dispatch into the correct traversal path for a single DataEntry
    fn collect_calls_from_entry(
        &self,
        entry: &DataEntry,
        entry_idx: usize,
        calls: &mut Vec<FunctionCallInfo>,
    ) {
        match entry {
            DataEntry::SimpleProperty { name, value, .. } => {
                let path = PathBuilder::data_root(name);
                self.collect_calls_from_value(value, &path, entry_idx, calls);
            }

            DataEntry::TableProperty { path: table_path, properties, .. } => {
                let base = PathBuilder::from_table_path(table_path);
                for prop in properties {
                    let full_path = PathBuilder::append(&base, &prop.name);
                    self.collect_calls_from_value(&prop.value, &full_path, entry_idx, calls);
                }
            }

            DataEntry::GroupArray { path: group_path, items, .. } => {
                let base = PathBuilder::from_table_path(group_path);
                for (idx, item) in items.iter().enumerate() {
                    let indexed_path = PathBuilder::index(&base, idx);
                    match item {
                        Value::Object { properties, .. } => {
                            for prop in properties {
                                let prop_path = PathBuilder::append(&indexed_path, &prop.key);
                                self.collect_calls_from_value(&prop.value, &prop_path, entry_idx, calls);
                            }
                        }
                        _ => {
                            self.collect_calls_from_value(item, &indexed_path, entry_idx, calls);
                        }
                    }
                }
            }

            DataEntry::ObjectProperty { name, object, .. } => {
                let base = PathBuilder::data_root(name);
                if let Value::Object { properties, .. } = object {
                    for prop in properties {
                        let prop_path = PathBuilder::append(&base, &prop.key);
                        self.collect_calls_from_value(&prop.value, &prop_path, entry_idx, calls);
                    }
                }
            }
        }
    }

    /// Recursively scan a Value for function calls.
    /// Only top-level calls at each value site are collected; nested calls
    /// inside arguments are deferred to iterative resolution.
    fn collect_calls_from_value(
        &self,
        value: &Value,
        path: &str,
        entry_idx: usize,
        calls: &mut Vec<FunctionCallInfo>,
    ) {
        match value {
            Value::Expression { expr, .. } => {
                if let Expression::FunctionCall {
                    function_name,
                    namespace,
                    arguments,
                    position: call_pos,
                } = expr.as_ref()
                {
                    if self.debug_config.is_verbose {
                        self.error_manager.log_debug(&format!(
                            "[FindCalls] Found {}{}() at '{}'",
                            namespace.as_ref().map(|n| format!("{}.", n)).unwrap_or_default(),
                            function_name,
                            path,
                        ));
                    }

                    calls.push(FunctionCallInfo {
                        path:          path.to_string(),
                        function_name: function_name.clone(),
                        namespace:     namespace.clone(),
                        arguments:     arguments.clone(),
                        position:      *call_pos,
                        entry_index:   entry_idx,
                    });
                }
                // Non-call expressions (arithmetic, etc.) — no nested function discovery needed here
            }

            Value::Array { values, .. } => {
                for (idx, item) in values.iter().enumerate() {
                    let elem_path = PathBuilder::index(path, idx);
                    self.collect_calls_from_value(item, &elem_path, entry_idx, calls);
                }
            }

            Value::Object { properties, .. } => {
                for prop in properties {
                    let prop_path = PathBuilder::append(path, &prop.key);
                    self.collect_calls_from_value(&prop.value, &prop_path, entry_idx, calls);
                }
            }

            Value::PrefixedConstructor { arguments, .. } => {
                for (idx, arg) in arguments.iter().enumerate() {
                    let arg_path = format!("{}.__ctor_arg{}", path, idx);
                    self.collect_calls_from_value(arg, &arg_path, entry_idx, calls);
                }
            }

            _ => {} // Literals, identifiers, enums (already resolved) — nothing to collect
        }
    }

    // ==================== PHASE 4: ITERATIVE RESOLUTION ====================

    /// Core iterative loop: execute discovered calls, replace results in AST,
    /// update data_context, repeat until all resolved or stuck.
    /// Returns (number_resolved, error_messages).
    fn execute_iterative_resolution(
        &mut self,
        mut pending_calls: Vec<FunctionCallInfo>,
    ) -> (usize, Vec<String>) {
        self.error_manager.create_scope("ExecuteIterativeResolution");

        let absolute_limit  = self.compute_absolute_recursion_limit();
        let max_iterations  = (pending_calls.len() * 2) + 10; // Dynamic ceiling

        let mut iteration     = 0usize;
        let mut success_count = 0usize;
        let mut errors: Vec<String> = Vec::new();

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "[Phase 4.4] Iterative resolution START — {} call(s), max_iter={}, abs_limit={}",
                pending_calls.len(), max_iterations, absolute_limit
            ));
        }

        loop {
            if pending_calls.is_empty() {
                break;
            }

            if iteration >= max_iterations {
                let stuck: Vec<String> = pending_calls.iter()
                    .map(|c| format!("{}() at '{}'", c.function_name, c.path))
                    .collect();

                let err = ResolverError::CircularDependency { stuck_calls: stuck };
                errors.push(err.to_string());
                self.error_manager.log_error(&err.to_string());
                break;
            }

            let mut still_pending: Vec<FunctionCallInfo> = Vec::new();
            let mut resolved_this_round = 0usize;

            for call in pending_calls {
                // --- Guard: are all arguments currently resolvable? ---
                if !self.are_arguments_resolvable(&call) {
                    if self.debug_config.is_verbose {
                        self.error_manager.log_debug(&format!(
                            "[Iterative] Defer {}() at '{}' — arg(s) not yet resolved",
                            call.function_name, call.path
                        ));
                    }
                    still_pending.push(call);
                    continue;
                }

                // --- Guard: scope / namespace validation ---
                if let Err(e) = self.validate_function_scope(&call) {
                    errors.push(e.to_string());
                    self.error_manager.log_error(&e.to_string());

                    self.resolution_history.push(ResolutionRecord {
                        function_name:      call.function_name.clone(),
                        path:               call.path.clone(),
                        result:             None,
                        arguments_snapshot: call.arguments.iter().map(|a| format!("{:?}", a)).collect(),
                        duration:           std::time::Duration::ZERO,
                        error:              Some(e.to_string()),
                    });
                    continue; // Don't abort the whole pass — keep resolving others
                }

                // --- Execute ---
                let call_start = Instant::now();

                match self.execute_single_call(&call) {
                    Ok(dix_result) => {
                        let elapsed = call_start.elapsed();

                        if self.debug_config.is_enabled {
                            self.error_manager.log_info(&format!(
                                "[Phase 4.4] ✓ {}() at '{}' → {:?}  ({:.3}ms)",
                                call.function_name, call.path, dix_result,
                                elapsed.as_secs_f64() * 1000.0,
                            ));
                        }

                        // DixValue → AST Value
                        let ast_value = self.dix_value_to_ast_value(&dix_result, call.position);

                        // Mutate AST in-place (O(1) Vec index write — no rebuild)
                        self.replace_value_at_path(&call.path, ast_value);

                        // Update shared data_context so subsequent calls can read this value
                        self.data_context.borrow_mut().insert(call.path.clone(), dix_result.clone());

                        // Mirror into resolved_values for final dump
                        self.resolved_values.insert(call.path.clone(), dix_result.clone());

                        // History record
                        self.resolution_history.push(ResolutionRecord {
                            function_name:      call.function_name.clone(),
                            path:               call.path.clone(),
                            result:             Some(dix_result),
                            arguments_snapshot: call.arguments.iter().map(|a| format!("{:?}", a)).collect(),
                            duration:           elapsed,
                            error:              None,
                        });

                        success_count      += 1;
                        resolved_this_round += 1;
                    }

                    Err(e) => {
                        let err_msg = e.to_string();
                        errors.push(err_msg.clone());
                        self.error_manager.log_error(&err_msg);

                        self.resolution_history.push(ResolutionRecord {
                            function_name:      call.function_name.clone(),
                            path:               call.path.clone(),
                            result:             None,
                            arguments_snapshot: call.arguments.iter().map(|a| format!("{:?}", a)).collect(),
                            duration:           call_start.elapsed(),
                            error:              Some(err_msg),
                        });
                    }
                }
            }

            // --- Stall detection: if nothing moved and calls remain → circular ---
            if resolved_this_round == 0 && !still_pending.is_empty() {
                let stuck: Vec<String> = still_pending.iter()
                    .map(|c| format!("{}() at '{}'", c.function_name, c.path))
                    .collect();

                let err = ResolverError::CircularDependency { stuck_calls: stuck };
                errors.push(err.to_string());
                self.error_manager.log_error(&err.to_string());
                break;
            }

            pending_calls = still_pending;
            iteration     += 1;
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "[Phase 4.4] Iterative resolution DONE — {} resolved, {} error(s), {} iteration(s)",
                success_count, errors.len(), iteration
            ));
        }

        self.error_manager.exit_scope();
        (success_count, errors)
    }

    // ==================== RECURSION LIMIT ====================

    /// Compute the absolute recursion ceiling.
    /// Base of 64 — scales linearly with DATA entry count and QuickFunction count,
    /// hard-capped at 1024 to prevent runaway nested execution.
    fn compute_absolute_recursion_limit(&self) -> usize {
        const BASE: usize  = 64;
        const CAP:  usize  = 1024;

        let entry_count = self.ast.data
            .as_ref()
            .map(|d| d.entries.len())
            .unwrap_or(0);

        let fn_count = self.ast.quick_functions
            .as_ref()
            .map(|qf| qf.functions.len())
            .unwrap_or(0);

        (BASE + entry_count * 2 + fn_count * 4).min(CAP)
    }

    // ==================== ARGUMENT RESOLUTION GUARDS ====================

    /// Returns true when every argument in the call can be turned into a DixValue
    /// right now (literals pass always; identifiers must exist in context or constants).
    fn are_arguments_resolvable(&self, call: &FunctionCallInfo) -> bool {
        let context = self.data_context.borrow();
        call.arguments.iter().all(|arg| self.is_value_resolvable(arg, &context))
    }

    /// Recursive resolvability check for a single Value
    fn is_value_resolvable(&self, value: &Value, context: &HashMap<String, DixValue>) -> bool {
        match value {
            Value::Integer { .. }
            | Value::Float   { .. }
            | Value::Str     { .. }
            | Value::Boolean { .. }
            | Value::Null    { .. } => true,

            Value::IdentifierValue { name, .. } => {
                context.contains_key(name)
                    || self.symbol_table.try_get_constant(name).is_some()
            }

            Value::Array { values, .. } => {
                values.iter().all(|v| self.is_value_resolvable(v, context))
            }

            Value::Object { properties, .. } => {
                properties.iter().all(|p| self.is_value_resolvable(&p.value, context))
            }

            // Expressions that are pure literals (already folded) are fine;
            // anything else (nested call, unresolved access) blocks.
            Value::Expression { expr, .. } => {
                matches!(expr.as_ref(), Expression::Literal(_))
            }

            _ => false,
        }
    }

    // ==================== SCOPE & NAMESPACE VALIDATION ====================

    /// Verify that the target function exists and is reachable from the call site.
    fn validate_function_scope(&self, call: &FunctionCallInfo) -> Result<(), ResolverError> {
        if let Some(ns_name) = &call.namespace {
            // ── Imported function ──
            let namespace = self.symbol_table
                .try_get_namespace(ns_name)
                .ok_or_else(|| ResolverError::NamespaceNotFound {
                    name:     ns_name.clone(),
                    location: call.path.clone(),
                    position: call.position,
                })?;

            if !namespace.quick_functions.contains_key(&call.function_name) {
                return Err(ResolverError::FunctionNotInNamespace {
                    namespace: ns_name.clone(),
                    function:  call.function_name.clone(),
                    location:  call.path.clone(),
                    position:  call.position,
                });
            }

            Ok(())
        } else {
            // ── Local function ──
            let exists = self.ast.quick_functions
                .as_ref()
                .map(|qf| qf.functions.iter().any(|f| f.name == call.function_name))
                .unwrap_or(false);

            if !exists {
                return Err(ResolverError::FunctionNotFound {
                    name:     call.function_name.clone(),
                    location: call.path.clone(),
                    position: call.position,
                });
            }

            Ok(())
        }
    }

    // ==================== SINGLE-CALL EXECUTION ====================

    /// Resolve arguments → locate function → hand off to interpreter → return result.
    fn execute_single_call(&mut self, call: &FunctionCallInfo) -> Result<DixValue, ResolverError> {
        // 1. Resolve all arguments from current context into DixValues
        let resolved_args = self.resolve_call_arguments(&call.arguments, &call.position)?;

        // 2. Locate the QuickFunction definition
        let function = self.locate_function(call)?;

        // 3. Build ExecutionContext (shares data_context with resolver)
        let exec_context = ExecutionContext::new(
            Rc::clone(&self.data_context),
            &self.debug_config,
        );

        // 4. Delegate to FunctionInterpreter
        self.interpreter
            .execute(&function, resolved_args, exec_context, call.namespace.as_deref())
            .map_err(|e| ResolverError::ExecutionFailed {
                function: call.function_name.clone(),
                location: call.path.clone(),
                inner:    e,
            })
    }

    /// Locate a QuickFunction — local or imported — cloning it out of the
    /// relevant source so we hold no borrow across the interpreter call.
    fn locate_function(&self, call: &FunctionCallInfo) -> Result<QuickFunction, ResolverError> {
        if let Some(ns_name) = &call.namespace {
            let ns = self.symbol_table
                .try_get_namespace(ns_name)
                .ok_or_else(|| ResolverError::NamespaceNotFound {
                    name:     ns_name.clone(),
                    location: call.path.clone(),
                    position: call.position,
                })?;

            ns.quick_functions
                .get(&call.function_name)
                .cloned()
                .ok_or_else(|| ResolverError::FunctionNotInNamespace {
                    namespace: ns_name.clone(),
                    function:  call.function_name.clone(),
                    location:  call.path.clone(),
                    position:  call.position,
                })
        } else {
            self.ast.quick_functions
                .as_ref()
                .and_then(|qf| qf.functions.iter().find(|f| f.name == call.function_name))
                .cloned()
                .ok_or_else(|| ResolverError::FunctionNotFound {
                    name:     call.function_name.clone(),
                    location: call.path.clone(),
                    position: call.position,
                })
        }
    }

    /// Turn each argument Value into a DixValue using current data_context + constants.
    fn resolve_call_arguments(
        &self,
        arguments: &[Value],
        position: &Position,
    ) -> Result<Vec<DixValue>, ResolverError> {
        let context = self.data_context.borrow();
        let mut resolved = Vec::with_capacity(arguments.len());

        for (idx, arg) in arguments.iter().enumerate() {
            let dix_val = self.resolve_argument_value(arg, &context)
                .ok_or_else(|| ResolverError::Fatal {
                    message: format!(
                        "Argument #{} could not be resolved at {} — value: {:?}",
                        idx, position, arg
                    ),
                })?;
            resolved.push(dix_val);
        }

        Ok(resolved)
    }

    /// Resolve a single argument Value → DixValue.
    /// Mirrors value_to_dix_value but additionally checks data_context and constants
    /// for IdentifierValue nodes.
    fn resolve_argument_value(
        &self,
        value: &Value,
        context: &HashMap<String, DixValue>,
    ) -> Option<DixValue> {
        match value {
            Value::Integer { value: v, .. }  => Some(DixValue::Int(*v)),
            Value::Float   { value: v, .. }  => Some(DixValue::Float(*v)),
            Value::Str     { value: v, .. }  => Some(DixValue::Str(v.clone())),
            Value::Boolean { value: v, .. }  => Some(DixValue::Bool(*v)),
            Value::Null    { .. }            => Some(DixValue::Null),

            Value::IdentifierValue { name, .. } => {
                // Priority 1: data_context (already-resolved DATA fields)
                if let Some(val) = context.get(name) {
                    return Some(val.clone());
                }
                // Priority 2: symbol_table constants (CONST declarations)
                self.symbol_table.try_get_constant(name).cloned()
            }

            Value::Array { values, .. } => {
                let mut dix_values = Vec::with_capacity(values.len());
                for v in values {
                    dix_values.push(self.resolve_argument_value(v, context)?);
                }
                Some(DixValue::Array(dix_values))
            }

            Value::Object { properties, .. } => {
                let mut map = HashMap::with_capacity(properties.len());
                for prop in properties {
                    let dv = self.resolve_argument_value(&prop.value, context)?;
                    map.insert(prop.key.clone(), dv);
                }
                Some(DixValue::Object(map))
            }

            _ => None, // Unresolvable at this stage
        }
    }

    // ==================== AST MUTATION HELPERS ====================

    /// Replace the value at a dot-notation path inside the AST's DATA section.
    /// Uses direct Vec index writes — no rebuild, no clone of the entries Vec.
    fn replace_value_at_path(&mut self, path: &str, new_value: Value) {
        let data_section = match &mut self.ast.data {
            Some(data) => data,
            None       => return,
        };

        let segments = PathBuilder::parse(path);
        if segments.is_empty() { return; }

        for entry in data_section.entries.iter_mut() {
            if Self::entry_matches_segments(entry, &segments) {
                Self::write_value_into_entry(entry, &segments, new_value);
                return;
            }
        }

        if self.debug_config.is_verbose {
            self.error_manager.log_debug(&format!(
                "[ReplaceValue] No entry matched path '{}'", path
            ));
        }
    }

    /// Does this entry's root path match the leading segments?
    fn entry_matches_segments(entry: &DataEntry, segments: &[PathSegment]) -> bool {
        match entry {
            DataEntry::SimpleProperty { name, .. }
            | DataEntry::ObjectProperty { name, .. } => {
                segments.len() >= 2 && segments[1].name == *name
            }
            DataEntry::TableProperty { path: tp, .. }
            | DataEntry::GroupArray   { path: tp, .. } => {
                PathBuilder::table_path_matches(tp, segments)
            }
        }
    }

    /// Navigate into the matched entry and overwrite the target value in place.
    fn write_value_into_entry(entry: &mut DataEntry, segments: &[PathSegment], new_value: Value) {
        match entry {
            DataEntry::SimpleProperty { value, .. } => {
                *value = new_value;
            }

            DataEntry::TableProperty { properties, .. } => {
                // Target is the last segment's name
                if let Some(target) = segments.last() {
                    for prop in properties.iter_mut() {
                        if prop.name == target.name {
                            prop.value = new_value;
                            return;
                        }
                    }
                }
            }

            DataEntry::GroupArray { items, .. } => {
                // Find the indexed segment, then drill into the object if needed
                if let Some(idx_seg) = segments.iter().find(|s| s.index.is_some()) {
                    let idx = idx_seg.index.unwrap();
                    if idx >= items.len() { return; }

                    // If last segment IS the index segment → replace entire item
                    if segments.last().map(|s| std::ptr::eq(s, idx_seg)).unwrap_or(false) {
                        items[idx] = new_value;
                        return;
                    }

                    // Otherwise drill into the object at [idx]
                    if let Value::Object { properties, .. } = &mut items[idx] {
                        if let Some(target) = segments.last() {
                            for prop in properties.iter_mut() {
                                if prop.key == target.name {
                                    prop.value = new_value;
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            DataEntry::ObjectProperty { object, .. } => {
                if let Value::Object { properties, .. } = object {
                    if let Some(target) = segments.last() {
                        for prop in properties.iter_mut() {
                            if prop.key == target.name {
                                prop.value = new_value;
                                return;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Convert a DixValue result back into an AST Value, stamping the original
    /// call-site Position onto every node (preserves source mapping).
    fn dix_value_to_ast_value(&self, dix_value: &DixValue, position: Position) -> Value {
        match dix_value {
            DixValue::Int(v)   => Value::Integer { value: *v, position },
            DixValue::Float(v) => Value::Float   { value: *v, position },
            DixValue::Str(s)   => Value::Str     { value: s.clone(), position },
            DixValue::Bool(b)  => Value::Boolean { value: *b, position },
            DixValue::Null     => Value::Null    { position },

            DixValue::Array(items) => {
                Value::Array {
                    values: items.iter()
                        .map(|item| self.dix_value_to_ast_value(item, position))
                        .collect(),
                    position,
                }
            }

            DixValue::Object(map) => {
                Value::Object {
                    properties: map.iter()
                        .map(|(key, val)| ObjectProperty {
                            key:      key.clone(),
                            value:    self.dix_value_to_ast_value(val, position),
                            position,
                        })
                        .collect(),
                    position,
                }
            }

            // Fallback: stringify unknown DixValue variants
            _ => Value::Str { value: format!("{:?}", dix_value), position },
        }
    }

    // ==================== DIAGNOSTIC UTILITIES ====================

    /// Dump every key-value pair currently in data_context (verbose debug only)
    fn dump_data_context(&self) {
        let context = self.data_context.borrow();

        self.error_manager.log_info("========== DATA CONTEXT DUMP ==========");
        self.error_manager.log_info(&format!("  entries: {}", context.len()));

        let mut keys: Vec<&String> = context.keys().collect();
        keys.sort_unstable();

        for key in keys {
            if let Some(val) = context.get(key) {
                self.error_manager.log_info(&format!("  {} = {:?}", key, val));
            }
        }
        self.error_manager.log_info("========================================");
    }

    /// Log a function-call breakdown grouped by name with local/imported counts
    fn log_function_call_breakdown(&self, calls: &[FunctionCallInfo]) {
        let local_count    = calls.iter().filter(|c| c.namespace.is_none()).count();
        let imported_count = calls.iter().filter(|c| c.namespace.is_some()).count();

        self.error_manager.log_info(&format!(
            "[DIAGNOSTIC] Breakdown: {} local, {} imported",
            local_count, imported_count
        ));

        let mut by_name: HashMap<&str, usize> = HashMap::new();
        for call in calls {
            *by_name.entry(&call.function_name).or_insert(0) += 1;
        }

        let mut sorted: Vec<_> = by_name.iter().collect();
        sorted.sort_unstable_by_key(|(name, _)| *name);

        for (name, count) in sorted {
            self.error_manager.log_info(&format!("    {}()  ×{}", name, count));
        }
    }

    /// Assemble a failed ValueResolutionResult (used when an early phase aborts)
    fn create_failed_result(&self, errors: Vec<String>) -> ValueResolutionResult {
        ValueResolutionResult {
            is_success:              false,
            original_ast:            Some(self.ast.clone()),
            resolved_ast:            None,
            function_calls_resolved: 0,
            errors,
            log_statements:          self.log_statements.clone(),
            resolution_duration:     self.start_time.elapsed(),
            resolution_history:      self.resolution_history.clone(),
        }
    }
            }
