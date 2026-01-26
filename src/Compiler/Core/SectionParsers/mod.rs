// src/Compiler/Core/SectionParsers/mod.rs

//! Section parsers for different DixScript sections

pub mod enums_section_parser;
pub mod dlm_section_parser;
pub mod security_section_parser;
pub mod imports_section_parser;

pub use enums_section_parser::EnumsSectionParser;
pub use dlm_section_parser::DlmSectionParser;
pub use security_section_parser::SecuritySectionParser;
pub use imports_section_parser::ImportsSectionParser;

// TODO: Add other section parsers:
// - DataSectionParser
// - QuickFuncsSectionParser