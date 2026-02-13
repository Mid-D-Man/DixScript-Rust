//! Section writers for binary format serialization
//!
//! Each writer handles serialization of a specific DixScript section
//! from AST structures into binary format.

mod config_section_writer;
mod enums_section_writer;
mod imports_section_writer;
mod security_section_writer;
mod data_section_writer;

// Re-exports
pub use config_section_writer::ConfigSectionWriter;
pub use enums_section_writer::EnumsSectionWriter;
pub use imports_section_writer::ImportsSectionWriter;
pub use security_section_writer::SecuritySectionWriter;
pub use data_section_writer::DataSectionWriter;

// Note: Need to import shared dependencies
pub(crate) use super::binary_format;
pub(crate) use super::section_offset;
pub(crate) use super::binary_serialization_context;
pub(crate) use super::binary_serialization_error;
pub(crate) use super::value_encoder;