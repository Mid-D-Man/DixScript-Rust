// mdix-lsp/src/document.rs
//! Per-document state.  Rebuilt on every didOpen / didChange.

use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::{EnhancementResult, SemanticAnalysisResult};
use dixscript::ErrorManager::ErrorManager;
use tower_lsp::lsp_types::Url;

/// Everything the server knows about one open .mdix file.
pub struct Document {
    pub uri:               Url,
    pub source:            String,
    pub error_manager:     ErrorManager,
    pub tokens:            Vec<dixscript::Compiler::Core::Tokenizer::Token>,
    pub ast:               Option<DixScript>,
    pub semantic_result:   Option<SemanticAnalysisResult>,
    pub enhancement_result:Option<EnhancementResult>,
    pub version:           i32,
    /// Lines removed from the start of `source` when @CONFIG was stripped.
    pub config_line_offset:usize,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("uri",                 &self.uri)
            .field("version",             &self.version)
            .field("source_len",          &self.source.len())
            .field("token_count",         &self.tokens.len())
            .field("has_ast",             &self.ast.is_some())
            .field("has_semantic_result", &self.semantic_result.is_some())
            .field("config_line_offset",  &self.config_line_offset)
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
        }
    }

    pub fn update(&mut self, source: String, version: i32) {
        self.source             = source;
        self.version            = version;
        self.error_manager      = ErrorManager::new_isolated();
        self.tokens             = Vec::new();
        self.ast                = None;
        self.semantic_result    = None;
        self.enhancement_result = None;
        self.config_line_offset = 0;
    }
}