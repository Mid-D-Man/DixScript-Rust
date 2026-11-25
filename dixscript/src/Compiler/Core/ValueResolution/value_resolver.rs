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

use crate::Builtins::Core::DixValue;
use crate::Builtins::Resolver;
use crate::Compiler::AST::arena::{Arena, DocData, Document, ExprId, ExprNode, ValueId, ValueNode};
use crate::Compiler::AST::{DixScript, EnumDeclaration, EnumField, EnumsSection, Position};
use crate::Compiler::Core::DebugMode;
use crate::Compiler::Utilities::SymbolTable;
use crate::ErrorManager::{DebugConfig, ErrorManager};

use super::ast_walker::ASTWalker;
use super::data_context;
use super::enum_resolution::{resolve_data_enums, EnumPass};
use super::function_interpreter::{FunctionInterpreter, InterpreterError};
use super::identifier_resolution;
use super::supporting_classes::{FunctionCallInfo, ResolutionRecord, ValueResolutionResult};
use super::write_back;

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
    /// Where the phases read the data section from: the document the caller attached, or a
    /// scratch copy of the owned data made when the resolver starts.
    data: DataSource<'a>,
    /// Whether any phase has changed the data section in the arena, so that the owned section
    /// the result is built from has to be replaced by an export of it.
    data_changed: bool,
    /// Whether the attached document is where the result goes, set by
    /// [`with_document_product`](Self::with_document_product). The owned tree then only supplies
    /// the functions, and nothing is exported back into it.
    document_product: bool,
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
        let func_count = ast
            .quick_functions
            .as_ref()
            .map(|qf| qf.functions.len())
            .unwrap_or(0);

        let data_entry_count = ast
            .data
            .as_ref()
            .map(|d| d.entries.len())
            .unwrap_or(0);

        Self::build(ast, func_count, data_entry_count, symbol_table, debug_mode, error_manager)
    }

    /// The resolver the loader uses: it takes the document and nothing else. The functions are
    /// the document's enhanced `@QUICKFUNCS` section, the data is its `@DATA`, and the result goes
    /// into it as with [`with_document_product`](Self::with_document_product), which this
    /// includes. The table sizes the constructors derive from the sections are taken from the
    /// document, so the run is the one `new_with_error_manager` with the same sections makes.
    pub(crate) fn for_document(
        symbol_table: &'a SymbolTable,
        debug_mode: DebugMode,
        error_manager: ErrorManager,
        document: &'a mut Document,
    ) -> Self {
        let func_count = document.quick_functions.as_ref().map_or(0, |qf| qf.functions.len());
        let data_entry_count = document.data.as_ref().map_or(0, |d| d.entries.len());
        let mut resolver = Self::build(
            DixScript::new(), func_count, data_entry_count, symbol_table, debug_mode, error_manager,
        );
        resolver.data = DataSource::Document(document);
        resolver.document_product = true;
        resolver
    }

    fn build(
        ast: DixScript,
        func_count: usize,
        data_entry_count: usize,
        symbol_table: &'a SymbolTable,
        debug_mode: DebugMode,
        error_manager: ErrorManager,
    ) -> Self {
        let debug_config = DebugConfig::from_debug_mode(debug_mode);

        let data_context: Rc<RefCell<FxHashMap<String, DixValue>>> = Rc::new(RefCell::new(
            FxHashMap::with_capacity_and_hasher(
                (data_entry_count * 4).max(MIN_CAPACITY),
                Default::default(),
            ),
        ));

        let interpreter = FunctionInterpreter::new_with_error_manager(
            symbol_table,
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
            data_changed: false,
            document_product: false,
        }
    }
    /// Gives the five phases (enum pre-resolution, the data context, the call finder, the
    /// write-back of results and the remaining identifiers) the document the data section was
    /// parsed into.
    ///
    /// They then read the document's `@DATA` from its arena instead of from a scratch copy of the
    /// owned data section, phases 1, 4 and 5 rewrite it there, and whether a data section exists
    /// is decided by the document. The owned tree passed to the constructor is still the one the
    /// result is built from: its data section is replaced by an export of the arena's when any
    /// phase changed it, or when it had none.
    pub fn with_document(mut self, document: &'a mut Document) -> Self {
        self.data = DataSource::Document(document);
        self
    }

    /// Like [`with_document`](Self::with_document), and the document is also where the result
    /// goes: the resolved `@DATA` section stays in its arena and the imported enums the data
    /// referenced are merged into its `enums` section, exactly as they are merged into an owned
    /// tree otherwise.
    ///
    /// The owned tree passed to the constructor is not read at all: the local functions are the
    /// document's own `@QUICKFUNCS` section, which is the enhanced one when the document went
    /// through the enhancer, and they are registered without a copy. No export of the data
    /// section is made, and [`ValueResolutionResult::resolved_ast`] is `None` on success and on
    /// failure.
    pub fn with_document_product(mut self, document: &'a mut Document) -> Self {
        self.data = DataSource::Document(document);
        self.document_product = true;
        self
    }

    // ==================== MAIN ORCHESTRATION ====================

    pub fn resolve(mut self) -> ValueResolutionResult {
        if self.debug_config.is_enabled {
            self.error_manager.log_info("[Phase 4.1] Starting value resolution");
        }

        self.attach_scratch_data();

        if let Err(e) = self.resolve_all_enum_values() {
            return self.create_failed_result(vec![e.to_string()]);
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
            self.sync_owned_data();
            let logs = self.interpreter.take_logs();
            self.log_statements.extend(logs);
            let duration = self.start_time.elapsed();
            return ValueResolutionResult {
                is_success: true,
                resolved_ast: (!self.document_product).then_some(self.ast),
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
        self.sync_owned_data();

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
            resolved_ast: (!self.document_product).then_some(self.ast),
            function_calls_resolved: success_count,
            errors,
            log_statements: self.log_statements,
            resolution_duration: duration,
            resolution_history: self.resolution_history,
        }
    }

    /// Replaces the owned data section the result is built from by an export of the arena's,
    /// when any phase changed it or the owned tree had none of its own. A run that changed
    /// nothing leaves the owned section as it was.
    fn sync_owned_data(&mut self) {
        if self.document_product || !(self.data_changed || self.ast.data.is_none()) {
            return;
        }
        if let Some((arena, data)) = self.data.parts() {
            self.ast.data = Some(data.export(arena));
        }
    }

    /// Without an attached document, copies the owned data section into a scratch arena once,
    /// for the phases to share.
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
    /// The walk reads and rewrites the data in the arena it lives in (see [`DataSource`]) and
    /// records whether it changed anything.
    fn run_enum_pass(&mut self) -> Result<Option<EnumPass>, ResolverError> {
        let Some((arena, data)) = self.data.parts() else { return Ok(None) };
        if self.debug_config.is_enabled {
            self.error_manager
                .log_info("Pre-processing: resolving all enum values to integers");
        }
        let pass = resolve_data_enums(arena, data, self.symbol_table)?;
        self.data_changed |= pass.changed;
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

        // The enums go where the result goes: into the document in product mode, into the owned
        // tree otherwise.
        let product = self.document_product;
        let target = match &mut self.data {
            DataSource::Document(document) if product => &mut document.enums,
            _ => &mut self.ast.enums,
        };
        let section = target.get_or_insert_with(|| EnumsSection {
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
/// Runs every call whose arguments are known, repeatedly, and writes each result into the data
/// section at the call's position (see `write_back`). Returns the number of calls resolved and
/// the errors of the ones that failed.
///
/// Whether any result was written is remembered, so that the owned data section the result is
/// built from is replaced by an export of the arena's at the end.
fn execute_iterative_resolution(
    &mut self,
    function_calls: Vec<FunctionCallInfo>,
) -> (usize, Vec<String>) {
    let total = function_calls.len();

    // The interpreter reads function bodies from the arena, so the functions it can call are
    // copied there once, before the first call. This runs only when phase 3 found a call, so a run
    // with no calls copies nothing.
    self.register_functions();

    // Where each call sits in the data tree, found once. Replacing a resolved call then costs
    // the depth of its node instead of a search of the whole tree.
    let target_positions: HashSet<Position> = function_calls.iter().map(|c| c.position).collect();
    let replace_index = match self.data.parts() {
        Some((arena, data)) => write_back::index_call_targets(arena, data, &target_positions),
        None => HashMap::new(),
    };
    let mut data_changed = false;

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

                    if let Some((arena, data)) = self.data.parts() {
                        data_changed |=
                            write_back::replace_value_at(arena, data, &replace_index, pos, &dix_value);
                    }
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

    self.data_changed |= data_changed;

    (resolved_count, errors)
}

    /// Registers the functions the interpreter can call in the arena the data section lives in.
    fn register_functions(&mut self) {
        // In product mode the document is the input as well as the result: its section holds the
        // local functions, already in the arena the data section lives in.
        if self.document_product {
            if let DataSource::Document(document) = &mut self.data {
                let Document { arena, data, quick_functions, .. } = &mut **document;
                if data.is_some() {
                    let local = quick_functions
                        .as_ref()
                        .map(|section| section.functions.as_slice())
                        .unwrap_or(&[]);
                    self.interpreter.register_document_functions(arena, local);
                }
            }
            return;
        }
        let local = self
            .ast
            .quick_functions
            .as_ref()
            .map(|section| section.functions.as_slice())
            .unwrap_or(&[]);
        if let Some((arena, _)) = self.data.parts() {
            self.interpreter.register_functions(arena, local);
        }
    }

    fn execute_call_raw(
        &mut self,
        call: &FunctionCallInfo,
    ) -> Result<DixValue, InterpreterError> {
        let Some((arena, _)) = self.data.parts() else {
            return Err(InterpreterError::UndefinedFunction {
                name:     call.function_name.clone(),
                position: call.position,
            });
        };
        self.interpreter.execute_data_call(
            arena,
            &call.function_name,
            call.namespace_name.as_deref(),
            call.arguments,
            call.position,
            &call.scope_context,
        )
    }

    // ==================== PHASE 4 HELPERS ====================

    fn has_unresolved_dependencies(&mut self, call: &FunctionCallInfo) -> bool {
        let Some((arena, _)) = self.data.parts() else { return false };
        let ctx = self.data_context.borrow();
        arena
            .expr_list(call.arguments)
            .iter()
            .any(|&arg| Self::expr_has_unresolved_ref(arena, arg, &ctx))
    }

    /// Whether `name` can be found nowhere in the data context, by exact key or by path suffix.
    ///
    /// A name is only truly unresolved if it cannot be found either way. Previously this was just
    /// `!ctx.contains_key(name)`, which caused false positives for results stored at nested paths
    /// like "db.primary.total" when the argument was just `total`.
    fn name_is_unresolved(name: &str, ctx: &FxHashMap<String, DixValue>) -> bool {
        if ctx.contains_key(name) {
            return false;
        }
        let suffix = format!(".{}", name);
        !ctx.keys().any(|k| k.ends_with(&suffix))
    }

    fn expr_has_unresolved_ref(
        arena: &Arena,
        expr: ExprId,
        ctx: &FxHashMap<String, DixValue>,
    ) -> bool {
        match *arena.expr(expr) {
            ExprNode::Identifier { name, .. } => Self::name_is_unresolved(arena.text(name), ctx),

            // After Phase 1 enum pre-resolution these are already Integer literals.
            ExprNode::EnumAccess { .. } => false,

            // A nested call is only "unresolved" if its own arguments contain
            // something we cannot evaluate yet.  The interpreter will execute it
            // inline, so the call itself is not a dependency gate.
            ExprNode::QuickFuncCall { arguments, .. }
            | ExprNode::ImportedFunctionCall { arguments, .. } => arena
                .expr_list(arguments)
                .iter()
                .any(|&a| Self::expr_has_unresolved_ref(arena, a, ctx)),

            ExprNode::ArithmeticOp { left, right, .. }
            | ExprNode::BitwiseOp { left, right, .. }
            | ExprNode::ComparisonOp { left, right, .. }
            | ExprNode::LogicalOp { left, right, .. } => {
                Self::expr_has_unresolved_ref(arena, left, ctx)
                    || Self::expr_has_unresolved_ref(arena, right, ctx)
            }

            ExprNode::Conditional { condition, true_value, false_value, .. } => {
                Self::expr_has_unresolved_ref(arena, condition, ctx)
                    || Self::expr_has_unresolved_ref(arena, true_value, ctx)
                    || Self::expr_has_unresolved_ref(arena, false_value, ctx)
            }

            ExprNode::Value { value, .. } => Self::value_has_unresolved_ref(arena, value, ctx),

            // For property access (e.g. `server.host`) reconstruct the full dotted
            // path and look it up directly rather than only checking the root object.
            ExprNode::PropertyAccess { object, property, .. } => {
                match Self::reconstruct_access_path(arena, object, arena.text(property)) {
                    Some(full_path) => Self::name_is_unresolved(&full_path, ctx),
                    // Cannot reconstruct — fall back to checking the base object.
                    None => Self::expr_has_unresolved_ref(arena, object, ctx),
                }
            }

            ExprNode::IndexAccess { object, index, .. } => {
                Self::expr_has_unresolved_ref(arena, object, ctx)
                    || Self::expr_has_unresolved_ref(arena, index, ctx)
            }

            // All other expression variants (literals, static calls, etc.)
            // are either self-contained or handled by the interpreter directly.
            _ => false,
        }
    }

    /// Reconstruct a fully-qualified dotted path from a chain of PropertyAccess
    /// nodes.  Returns `None` if the chain contains a non-identifier base.
    fn reconstruct_access_path(arena: &Arena, object: ExprId, property: &str) -> Option<String> {
        match *arena.expr(object) {
            ExprNode::Identifier { name, .. } => Some(format!("{}.{}", arena.text(name), property)),
            ExprNode::PropertyAccess { object: inner, property: inner_prop, .. } => {
                Self::reconstruct_access_path(arena, inner, arena.text(inner_prop))
                    .map(|base| format!("{}.{}", base, property))
            }
            _ => None,
        }
    }

    fn value_has_unresolved_ref(
        arena: &Arena,
        value: ValueId,
        ctx: &FxHashMap<String, DixValue>,
    ) -> bool {
        match *arena.value(value) {
            ValueNode::Identifier { value: id, .. } => Self::name_is_unresolved(arena.text(id), ctx),
            ValueNode::Expression { expr, .. } => Self::expr_has_unresolved_ref(arena, expr, ctx),
            ValueNode::Array { values, .. } | ValueNode::NestedArray { values, .. } => arena
                .value_list(values)
                .iter()
                .any(|&v| Self::value_has_unresolved_ref(arena, v, ctx)),
            ValueNode::Object { properties, .. } => arena
                .prop_list(properties)
                .iter()
                .any(|p| Self::value_has_unresolved_ref(arena, p.value, ctx)),
            ValueNode::PrefixedConstructor { arguments, .. } => arena
                .value_list(arguments)
                .iter()
                .any(|&a| Self::value_has_unresolved_ref(arena, a, ctx)),
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

    // ==================== PHASE 5: IDENTIFIER RESOLUTION ====================

    /// Resolves the identifiers left in the data section (phase 5), pass by pass, until a pass
    /// resolves none. The walk of one pass is `identifier_resolution::resolve_pass`; this method
    /// keeps the context in step between passes and logs.
    fn resolve_remaining_identifiers(&mut self) {
        let node_estimate = self.data.parts().map(|(_, data)| data.entries.len() * 6).unwrap_or(0);
        let max_passes = (node_estimate / 4).max(8).min(64);

        let mut total_resolved = 0usize;
        let mut final_skipped = 0usize;

        for pass in 1..=max_passes {
            let Some((arena, data)) = self.data.parts() else { break };
            let mut newly_resolved: Vec<(String, DixValue)> = Vec::new();
            let (resolved_this_pass, skipped_this_pass) = {
                let ctx = self.data_context.borrow();
                identifier_resolution::resolve_pass(arena, data, &ctx, &mut newly_resolved)
            };

            if resolved_this_pass > 0 {
                self.data_changed = true;
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

    fn create_failed_result(&self, errors: Vec<String>) -> ValueResolutionResult {
        ValueResolutionResult {
            is_success: false,
            resolved_ast: (!self.document_product).then(|| self.ast.clone()),
            function_calls_resolved: 0,
            errors,
            log_statements: self.log_statements.clone(),
            resolution_duration: self.start_time.elapsed(),
            resolution_history: self.resolution_history.clone(),
        }
    }
}
