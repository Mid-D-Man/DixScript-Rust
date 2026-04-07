
//! # Value Resolution — compile-time QuickFunction execution
//!
//! ## Pipeline phases
//! | Phase | Module               | Role                                                      |
//! |-------|----------------------|-----------------------------------------------------------|
//! | 1     | `value_resolver`     | Enum pre-resolution (EnumValue → Integer)                 |
//! | 2     | `value_resolver`     | Initial data context build from literal DATA entries      |
//! | 3     | `ast_walker`         | Discover all QuickFunction call sites in @DATA            |
//! | 4     | `value_resolver`     | Iterative execution and AST replacement                   |
//! | 5     | `value_resolver`     | Resolve remaining Identifier references                   |
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
pub mod value_resolver;
pub mod function_interpreter;

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
