use super::position::Position;
use super::data_types::{DataType, DeclarationType};
use super::expressions::Expression;
use super::values::Value;

/// QuickFunction statements
#[derive(Debug, Clone, PartialEq)]
pub enum QuickFuncStatement {
    // Return statement
    Return {
        value: Expression,
        position: Position,
    },
    
    // If statement
    If {
        condition: Expression,
        then_branch: Vec<QuickFuncStatement>,
        else_branch: Option<Vec<QuickFuncStatement>>,
        position: Position,
    },
    
    // Switch statement
    Switch {
        expression: Expression,
        cases: Vec<SwitchCase>,
        default_case: Option<SwitchCase>,
        position: Position,
    },
    
    // Assignment
    Assignment {
        variable: String,
        value: Expression,
        position: Position,
    },
    
    // Arithmetic assignment (+=, -=, etc.)
    ArithmeticAssignment {
        variable: String,
        operator: String,
        value: Expression,
        position: Position,
    },
    
    // Object creation
    ObjectCreation {
        variable: String,
        object: Value, // Should be Value::Object
        position: Position,
    },
    
    // Log statement
    Log {
        value: Expression,
        position: Position,
    },
    
    // Expression statement
    ExpressionStatement {
        expression: Expression,
        position: Position,
    },
    
    // Variable declaration: let x = 5, let mut y<int> = 10, const z = 15
    VariableDeclaration {
        declaration_type: DeclarationType,
        is_mutable: bool,
        variable_name: String,
        data_type: Option<DataType>,
        value: Expression,
        position: Position,
    },
}

impl QuickFuncStatement {
    pub fn position(&self) -> Position {
        match self {
            QuickFuncStatement::Return { position, .. } => *position,
            QuickFuncStatement::If { position, .. } => *position,
            QuickFuncStatement::Switch { position, .. } => *position,
            QuickFuncStatement::Assignment { position, .. } => *position,
            QuickFuncStatement::ArithmeticAssignment { position, .. } => *position,
            QuickFuncStatement::ObjectCreation { position, .. } => *position,
            QuickFuncStatement::Log { position, .. } => *position,
            QuickFuncStatement::ExpressionStatement { position, .. } => *position,
            QuickFuncStatement::VariableDeclaration { position, .. } => *position,
        }
    }
}

impl std::fmt::Display for QuickFuncStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuickFuncStatement::Return { value, .. } => {
                write!(f, "return {}", value)
            }
            
            QuickFuncStatement::If { condition, then_branch, else_branch, .. } => {
                write!(f, "if: {} {{ ", condition)?;
                for stmt in then_branch {
                    write!(f, "{}; ", stmt)?;
                }
                write!(f, "}}")?;
                if let Some(else_stmts) = else_branch {
                    write!(f, " else {{ ")?;
                    for stmt in else_stmts {
                        write!(f, "{}; ", stmt)?;
                    }
                    write!(f, "}}")?;
                }
                Ok(())
            }
            
            QuickFuncStatement::Switch { expression, cases, default_case, .. } => {
                write!(f, "chk: {} {{ ", expression)?;
                for case in cases {
                    write!(f, "{} ", case)?;
                }
                if let Some(default) = default_case {
                    write!(f, "{} ", default)?;
                }
                write!(f, "}}")
            }
            
            QuickFuncStatement::Assignment { variable, value, .. } => {
                write!(f, "{} = {}", variable, value)
            }
            
            QuickFuncStatement::ArithmeticAssignment { variable, operator, value, .. } => {
                write!(f, "{} {} {}", variable, operator, value)
            }
            
            QuickFuncStatement::ObjectCreation { variable, object, .. } => {
                write!(f, "{} = {}", variable, object)
            }
            
            QuickFuncStatement::Log { value, .. } => {
                write!(f, "log: {}", value)
            }
            
            QuickFuncStatement::ExpressionStatement { expression, .. } => {
                write!(f, "{}", expression)
            }
            
            QuickFuncStatement::VariableDeclaration {
                declaration_type,
                is_mutable,
                variable_name,
                data_type,
                value,
                ..
            } => {
                write!(f, "{}", declaration_type)?;
                if *is_mutable {
                    write!(f, " mut")?;
                }
                write!(f, " {}", variable_name)?;
                if let Some(dt) = data_type {
                    write!(f, "<{}>", dt)?;
                }
                write!(f, " = {}", value)
            }
        }
    }
}

/// Switch case
#[derive(Debug, Clone, PartialEq)]
pub struct SwitchCase {
    pub case_value: Value,
    pub statements: Vec<QuickFuncStatement>,
    pub position: Position,
}

impl SwitchCase {
    pub fn new(case_value: Value, statements: Vec<QuickFuncStatement>, position: Position) -> Self {
        SwitchCase {
            case_value,
            statements,
            position,
        }
    }
}

impl std::fmt::Display for SwitchCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "-> {} {{ ", self.case_value)?;
        for stmt in &self.statements {
            write!(f, "{}; ", stmt)?;
        }
        write!(f, "}}")
    }
      }
