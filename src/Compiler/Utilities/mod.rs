// src/Compiler/Utilities/mod.rs (add to existing)
pub mod identifier_pattern_analyzer;
mod security_utilities;  
pub use identifier_pattern_analyzer::{
    IdentifierPatternAnalyzer,
    IdentifierPattern,
    IdentifierPatternType,
};
pub use security_utilities::SecurityUtilities;  
