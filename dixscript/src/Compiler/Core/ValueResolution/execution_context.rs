
//! Scoped variable environment for QuickFunction execution.
//!
//! Parent context uses `Rc<RefCell<…>>` for shared mutable access.
//! `Rc` (not `Arc`) is intentional: value resolution is single-threaded.
//! All fallible methods return `Result<_, ExecutionError>` — no panics in
//! library code except the scope-stack invariant, which represents a
//! compiler bug and carries a descriptive message.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::Builtins::Core::DixValue;
use super::supporting_classes::ExecutionError;

pub struct ExecutionContext {
    scopes: Vec<FxHashMap<String, DixValue>>,
    function_name: String,
    parent_context: Option<Rc<RefCell<ExecutionContext>>>,
}

impl ExecutionContext {
    pub fn new(
        function_name: &str,
        parent: Option<Rc<RefCell<ExecutionContext>>>,
    ) -> Self {
        let mut scopes = Vec::with_capacity(4);
        scopes.push(FxHashMap::default());

        ExecutionContext {
            scopes,
            function_name: function_name.to_string(),
            parent_context: parent,
        }
    }

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
            .expect("scope stack invariant violated: scopes vec is empty");

        if current_scope.contains_key(name) {
            return Err(ExecutionError::VariableAlreadyDefined(name.to_string()));
        }

        current_scope.insert(name.to_string(), value);
        Ok(())
    }

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

        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return Ok(());
            }
        }

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

    /// Returns an owned clone to avoid holding a borrow across the
    /// `RefCell` boundary of the parent.
    pub fn get_variable(&self, name: &str) -> Result<DixValue, ExecutionError> {
        if name.is_empty() {
            return Err(ExecutionError::InvalidVariableName(
                "Variable name cannot be empty".to_string(),
            ));
        }

        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Ok(value.clone());
            }
        }

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

    pub fn has_variable(&self, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        for scope in self.scopes.iter().rev() {
            if scope.contains_key(name) {
                return true;
            }
        }

        if let Some(ref parent) = self.parent_context {
            return parent.borrow().has_variable(name);
        }

        false
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(FxHashMap::default());
    }

    pub fn exit_scope(&mut self) -> Result<(), ExecutionError> {
        if self.scopes.len() <= 1 {
            return Err(ExecutionError::CannotExitRootScope);
        }
        self.scopes.pop();
        Ok(())
    }

    /// Flatten the entire scope chain (parent first, then local bottom → top)
    /// into a single owned map. Inner scopes shadow outer ones.
    pub fn get_all_variables(&self) -> FxHashMap<String, DixValue> {
        let mut all = FxHashMap::default();

        if let Some(ref parent) = self.parent_context {
            all = parent.borrow().get_all_variables();
        }

        for scope in &self.scopes {
            for (key, value) in scope {
                all.insert(key.clone(), value.clone());
            }
        }

        all
    }

    pub fn scope_depth(&self) -> usize {
        self.scopes.len()
    }

    pub fn function_name(&self) -> &str {
        &self.function_name
    }

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
        write!(
            f,
            "ExecutionContext[{}]: {} variables, depth {}",
            self.function_name,
            self.get_all_variables().len(),
            self.scope_depth()
        )
    }
}

/// Fully owned, immutable point-in-time copy of an `ExecutionContext`'s state.
/// Safe to store, log, or use for rollback without borrow or lifetime concerns.
#[derive(Debug, Clone)]
pub struct ExecutionContextSnapshot {
    pub variables: FxHashMap<String, DixValue>,
    pub function_name: String,
    pub scope_depth: usize,
}

impl ExecutionContextSnapshot {
    pub fn new(
        variables: FxHashMap<String, DixValue>,
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
