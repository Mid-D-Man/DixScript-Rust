// src/Compiler/Utilities/identifier_pattern_analyzer.rs
//  v1.0.0 - Resolved all ownership and borrowing issues

use crate::Compiler::Core::Tokenizer::{Token, TokenType};
use crate::Compiler::AST::Position;
use crate::ErrorManager::ErrorManager;

/// Pattern types for identifier sequences
/// Used across DATA and QUICKFUNCS sections
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierPatternType {
    Unknown,
    SimpleIdentifier,           // x
    LocalFunctionCall,          // func()
    LocalEnumAccess,            // Status.ACTIVE
    ImportedFunctionCall,       // utils.func()
    ImportedEnumAccess,         // utils.Status.ACTIVE
    StaticMethodCall,           // Math.sqrt() (uppercase class)
    TableOrGroupSyntax,         // data.users: or data.users:: (DATA section only)
}

/// Represents an analyzed identifier pattern
#[derive(Debug, Clone)]
pub struct IdentifierPattern {
    pub pattern_type: IdentifierPatternType,
    pub first_part: String,
    pub second_part: Option<String>,
    pub third_part: Option<String>,
    pub position: Position,
}

impl IdentifierPattern {
    pub fn new(
        pattern_type: IdentifierPatternType,
        first_part: String,
        position: Position,
    ) -> Self {
        IdentifierPattern {
            pattern_type,
            first_part,
            second_part: None,
            third_part: None,
            position,
        }
    }

    pub fn with_second(
        pattern_type: IdentifierPatternType,
        first_part: String,
        second_part: String,
        position: Position,
    ) -> Self {
        IdentifierPattern {
            pattern_type,
            first_part,
            second_part: Some(second_part),
            third_part: None,
            position,
        }
    }

    pub fn with_third(
        pattern_type: IdentifierPatternType,
        first_part: String,
        second_part: String,
        third_part: String,
        position: Position,
    ) -> Self {
        IdentifierPattern {
            pattern_type,
            first_part,
            second_part: Some(second_part),
            third_part: Some(third_part),
            position,
        }
    }
}

/// Identifier pattern analysis utilities
pub struct IdentifierPatternAnalyzer;

impl IdentifierPatternAnalyzer {
    /// Analyze identifier pattern in QUICKFUNCS context
    /// Simpler than DATA - no table/group syntax
    pub fn analyze_quickfuncs_pattern(
        first_identifier: &str,
        position: Position,
        tokens: &[Token],
        current_position: usize,
        error_manager: Option<&ErrorManager>,
    ) -> IdentifierPattern {
        Self::log_debug(
            error_manager,
            &format!("[QUICKFUNCS] Analyzing pattern: {}", first_identifier),
        );

        let next_token = Self::peek_ahead(tokens, current_position, 1);

        if next_token.is_none() {
            return IdentifierPattern::new(
                IdentifierPatternType::SimpleIdentifier,
                first_identifier.to_string(),
                position,
            );
        }

        let next = next_token.unwrap();

        // Check for function call: identifier(...)
        if let TokenType::Symbol(sym) = &next.token_type {
            if *sym == '(' {
                return IdentifierPattern::new(
                    IdentifierPatternType::LocalFunctionCall,
                    first_identifier.to_string(),
                    position,
                );
            }

            // Check for dot - multiple possibilities
            if *sym == '.' {
                return Self::analyze_dot_pattern_quickfuncs(
                    first_identifier,
                    position,
                    tokens,
                    current_position,
                    error_manager,
                );
            }
        }

        IdentifierPattern::new(
            IdentifierPatternType::SimpleIdentifier,
            first_identifier.to_string(),
            position,
        )
    }

    /// Analyze identifier pattern in DATA context
    /// More complex - includes table/group syntax detection
    pub fn analyze_data_pattern(
        first_identifier: &str,
        position: Position,
        tokens: &[Token],
        current_position: usize,
        error_manager: Option<&ErrorManager>,
    ) -> IdentifierPattern {
        Self::log_debug(
            error_manager,
            &format!("[DATA] Analyzing pattern: {}", first_identifier),
        );

        let next_token = Self::peek_ahead(tokens, current_position, 1);

        if next_token.is_none() {
            return IdentifierPattern::new(
                IdentifierPatternType::SimpleIdentifier,
                first_identifier.to_string(),
                position,
            );
        }

        let next = next_token.unwrap();

        // Check for function call: identifier(...)
        if let TokenType::Symbol(sym) = &next.token_type {
            if *sym == '(' {
                return IdentifierPattern::new(
                    IdentifierPatternType::LocalFunctionCall,
                    first_identifier.to_string(),
                    position,
                );
            }

            // Check for dot - multiple possibilities
            if *sym == '.' {
                return Self::analyze_dot_pattern_data(
                    first_identifier,
                    position,
                    tokens,
                    current_position,
                    error_manager,
                );
            }

            // Check for table/group syntax: identifier: or identifier::
            if *sym == ':' {
                return IdentifierPattern::new(
                    IdentifierPatternType::TableOrGroupSyntax,
                    first_identifier.to_string(),
                    position,
                );
            }
        }

        // Check for DoubleColon token
        if matches!(next.token_type, TokenType::DoubleColon) {
            return IdentifierPattern::new(
                IdentifierPatternType::TableOrGroupSyntax,
                first_identifier.to_string(),
                position,
            );
        }

        IdentifierPattern::new(
            IdentifierPatternType::SimpleIdentifier,
            first_identifier.to_string(),
            position,
        )
    }

