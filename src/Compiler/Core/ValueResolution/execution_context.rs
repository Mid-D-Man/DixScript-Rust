// src/Compiler/Core/ValueResolution/execution_context.rs
//!
//! Scoped variable environment for QuickFunction execution.
//!
//! ## Design decisions
//! - Parent context uses `Rc<RefCell<…>>` for shared mutable access.
//!   `Rc` (not `Arc`) is intentional: value resolution is single-threaded.
//! - All fallible methods return `Result<_, ExecutionError>` — no panics.
//! - `get_variable` returns an owned `DixValue` (clone).  This sidesteps
//!   the lifetime issue of returning a reference into a `RefCell` and is
//!   fine here because variable lookup is not on the innermost hot path.
//! - `ExecutionContextSnapshot` is a fully owned, immutable point-in-time
//!   copy — safe to store, log, or use for rollback.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::Builtins::Core::DixValue;
use super::supporting_classes::ExecutionError;

// ==================== EXECUTION CONTEXT ====================

/// Manages variable scopes during QuickFunction execution.
///
/// Scope stack layout (bottom → top):
/// ```text
/// [ root_scope ]   ← always present, created at construction
/// [ inner_scope ]  ← pushed/popped by enter_scope / exit_scope
/// [ …           ]
/// ```
pub struct ExecutionContext {
    /// Scope stack.  Index 0 is the root (function-level) scope.
    scopes: Vec<HashMap<String, DixValue>>,
    /// Function name this context belongs to (carried into error messages).
    function_name: String,
    /// Optional parent context for closure / nested-function variable lookup.
    parent_context: Option<Rc<RefCell<ExecutionContext>>>,
}

impl ExecutionContext {
    /// Create a new context for `function_name`.
    ///
    /// `parent` – supply when the function is a closure or needs access to
    /// an enclosing scope's variables.
    pub fn new(
        function_name: &str,
        parent: Option<Rc<RefCell<ExecutionContext>>>,
    ) -> Self {
        let mut scopes = Vec::with_capacity(4);
        scopes.push(HashMap::new()); // root scope

        ExecutionContext {
            scopes,
            function_name: function_name.to_string(),
            parent_context: parent,
        }
    }

    // ==================== VARIABLE OPERATIONS ====================

    /// Define a NEW variable in the current (innermost) scope.
    ///
    /// Fails if `name` is empty or already defined in this scope.
    pub fn define_variable(
        &mut self,
        name: &str,
        value: DixValue,
    ) -> Result<(), ExecutionError> {
        if name.is_empty() {
            return Err(ExecutionError::InvalidVariableName(
                "Variable name cannot be empty".to_string(),
            ));
        }

        let current_scope = self
            .scopes
            .last_mut()
            .expect("scope stack invariant: never empty");

        if current_scope.contains_key(name) {
            return Err(ExecutionError::VariableAlreadyDefined(name.to_string()));
        }

        current_scope.insert(name.to_string(), value);
        Ok(())
    }

