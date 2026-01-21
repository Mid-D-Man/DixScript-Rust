use super::position::Position;
use super::data_types::DataType;
use super::statements::QuickFuncStatement;
use super::expressions::Expression;

/// @QUICKFUNCS Section
#[derive(Debug, Clone, PartialEq)]
pub struct QuickFuncsSection {
    pub functions: Vec<QuickFunction>,
    pub position: Position,
}

impl QuickFuncsSection {
    pub fn new(functions: Vec<QuickFunction>, position: Position) -> Self {
        QuickFuncsSection { functions, position }
    }
}

impl std::fmt::Display for QuickFuncsSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "@QUICKFUNCS(")?;
        for (i, func) in self.functions.iter().enumerate() {
            write!(f, "  ~{}", func.name)?;
            if let Some(ref rt) = func.return_type {
                write!(f, "<{}>", rt)?;
            }
            if let Some(ref scope) = func.scope_list {
                write!(f, " => {}", scope.join(","))?;
            }
            write!(f, "(")?;
            for (j, param) in func.parameters.iter().enumerate() {
                write!(f, "{}", param)?;
                if j < func.parameters.len() - 1 {
                    write!(f, ", ")?;
                }
            }
            writeln!(f, ") {{")?;
            for stmt in &func.body {
                writeln!(f, "    {}", stmt)?;
            }
            write!(f, "  }}")?;
            if i < self.functions.len() - 1 {
                writeln!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }
        write!(f, ")")
    }
}

/// QuickFunction definition
#[derive(Debug, Clone, PartialEq)]
pub struct QuickFunction {
    pub name: String,
    pub return_type: Option<DataType>,
    pub scope_list: Option<Vec<String>>,
    pub parameters: Vec<QuickFuncParam>,
    pub body: Vec<QuickFuncStatement>,
    pub position: Position,
}

impl QuickFunction {
    pub fn new(
        name: String,
        return_type: Option<DataType>,
        scope_list: Option<Vec<String>>,
        parameters: Vec<QuickFuncParam>,
        body: Vec<QuickFuncStatement>,
        position: Position,
    ) -> Self {
        QuickFunction {
            name,
            return_type,
            scope_list,
            parameters,
            body,
            position,
        }
    }
}

impl std::fmt::Display for QuickFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "~{}", self.name)?;
        if let Some(ref rt) = self.return_type {
            write!(f, "<{}>", rt)?;
        }
        if let Some(ref scope) = self.scope_list {
            write!(f, " => {}", scope.join(","))?;
        }
        write!(f, "(")?;
        for (i, param) in self.parameters.iter().enumerate() {
            write!(f, "{}", param)?;
            if i < self.parameters.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, ") {{ ")?;
        for stmt in &self.body {
            write!(f, "{}; ", stmt)?;
        }
        write!(f, "}}")
    }
}

/// QuickFunction parameter
#[derive(Debug, Clone, PartialEq)]
pub struct QuickFuncParam {
    pub name: String,
    pub data_type: Option<DataType>,
    pub default_value: Option<Expression>,
    pub position: Position,
}

impl QuickFuncParam {
    pub fn new(
        name: String,
        data_type: Option<DataType>,
        default_value: Option<Expression>,
        position: Position,
    ) -> Self {
        QuickFuncParam {
            name,
            data_type,
            default_value,
            position,
        }
    }
}

impl std::fmt::Display for QuickFuncParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(ref dt) = self.data_type {
            write!(f, "<{}>", dt)?;
        }
        if let Some(ref default) = self.default_value {
            write!(f, " = {}", default)?;
        }
        Ok(())
    }
      }
