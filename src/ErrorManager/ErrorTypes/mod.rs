//! # ErrorManager Module
//!
//! Comprehensive error handling for DixScript compilation and runtime
//! Uses Rust's Result<T, E> pattern with strongly-typed error categories

// Module declarations (private)
mod lexical_error;
mod parse_error;
mod semantic_error;
mod ast_enhancement_error;
mod value_resolution_error;
mod dlm_error;
mod binary_serialization_error;
mod config_error;
mod runtime_error;
mod general_error;
// Module declarations (private)
mod error_enums;

// Public re-exports
pub use lexical_error::{LexicalError, LexicalErrorType};
pub use parse_error::{ParseError, ParseErrorType};
pub use semantic_error::{SemanticError, SemanticErrorType};
pub use ast_enhancement_error::{AstEnhancementError, AstEnhancementErrorType};
pub use value_resolution_error::{ValueResolutionError, ValueResolutionErrorType};
pub use dlm_error::{DLMError, DLMErrorType, DLMPipelineException};
pub use binary_serialization_error::{BinarySerializationError, BinarySerializationErrorType};
pub use config_error::{ConfigError, ConfigErrorType};
pub use runtime_error::{RuntimeError, RuntimeErrorType};
pub use general_error::GeneralError;
pub use error_enums::{ErrorSeverity, ErrorSource};