use super::position::Position;
use super::data_types::{DLMModuleType, DLMModuleSubtype};

/// @DLM Section
#[derive(Debug, Clone, PartialEq)]
pub struct DLMSection {
    pub modules: Vec<DLMModule>,
    pub position: Position,
}

impl DLMSection {
    pub fn new(modules: Vec<DLMModule>, position: Position) -> Self {
        DLMSection { modules, position }
    }
}

impl std::fmt::Display for DLMSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "@DLM(")?;
        for (i, module) in self.modules.iter().enumerate() {
            write!(f, "{}", module)?;
            if i < self.modules.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ")")
    }
}

/// DLM Module
#[derive(Debug, Clone, PartialEq)]
pub struct DLMModule {
    pub module_type: DLMModuleType,
    pub subtype: Option<DLMModuleSubtype>,
    pub position: Position,
}

impl DLMModule {
    pub fn new(
        module_type: DLMModuleType,
        subtype: Option<DLMModuleSubtype>,
        position: Position,
    ) -> Self {
        DLMModule {
            module_type,
            subtype,
            position,
        }
    }
}

impl std::fmt::Display for DLMModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.module_type)?;
        if let Some(ref subtype) = self.subtype {
            write!(f, ".{}", subtype)?;
        }
        Ok(())
    }
}