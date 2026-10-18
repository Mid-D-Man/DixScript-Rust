// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/AST/mod.rs"
// ============================================================================
//! # AST - Abstract Syntax Tree for DixScript
//!
//! This module contains all AST node definitions for DixScript v1.0.0
//!
//! The types of each submodule are re-exported flat, so `Compiler::AST::Value` and
//! `Compiler::AST::DixScript` are the usual paths. The one exception is [`arena`]: an index
//! arena for the `Value` family and an arena-resident form of a whole `DixScript`. It is
//! reached as `Compiler::AST::arena`. So far only the `@SECURITY`, `@RAW`, `@SCHEMA` and `@DATA`
//! section parsers use it, and each exports the owned types again at the end of its section.

pub mod position;
pub mod data_types;
pub mod config;
pub mod imports;
pub mod dlm;
pub mod enums;
pub mod security;
pub mod raw;
pub mod schema;
pub mod data;
pub mod values;
pub mod expressions;
pub mod statements;
pub mod quickfuncs;
pub mod root;
pub mod helpers;
pub mod arena;
pub mod Visitors;

// Re-exports for convenience
pub use position::Position;
pub use data_types::{
    DataType,
    ElemType,
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
pub use raw::{RawBlock, RawField, RawContent};
pub use schema::{SchemaBlock, SchemaField};
pub use data::{DataSection, DataEntry, TablePath, PropertyAssignment};
pub use values::{Value, ObjectProperty};
pub use expressions::Expression;
pub use statements::{QuickFuncStatement, SwitchCase};
pub use quickfuncs::{QuickFuncsSection, QuickFunction, QuickFuncParam};
pub use root::DixScript;

pub use helpers::*;
pub use Visitors::*;
