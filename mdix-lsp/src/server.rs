// mdix-lsp/src/server.rs
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::analyzer::{run_pipeline, get_resolved_ast};
use crate::capabilities::server_capabilities;
use crate::converters::to_diagnostics;
use crate::document::Document;
use crate::features;
use crate::features::code_lens::{
    CMD_COMPILE, CMD_MINIFY, CMD_SHOW_AST,
    CMD_TO_JSON, CMD_TO_TOML, CMD_VALIDATE,
};
use crate::features::commands::{
    run_compile, run_convert_to_json, run_convert_to_toml,
    run_minify, run_show_ast, run_validate, CommandResult,
};

const ANALYSIS_TIMEOUT_SECS: u64 = 10;

pub struct Backend {
    pub client:             Client,
    pub documents:          Arc<DashMap<Url, Document>>,
    pub shutdown_requested: AtomicBool,
    pub analysis_tasks:     StdMutex<Vec<tokio::task::JoinHandle<()>>>,
    pub pending_versions:   Arc<DashMap<Url, i32>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Backend {
            client,
            documents:          Arc::new(DashMap::new()),
            shutdown_requested: AtomicBool::new(false),
            analysis_tasks:     StdMutex::new(Vec::new()),
            pending_versions:   Arc::new(DashMap::new()),
        }
    }

    fn spawn_analysis(&self, uri: Url, source: String, version: i32) {
        if self.shutdown_requested.load(Ordering::Relaxed) { return; }

        self.pending_versions.insert(uri.clone(), version);

        let client    = self.client.clone();
        let documents = Arc::clone(&self.documents);
        let versions  = Arc::clone(&self.pending_versions);

        let handle = tokio::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(ANALYSIS_TIMEOUT_SECS),
                tokio::task::spawn_blocking({
                    let uri = uri.clone();
                    move || {
                        let mut doc = Document::new(uri.clone(), source, version);
                        let errors  = run_pipeline(&mut doc);
                        (uri, doc, errors, version)
                    }
                }),
            ).await;

            match result {
                Err(_) => tracing::warn!("Analysis timed out for {}", uri),
                Ok(Err(e)) => tracing::error!("Analysis panicked: {:?}", e),
                Ok(Ok((uri, doc, errors, ver))) => {
                    let latest = versions.get(&uri).map(|v| *v).unwrap_or(ver);
                    if ver >= latest {
                        let diags = to_diagnostics(&errors);
                        documents.insert(uri.clone(), doc);
                        client.publish_diagnostics(uri, diags, Some(ver)).await;
                    }
                }
            }
        });

        if let Ok(mut tasks) = self.analysis_tasks.lock() {
            tasks.retain(|h| !h.is_finished());
            tasks.push(handle);
        }
    }

    async fn show_message(&self, success: bool, msg: &str) {
        let kind = if success { MessageType::INFO } else { MessageType::ERROR };
        self.client.show_message(kind, msg).await;
    }

    /// Get the fully value-resolved AST for a document (for commands only).
    /// Runs value resolution synchronously — must be called from spawn_blocking.
    ///
    /// Falls back to the enhanced-but-unresolved AST when:
    ///   - the document has no QuickFuncs / no @DATA (nothing to resolve)
    ///   - value resolution fails or panics
    ///
    /// Returns None only when the document is not in the store or has never
    /// been parsed.
    fn resolved_ast_for_command(
        doc_ref: &Option<dashmap::mapref::one::Ref<'_, Url, Document>>,
    ) -> Option<dixscript::Compiler::AST::DixScript> {
        let doc = doc_ref.as_ref()?;
        // get_resolved_ast returns Some(ast) even on resolution failure
        // (falls back to enhanced AST), so we only get None if doc.ast is None.
        get_resolved_ast(doc).or_else(|| doc.ast.clone())
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> LspResult<InitializeResult> {
        tracing::info!("mdix-lsp initialize");
        Ok(InitializeResult {
            capabilities: server_capabilities(),
            server_info:  Some(ServerInfo {
                name:    "mdix-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("mdix-lsp initialized");
        self.client.log_message(MessageType::INFO, "mdix-lsp ready").await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        tracing::info!("mdix-lsp shutdown");
        self.shutdown_requested.store(true, Ordering::SeqCst);
        let handles: Vec<_> = self.analysis_tasks.lock()
            .map(|mut g| g.drain(..).collect()).unwrap_or_default();
        if !handles.is_empty() {
            tokio::spawn(async move {
                for h in handles {
                    h.abort();
                    let _ = tokio::time::timeout(
                        std::time::Duration::from_millis(50), h,
                    ).await;
                }
            });
        }
        Ok(())
    }

    // ── Document lifecycle ────────────────────────────────────────────────────

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.spawn_analysis(doc.uri, doc.text, doc.version);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.spawn_analysis(
                params.text_document.uri,
                change.text,
                params.text_document.version,
            );
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        self.pending_versions.remove(&uri);
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    // ── Completion ────────────────────────────────────────────────────────────

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri     = &params.text_document_position.text_document.uri;
        let pos     = params.text_document_position.position;
        let trigger = params.context.and_then(|c| c.trigger_character);
        Ok(features::completions::provide(
            self.documents.get(uri).as_deref(), pos, trigger.as_deref(),
        ))
    }

    // ── Signature help ────────────────────────────────────────────────────────

    async fn signature_help(&self, params: SignatureHelpParams) -> LspResult<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(features::signature_help::provide(
            self.documents.get(uri).as_deref(), pos, params.context,
        ))
    }

    // ── Hover ─────────────────────────────────────────────────────────────────

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(features::hover::provide(self.documents.get(uri).as_deref(), pos))
    }

    // ── Go-to definition ──────────────────────────────────────────────────────

    async fn goto_definition(&self, params: GotoDefinitionParams) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(features::goto_definition::provide(self.documents.get(uri).as_deref(), pos))
    }

    // ── Document highlight ────────────────────────────────────────────────────

    async fn document_highlight(&self, params: DocumentHighlightParams) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(features::document_highlights::provide(self.documents.get(uri).as_deref(), pos))
    }

    // ── References ────────────────────────────────────────────────────────────

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri  = &params.text_document_position.text_document.uri;
        let pos  = params.text_document_position.position;
        let incl = params.context.include_declaration;
        Ok(features::references::provide(self.documents.get(uri).as_deref(), uri, pos, incl))
    }

    // ── Rename ────────────────────────────────────────────────────────────────

    async fn prepare_rename(&self, params: TextDocumentPositionParams) -> LspResult<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        Ok(features::rename::prepare(self.documents.get(uri).as_deref(), params.position))
    }

    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        Ok(features::rename::provide(
            self.documents.get(uri).as_deref(), uri, pos, &params.new_name,
        ))
    }

    // ── Document symbols ──────────────────────────────────────────────────────

    async fn document_symbol(&self, params: DocumentSymbolParams) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        Ok(features::document_symbols::provide(self.documents.get(uri).as_deref()))
    }

    // ── Semantic tokens ───────────────────────────────────────────────────────

    async fn semantic_tokens_full(&self, params: SemanticTokensParams) -> LspResult<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        Ok(features::semantic_tokens::provide(self.documents.get(uri).as_deref()))
    }

    // ── Colors ────────────────────────────────────────────────────────────────

    async fn document_color(&self, params: DocumentColorParams) -> LspResult<Vec<ColorInformation>> {
        let uri = &params.text_document.uri;
        Ok(features::document_color::provide(self.documents.get(uri).as_deref()))
    }

    async fn color_presentation(&self, params: ColorPresentationParams) -> LspResult<Vec<ColorPresentation>> {
        Ok(features::document_color::presentation(params.color, params.range))
    }

    // ── Inlay hints ───────────────────────────────────────────────────────────

    async fn inlay_hint(&self, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        Ok(features::inlay_hints::provide(self.documents.get(uri).as_deref()))
    }

    // ── Code actions ──────────────────────────────────────────────────────────

    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let uri   = &params.text_document.uri;
        let diags = &params.context.diagnostics;
        Ok(features::code_actions::provide(self.documents.get(uri).as_deref(), diags))
    }

    // ── Folding ───────────────────────────────────────────────────────────────

    async fn folding_range(&self, params: FoldingRangeParams) -> LspResult<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;
        Ok(features::folding::provide(self.documents.get(uri).as_deref()))
    }

    // ── Formatting ────────────────────────────────────────────────────────────

    async fn formatting(&self, params: DocumentFormattingParams) -> LspResult<Option<Vec<TextEdit>>> {
        let uri  = &params.text_document.uri;
        let opts = &params.options;
        Ok(features::formatting::provide(self.documents.get(uri).as_deref(), opts))
    }

    // ── On-type formatting ────────────────────────────────────────────────────

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        if params.ch != "\n" {
            return Ok(None);
        }

        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let doc_ref = self.documents.get(uri);
        let source  = match &doc_ref {
            Some(d) => d.source.clone(),
            None    => return Ok(None),
        };
        drop(doc_ref);

        let prev_line_idx = match pos.line.checked_sub(1) {
            Some(l) => l as usize,
            None    => return Ok(None),
        };

        let lines: Vec<&str> = source.lines().collect();
        let prev_line = match lines.get(prev_line_idx) {
            Some(l) => *l,
            None    => return Ok(None),
        };

        let last_ch = prev_line.trim_end().chars().last();
        let (open, close) = match last_ch {
            Some('{') => ('{', '}'),
            Some('(') => ('(', ')'),
            Some('[') => ('[', ']'),
            _         => return Ok(None),
        };

        let indent: String = prev_line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();

        let inner_indent_size = params.options.tab_size as usize;
        let inner_indent      = if params.options.insert_spaces {
            " ".repeat(inner_indent_size)
        } else {
            "\t".to_string()
        };

        let cur_line_text = lines.get(pos.line as usize).copied().unwrap_or("");

        let next_line_has_close = lines
            .get(pos.line as usize + 1)
            .map(|l| l.trim_start().starts_with(close))
            .unwrap_or(false);

        let mut edits: Vec<TextEdit> = Vec::new();

        if next_line_has_close {
            edits.push(TextEdit {
                range: Range::new(
                    Position::new(pos.line, 0),
                    Position::new(pos.line, cur_line_text.len() as u32),
                ),
                new_text: format!("{}{}{}", indent, inner_indent, cur_line_text.trim_start()),
            });
        } else {
            let new_text = format!(
                "{}{}\n{}{}",
                indent, inner_indent,
                indent, close
            );
            edits.push(TextEdit {
                range: Range::new(
                    Position::new(pos.line, 0),
                    Position::new(pos.line, cur_line_text.len() as u32),
                ),
                new_text,
            });
        }

        let _ = open;

        if edits.is_empty() { Ok(None) } else { Ok(Some(edits)) }
    }

    // ── CodeLens (play button) ────────────────────────────────────────────────

    async fn code_lens(&self, params: CodeLensParams) -> LspResult<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;
        Ok(features::code_lens::provide(self.documents.get(uri).as_deref()))
    }

    // ── Execute command ───────────────────────────────────────────────────────
    //
    // Commands that produce output (→ JSON, → TOML, minify) use the fully
    // value-resolved AST so QuickFunc calls in @DATA are expanded to their
    // actual objects/values rather than appearing as null.
    //
    // Value resolution runs synchronously inside spawn_blocking, so it never
    // blocks the async executor.  The regular analysis pipeline is NOT changed —
    // resolution only happens on demand here.

    async fn execute_command(&self, params: ExecuteCommandParams) -> LspResult<Option<serde_json::Value>> {
        let command = params.command.as_str();

        let uri_str = params.arguments
            .first()
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let uri = uri_str
            .as_deref()
            .and_then(|s| Url::parse(s).ok());

        let source_path: Option<std::path::PathBuf> = uri
            .as_ref()
            .and_then(|u| u.to_file_path().ok());

        match command {

            // ── Validate ─────────────────────────────────────────────────────
            // Uses the existing diagnostic state — no resolution needed.
            CMD_VALIDATE => {
                let doc_ref = uri.as_ref().and_then(|u| self.documents.get(u));
                let (errors, warnings) = if let Some(doc) = &doc_ref {
                    let all   = doc.error_manager.get_all_errors_flat();
                    let errs  = all.iter().filter(|e| {
                        matches!(e.severity(),
                            dixscript::ErrorManager::ErrorSeverity::Error
                            | dixscript::ErrorManager::ErrorSeverity::Fatal)
                    }).count();
                    let warns = all.iter().filter(|e| {
                        matches!(e.severity(),
                            dixscript::ErrorManager::ErrorSeverity::Warning)
                    }).count();
                    (errs, warns)
                } else {
                    (0, 0)
                };
                let result = run_validate(errors, warnings);
                self.show_message(result.success, &result.message).await;
            }

            // ── Convert → JSON ────────────────────────────────────────────────
            // Runs value resolution so QuickFunc calls in @DATA are expanded.
            CMD_TO_JSON => {
                let doc_ref    = uri.as_ref().and_then(|u| self.documents.get(u));
                let path_clone = source_path.clone();

                let result = tokio::task::spawn_blocking(move || {
                    match Self::resolved_ast_for_command(&doc_ref) {
                        Some(ast) => run_convert_to_json(&ast, path_clone.as_deref()),
                        None => CommandResult::err(
                            "File has not been parsed yet — wait for analysis to complete."
                        ),
                    }
                }).await.unwrap_or_else(|_| {
                    CommandResult::err("JSON conversion task panicked.")
                });

                self.show_message(result.success, &result.message).await;
            }

            // ── Convert → TOML ────────────────────────────────────────────────
            // Runs value resolution so QuickFunc calls in @DATA are expanded.
            CMD_TO_TOML => {
                let doc_ref    = uri.as_ref().and_then(|u| self.documents.get(u));
                let path_clone = source_path.clone();

                let result = tokio::task::spawn_blocking(move || {
                    match Self::resolved_ast_for_command(&doc_ref) {
                        Some(ast) => run_convert_to_toml(&ast, path_clone.as_deref()),
                        None => CommandResult::err(
                            "File has not been parsed yet — wait for analysis to complete."
                        ),
                    }
                }).await.unwrap_or_else(|_| {
                    CommandResult::err("TOML conversion task panicked.")
                });

                self.show_message(result.success, &result.message).await;
            }

            // ── Minify ────────────────────────────────────────────────────────
            // Runs value resolution so the minified output contains actual
            // resolved values rather than function call stubs.
            CMD_MINIFY => {
                let doc_ref    = uri.as_ref().and_then(|u| self.documents.get(u));
                let path_clone = source_path.clone();

                let result = tokio::task::spawn_blocking(move || {
                    match Self::resolved_ast_for_command(&doc_ref) {
                        Some(ast) => run_minify(&ast, path_clone.as_deref()),
                        None => CommandResult::err(
                            "File has not been parsed yet — wait for analysis to complete."
                        ),
                    }
                }).await.unwrap_or_else(|_| {
                    CommandResult::err("Minify task panicked.")
                });

                self.show_message(result.success, &result.message).await;
            }

            // ── Compile ───────────────────────────────────────────────────────
            // Shells out to `mdix compile` — no in-process resolution needed.
            CMD_COMPILE => {
                let doc_ref    = uri.as_ref().and_then(|u| self.documents.get(u));
                let ast_clone  = doc_ref.as_ref().and_then(|d| d.ast.clone());
                let path_clone = source_path.clone();

                let result = tokio::task::spawn_blocking(move || {
                    run_compile(path_clone.as_deref(), ast_clone.as_ref())
                }).await.unwrap_or_else(|_| {
                    CommandResult::err("Compile task panicked.")
                });

                self.show_message(result.success, &result.message).await;
            }

            // ── Show AST ──────────────────────────────────────────────────────
            // Uses the enhanced AST (resolution not needed for structure display).
            CMD_SHOW_AST => {
                let doc_ref = uri.as_ref().and_then(|u| self.documents.get(u));
                let result  = if let Some(doc) = &doc_ref {
                    if let Some(ast) = &doc.ast {
                        run_show_ast(ast)
                    } else {
                        CommandResult::err("AST not available — wait for analysis.")
                    }
                } else {
                    CommandResult::err("Document not found.")
                };
                self.show_message(result.success, &result.message).await;
            }

            other => {
                tracing::warn!("Unknown command: {}", other);
                self.client
                    .show_message(
                        MessageType::WARNING,
                        &format!("Unknown command: {}", other),
                    )
                    .await;
            }
        }

        Ok(None)
    }
                }
