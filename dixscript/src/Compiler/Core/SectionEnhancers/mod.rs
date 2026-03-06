// src/Compiler/Core/SectionEnhancers/mod.rs
//! AST enhancers for different sections

pub mod qualified_identifier_resolution;
pub mod qualified_identifier_resolver;
pub mod quickfuncs_ast_enhancer;

pub use qualified_identifier_resolution::{
    QualifiedIdentifierKey, QualifiedIdentifierResolution, QualifiedIdentifierType,
};
pub use qualified_identifier_resolver::QualifiedIdentifierResolver;
pub use quickfuncs_ast_enhancer::QuickFunctionsAstEnhancer;
