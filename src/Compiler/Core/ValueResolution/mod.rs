// src/Compiler/Core/ValueResolution/mod.rs
//!
//! # Value Resolution — compile-time QuickFunction execution
//!
//! ## Current modules
//! | Module               | Role                                                      |
//! |----------------------|-----------------------------------------------------------|
//! | `supporting_classes` | Shared data types used across the pipeline               |
//! | `execution_context`  | Scoped variable environment for function execution       |
//! | `ast_walker`         | Discovers all QuickFunction calls in @DATA                |
//!
//! ## Pending (stubs reserved for next phase)
//! | Module                 | Role                                                    |
//! |------------------------|---------------------------------------------------------|
//! | `value_resolver`       | Orchestrates the full resolution pass                   |
//! | `function_interpreter`| Executes QuickFunction bodies against an ExecutionContext|

pub mod supporting_classes;
pub mod execution_context;
pub mod ast_walker;

// ── convenience re-exports ──────────────────────────────────────────────────

pub use supporting_classes::{
    ValueResolutionResult,
    FunctionCallInfo,
    ResolutionRecord,
    ScopeTracker,
    FunctionRegistry,
    ExecutionError,
    FunctionExecutionError,
};

pub use execution_context::{ExecutionContext, ExecutionContextSnapshot};

pub use ast_walker::ASTWalker;

// ── stubs for upcoming modules ──────────────────────────────────────────────
// pub mod value_resolver;
// pub mod function_interpreter;
