// src/Compiler/Core/SectionParsers/mod.rs

//! Section parsers for different DixScript sections

pub mod enums_section_parser;

pub use enums_section_parser::EnumsSectionParser;

// TODO: Add other section parsers:
// - ConfigSectionParser
// - DataSectionParser
// - QuickFuncsSectionParser
// - etc.