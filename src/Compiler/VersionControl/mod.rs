// src/Compiler/VersionControl/mod.rs
pub mod version_manager;
pub mod compatibility_result;
mod forward_compatibility_manager;

pub use version_manager::VersionManager;
pub use compatibility_result::CompatibilityResult;