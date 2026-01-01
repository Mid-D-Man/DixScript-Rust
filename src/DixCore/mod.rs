//! # DixCore - C#-like Collection Types
//!
//! Provides C#-style collection interfaces for DixScript
//! All methods use PascalCase naming convention

// Module declarations - PRIVATE (no pub keyword)
mod immutable_array;
mod list;
mod dictionary;
mod hash_set;
mod linq;

// Re-exports - PUBLIC TYPES ONLY
pub use immutable_array::ImmutableArray;
pub use list::List;
pub use dictionary::Dictionary;
pub use hash_set::HashSet;
pub use linq::Linq;