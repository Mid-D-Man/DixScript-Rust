use super::position::Position;
use super::values::Value;

/// @SECURITY Section
#[derive(Debug, Clone, PartialEq)]
pub struct SecuritySection {
    pub entries: Vec<SecurityEntry>,
    pub position: Position,
}

impl SecuritySection {
    pub fn new(entries: Vec<SecurityEntry>, position: Position) -> Self {
        SecuritySection { entries, position }
    }
}

impl std::fmt::Display for SecuritySection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "@SECURITY(")?;
        for (i, entry) in self.entries.iter().enumerate() {
            writeln!(f, "  {} -> {{", entry.block_key)?;
            for (j, field) in entry.fields.iter().enumerate() {
                write!(f, "    {} = {}", field.key, field.value)?;
                if j < entry.fields.len() - 1 {
                    writeln!(f, ",")?;
                } else {
                    writeln!(f)?;
                }
            }
            write!(f, "  }}")?;
            if i < self.entries.len() - 1 {
                writeln!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }
        write!(f, ")")
    }
}

/// Security entry
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityEntry {
    pub block_key: String,
    pub fields: Vec<SecurityField>,
    pub position: Position,
}

impl SecurityEntry {
    pub fn new(block_key: String, fields: Vec<SecurityField>, position: Position) -> Self {
        SecurityEntry {
            block_key,
            fields,
            position,
        }
    }
}

impl std::fmt::Display for SecurityEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -> {{ ", self.block_key)?;
        for (i, field) in self.fields.iter().enumerate() {
            write!(f, "{}", field)?;
            if i < self.fields.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, " }}")
    }
}

/// Security field
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityField {
    pub key: String,
    pub value: Value,
    pub position: Position,
}

impl SecurityField {
    pub fn new(key: String, value: Value, position: Position) -> Self {
        SecurityField { key, value, position }
    }
}

impl std::fmt::Display for SecurityField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} = {}", self.key, self.value)
    }
      }
