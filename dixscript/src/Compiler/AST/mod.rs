//! # AST - Abstract Syntax Tree for DixScript
//!
//! This module contains all AST node definitions for DixScript v1.0.0

pub mod position;
pub mod data_types;
pub mod config;
pub mod imports;
pub mod dlm;
pub mod enums;
pub mod security;
pub mod data;
pub mod values;
pub mod expressions;
pub mod statements;
pub mod quickfuncs;
pub mod root;
pub mod helpers;
pub mod Visitors;

// Re-exports for convenience
pub use position::Position;
pub use data_types::{
    DataType,
    ElemType,            // ← new typed-collection element type
    ErrorHandlingStrategy,
    CompatibilityMode,
    DebugMode,
    DLMModuleType,
    DLMModuleSubtype,
    DeclarationType,
};
pub use config::{ConfigSection, ConfigEntry, ConfigValue};
pub use imports::{ImportsSection, ImportDeclaration};
pub use dlm::{DLMSection, DLMModule};
pub use enums::{EnumsSection, EnumDeclaration, EnumField};
pub use security::{SecuritySection, SecurityEntry, SecurityField};
pub use data::{DataSection, DataEntry, TablePath, PropertyAssignment};
pub use values::{Value, ObjectProperty};
pub use expressions::Expression;
pub use statements::{QuickFuncStatement, SwitchCase};
pub use quickfuncs::{QuickFuncsSection, QuickFunction, QuickFuncParam};
pub use root::DixScript;

pub use helpers::*;
pub use Visitors::*;
