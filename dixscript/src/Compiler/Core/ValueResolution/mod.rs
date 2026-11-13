// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/Core/ValueResolution/mod.rs"
// ============================================================================
//! # Value Resolution — compile-time QuickFunction execution
//!
//! ## Pipeline phases
//! | Phase | Module               | Role                                                      |
//! |-------|----------------------|-----------------------------------------------------------|
//! | 1     | `enum_resolution`    | Enum pre-resolution (EnumValue → Integer)                 |
//! | 2     | `data_context`       | Initial data context build from literal DATA entries      |
//! | 3     | `ast_walker`         | Discover all QuickFunction call sites in @DATA            |
//! | 4     | `value_resolver`, `write_back` | Iterative execution; results written into the arena |
//! | 5     | `value_resolver`, `identifier_resolution` | Resolve remaining Identifier references |
//!
//! `value_resolver` runs the phases in order and owns the loops of phases 4 and 5; the other
//! modules do the walks over the arena data section.
//!
//! ## Supporting modules
//! | Module               | Role                                                      |
//! |----------------------|-----------------------------------------------------------|
//! | `supporting_classes` | Shared data types used across the pipeline                |
//! | `execution_context`  | Scoped variable environment for function execution        |
//! | `function_interpreter`| Executes QuickFunction bodies against an ExecutionContext|

pub mod supporting_classes;
pub mod execution_context;
pub mod ast_walker;
pub(crate) mod data_context;
pub(crate) mod enum_resolution;
pub mod value_resolver;
pub mod function_interpreter;
pub(crate) mod identifier_resolution;
pub(crate) mod write_back;

pub use supporting_classes::{
    ValueResolutionResult,
    FunctionCallInfo,
    ResolutionRecord,
    ScopeTracker,
    FunctionRegistry,
    ExecutionError,
    FunctionExecutionError,
};

pub use crate::Compiler::Utilities::symbol_table::ImportedNamespace;

pub use execution_context::{ExecutionContext, ExecutionContextSnapshot};
pub use ast_walker::ASTWalker;
pub use value_resolver::{ValueResolver, ResolverError};
pub use function_interpreter::{FunctionInterpreter, InterpreterError, LambdaAst};
