// src/Compiler/AST/Visitors/mod.rs

//! AST Visitors - Non-destructive traversal patterns for semantic analysis
//!
//! Provides visitor traits and implementations for analyzing AST nodes
//! without modifying them.

pub mod ast_visitor_base;
pub mod type_inference_visitor;

// Re-exports
pub use ast_visitor_base::AstVisitorBase;
pub use type_inference_visitor::TypeInferenceVisitor;

// ==================== VISITOR UTILITIES ====================

/// Helper trait for validating identifiers
pub trait IdentifierValidator {
    /// Check if a string is a valid identifier (letter/underscore start, alphanumeric+underscore)
    fn is_valid_identifier(name: &str) -> bool {
        if name.is_empty() {
            return false;
        }

        let mut chars = name.chars();
        
        // First character must be letter or underscore
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {},
            _ => return false,
        }

        // Rest must be alphanumeric or underscore
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }
}

/// Helper trait for checking duplicates
pub trait DuplicateChecker {
    /// Check if a collection has duplicates (case-insensitive)
    fn has_duplicates_ci(items: &[impl AsRef<str>]) -> bool {
        use std::collections::HashSet;
        
        let mut seen = HashSet::with_capacity(items.len());
        
        for item in items {
            let lower = item.as_ref().to_lowercase();
            if !seen.insert(lower) {
                return true;
            }
        }
        
        false
    }

    /// Find duplicate items (case-insensitive)
    fn find_duplicates_ci(items: &[impl AsRef<str>]) -> Vec<String> {
        use std::collections::HashMap;
        
        let mut counts: HashMap<String, usize> = HashMap::new();
        
        for item in items {
            let lower = item.as_ref().to_lowercase();
            *counts.entry(lower).or_insert(0) += 1;
        }

        counts.into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(name, _)| name)
            .collect()
    }
      }
