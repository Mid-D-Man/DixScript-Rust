// src/Compiler/VersionControl/compatibility_result.rs
//! Compatibility check results for version management

use std::fmt;

/// Result of a version compatibility check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityResult {
    /// Feature/version is fully compatible
    Compatible,

    /// Feature/version is compatible but with warnings
    CompatibleWithWarnings {
        warnings: Vec<String>,
    },

    /// Feature/version is not compatible
    Incompatible {
        reason: String,
        required_version: Option<String>,
    },

    /// Feature/version is deprecated
    Deprecated {
        message: String,
        suggested_alternative: Option<String>,
    },
}

impl CompatibilityResult {
    /// Create a compatible result
    pub fn compatible() -> Self {
        CompatibilityResult::Compatible
    }

    /// Create a compatible result with warnings
    pub fn compatible_with_warnings(warnings: Vec<String>) -> Self {
        CompatibilityResult::CompatibleWithWarnings { warnings }
    }

    /// Create an incompatible result
    pub fn incompatible(reason: impl Into<String>) -> Self {
        CompatibilityResult::Incompatible {
            reason: reason.into(),
            required_version: None,
        }
    }

    /// Create an incompatible result with required version
    pub fn incompatible_with_version(reason: impl Into<String>, required_version: impl Into<String>) -> Self {
        CompatibilityResult::Incompatible {
            reason: reason.into(),
            required_version: Some(required_version.into()),
        }
    }

    /// Create a deprecated result
    pub fn deprecated(message: impl Into<String>) -> Self {
        CompatibilityResult::Deprecated {
            message: message.into(),
            suggested_alternative: None,
        }
    }

    /// Create a deprecated result with suggested alternative
    pub fn deprecated_with_alternative(message: impl Into<String>, alternative: impl Into<String>) -> Self {
        CompatibilityResult::Deprecated {
            message: message.into(),
            suggested_alternative: Some(alternative.into()),
        }
    }

    /// Check if result indicates compatibility
    pub fn is_compatible(&self) -> bool {
        matches!(self, CompatibilityResult::Compatible | CompatibilityResult::CompatibleWithWarnings { .. })
    }

    /// Check if result has warnings
    pub fn has_warnings(&self) -> bool {
        matches!(self, CompatibilityResult::CompatibleWithWarnings { .. })
    }

    /// Get warnings if present
    pub fn warnings(&self) -> Option<&[String]> {
        match self {
            CompatibilityResult::CompatibleWithWarnings { warnings } => Some(warnings),
            _ => None,
        }
    }
}

impl fmt::Display for CompatibilityResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompatibilityResult::Compatible => write!(f, "Compatible"),
            CompatibilityResult::CompatibleWithWarnings { warnings } => {
                write!(f, "Compatible with {} warning(s)", warnings.len())
            }
            CompatibilityResult::Incompatible { reason, required_version } => {
                match required_version {
                    Some(version) => write!(f, "Incompatible: {} (requires {})", reason, version),
                    None => write!(f, "Incompatible: {}", reason),
                }
            }
            CompatibilityResult::Deprecated { message, suggested_alternative } => {
                match suggested_alternative {
                    Some(alt) => write!(f, "Deprecated: {} (use {} instead)", message, alt),
                    None => write!(f, "Deprecated: {}", message),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compatible() {
        let result = CompatibilityResult::compatible();
        assert!(result.is_compatible());
        assert!(!result.has_warnings());
    }

    #[test]
    fn test_compatible_with_warnings() {
        let warnings = vec!["Warning 1".to_string(), "Warning 2".to_string()];
        let result = CompatibilityResult::compatible_with_warnings(warnings.clone());
        assert!(result.is_compatible());
        assert!(result.has_warnings());
        assert_eq!(result.warnings(), Some(warnings.as_slice()));
    }

    #[test]
    fn test_incompatible() {
        let result = CompatibilityResult::incompatible("Feature not found");
        assert!(!result.is_compatible());
    }

    #[test]
    fn test_display() {
        assert_eq!(
            CompatibilityResult::compatible().to_string(),
            "Compatible"
        );
        assert_eq!(
            CompatibilityResult::deprecated_with_alternative(
                "old_feature",
                "new_feature"
            ).to_string(),
            "Deprecated: old_feature (use new_feature instead)"
        );
    }
}