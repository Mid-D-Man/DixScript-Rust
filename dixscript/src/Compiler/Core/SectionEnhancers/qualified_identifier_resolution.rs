
//! Resolution metadata for QualifiedIdentifier nodes
//! Tracks what type a qualified identifier actually resolves to

use crate::Compiler::AST::{Expression, Position};

/// Type of qualified identifier after resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QualifiedIdentifierType {
    /// Could not determine type (will be runtime resolved)
    Unknown,
    
    /// Local enum access: Status.ACTIVE
    LocalEnumAccess,
    
    /// Imported enum access: utils.Status.ACTIVE
    ImportedEnumAccess,
    
    /// Imported function call: utils.calculateTax()
    ImportedFunctionCall,
    
    /// Static builtin object access: Math.sqrt(), DateTime.now()
    StaticObjectAccess,
    
    /// Object property access or instance method call: user.name, text.upper()
    /// This includes local variables, data section variables, and parameters
    ObjectPropertyAccess,
    
    /// Reference to an enum type (not a value): utils.Status
    /// Used when passing enum type around (rare)
    NamespaceEnumReference,
}

impl std::fmt::Display for QualifiedIdentifierType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QualifiedIdentifierType::Unknown => write!(f, "Unknown"),
            QualifiedIdentifierType::LocalEnumAccess => write!(f, "LocalEnumAccess"),
            QualifiedIdentifierType::ImportedEnumAccess => write!(f, "ImportedEnumAccess"),
            QualifiedIdentifierType::ImportedFunctionCall => write!(f, "ImportedFunctionCall"),
            QualifiedIdentifierType::StaticObjectAccess => write!(f, "StaticObjectAccess"),
            QualifiedIdentifierType::ObjectPropertyAccess => write!(f, "ObjectPropertyAccess"),
            QualifiedIdentifierType::NamespaceEnumReference => write!(f, "NamespaceEnumReference"),
        }
    }
}

/// Key for identifying a QualifiedIdentifier in the resolution map
/// Uses position + parts + is_call for uniqueness
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedIdentifierKey {
    pub position: Position,
    pub parts: Vec<String>,
    pub is_call: bool,
}

impl QualifiedIdentifierKey {
    /// Create key from QualifiedIdentifier expression
    pub fn from_qualified_identifier(expr: &Expression) -> Option<Self> {
        if let Expression::QualifiedIdentifier { parts, arguments, position } = expr {
            Some(QualifiedIdentifierKey {
                position: *position,
                parts: parts.clone(),
                is_call: arguments.is_some(),
            })
        } else {
            None
        }
    }
}

/// Resolution metadata for a QualifiedIdentifier node
#[derive(Debug, Clone)]
pub struct QualifiedIdentifierResolution {
    /// What type this qualified identifier resolved to
    pub resolved_type: QualifiedIdentifierType,
    
    /// Additional context about the resolution
    /// - For LocalEnumAccess: enum name
    /// - For ImportedEnumAccess: "namespace.EnumName"
    /// - For ImportedFunctionCall: namespace name
    /// - For StaticObjectAccess: object name (e.g., "Math")
    /// - For ObjectPropertyAccess: context ("local", "data", "unknown")
    pub context: Option<String>,
    
    /// Original parts for reference
    pub parts: Vec<String>,
    
    /// Whether this is a call
    pub is_call: bool,
    
    /// Position in source
    pub position: Position,
}

impl QualifiedIdentifierResolution {
    /// Create new resolution
    pub fn new(
        resolved_type: QualifiedIdentifierType,
        context: Option<String>,
        parts: Vec<String>,
        is_call: bool,
        position: Position,
    ) -> Self {
        QualifiedIdentifierResolution {
            resolved_type,
            context,
            parts,
            is_call,
            position,
        }
    }
    
    /// Create from QualifiedIdentifier expression
    pub fn from_expression(
        expr: &Expression,
        resolved_type: QualifiedIdentifierType,
        context: Option<String>,
    ) -> Option<Self> {
        if let Expression::QualifiedIdentifier { parts, arguments, position } = expr {
            Some(QualifiedIdentifierResolution {
                resolved_type,
                context,
                parts: parts.clone(),
                is_call: arguments.is_some(),
                position: *position,
            })
        } else {
            None
        }
    }
}

impl std::fmt::Display for QualifiedIdentifierResolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts = self.parts.join(".");
        let call_suffix = if self.is_call { "()" } else { "" };
        let context_str = self.context.as_ref()
            .map(|c| format!(" [{}]", c))
            .unwrap_or_default();
        
        write!(f, "{}{} → {}{}", parts, call_suffix, self.resolved_type, context_str)
    }
  }
