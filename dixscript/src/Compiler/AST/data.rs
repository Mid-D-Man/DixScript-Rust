use super::position::Position;
use super::data_types::DataType;
use super::values::Value;

/// @DATA Section
#[derive(Debug, Clone, PartialEq)]
pub struct DataSection {
    pub entries: Vec<DataEntry>,
    pub position: Position,
}

impl DataSection {
    pub fn new(entries: Vec<DataEntry>, position: Position) -> Self {
        DataSection { entries, position }
    }
}

impl std::fmt::Display for DataSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "@DATA(")?;
        for (i, entry) in self.entries.iter().enumerate() {
            write!(f, "  {}", entry)?;
            if i < self.entries.len() - 1 {
                writeln!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }
        write!(f, ")")
    }
}

/// Data entries (different types)
#[derive(Debug, Clone, PartialEq)]
pub enum DataEntry {
    SimpleProperty {
        name: String,
        data_type: Option<DataType>,
        value: Value,
        position: Position,
    },
    TableProperty {
        path: TablePath,
        properties: Vec<PropertyAssignment>,
        position: Position,
    },
    GroupArray {
        path: TablePath,
        items: Vec<Value>,
        position: Position,
    },
    ObjectProperty {
        name: String,
        data_type: Option<DataType>,
        object: Box<Value>, // Boxed to avoid circular size issue
        position: Position,
    },
}

impl DataEntry {
    pub fn position(&self) -> Position {
        match self {
            DataEntry::SimpleProperty { position, .. } => *position,
            DataEntry::TableProperty { position, .. } => *position,
            DataEntry::GroupArray { position, .. } => *position,
            DataEntry::ObjectProperty { position, .. } => *position,
        }
    }
}

impl std::fmt::Display for DataEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataEntry::SimpleProperty { name, data_type, value, .. } => {
                write!(f, "{}", name)?;
                if let Some(dt) = data_type {
                    write!(f, "<{}>", dt)?;
                }
                write!(f, " = {}", value)
            }
            DataEntry::TableProperty { path, properties, .. } => {
                write!(f, "{}: ", path)?;
                for (i, prop) in properties.iter().enumerate() {
                    write!(f, "{}", prop)?;
                    if i < properties.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                Ok(())
            }
            DataEntry::GroupArray { path, items, .. } => {
                write!(f, "{}:: ", path)?;
                for (i, item) in items.iter().enumerate() {
                    write!(f, "{}", item)?;
                    if i < items.len() - 1 {
                        write!(f, ", ")?;
                    }
                }
                Ok(())
            }
            DataEntry::ObjectProperty { name, data_type, object, .. } => {
                write!(f, "{}", name)?;
                if let Some(dt) = data_type {
                    write!(f, "<{}>", dt)?;
                }
                write!(f, " = {}", object)
            }
        }
    }
}

/// Table path (e.g., user.profile.settings)
#[derive(Debug, Clone, PartialEq)]
pub struct TablePath {
    pub segments: Vec<String>,
}

impl TablePath {
    pub fn new(segments: Vec<String>) -> Self {
        TablePath { segments }
    }
}

impl std::fmt::Display for TablePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.segments.join("."))
    }
}

/// Property assignment in table properties
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyAssignment {
    pub name: String,
    pub data_type: Option<DataType>,
    pub value: Value,
    pub position: Position,
}

impl PropertyAssignment {
    pub fn new(
        name: String,
        data_type: Option<DataType>,
        value: Value,
        position: Position,
    ) -> Self {
        PropertyAssignment {
            name,
            data_type,
            value,
            position,
        }
    }
}

impl std::fmt::Display for PropertyAssignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(ref dt) = self.data_type {
            write!(f, "<{}>", dt)?;
        }
        write!(f, " = {}", self.value)
    }
                                       }
