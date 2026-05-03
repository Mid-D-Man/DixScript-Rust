// mdix-lsp/src/document.rs
use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::{EnhancementResult, SemanticAnalysisResult};
use dixscript::ErrorManager::ErrorManager;
use tower_lsp::lsp_types::Url;

pub struct Document {
    pub uri:                Url,
    pub source:             String,
    pub error_manager:      ErrorManager,
    pub tokens:             Vec<dixscript::Compiler::Core::Tokenizer::Token>,
    pub ast:                Option<DixScript>,
    pub semantic_result:    Option<SemanticAnalysisResult>,
    pub enhancement_result: Option<EnhancementResult>,
    pub version:            i32,
    /// Line offset from @CONFIG stripping (always 0 — kept for compatibility).
    pub config_line_offset: usize,
    /// The 0-based LSP line range (start, end) of the @CONFIG block in the
    /// original source. None if the file has no @CONFIG section.
    ///
    /// Used by hover, completions, semantic tokens, and folding to handle
    /// @CONFIG correctly — since @CONFIG is stripped before tokenisation,
    /// NO tokens carry SectionId::Config. Position-based detection is the
    /// only way to answer requests for lines inside the config block.
    pub config_line_range:  Option<(u32, u32)>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("uri",               &self.uri)
            .field("version",           &self.version)
            .field("source_len",        &self.source.len())
            .field("token_count",       &self.tokens.len())
            .field("has_ast",           &self.ast.is_some())
            .field("has_semantic",      &self.semantic_result.is_some())
            .field("config_line_range", &self.config_line_range)
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
            config_line_offset: 0,
            config_line_range:  None,
        }
    }

    /// Return true if `line` (0-based LSP) falls inside the @CONFIG block.
    pub fn line_in_config(&self, line: u32) -> bool {
        match self.config_line_range {
            Some((start, end)) => line >= start && line <= end,
            None               => false,
        }
    }

    /// Return true if `pos` falls inside the @CONFIG block.
    pub fn pos_in_config(&self, pos: tower_lsp::lsp_types::Position) -> bool {
        self.line_in_config(pos.line)
    }

    /// Given a line inside @CONFIG, extract the key at that line by scanning
    /// the source text (no tokens available for @CONFIG lines).
    pub fn config_key_at_line(&self, line: u32) -> Option<String> {
        let line_text = self.source.lines().nth(line as usize)?;
        let trimmed = line_text.trim();
        if let Some(arrow_pos) = trimmed.find("->") {
            let key = trimmed[..arrow_pos].trim().to_string();
            if !key.is_empty() && !key.starts_with('@') {
                return Some(key);
            }
        }
        None
    }

    /// Given a position inside @CONFIG, determine which side of `->` the
    /// cursor is on. Returns `true` if on the value side.
    pub fn cursor_on_value_side(&self, pos: tower_lsp::lsp_types::Position) -> bool {
        let line_text = match self.source.lines().nth(pos.line as usize) {
            Some(l) => l,
            None    => return false,
        };
        if let Some(arrow_byte) = line_text.find("->") {
            // arrow_byte is a byte offset; pos.character is a char offset.
            // For ASCII (all config keys/values), they're the same.
            return (pos.character as usize) > arrow_byte + 2;
        }
        false
    }
}
