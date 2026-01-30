use crate::Compiler::AST::Position;
use std::fmt;

/// Represents a location where one function calls another
/// Used for detailed error reporting when cycles are detected
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallSite {
    pub caller: String,
    pub callee: String,
    pub position: Position,
}

impl CallSite {
    pub fn new(caller: String, callee: String, position: Position) -> Self {
        CallSite {
            caller,
            callee,
            position,
        }
    }
}

impl fmt::Display for CallSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.position.is_valid() {
            write!(f, "{} → {} at {}", self.caller, self.callee, self.position)
        } else {
            write!(f, "{} → {}", self.caller, self.callee)
        }
    }
}
