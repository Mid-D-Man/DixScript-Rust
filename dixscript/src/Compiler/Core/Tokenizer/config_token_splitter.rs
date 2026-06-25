//! Splits a full DixScript token stream at the `@CONFIG` section boundary.
//!
//! pipeline
//!
//!   Tokenizer (full source)
//!       ↓
//!   split_config_tokens
//!       ├─ config_tokens → ConfigSectionHandler::process_config_tokens
//!       └─ rest_tokens   → GeneralParser
//!
//! Because the tokenizer runs on the FULL source, all token positions are
//! accurate relative to the original file with no offset arithmetic.

use crate::Compiler::Core::Tokenizer::token::{Token, TokenType};

/// Result of splitting a full token stream at the `@CONFIG` boundary.
pub struct TokenSplitResult {
    /// Tokens belonging to the `@CONFIG` section — from the `SectionConfig`
    /// token through the section's closing `)`, inclusive.
    /// Empty when the source contains no `@CONFIG` section.
    pub config_tokens: Vec<Token>,

    /// All other tokens in source order, ending with `EndOfFile`.
    /// Forwarded unchanged to `GeneralParser`.
    pub rest_tokens: Vec<Token>,
}

/// Splits `tokens` (the full output of `Tokenizer::tokenize`) at the
/// `@CONFIG` section.
///
/// # Algorithm
/// 1. Find the `SectionConfig` token.
/// 2. From there, locate the opening `(` (skipping any intervening tokens).
/// 3. Track parenthesis depth to find the matching `)`.
/// 4. `config_tokens` = `[SectionConfig ..= closing_paren]`
///    `rest_tokens`   = everything before `SectionConfig` +
///                      everything after `closing_paren`, in order.
///
/// If no `SectionConfig` is found, `config_tokens` is empty and
/// `rest_tokens` receives all tokens.
pub fn split_config_tokens(tokens: Vec<Token>) -> TokenSplitResult {
    // ── 1. Locate @CONFIG ─────────────────────────────────────────────────
    let config_start = match tokens
        .iter()
        .position(|t| matches!(t.token_type, TokenType::SectionConfig))
    {
        Some(i) => i,
        None => {
            return TokenSplitResult {
                config_tokens: vec![],
                rest_tokens:   tokens,
            }
        }
    };

    // ── 2. Find opening '(' ───────────────────────────────────────────────
    let open_paren = match tokens[config_start..]
        .iter()
        .position(|t| matches!(t.token_type, TokenType::Symbol('(')))
    {
        Some(rel) => config_start + rel,
        None => {
            // @CONFIG with no opening paren — treat tail as config, rest before.
            let rest = tokens[..config_start].to_vec();
            let cfg  = tokens[config_start..].to_vec();
            return TokenSplitResult { config_tokens: cfg, rest_tokens: rest };
        }
    };

    // ── 3. Find matching closing ')' ──────────────────────────────────────
    let mut depth: i32 = 0;
    let mut close_paren: Option<usize> = None;

    for (offset, token) in tokens[open_paren..].iter().enumerate() {
        match &token.token_type {
            TokenType::Symbol('(') => depth += 1,
            TokenType::Symbol(')') => {
                depth -= 1;
                if depth == 0 {
                    close_paren = Some(open_paren + offset);
                    break;
                }
            }
            // Stop scanning at a new section keyword or EOF so a
            // malformed (unclosed) @CONFIG doesn't eat the whole file.
            TokenType::SectionDLM
            | TokenType::SectionEnums
            | TokenType::SectionImports
            | TokenType::SectionQuickFuncs
            | TokenType::SectionData
            | TokenType::SectionSecurity
            | TokenType::EndOfFile => break,
            _ => {}
        }
    }

    // Fallback for an unclosed section: stop just before EOF.
    let close_paren = close_paren.unwrap_or_else(|| {
        tokens
            .iter()
            .rposition(|t| !matches!(t.token_type, TokenType::EndOfFile))
            .unwrap_or(tokens.len().saturating_sub(1))
    });

    // ── 4. Build the two vecs ─────────────────────────────────────────────
    let config_tokens: Vec<Token> = tokens[config_start..=close_paren].to_vec();

    let mut rest_tokens: Vec<Token> = tokens[..config_start].to_vec();
    if close_paren + 1 < tokens.len() {
        rest_tokens.extend_from_slice(&tokens[close_paren + 1..]);
    }

    TokenSplitResult { config_tokens, rest_tokens }
  }
