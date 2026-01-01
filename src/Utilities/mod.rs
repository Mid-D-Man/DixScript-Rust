//! # Utilities Module
//!
//! Core utility types and functions

// Module declarations (PRIVATE - lowercase with underscores)
mod token;
mod keyword_definitions;
mod mid_logger;
mod mid_helper_functions;
mod utilities;

// Re-exports (PUBLIC TYPES ONLY)
pub use token::{Token, TokenType};
pub use keyword_definitions::Keywords;
pub use mid_logger::{MID_Logger, LogLevel};
pub use mid_helper_functions::MID_HelperFunctions;
pub use utilities::{StringExtensions, ObjectExtensions};

//no result for core rust has that oly for language specific wrapper