//! # DixScript - MidManStudio Data Interchange Extension Script
//!
//! Secure, efficient data interchange format with built-in encryption,
//! compile-time functions, and cross-platform support.

// Module declarations
pub mod DixCore;
pub mod Utilities;
pub mod ErrorManager;
pub mod Builtins;
pub mod Compiler;
pub mod Runtime;

// Re-exports for convenience
pub use DixCore::*;
pub use Utilities::*;
pub use ErrorManager::*;