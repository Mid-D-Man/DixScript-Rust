//! Per-document state for the LSP.
//!
//! `doc.tokens` holds the FULL token stream produced by the tokenizer on the
//! original source, including all @CONFIG tokens with their real positions.
//! This means every LSP feature (hover, completions, semantic tokens, folding,
//! etc.) can work directly from the token stream without any source scanning
//! workarounds.
//!
//! The former `config_line_range` / `config_line_offset` fields are gone.
//! Methods that answer "is this position inside @CONFIG?" now query the token
//! stream — any token at that line with SectionId::Config gives the answer.

use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::{EnhancementResult, SemanticAnalysisResult};
use dixscript::Compiler::Core::Tokenizer::Token;
use dixscript::Compiler::Core::Tokenizer::token::SectionId;
use dixscript::ErrorManager::ErrorManager;
use tower_lsp::lsp_types::Url;

pub struct Document {
    pub uri:                Url,
    pub source:             String,
    pub error_manager:      ErrorManager,
    /// Full token stream from the tokenizer, including @CONFIG tokens.
    /// Positions are 1-based and accurate relative to the original file.
    pub tokens:             Vec<Token>,
    pub ast:                Option<DixScript>,
    pub semantic_result:    Option<SemanticAnalysisResult>,
    pub enhancement_result: Option<EnhancementResult>,
    pub version:            i32,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("uri",          &self.uri)
            .field("version",      &self.version)
            .field("source_len",   &self.source.len())
            .field("token_count",  &self.tokens.len())
            .field("has_ast",      &self.ast.is_some())
            .field("has_semantic", &self.semantic_result.is_some())
            .finish()
    }
}

impl Document {
    pub fn new(uri: Url, source: String, version: i32) -> Self {
        Document {
            uri,
            source,
            error_manager:      ErrorManager::new_isolated(),
            tokens:             Vec::new(),
            ast:                None,
            semantic_result:    None,
            enhancement_result: None,
            version,
        }
    }

    // ── Config-section queries ────────────────────────────────────────────────
    // All implemented via the real token stream — no source scanning, no stored
    // line ranges, no workaround fields.

    /// Returns true if `line` (0-based LSP) has any @CONFIG token on it.
    ///
    /// Blank lines and comment lines inside @CONFIG have no tokens and return
    /// false — this is intentional; hover/completion on a blank line inside
    /// @CONFIG will fall through to the general handler gracefully.
    #[inline]
    pub fn line_in_config(&self, line: u32) -> bool {
        let token_line = (line + 1) as usize; // LSP 0-based → token 1-based
        self.tokens
            .iter()
            .any(|t| t.line == token_line && t.section == SectionId::Config)
    }

    /// Returns true if `pos` falls on a line that contains @CONFIG tokens.
    #[inline]
    pub fn pos_in_config(&self, pos: tower_lsp::lsp_types::Position) -> bool {
        self.line_in_config(pos.line)
    }

    /// Given a line inside @CONFIG, extract the key at that line by scanning
    /// the source text. Returns `None` if the line has no `key -> value` pattern
    /// or if the line is not inside @CONFIG.
    pub fn config_key_at_line(&self, line: u32) -> Option<String> {
        if !self.line_in_config(line) {
            return None;
        }
        let line_text = self.source.lines().nth(line as usize)?;
        let trimmed = line_text.trim();
        let arrow_pos = trimmed.find("->")?;
        let key = trimmed[..arrow_pos].trim().to_string();
        if key.is_empty() || key.starts_with('@') {
            return None;
        }
        Some(key)
    }

    /// Returns true if the cursor is on the value side of `->` on a config line.
    pub fn cursor_on_value_side(
        &self,
        pos: tower_lsp::lsp_types::Position,
    ) -> bool {
        let line_text = match self.source.lines().nth(pos.line as usize) {
            Some(l) => l,
            None    => return false,
        };
        match line_text.find("->") {
            // arrow_byte is a byte offset; pos.character is a char offset.
            // For ASCII (all config keys/values) these are equal.
            Some(arrow_byte) => (pos.character as usize) > arrow_byte + 2,
            None             => false,
        }
    }

    // ── Token helpers ─────────────────────────────────────────────────────────

    /// Returns the token at `pos` (0-based LSP position), if any.
    ///
    /// Matches the first token whose 1-based (line, column) converts to the
    /// given 0-based LSP position. Useful for hover and goto-definition.
    pub fn token_at(&self, pos: tower_lsp::lsp_types::Position) -> Option<&Token> {
        let target_line = (pos.line + 1) as usize;
        let target_col  = (pos.character + 1) as usize;
        self.tokens.iter().find(|t| {
            t.line == target_line
                && t.column <= target_col
                && target_col < t.column + token_source_len(t)
        })
    }

    /// Returns all tokens on `line` (0-based LSP).
    pub fn tokens_on_line(&self, line: u32) -> Vec<&Token> {
        let target_line = (line + 1) as usize;
        self.tokens
            .iter()
            .filter(|t| t.line == target_line)
            .collect()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Approximate source-text byte length of a token — used for rough span
/// matching in `token_at`. Mirrors the logic in semantic_tokens.rs.
fn token_source_len(token: &Token) -> usize {
    use dixscript::Compiler::Core::Tokenizer::TokenType;
    match &token.token_type {
        TokenType::String(s)             => s.len() + 2,
        TokenType::StringSingle(s)       => s.len() + 2,
        TokenType::InterpolatedString(s) => s.len() + 3,
        TokenType::HexColor(h)           => h.trim_start_matches('#').len() + 1,
        TokenType::Comment(c)            => c.len() + 2,
        TokenType::Long(l)               => format!("{}L", l).len(),
        TokenType::SectionConfig         => 7,
        TokenType::SectionImports        => 8,
        TokenType::SectionDLM            => 4,
        TokenType::SectionEnums          => 6,
        TokenType::SectionQuickFuncs     => 11,
        TokenType::SectionData           => 5,
        TokenType::SectionSecurity       => 9,
        TokenType::DoubleColon           => 2,
        TokenType::Arrow                 => 2,
        TokenType::SwitchCase            => 2,

        TokenType::Bool(b)               => if *b { 4 } else { 5 },
        _ => {
            let v = token.get_token_value();
            if v.is_empty() { 1 } else { v.len() }
        }
    }
}
