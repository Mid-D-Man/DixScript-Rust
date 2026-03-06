//! Per-document state. Rebuilt on every didOpen / didChange.

use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::Core::{EnhancementResult, SemanticAnalysisResult};
use dixscript::ErrorManager::ErrorManager;
use tower_lsp::lsp_types::Url;

/// Everything the server knows about one open .mdix file.
///
/// Rebuilt from scratch on every text change — the compiler pipeline is fast
/// enough that full re-analysis on each keystroke is acceptable.
#[derive(Debug)]
pub struct Document {
    /// The editor's canonical URI for this file.
    pub uri: Url,

    /// Current source text as received from the editor (full sync).
    pub source: String,

    /// Isolated error manager for this document.
    /// Never shares state with other documents or the CLI singleton.
    pub error_manager: ErrorManager,

    /// Token stream produced by the lexer.
    /// `None` if tokenization failed before producing any tokens.
    pub tokens: Vec<dixscript::Compiler::Core::Tokenizer::Token>,

    /// Parsed AST. `None` if the parser could not produce any output.
    pub ast: Option<DixScript>,

    /// Results of semantic analysis. `None` if analysis was skipped
    /// (e.g. because the parser produced no AST).
    pub semantic_result: Option<SemanticAnalysisResult>,

    /// Results of AST enhancement. `None` if enhancement was skipped.
    pub enhancement_result: Option<EnhancementResult>,

    /// Version counter from the editor. Used to discard stale analysis results
    /// when a newer change arrives before the current analysis finishes.
    pub version: i32,
}

impl Document {
    /// Create a blank document slot before analysis runs.
    pub fn new(uri: Url, source: String, version: i32) -> Self {
        Document {
            uri,
            source,
            error_manager: ErrorManager::new_isolated(),
            tokens: Vec::new(),
            ast: None,
            semantic_result: None,
            enhancement_result: None,
            version,
        }
    }

    /// Replace the source and reset all derived state.
    /// Called before re-running the pipeline on a new version of the document.
    pub fn update(&mut self, source: String, version: i32) {
        self.source             = source;
        self.version            = version;
        self.error_manager      = ErrorManager::new_isolated();
        self.tokens             = Vec::new();
        self.ast                = None;
        self.semantic_result    = None;
        self.enhancement_result = None;
    }
}
