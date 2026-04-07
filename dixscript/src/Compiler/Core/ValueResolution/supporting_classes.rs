
//! Shared types for the value resolution pipeline.

use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rustc_hash::FxHashMap;

use crate::Builtins::Core::DixValue;
use crate::Compiler::AST::{Expression, QuickFunction};

pub struct ValueResolutionResult {
    pub is_success: bool,
    /// Original AST before resolution was attempted.
    pub original_ast: Option<crate::Compiler::AST::DixScript>,
    /// AST after successful resolution (`None` on failure).
    pub resolved_ast: Option<crate::Compiler::AST::DixScript>,
    pub function_calls_resolved: usize,
    pub errors: Vec<String>,
    /// Diagnostic log statements produced during the pass.
    pub log_statements: Vec<String>,
    pub resolution_duration: Duration,
    /// Ordered log of every resolution attempt.
    pub resolution_history: Vec<ResolutionRecord>,
}

impl ValueResolutionResult {
    pub fn new() -> Self {
        ValueResolutionResult {
            is_success: false,
            original_ast: None,
            resolved_ast: None,
            function_calls_resolved: 0,
            errors: Vec::new(),
            log_statements: Vec::new(),
            resolution_duration: Duration::ZERO,
            resolution_history: Vec::new(),
        }
    }
}

impl Default for ValueResolutionResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata captured for every QuickFunction call discovered inside @DATA.
///
/// `namespace_name == None`  → local function call
/// `namespace_name == Some`  → imported (namespaced) function call
///
/// `entry_path` is the sole stable identifier for AST replacement — the
/// redundant `Line`/`Column` fields from the C# version are merged into
/// `position`.
#[derive(Debug, Clone)]
pub struct FunctionCallInfo {
    /// Bare function name (no namespace prefix).
    pub function_name: String,
    /// Namespace alias if imported; `None` for local calls.
    pub namespace_name: Option<String>,
    /// Cloned argument expressions.
    pub arguments: Vec<Expression>,
    /// Full DATA-rooted path where the call was found.
    pub location: String,
    /// Scope path without the final segment — used for function-scope matching.
    pub scope: String,
    /// Top-level DATA entry path that contains this call, used for reliable
    /// AST replacement.
    pub entry_path: String,
    pub position: crate::Compiler::AST::Position,
    /// Snapshot of scope-local variables at the point the call was found.
    pub scope_context: FxHashMap<String, String>,
}

impl FunctionCallInfo {
    /// `namespace.function` for imports, or just `function` for locals.
    pub fn fully_qualified_name(&self) -> String {
        match &self.namespace_name {
            Some(ns) if !ns.is_empty() => format!("{}.{}", ns, self.function_name),
            _ => self.function_name.clone(),
        }
    }
}

impl fmt::Display for FunctionCallInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}() at {} [entry: {}, pos: {}]",
            self.fully_qualified_name(),
            self.location,
            self.entry_path,
            self.position
        )
    }
}

/// Immutable audit entry for a single resolution attempt.
#[derive(Debug, Clone)]
pub struct ResolutionRecord {
    pub function_name: String,
    pub namespace_name: Option<String>,
    pub location: String,
    pub scope: String,
    /// String representations of arguments — for logging, not re-parsing.
    pub arguments: Vec<String>,
    pub result: Option<DixValue>,
    pub success: bool,
    pub error_message: String,
    pub timestamp: DateTime<Utc>,
}

impl ResolutionRecord {
    pub fn display_name(&self) -> String {
        match &self.namespace_name {
            Some(ns) if !ns.is_empty() => format!("{}.{}", ns, self.function_name),
            _ => self.function_name.clone(),
        }
    }
}

impl fmt::Display for ResolutionRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.success { "OK" } else { "FAIL" };
        write!(f, "[{}] {} at {}", status, self.display_name(), self.location)?;
        if !self.success && !self.error_message.is_empty() {
            write!(f, " — {}", self.error_message)?;
        }
        Ok(())
    }
}

/// Lightweight scope/path tracker used by `ASTWalker` during @DATA traversal.
///
/// Maintains a segment stack rooted at `"DATA"`. Array indices (e.g. `[0]`)
/// are pushed verbatim; `PathBuilder` handles dot-vs-bracket formatting.
///
/// Critical invariant: `reset_to_root()` must be called before each
/// top-level DATA entry to prevent scope bleeding between entries.
pub struct ScopeTracker {
    /// Segment stack. Index 0 is always `"DATA"`.
    path_segments: Vec<String>,
    /// Variables registered in the current scope (cleared on table entry).
    current_scope_variables: FxHashMap<String, String>,
}

impl ScopeTracker {
    pub fn new() -> Self {
        // Capacity 8 covers typical DixScript nesting depth without reallocation.
        let mut path_segments = Vec::with_capacity(8);
        path_segments.push("DATA".to_string());

        ScopeTracker {
            path_segments,
            current_scope_variables: FxHashMap::default(),
        }
    }