    // ==================== PRIVATE HELPERS ====================

    fn analyze_dot_pattern_quickfuncs(
        first_identifier: &str,
        position: Position,
        tokens: &[Token],
        current_position: usize,
        error_manager: Option<&ErrorManager>,
    ) -> IdentifierPattern {
        let after_dot = Self::peek_ahead(tokens, current_position, 2);

        if after_dot.is_none() {
            return IdentifierPattern::new(
                IdentifierPatternType::Unknown,
                first_identifier.to_string(),
                position,
            );
        }

        let second_token = after_dot.unwrap();

        // Extract second identifier value
        let second_id = match &second_token.token_type {
            TokenType::Identifier(id) => id.as_str(),
            _ => {
                return IdentifierPattern::new(
                    IdentifierPatternType::SimpleIdentifier,
                    first_identifier.to_string(),
                    position,
                );
            }
        };

        let after_second = Self::peek_ahead(tokens, current_position, 3);

        // STATIC METHOD: ClassName.method() (uppercase first letter)
        if first_identifier.chars().next().map_or(false, |c| c.is_uppercase()) {
            if let Some(token) = after_second {
                if let TokenType::Symbol(sym) = &token.token_type {
                    if *sym == '(' {
                        Self::log_debug(
                            error_manager,
                            &format!("Pattern: {}.{}() - Static Method", first_identifier, second_id),
                        );
                        return IdentifierPattern::with_second(
                            IdentifierPatternType::StaticMethodCall,
                            first_identifier.to_string(),
                            second_id.to_string(),
                            position,
                        );
                    }
                }
            }
        }

        // IMPORTED FUNCTION: namespace.function() (lowercase first letter)
        if first_identifier.chars().next().map_or(false, |c| c.is_lowercase()) {
            if let Some(token) = after_second {
                if let TokenType::Symbol(sym) = &token.token_type {
                    if *sym == '(' {
                        Self::log_debug(
                            error_manager,
                            &format!("Pattern: {}.{}() - Imported Function", first_identifier, second_id),
                        );
                        return IdentifierPattern::with_second(
                            IdentifierPatternType::ImportedFunctionCall,
                            first_identifier.to_string(),
                            second_id.to_string(),
                            position,
                        );
                    }
                }
            }
        }

        // IMPORTED ENUM: namespace.EnumName.VALUE (3 parts, no parens)
        if let Some(token) = after_second {
            if let TokenType::Symbol(sym) = &token.token_type {
                if *sym == '.' {
                    let third_part = Self::peek_ahead(tokens, current_position, 4);
                    if let Some(third_token) = third_part {
                        if let TokenType::Identifier(id) = &third_token.token_type {
                            let third_id = id.as_str();
                            let after_third = Self::peek_ahead(tokens, current_position, 5);

                            // Make sure it's NOT followed by '('
                            let is_not_call = after_third.map_or(true, |t| {
                                !matches!(&t.token_type, TokenType::Symbol(s) if *s == '(')
                            });

                            if is_not_call {
                                Self::log_debug(
                                    error_manager,
                                    &format!(
                                        "Pattern: {}.{}.{} - Imported Enum",
                                        first_identifier, second_id, third_id
                                    ),
                                );
                                return IdentifierPattern::with_third(
                                    IdentifierPatternType::ImportedEnumAccess,
                                    first_identifier.to_string(),
                                    second_id.to_string(),
                                    third_id.to_string(),
                                    position,
                                );
                            }
                        }
                    }
                }
            }
        }

        // LOCAL ENUM: EnumName.VALUE (2 parts, no parens)
        let is_not_call = after_second.map_or(true, |t| {
            !matches!(&t.token_type, TokenType::Symbol(s) if *s == '(')
        });

        if is_not_call {
            Self::log_debug(
                error_manager,
                &format!("Pattern: {}.{} - Local Enum", first_identifier, second_id),
            );
            return IdentifierPattern::with_second(
                IdentifierPatternType::LocalEnumAccess,
                first_identifier.to_string(),
                second_id.to_string(),
                position,
            );
        }

        IdentifierPattern::new(
            IdentifierPatternType::SimpleIdentifier,
            first_identifier.to_string(),
            position,
        )
    }

