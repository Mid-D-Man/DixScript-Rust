//! # Utilities Module
//!
//! Core utility types and functions

mod keyword_definitions;
mod mid_logger;
mod mid_helper_functions;
mod utilities;

// Re-exports (PUBLIC TYPES ONLY)
pub use crate::Compiler::Core::Tokenizer::token::{Token, TokenType};
pub use keyword_definitions::Keywords;
pub use mid_logger::{LogLevel, MID_Logger};
pub use mid_helper_functions::MID_HelperFunctions;
pub use utilities::{ObjectExtensions, StringExtensions};

//no result for core rust has that oly for language specific wrapper