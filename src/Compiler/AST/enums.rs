use super::position::Position;

/// @ENUMS Section
#[derive(Debug, Clone, PartialEq)]
pub struct EnumsSection {
    pub enums: Vec<EnumDeclaration>,
    pub position: Position,
}

impl EnumsSection {
    pub fn new(enums: Vec<EnumDeclaration>, position: Position) -> Self {
        EnumsSection { enums, position }
    }
}

impl std::fmt::Display for EnumsSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "@ENUMS(")?;
        for (i, enum_decl) in self.enums.iter().enumerate() {
            writeln!(f, "  {} {{", enum_decl.name)?;
            for (j, field) in enum_decl.fields.iter().enumerate() {
                write!(f, "    {}", field.name)?;
                if let Some(value) = field.value {
                    write!(f, " = {}", value)?;
                }
                if j < enum_decl.fields.len() - 1 {
                    writeln!(f, ",")?;
                } else {
                    writeln!(f)?;
                }
            }
            write!(f, "  }}")?;
            if i < self.enums.len() - 1 {
                writeln!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }
        write!(f, ")")
    }
}

/// Enum declaration
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDeclaration {
    pub name: String,
    pub fields: Vec<EnumField>,
    pub position: Position,
}

impl EnumDeclaration {
    pub fn new(name: String, fields: Vec<EnumField>, position: Position) -> Self {
        EnumDeclaration { name, fields, position }
    }
}

impl std::fmt::Display for EnumDeclaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {{ ", self.name)?;
        for (i, field) in self.fields.iter().enumerate() {
            write!(f, "{}", field)?;
            if i < self.fields.len() - 1 {
                write!(f, ", ")?;
            }
        }
        write!(f, " }}")
    }
}

/// Enum field
#[derive(Debug, Clone, PartialEq)]
pub struct EnumField {
    pub name: String,
    pub value: Option<i32>,
    pub position: Position,
}

impl EnumField {
    pub fn new(name: String, value: Option<i32>, position: Position) -> Self {
        EnumField { name, value, position }
    }
}

impl std::fmt::Display for EnumField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(value) = self.value {
            write!(f, " = {}", value)?;
        }
        Ok(())
    }
}