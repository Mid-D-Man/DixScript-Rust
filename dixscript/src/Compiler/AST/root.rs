// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/compiler.md, section "Compiler/AST/raw.rs"
// ============================================================================
use super::config::ConfigSection;
use super::imports::ImportsSection;
use super::dlm::DLMSection;
use super::enums::EnumsSection;
use super::quickfuncs::QuickFuncsSection;
use super::data::DataSection;
use super::security::SecuritySection;
use super::raw::RawBlock;

/// Main DixScript AST structure
/// Represents a complete parsed .mdix file
#[derive(Debug, Clone, PartialEq)]
pub struct DixScript {
    pub config: Option<ConfigSection>,
    pub imports: Option<ImportsSection>,
    pub dlm: Option<DLMSection>,
    pub enums: Option<EnumsSection>,
    pub quick_functions: Option<QuickFuncsSection>,
    pub data: Option<DataSection>,
    pub security: Option<SecuritySection>,
    /// `Vec`, not `Option<T>` — every `@RAW(...)` block in the file is
    /// independent (each has its own required, file-unique `meta_data.id`),
    /// so multiple blocks don't merge into one logical section the way
    /// repeated `@DATA`/`@QUICKFUNCS`/`@ENUMS` blocks do. An empty file has
    /// an empty Vec, not None.
    pub raw: Vec<RawBlock>,
}

impl DixScript {
    /// Create a new empty DixScript AST
    pub fn new() -> Self {
        DixScript {
            config: None,
            imports: None,
            dlm: None,
            enums: None,
            quick_functions: None,
            data: None,
            security: None,
            raw: Vec::new(),
        }
    }
    
    /// Create a DixScript AST with all sections
    pub fn with_sections(
        config: Option<ConfigSection>,
        imports: Option<ImportsSection>,
        dlm: Option<DLMSection>,
        enums: Option<EnumsSection>,
        quick_functions: Option<QuickFuncsSection>,
        data: Option<DataSection>,
        security: Option<SecuritySection>,
        raw: Vec<RawBlock>,
    ) -> Self {
        DixScript {
            config,
            imports,
            dlm,
            enums,
            quick_functions,
            data,
            security,
            raw,
        }
    }
}

impl Default for DixScript {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DixScript {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // @CONFIG Section
        if let Some(ref config) = self.config {
            writeln!(f, "{}", config)?;
            writeln!(f)?;
        }
        
        // @IMPORTS Section
        if let Some(ref imports) = self.imports {
            writeln!(f, "{}", imports)?;
            writeln!(f)?;
        }
        
        // @DLM Section
        if let Some(ref dlm) = self.dlm {
            writeln!(f, "{}", dlm)?;
            writeln!(f)?;
        }
        
        // @ENUMS Section
        if let Some(ref enums) = self.enums {
            writeln!(f, "{}", enums)?;
            writeln!(f)?;
        }
        
        // @QUICKFUNCS Section
        if let Some(ref quick_funcs) = self.quick_functions {
            writeln!(f, "{}", quick_funcs)?;
            writeln!(f)?;
        }
        
        // @DATA Section
        if let Some(ref data) = self.data {
            writeln!(f, "{}", data)?;
            writeln!(f)?;
        }
        
        // @SECURITY Section
        if let Some(ref security) = self.security {
            writeln!(f, "{}", security)?;
        }

        // @RAW blocks — zero or more, each printed as its own @RAW(...)
        for raw_block in &self.raw {
            writeln!(f, "{}", raw_block)?;
            writeln!(f)?;
        }
        
        Ok(())
    }
}

impl std::cmp::Eq for DixScript {}
