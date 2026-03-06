use super::position::Position;
use super::config::ConfigSection;
use super::imports::ImportsSection;
use super::dlm::DLMSection;
use super::enums::EnumsSection;
use super::quickfuncs::QuickFuncsSection;
use super::data::DataSection;
use super::security::SecuritySection;

/// Main DixScript AST structure
/// Represents a complete parsed .dixscript file
#[derive(Debug, Clone, PartialEq)]
pub struct DixScript {
    pub config: Option<ConfigSection>,
    pub imports: Option<ImportsSection>,
    pub dlm: Option<DLMSection>,
    pub enums: Option<EnumsSection>,
    pub quick_functions: Option<QuickFuncsSection>,
    pub data: Option<DataSection>,
    pub security: Option<SecuritySection>,
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
    ) -> Self {
        DixScript {
            config,
            imports,
            dlm,
            enums,
            quick_functions,
            data,
            security,
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
        
        Ok(())
    }
}

impl std::cmp::Eq for DixScript {}
