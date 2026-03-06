use super::position::Position;

/// @IMPORTS Section
#[derive(Debug, Clone, PartialEq)]
pub struct ImportsSection {
    pub imports: Vec<ImportDeclaration>,
    pub position: Position,
}

impl ImportsSection {
    pub fn new(imports: Vec<ImportDeclaration>, position: Position) -> Self {
        ImportsSection { imports, position }
    }
}

impl std::fmt::Display for ImportsSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "@IMPORTS(")?;
        for (i, import) in self.imports.iter().enumerate() {
            write!(f, "  {}", import)?;
            if i < self.imports.len() - 1 {
                writeln!(f, ",")?;
            } else {
                writeln!(f)?;
            }
        }
        write!(f, ")")
    }
}

/// Import declaration
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDeclaration {
    pub alias: String,
    pub path: String,
    pub is_cloud_import: bool,
    pub verify_hash: Option<String>,
    pub position: Position,
}

impl ImportDeclaration {
    pub fn new(
        alias: String,
        path: String,
        is_cloud_import: bool,
        verify_hash: Option<String>,
        position: Position,
    ) -> Self {
        ImportDeclaration {
            alias,
            path,
            is_cloud_import,
            verify_hash,
            position,
        }
    }

    /// Constructor for local imports (backward compatible)
    pub fn local(
        alias: String,
        path: String,
        verify_hash: Option<String>,
        position: Position,
    ) -> Self {
        ImportDeclaration::new(alias, path, false, verify_hash, position)
    }
}

impl std::fmt::Display for ImportDeclaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let from_keyword = if self.is_cloud_import { "from_cloud" } else { "from" };
        write!(f, "{} {} \"{}\"", self.alias, from_keyword, self.path)?;
        if let Some(ref hash) = self.verify_hash {
            write!(f, " verify \"{}\"", hash)?;
        }
        Ok(())
    }
}