    /// Update an EXISTING variable anywhere in the scope chain.
    ///
    /// Searches innermost scope first, then walks outward through parents.
    /// Fails if the variable is not found anywhere.
    pub fn set_variable(
        &mut self,
        name: &str,
        value: DixValue,
    ) -> Result<(), ExecutionError> {
        if name.is_empty() {
            return Err(ExecutionError::InvalidVariableName(
                "Variable name cannot be empty".to_string(),
            ));
        }

        // Search local scopes (innermost first)
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return Ok(());
            }
        }

        // Delegate to parent context
        if let Some(ref parent) = self.parent_context {
            let mut parent_ctx = parent.borrow_mut();
            if parent_ctx.has_variable(name) {
                return parent_ctx.set_variable(name, value);
            }
        }

        Err(ExecutionError::UndefinedVariable {
            name: name.to_string(),
            function_name: self.function_name.clone(),
        })
    }

    /// Retrieve a variable's value (owned clone) by searching innermost-first.
    ///
    /// Falls back to the parent context.  Returns an owned `DixValue` to
    /// avoid holding a borrow across the `RefCell` boundary of the parent.
    pub fn get_variable(&self, name: &str) -> Result<DixValue, ExecutionError> {
        if name.is_empty() {
            return Err(ExecutionError::InvalidVariableName(
                "Variable name cannot be empty".to_string(),
            ));
        }

        // Search local scopes (innermost first)
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Ok(value.clone());
            }
        }

        // Delegate to parent
        if let Some(ref parent) = self.parent_context {
            let parent_ctx = parent.borrow();
            if parent_ctx.has_variable(name) {
                return parent_ctx.get_variable(name);
            }
        }

        Err(ExecutionError::UndefinedVariable {
            name: name.to_string(),
            function_name: self.function_name.clone(),
        })
    }

    /// Check whether a variable exists in any reachable scope.
    pub fn has_variable(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        // Local scopes
        for scope in self.scopes.iter().rev() {
            if scope.contains_key(name) {
                return true;
            }
        }

        // Parent
        if let Some(ref parent) = self.parent_context {
            return parent.borrow().has_variable(name);
        }

        false
    }

    // ==================== SCOPE MANAGEMENT ====================

    /// Push a new nested scope onto the stack.
    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the current scope.  The root scope can never be popped.
    pub fn exit_scope(&mut self) -> Result<(), ExecutionError> {
        if self.scopes.len() <= 1 {
            return Err(ExecutionError::CannotExitRootScope);
        }
        self.scopes.pop();
        Ok(())
    }

    // ==================== INTROSPECTION ====================

    /// Flatten the entire variable chain (parent first, then local bottom→top)
    /// into a single owned map.  Inner scopes shadow outer ones.
    pub fn get_all_variables(&self) -> HashMap<String, DixValue> {
        let mut all = HashMap::new();

        // Start with parent variables
        if let Some(ref parent) = self.parent_context {
            all = parent.borrow().get_all_variables();
        }

        // Overlay local scopes bottom → top (inner scopes win)
        for scope in &self.scopes {
            for (key, value) in scope {
                all.insert(key.clone(), value.clone());
            }
        }

        all
    }

    /// Current scope depth (1 = root only).
    pub fn scope_depth(&self) -> usize {
        self.scopes.len()
    }

    /// The function name this context was created for.
    pub fn function_name(&self) -> &str {
        &self.function_name
    }

    /// Create an immutable snapshot of the current state.
    pub fn create_snapshot(&self) -> ExecutionContextSnapshot {
        ExecutionContextSnapshot::new(
            self.get_all_variables(),
            self.function_name.clone(),
            self.scope_depth(),
        )
    }
}

impl fmt::Display for ExecutionContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let var_count = self.get_all_variables().len();
        write!(
            f,
            "ExecutionContext[{}]: {} variables, depth {}",
            self.function_name,
            var_count,
            self.scope_depth()
        )
    }
}

// ==================== EXECUTION CONTEXT SNAPSHOT ====================

/// Fully owned, immutable point-in-time copy of an ExecutionContext's state.
///
/// Safe to store, log, compare, or use for rollback without any borrow
/// or lifetime concerns.
#[derive(Debug, Clone)]
pub struct ExecutionContextSnapshot {
    /// All variables visible at snapshot time (flattened scope chain).
    pub variables: HashMap<String, DixValue>,
    /// Function name the source context belonged to.
    pub function_name: String,
    /// Scope depth at snapshot time.
    pub scope_depth: usize,
}

impl ExecutionContextSnapshot {
    pub fn new(
        variables: HashMap<String, DixValue>,
        function_name: String,
        scope_depth: usize,
    ) -> Self {
        ExecutionContextSnapshot {
            variables,
            function_name,
            scope_depth,
        }
    }
}

impl fmt::Display for ExecutionContextSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Snapshot[{}]: {} variables, depth {}",
            self.function_name,
            self.variables.len(),
            self.scope_depth
        )
    }
          }
