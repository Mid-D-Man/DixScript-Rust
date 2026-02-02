// src/Compiler/Core/ValueResolution/supporting_classes.rs
//!
//! Shared types for the value resolution pipeline:
//!   ValueResolutionResult  – aggregate result handed back to the caller
//!   FunctionCallInfo       – metadata for every QuickFunction call found in @DATA
//!   ResolutionRecord       – per-call audit entry
//!   ScopeTracker           – lightweight path-stack used by ASTWalker
//!   FunctionRegistry       – name → QuickFunction lookup table
//!   DebugConfig            – cached debug flags (avoids per-call checks + string formatting)
//!   ExecutionError         – typed errors from ExecutionContext
//!   FunctionExecutionError – typed errors from function-level execution

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::Builtins::Core::DixValue;
use crate::Compiler::AST::{Expression, QuickFunction};
use crate::Compiler::Core::DebugMode;
use crate::Compiler::Utilities::PathBuilder;

// ==================== DEBUG CONFIG ====================

/// Flags cached once at construction time so that every log-site is a single
/// bool check.  The real win: `format!(…)` is never called when debug is off.
pub(crate) struct DebugConfig {
    /// `true` when DebugMode is Regular or Verbose.
    pub is_enabled: bool,
    /// `true` only when DebugMode is Verbose.
    pub is_verbose: bool,
}

impl DebugConfig {
    pub fn from_mode(mode: DebugMode) -> Self {
        DebugConfig {
            is_enabled: mode != DebugMode::Off,
            is_verbose: mode == DebugMode::Verbose,
        }
    }
}

// ==================== VALUE RESOLUTION RESULT ====================

/// Aggregate result of the value resolution pass.
///
/// Callers check `is_success` first; on failure `errors` has human-readable
/// messages and `resolution_history` gives a per-call audit trail.
#[derive(Debug, Clone)]
pub struct ValueResolutionResult {
    pub is_success: bool,
    /// Original AST before resolution was attempted.
    pub original_ast: Option<crate::Compiler::AST::DixScript>,
    /// AST after successful resolution (`None` on failure).
    pub resolved_ast: Option<crate::Compiler::AST::DixScript>,
    /// Number of function calls that were successfully resolved.
    pub function_calls_resolved: usize,
    /// Human-readable error messages collected during the pass.
    pub errors: Vec<String>,
    /// Diagnostic log statements produced during the pass.
    pub log_statements: Vec<String>,
    /// Wall-clock time spent in resolution.
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

// ==================== FUNCTION CALL INFO ====================

/// Metadata captured for every QuickFunction call discovered inside @DATA.
///
/// `namespace_name == None`  → local function call
/// `namespace_name == Some`  → imported (namespaced) function call
///
/// **Improvement over C#:** the redundant `Line`/`Column` fields are merged
/// into the single `position: Position` field (they were always identical to
/// `OriginalCallPosition`).  The `ParentEntry` reference is also removed;
/// `entry_path` is the sole stable identifier for AST replacement — exactly
/// as the original C# comment noted ("CRITICAL: Store entry path for reliable
/// replacement").
#[derive(Debug, Clone)]
pub struct FunctionCallInfo {
    /// Bare function name (no namespace prefix).
    pub function_name: String,
    /// Namespace alias if imported; `None` for local calls.
    pub namespace_name: Option<String>,
    /// Cloned argument expressions (matches C# `.ToList()` semantics).
    pub arguments: Vec<Expression>,
    /// Full DATA-rooted path where the call was found
    /// (e.g. `"DATA.orders.order_001.price"`).
    pub location: String,
    /// Scope path without the final segment — used for function-scope matching.
    pub scope: String,
    /// The top-level DATA entry path that contains this call.
    /// Used for reliable replacement later in the pipeline.
    pub entry_path: String,
    /// Source position of the original function-call token.
    pub position: crate::Compiler::AST::Position,
    /// Snapshot of scope-local variables at the point the call was found.
    pub scope_context: HashMap<String, String>,
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

// ==================== RESOLUTION RECORD ====================

/// Immutable audit entry for a single resolution attempt.
#[derive(Debug, Clone)]
pub struct ResolutionRecord {
    pub function_name: String,
    /// Namespace alias for imported functions; `None` for local.
    pub namespace_name: Option<String>,
    pub location: String,
    pub scope: String,
    /// String representations of arguments (for logging — not re-parsed).
    pub arguments: Vec<String>,
    /// The resolved value, if resolution succeeded.
    pub result: Option<DixValue>,
    pub success: bool,
    /// Error message on failure; empty on success.
    pub error_message: String,
    /// When the resolution attempt was made.
    pub timestamp: DateTime<Utc>,
}

impl ResolutionRecord {
    /// `namespace.function` or just `function`.
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

// ==================== SCOPE TRACKER ====================

/// Lightweight scope/path tracker used by ASTWalker during @DATA traversal.
///
/// Maintains a segment stack rooted at `"DATA"`.  Array indices (e.g. `[0]`)
/// are pushed verbatim; `PathBuilder` handles dot-vs-bracket formatting.
///
/// **Critical invariant:** `reset_to_root()` must be called before each
/// top-level DATA entry to prevent scope bleeding.
pub struct ScopeTracker {
    /// Segment stack.  Index 0 is always `"DATA"`.
    path_segments: Vec<String>,
    /// Variables registered in the current scope (cleared on table entry).
    current_scope_variables: HashMap<String, String>,
}

impl ScopeTracker {
    pub fn new() -> Self {
        ScopeTracker {
            path_segments: vec!["DATA".to_string()],
            current_scope_variables: HashMap::new(),
        }
    }