    pub fn enter_scope(&mut self, segment: &str) {
        self.path_segments.push(segment.to_string());
    }

    /// Never pops below the root `"DATA"` segment.
    pub fn exit_scope(&mut self) {
        if self.path_segments.len() > 1 {
            self.path_segments.pop();
        }
    }

    /// Reset to root and clear registered variables. Must be called before
    /// each top-level DATA entry.
    pub fn reset_to_root(&mut self) {
        self.path_segments.clear();
        self.path_segments.push("DATA".to_string());
        self.current_scope_variables.clear();
    }

    /// Build the current full path via `PathBuilder`.
    /// Skips the root segment — `PathBuilder` prepends `"DATA"` automatically.
    pub fn get_current_path(&self) -> String {
        let segments: Vec<&str> = self
            .path_segments
            .iter()
            .skip(1)
            .map(|s| s.as_str())
            .collect();
        crate::Compiler::Utilities::PathBuilder::build(&segments)
    }

    /// Scope path = current path minus the final segment, used for
    /// function-scope matching.
    pub fn get_current_scope(&self) -> String {
        let len = self.path_segments.len();

        if len <= 2 {
            return "DATA".to_string();
        }

        let segments: Vec<&str> = self
            .path_segments
            .iter()
            .skip(1)
            .take(len - 2)
            .map(|s| s.as_str())
            .collect();
        crate::Compiler::Utilities::PathBuilder::build(&segments)
    }

    pub fn register_variable(&mut self, name: &str, full_path: &str) {
        self.current_scope_variables
            .insert(name.to_string(), full_path.to_string());
    }

    pub fn clear_scope_variables(&mut self) {
        self.current_scope_variables.clear();
    }

    /// Immutable snapshot of currently registered variables.
    pub fn get_scope_variables_snapshot(&self) -> FxHashMap<String, String> {
        self.current_scope_variables.clone()
    }
}

impl Default for ScopeTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry of `QuickFunction` definitions available during value resolution.
/// Duplicate registration is caught at insert time.
pub struct FunctionRegistry {
    functions: FxHashMap<String, QuickFunction>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        FunctionRegistry {
            functions: FxHashMap::default(),
        }
    }

    /// Returns `Err` if a function with the same name is already registered.
    pub fn register(&mut self, function: QuickFunction) -> Result<(), FunctionExecutionError> {
        let name = function.name.clone();
        if self.functions.contains_key(&name) {
            return Err(FunctionExecutionError::new(format!(
                "Function '{}' already registered",
                name
            )));
        }
        self.functions.insert(name, function);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&QuickFunction> {
        self.functions.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    pub fn count(&self) -> usize {
        self.functions.len()
    }

    pub fn function_names(&self) -> impl Iterator<Item = &String> {
        self.functions.keys()
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Typed errors produced by `ExecutionContext` operations.
#[derive(Debug, Clone)]
pub enum ExecutionError {
    InvalidVariableName(String),
    VariableAlreadyDefined(String),
    UndefinedVariable {
        name: String,
        function_name: String,
    },
    CannotExitRootScope,
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionError::InvalidVariableName(msg) => {
                write!(f, "Invalid variable name: {}", msg)
            }
            ExecutionError::VariableAlreadyDefined(name) => {
                write!(f, "Variable '{}' already defined in current scope", name)
            }
            ExecutionError::UndefinedVariable { name, function_name } => {
                write!(f, "Undefined variable '{}' in function '{}'", name, function_name)
            }
            ExecutionError::CannotExitRootScope => write!(f, "Cannot exit root scope"),
        }
    }
}

impl std::error::Error for ExecutionError {}

/// Error type for function-level execution failures. Supports chaining via
/// an optional inner error.
#[derive(Debug, Clone)]
pub struct FunctionExecutionError {
    pub message: String,
    pub inner: Option<Box<FunctionExecutionError>>,
}

impl FunctionExecutionError {
    pub fn new(message: impl Into<String>) -> Self {
        FunctionExecutionError {
            message: message.into(),
            inner: None,
        }
    }

    pub fn with_inner(message: impl Into<String>, inner: FunctionExecutionError) -> Self {
        FunctionExecutionError {
            message: message.into(),
            inner: Some(Box::new(inner)),
        }
    }
}

impl fmt::Display for FunctionExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(ref inner) = self.inner {
            write!(f, " | caused by: {}", inner)?;
        }
        Ok(())
    }
}

impl std::error::Error for FunctionExecutionError {}

impl From<ExecutionError> for FunctionExecutionError {
    fn from(err: ExecutionError) -> Self {
        FunctionExecutionError::new(err.to_string())
    }
}
