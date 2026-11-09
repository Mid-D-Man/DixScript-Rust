// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/ValueResolution/value_resolver.rs"
// ============================================================================
//! Orchestrates compile-time value resolution across five phases.
//!
//! Phase 1: Enum pre-resolution — EnumValue/EnumAccess → Integer.
//! Phase 2: Initial data context build from literal DATA entries.
//! Phase 3: QuickFunction call discovery via ASTWalker.
//! Phase 4: Iterative execution and AST replacement.
//! Phase 5: Remaining Identifier reference resolution.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use web_time::Instant;

use chrono::Utc;
use crate::Utilities::RustcHash::FxHashMap;

use crate::Builtins::Core::{DixType, DixValue};
use crate::Builtins::Resolver;
use crate::Compiler::AST::arena::{Arena, DocData, Document};
use crate::Compiler::AST::{
    DataEntry, DataSection, DixScript, EnumDeclaration, EnumField, EnumsSection, Expression,
    ObjectProperty, Position, PropertyAssignment, Value,
};
use crate::Compiler::Core::DebugMode;
use crate::Compiler::Utilities::{PathBuilder, SymbolTable, ImportedNamespace};
use crate::ErrorManager::{DebugConfig, ErrorManager};

use super::ast_walker::ASTWalker;
use super::data_context;
use super::enum_resolution::{resolve_data_enums, EnumPass};
use super::execution_context::ExecutionContext;
use super::function_interpreter::{FunctionInterpreter, InterpreterError};
use super::supporting_classes::{FunctionCallInfo, ResolutionRecord, ValueResolutionResult};

const MAX_RESOLUTION_ITERATIONS: usize = 10_000;
pub(super) const MIN_CAPACITY: usize = 8;

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

// ==================== DATA SOURCE ====================

/// Where the arena phases (1 to 3) of a resolver read the data section from.
enum DataSource<'a> {
    /// Nothing attached; `resolve` makes this a [`Scratch`](DataSource::Scratch) when the owned
    /// tree has a data section.
    None,
    /// The caller's document; its `data` decides whether there is a data section.
    Document(&'a mut Document),
    /// A copy of the owned data section, made once for the run.
    Scratch { arena: Arena, data: DocData },
}