    fn analyze_dot_pattern_data(
        first_identifier: &str,
        position: Position,
        tokens: &[Token],
        current_position: usize,
        error_manager: Option<&ErrorManager>,
    ) -> IdentifierPattern {
        let after_dot = Self::peek_ahead(tokens, current_position, 2);

        if after_dot.is_none() {
            return IdentifierPattern::new(
                IdentifierPatternType::Unknown,
                first_identifier.to_string(),
                position,
            );
        }

        let second_token = after_dot.unwrap();

        // Extract second identifier value
        let second_id = match &second_token.token_type {
            TokenType::Identifier(id) => id.as_str(),
            _ => {
                return IdentifierPattern::new(
                    IdentifierPatternType::SimpleIdentifier,
                    first_identifier.to_string(),
                    position,
                );
            }
        };

        let after_second = Self::peek_ahead(tokens, current_position, 3);

        // Check for namespace.function()
        if let Some(token) = after_second {
            if let TokenType::Symbol(sym) = &token.token_type {
                if *sym == '(' {
                    Self::log_debug(
                        error_manager,
                        &format!("Pattern: {}.{}() - Imported Function", first_identifier, second_id),
                    );
                    return IdentifierPattern::with_second(
                        IdentifierPatternType::ImportedFunctionCall,
                        first_identifier.to_string(),
                        second_id.to_string(),
                        position,
                    );
                }
            }
        }

        // Check for namespace.Enum.VALUE (3 parts)
        if let Some(token) = after_second {
            if let TokenType::Symbol(sym) = &token.token_type {
                if *sym == '.' {
                    let third_part = Self::peek_ahead(tokens, current_position, 4);
                    if let Some(third_token) = third_part {
                        if let TokenType::Identifier(id) = &third_token.token_type {
                            let third_id = id.as_str();
                            let after_third = Self::peek_ahead(tokens, current_position, 5);

                            // Not followed by ( or : or ::
                            // Handles: comma, newline, identifier, or end of tokens
                            let is_enum = match after_third {
                                None => true,
                                Some(t) => match &t.token_type {
                                    TokenType::Symbol(s) => *s != '(' && *s != ':',
                                    TokenType::DoubleColon => false,
                                    _ => true, // comma, newline, identifier, etc.
                                }
                            };

                            if is_enum {
                                Self::log_debug(
                                    error_manager,
                                    &format!(
                                        "Pattern: {}.{}.{} - Imported Enum",
                                        first_identifier, second_id, third_id
                                    ),
                                );
                                return IdentifierPattern::with_third(
                                    IdentifierPatternType::ImportedEnumAccess,
                                    first_identifier.to_string(),
                                    second_id.to_string(),
                                    third_id.to_string(),
                                    position,
                                );
                            }
                        }
                    }
                }
            }
        }

        // Check for local enum: Enum.VALUE
        // FIXED: Correctly detect when NOT followed by '(' or ':' or '::'
        // Handles: enum followed by comma, newline, identifier, or end of tokens
        // IMPORTANT: Must explicitly check for DoubleColon to avoid misclassifying table syntax
        let is_local_enum = match after_second {
            None => true, // Nothing after = local enum
            Some(t) => match &t.token_type {
                TokenType::Symbol(s) => *s != '(' && *s != ':', // Symbol but not call or table
                TokenType::DoubleColon => false, // Explicitly not enum (table syntax)
                _ => true, // Non-symbol (comma, newline, identifier) = local enum
            }
        };

        if is_local_enum {
            Self::log_debug(
                error_manager,
                &format!("Pattern: {}.{} - Local Enum", first_identifier, second_id),
            );
            return IdentifierPattern::with_second(
                IdentifierPatternType::LocalEnumAccess,
                first_identifier.to_string(),
                second_id.to_string(),
                position,
            );
        }

        // Check for table syntax: identifier.property: or identifier.property::
        if let Some(token) = after_second {
            if let TokenType::Symbol(sym) = &token.token_type {
                if *sym == ':' {
                    Self::log_debug(
                        error_manager,
                        &format!("Pattern: {}.{}: - Table Property", first_identifier, second_id),
                    );
                    return IdentifierPattern::with_second(
                        IdentifierPatternType::TableOrGroupSyntax,
                        first_identifier.to_string(),
                        second_id.to_string(),
                        position,
                    );
                }
            }

            if matches!(token.token_type, TokenType::DoubleColon) {
                Self::log_debug(
                    error_manager,
                    &format!("Pattern: {}.{}:: - Group Array", first_identifier, second_id),
                );
                return IdentifierPattern::with_second(
                    IdentifierPatternType::TableOrGroupSyntax,
                    first_identifier.to_string(),
                    second_id.to_string(),
                    position,
                );
            }
        }

        // Fallback: simple identifier
        IdentifierPattern::new(
            IdentifierPatternType::SimpleIdentifier,
            first_identifier.to_string(),
            position,
        )
    }

    /// Peek ahead N tokens without advancing position
    /// Returns None if out of bounds
    fn peek_ahead(tokens: &[Token], current_position: usize, offset: usize) -> Option<&Token> {
        let look_ahead_pos = current_position.checked_add(offset)?;
        tokens.get(look_ahead_pos)
    }

    /// Helper for debug logging (only logs if errorManager present and debug enabled)
    fn log_debug(error_manager: Option<&ErrorManager>, message: &str) {
        if let Some(em) = error_manager {
            em.log_debug(&format!("[IdentifierPatternAnalyzer] {}", message));
        }
    }
}