    /// Push a path segment.
    pub fn enter_scope(&mut self, segment: &str) {
        self.path_segments.push(segment.to_string());
    }

    /// Pop the most recent segment.  Never pops below the root `"DATA"`.
    pub fn exit_scope(&mut self) {
        if self.path_segments.len() > 1 {
            self.path_segments.pop();
        }
    }

    /// Reset to root and clear registered variables.
    /// Must be called before each top-level DATA entry.
    pub fn reset_to_root(&mut self) {
        self.path_segments.clear();
        self.path_segments.push("DATA".to_string());
        self.current_scope_variables.clear();
    }

    /// Build the current full path via PathBuilder.
    ///
    /// Skips the root segment — PathBuilder prepends `"DATA"` automatically.
    pub fn get_current_path(&self) -> String {
        let segments: Vec<&str> = self
            .path_segments
            .iter()
            .skip(1) // skip "DATA" at index 0
            .map(|s| s.as_str())
            .collect();
        PathBuilder::build(&segments)
    }

    /// Scope path = current path minus the final segment.
    /// Used for function-scope matching.
    pub fn get_current_scope(&self) -> String {
        let len = self.path_segments.len();

        if len <= 2 {
            return "DATA".to_string();
        }

        // skip "DATA" (index 0), take everything except the last segment
        let segments: Vec<&str> = self
            .path_segments
            .iter()
            .skip(1)
            .take(len - 2) // total - 1 (DATA) - 1 (last) = len - 2
            .map(|s| s.as_str())
            .collect();
        PathBuilder::build(&segments)
    }

    /// Register a variable name → full-path mapping.
    pub fn register_variable(&mut self, name: &str, full_path: &str) {
        self.current_scope_variables
            .insert(name.to_string(), full_path.to_string());
    }

    /// Clear all registered variables (called on table entry).
    pub fn clear_scope_variables(&mut self) {
        self.current_scope_variables.clear();
    }

    /// Immutable snapshot of currently registered variables.
    pub fn get_scope_variables_snapshot(&self) -> HashMap<String, String> {
        self.current_scope_variables.clone()
    }
}

impl Default for ScopeTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== FUNCTION REGISTRY ====================

/// Registry of QuickFunction definitions available during value resolution.
/// Duplicate registration is caught at insert time.
pub struct FunctionRegistry {
    functions: HashMap<String, QuickFunction>,
}

impl FunctionRegistry {
    pub fn new() -> Self {
        FunctionRegistry {
            functions: HashMap::new(),
        }
    }

    /// Register a QuickFunction.  Returns `Err` if a function with the same
    /// name is already present.
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

    /// Look up a function by name.
    pub fn get(&self, name: &str) -> Option<&QuickFunction> {
        self.functions.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    pub fn count(&self) -> usize {
        self.functions.len()
    }

    /// Iterator over all registered function names.
    pub fn function_names(&self) -> impl Iterator<Item = &String> {
        self.functions.keys()
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== EXECUTION ERROR ====================

/// Typed errors produced by ExecutionContext operations.
///
/// Each variant carries enough context for a meaningful message without
/// requiring the caller to format strings on the call path.
#[derive(Debug, Clone)]
pub enum ExecutionError {
    /// Name was empty or whitespace-only.
    InvalidVariableName(String),
    /// A variable with this name already exists in the innermost scope.
    VariableAlreadyDefined(String),
    /// Variable not found in any local scope or parent context.
    UndefinedVariable {
        name: String,
        function_name: String,
    },
    /// Attempted to pop the root scope.
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
            ExecutionError::UndefinedVariable {
                name,
                function_name,
            } => write!(
                f,
                "Undefined variable '{}' in function '{}'",
                name, function_name
            ),
            ExecutionError::CannotExitRootScope => write!(f, "Cannot exit root scope"),
        }
    }
}

impl std::error::Error for ExecutionError {}

// ==================== FUNCTION EXECUTION ERROR ====================

/// Error type for function-level execution failures.
/// Supports chaining via an optional inner error.
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

    pub fn with_inner(
        message: impl Into<String>,
        inner: FunctionExecutionError,
    ) -> Self {
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