impl DataSource<'_> {
    /// The arena and the data section in it, or `None` when there is no data section.
    fn parts(&mut self) -> Option<(&mut Arena, &mut DocData)> {
        match self {
            DataSource::None => None,
            DataSource::Document(document) => {
                let Document { arena, data, .. } = &mut **document;
                data.as_mut().map(|data| (arena, data))
            }
            DataSource::Scratch { arena, data } => Some((arena, data)),
        }
    }
}

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
    /// Where phases 1 to 3 read the data section from: the document the caller attached, or a
    /// scratch copy of the owned data made when the resolver starts.
    data: DataSource<'a>,
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
            data: DataSource::None,
        }
    }
    /// Gives phases 1 to 3 (enum pre-resolution, the data context and the call finder) the
    /// document the data section was parsed into.
    ///
    /// They then read the document's `@DATA` from its arena instead of from a scratch copy of the
    /// owned data section, phase 1 rewrites it there, and whether a data section exists is decided
    /// by the document. The owned tree passed to the constructor is still the one the later
    /// phases read and the result is built from.
    pub fn with_document(mut self, document: &'a mut Document) -> Self {
        self.data = DataSource::Document(document);
        self
    }

    // ==================== MAIN ORCHESTRATION ====================

    pub fn resolve(mut self) -> ValueResolutionResult {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("[Phase 4.1] Starting value resolution");
        }

        let original_ast = self.ast.clone();

        self.attach_scratch_data();

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

    /// Without an attached document, copies the owned data section into a scratch arena once,
    /// for phases 1 to 3 to share.
    fn attach_scratch_data(&mut self) {
        if matches!(self.data, DataSource::None) {
            if let Some(owned) = self.ast.data.as_ref() {
                let mut arena = Arena::new();
                let data = DocData::from_owned(&mut arena, owned);
                self.data = DataSource::Scratch { arena, data };
            }
        }
    }

    // ==================== PHASE 1: ENUM PRE-RESOLUTION ====================

    /// Resolves the enum references of the data section (phase 1).
    ///
    /// The walk itself is `enum_resolution::resolve_data_enums`; this method decides which tree
    /// it reads and keeps the owned tree the later phases read in step with the result.
    fn resolve_all_enum_values(&mut self) -> Result<(), ResolverError> {
        let Some(pass) = self.run_enum_pass()? else {
            if self.debug_config.is_enabled {
                self.error_manager
                    .log_debug("No DATA section — skipping enum pre-resolution");
            }
            return Ok(());
        };

        self.merge_imported_enums(pass.usages);

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Resolved {} local + {} imported enum values",
                pass.local, pass.imported
            ));
        }

        Ok(())
    }

    /// Runs the enum walk over the data section, or returns `None` when there is none.
    ///
    /// The walk reads and rewrites the data in the arena it lives in (see [`DataSource`]). The
    /// owned data the later phases read is replaced by an export of the result only when the walk
    /// changed something, or when the owned tree had no data of its own.
    fn run_enum_pass(&mut self) -> Result<Option<EnumPass>, ResolverError> {
        let Some((arena, data)) = self.data.parts() else { return Ok(None) };
        if self.debug_config.is_enabled {
            self.error_manager
                .log_info("Pre-processing: resolving all enum values to integers");
        }
        let pass = resolve_data_enums(arena, data, self.symbol_table)?;
        if pass.changed || self.ast.data.is_none() {
            self.ast.data = Some(data.export(arena));
        }
        Ok(Some(pass))
    }

    /// Merges the imported enums the walk referenced into this file's own `@ENUMS` section, so
    /// `DixData::from_ast`'s `extract_enums_section` and `value_encoder.rs`'s `local_enums`
    /// table, neither of which has symbol table access, see them too.
    ///
    /// Each enum is keyed by its full qualified name (`Types.Priority`), which can never collide
    /// with a local enum's bare name (`Priority`), so the merge is additive and never shadows a
    /// real local declaration. The section is created when the file has none.
    fn merge_imported_enums(&mut self, usages: FxHashMap<String, HashMap<String, i32>>) {
        if usages.is_empty() {
            return;
        }

        let section = self.ast.enums.get_or_insert_with(|| EnumsSection {
            enums: Vec::new(),
            position: Position::UNKNOWN,
        });

        let mut merged_count = 0usize;
        for (qualified_name, fields) in usages {
            if section.enums.iter().any(|d| d.name == qualified_name) {
                continue; // already merged (e.g. resolve() called more than once)
            }

            let mut field_list: Vec<EnumField> = fields
                .into_iter()
                .map(|(field_name, field_value)| {
                    EnumField::new(field_name, Some(field_value), Position::UNKNOWN)
                })
                .collect();
            // Cosmetic only (affects declaration order if this synthesized section is ever
            // introspected or serialized back to source); lookups are by name.
            field_list.sort_by_key(|f| f.value.unwrap_or(0));

            section.enums.push(EnumDeclaration::new(
                qualified_name,
                field_list,
                Position::UNKNOWN,
            ));
            merged_count += 1;
        }

        if self.debug_config.is_enabled {
            self.error_manager.log_info(&format!(
                "Merged {} imported enum declaration(s) into local @ENUMS",
                merged_count
            ));
        }
    }

    // ==================== PHASE 2: DATA CONTEXT BUILD ====================

    /// Builds the data context from the data section (see `data_context.rs`).
    fn build_initial_data_context(&mut self) {
        let Some((arena, data)) = self.data.parts() else { return };

        let estimated = (data.entries.len() * 4).max(MIN_CAPACITY);
        let mut context = self.data_context.borrow_mut();
        context.reserve(estimated);

        let total_inserted: usize = data
            .entries
            .iter()
            .map(|entry| data_context::populate_entry(arena, entry, &mut context))
            .sum();

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
        let Some((arena, data)) = self.data.parts() else { return };

        // Up to 8 passes handles chains like a=b, b=c, c=1 that resolve in order.
        for pass in 0..8usize {
            let mut changed = false;
            let mut ctx = self.data_context.borrow_mut();

            for entry in &data.entries {
                changed |= data_context::resolve_aliases_in_entry(arena, entry, &mut ctx);
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

    // ==================== PHASE 3: FUNCTION CALL DISCOVERY ====================

    fn find_all_function_calls(&mut self) -> Vec<FunctionCallInfo> {
        let Some((arena, data)) = self.data.parts() else { return Vec::new() };

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

        walker.find_all_in(arena, data)
    }



    // ==================== PHASE 4: ITERATIVE RESOLUTION ====================
fn execute_iterative_resolution(
    &mut self,
    function_calls: Vec<FunctionCallInfo>,
) -> (usize, Vec<String>) {
    let total = function_calls.len();

    // Where each call sits in the data tree, found once. Replacing a resolved call then costs
    // the depth of its node instead of a search of the whole tree.
    let target_positions: HashSet<Position> = function_calls.iter().map(|c| c.position).collect();
    let replace_index = self.index_call_targets(&target_positions);

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
                    self.replace_value_at(&replace_index, pos, new_value);
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

    // ==================== AST REPLACEMENT ====================

    /// Path to the first node (depth first, the order the search below uses) at each target
    /// position: the entry index, then at every level the index of the child to step into
    /// (property, item, array element or constructor argument).
    fn index_call_targets(&self, targets: &HashSet<Position>) -> HashMap<Position, Vec<u32>> {
        let mut found = HashMap::with_capacity(targets.len());
        let Some(data) = self.ast.data.as_ref() else {
            return found;
        };
        let mut path: Vec<u32> = Vec::with_capacity(16);
        for (i, entry) in data.entries.iter().enumerate() {
            if found.len() == targets.len() {
                break;
            }
            path.push(i as u32);
            Self::index_entry(entry, targets, &mut path, &mut found);
            path.pop();
        }
        found
    }

    fn index_entry(
        entry: &DataEntry,
        targets: &HashSet<Position>,
        path: &mut Vec<u32>,
        found: &mut HashMap<Position, Vec<u32>>,
    ) {
        match entry {
            DataEntry::SimpleProperty { value, .. } => Self::index_value(value, targets, path, found),
            DataEntry::TableProperty { properties, .. } => {
                for (i, prop) in properties.iter().enumerate() {
                    path.push(i as u32);
                    Self::index_value(&prop.value, targets, path, found);
                    path.pop();
                }
            }
            DataEntry::GroupArray { items, .. } => {
                for (i, item) in items.iter().enumerate() {
                    path.push(i as u32);
                    Self::index_value(item, targets, path, found);
                    path.pop();
                }
            }
            DataEntry::ObjectProperty { object, .. } => Self::index_value(object, targets, path, found),
        }
    }

    fn index_value(
        value: &Value,
        targets: &HashSet<Position>,
        path: &mut Vec<u32>,
        found: &mut HashMap<Position, Vec<u32>>,
    ) {
        if let Some(pos) = Self::value_position(value) {
            if targets.contains(&pos) && !found.contains_key(&pos) {
                found.insert(pos, path.clone());
            }
        }
        match value {
            Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                for (i, item) in values.iter().enumerate() {
                    path.push(i as u32);
                    Self::index_value(item, targets, path, found);
                    path.pop();
                }
            }
            Value::Object { properties, .. } => {
                for (i, prop) in properties.iter().enumerate() {
                    path.push(i as u32);
                    Self::index_value(&prop.value, targets, path, found);
                    path.pop();
                }
            }
            Value::PrefixedConstructor { arguments, .. } => {
                for (i, arg) in arguments.iter().enumerate() {
                    path.push(i as u32);
                    Self::index_value(arg, targets, path, found);
                    path.pop();
                }
            }
            _ => {}
        }
    }

    /// The node a path from `index_call_targets` points at, or `None` if the tree no longer has
    /// that shape.
    fn slot_at_path<'s>(entries: &'s mut [DataEntry], path: &[u32]) -> Option<&'s mut Value> {
        let (&first, rest) = path.split_first()?;
        let entry = entries.get_mut(first as usize)?;
        let (mut value, rest): (&mut Value, &[u32]) = match entry {
            DataEntry::SimpleProperty { value, .. } => (value, rest),
            DataEntry::ObjectProperty { object, .. } => (object, rest),
            DataEntry::TableProperty { properties, .. } => {
                let (&i, rest) = rest.split_first()?;
                (&mut properties.get_mut(i as usize)?.value, rest)
            }
            DataEntry::GroupArray { items, .. } => {
                let (&i, rest) = rest.split_first()?;
                (items.get_mut(i as usize)?, rest)
            }
        };
        for &i in rest {
            value = match value {
                Value::Array { values, .. } | Value::NestedArray { values, .. } => {
                    values.get_mut(i as usize)?
                }
                Value::Object { properties, .. } => &mut properties.get_mut(i as usize)?.value,
                Value::PrefixedConstructor { arguments, .. } => arguments.get_mut(i as usize)?,
                _ => return None,
            };
        }
        Some(value)
    }

    /// Replace the node at `target`. Uses the indexed path and checks that the node there still
    /// has the target position. If an earlier replacement changed the tree under the index, it
    /// falls back to the search, so the result is the same as searching every time.
    fn replace_value_at(
        &mut self,
        index: &HashMap<Position, Vec<u32>>,
        target: Position,
        new_value: Value,
    ) {
        let Some(path) = index.get(&target) else {
            // Not in the tree when the index was built, so a search would not find it either.
            return;
        };
        if let Some(data) = self.ast.data.as_mut() {
            if let Some(slot) = Self::slot_at_path(&mut data.entries, path) {
                if Self::value_position(slot) == Some(target) {
                    *slot = new_value;
                    return;
                }
            }
        }
        self.replace_value_in_ast_by_location("", target, new_value);
    }

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
