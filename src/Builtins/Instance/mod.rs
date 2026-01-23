// src/Builtins/Instance/mod.rs
//! Instance - Instance methods for built-in types

pub mod number_methods;
pub mod string_methods;
pub mod array_methods;
pub mod tuple_methods;
pub mod universal_methods;
pub mod regex_methods;
pub mod blob_methods;

// Re-export for convenience
pub use number_methods::{get_int_methods, get_float_methods, get_double_methods};
pub use string_methods::get_methods as get_string_methods;
pub use array_methods::get_methods as get_array_methods;
pub use tuple_methods::get_methods as get_tuple_methods;
pub use universal_methods::get_methods as get_universal_methods;
pub use regex_methods::get_methods as get_regex_methods;
pub use blob_methods::get_methods as get_blob_methods;