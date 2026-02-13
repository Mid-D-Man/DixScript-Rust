//! Section readers for binary format deserialization
//!
//! Each reader handles deserialization of a specific DixScript section
//! from binary format back into AST structures.

mod config_section_reader;
mod enums_section_reader;
mod imports_section_reader;
mod security_section_reader;
mod data_section_reader;

// Re-exports
pub use config_section_reader::ConfigSectionReader;
pub use enums_section_reader::EnumsSectionReader;
pub use imports_section_reader::ImportsSectionReader;
pub use security_section_reader::SecuritySectionReader;
pub use data_section_reader::DataSectionReader;

// Note: Need to import shared dependencies
pub(crate) use super::binary_format;
pub(crate) use super::section_offset;
pub(crate) use super::binary_serialization_context;
pub(crate) use super::binary_serialization_error;
pub(crate) use super::value_decoder;