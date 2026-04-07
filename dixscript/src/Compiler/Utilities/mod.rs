
//! Compiler utilities

pub mod identifier_pattern_analyzer;
pub mod security_utilities;
pub mod symbol_table;
pub mod path_builder;
pub mod comment_filter;
pub mod file_permissions;

pub use identifier_pattern_analyzer::{
    IdentifierPatternAnalyzer,
    IdentifierPattern,
    IdentifierPatternType,
};
pub use security_utilities::SecurityUtilities;
pub use symbol_table::{
    SymbolTable,
    FunctionSignature,
    ParameterInfo,
    VariableInfo,
    DixFunctionSignature,
    ImportedNamespace,
    QuickFunctionInfo,
};
pub use path_builder::PathBuilder;
pub use comment_filter::CommentFilter;
