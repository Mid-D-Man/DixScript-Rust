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
