//! Function call analysis - cycle detection and call graph construction
//!
//! Detects circular dependencies in @QUICKFUNCS at compile-time using DFS.
//!
//! ## Modules
//! - `call_site` - Location where one function calls another
//! - `call_graph` - Directed graph of function calls
//! - `call_collector` - AST visitor that builds the call graph
//! - `cycle_detector` - High-level validator that reports cycles

pub mod call_site;
pub mod call_graph;
pub mod call_collector;
pub mod cycle_detector;

pub use call_site::CallSite;
pub use call_graph::{CallGraph, CallGraphStats};
pub use call_collector::FunctionCallCollector;
pub use cycle_detector::CycleDetectionValidator;
