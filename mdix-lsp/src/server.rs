// mdix-lsp/src/server.rs
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::analyzer::run_pipeline;
use crate::capabilities::server_capabilities;
use crate::converters::to_diagnostics;
use crate::document::Document;
use crate::features;

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
        if self.shutdown_requested.load(Ordering::Relaxed) {
            return;
        }

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
            )
            .await;

            match result {
                Err(_timeout) => {
                    tracing::warn!(
                        "Analysis timed out after {}s for {}",
                        ANALYSIS_TIMEOUT_SECS, uri
                    );
                }
                Ok(Err(panic_err)) => {
                    tracing::error!("Analysis task panicked: {:?}", panic_err);
                }
                Ok(Ok((uri, doc, errors, ver))) => {
                    let latest = versions.get(&uri).map(|v| *v).unwrap_or(ver);
                    if ver >= latest {
                        let diags = to_diagnostics(&errors);
                        tracing::debug!(
                            "Publishing {} diagnostics for {} (v{})",
                            diags.len(), uri, ver
                        );
                        documents.insert(uri.clone(), doc);
                        client.publish_diagnostics(uri, diags, Some(ver)).await;
                    } else {
                        tracing::debug!(
                            "Discarding stale analysis v{} for {} (latest v{})",
                            ver, uri, latest
                        );
                    }
                }
            }
        });

        if let Ok(mut tasks) = self.analysis_tasks.lock() {
            tasks.retain(|h| !h.is_finished());
            tasks.push(handle);
        }
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

    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("mdix-lsp initialized");
        self.client.log_message(MessageType::INFO, "mdix-lsp ready").await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        tracing::info!("mdix-lsp shutdown");
        self.shutdown_requested.store(true, Ordering::SeqCst);

        let task_handles: Vec<_> = self.analysis_tasks
            .lock()
            .map(|mut g| g.drain(..).collect())
            .unwrap_or_default();

        if !task_handles.is_empty() {
            tokio::spawn(async move {
                for handle in task_handles {
                    handle.abort();
                    let _ = tokio::time::timeout(
                        std::time::Duration::from_millis(50),
                        handle,
                    ).await;
                }
                tracing::debug!("All analysis tasks aborted");
            });
        }

        Ok(())
    }

    // ── Document lifecycle ────────────────────────────────────────────────────

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        tracing::debug!("didOpen: {} v{}", doc.uri, doc.version);
        self.spawn_analysis(doc.uri, doc.text, doc.version);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            tracing::debug!(
                "didChange: {} v{}",
                params.text_document.uri,
                params.text_document.version
            );
            self.spawn_analysis(
                params.text_document.uri,
                change.text,
                params.text_document.version,
            );
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        tracing::debug!("didClose: {}", uri);
        self.documents.remove(&uri);
        self.pending_versions.remove(&uri);
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    // ── Completion ────────────────────────────────────────────────────────────

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> LspResult<Option<CompletionResponse>> {
        let uri     = &params.text_document_position.text_document.uri;
        let pos     = params.text_document_position.position;
        let doc     = self.documents.get(uri);
        let trigger = params.context.and_then(|ctx| ctx.trigger_character);
        Ok(features::completions::provide(doc.as_deref(), pos, trigger.as_deref()))
    }

    // ── Signature help ────────────────────────────────────────────────────────

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> LspResult<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let ctx = params.context;
        let doc = self.documents.get(uri);
        Ok(features::signature_help::provide(doc.as_deref(), pos, ctx))
    }

    // ── Hover ─────────────────────────────────────────────────────────────────

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let doc = self.documents.get(uri);
        Ok(features::hover::provide(doc.as_deref(), pos))
    }

    // ── Go-to definition ──────────────────────────────────────────────────────

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let doc = self.documents.get(uri);
        Ok(features::goto_definition::provide(doc.as_deref(), pos))
    }

    // ── Document highlight ────────────────────────────────────────────────────

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let doc = self.documents.get(uri);
        Ok(features::document_highlight::provide(doc.as_deref(), pos))
    }

    // ── References ────────────────────────────────────────────────────────────

    async fn references(
        &self,
        params: ReferenceParams,
    ) -> LspResult<Option<Vec<Location>>> {
        let uri  = &params.text_document_position.text_document.uri;
        let pos  = params.text_document_position.position;
        let doc  = self.documents.get(uri);
        let incl = params.context.include_declaration;
        Ok(features::references::provide(doc.as_deref(), uri, pos, incl))
    }

    // ── Rename ────────────────────────────────────────────────────────────────

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let pos = params.position;
        let doc = self.documents.get(uri);
        Ok(features::rename::prepare(doc.as_deref(), pos))
    }

    async fn rename(
        &self,
        params: RenameParams,
    ) -> LspResult<Option<WorkspaceEdit>> {
        let uri      = &params.text_document_position.text_document.uri;
        let pos      = params.text_document_position.position;
        let new_name = &params.new_name;
        let doc      = self.documents.get(uri);
        Ok(features::rename::provide(doc.as_deref(), uri, pos, new_name))
    }

    // ── Document symbols ──────────────────────────────────────────────────────

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri);
        Ok(features::document_symbols::provide(doc.as_deref()))
    }

    // ── Semantic tokens ───────────────────────────────────────────────────────

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri);
        Ok(features::semantic_tokens::provide(doc.as_deref()))
    }

    // ── Colors ────────────────────────────────────────────────────────────────

    async fn document_color(
        &self,
        params: DocumentColorParams,
    ) -> LspResult<Vec<ColorInformation>> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri);
        Ok(features::document_color::provide(doc.as_deref()))
    }

    async fn color_presentation(
        &self,
        params: ColorPresentationParams,
    ) -> LspResult<Vec<ColorPresentation>> {
        Ok(features::document_color::presentation(params.color, params.range))
    }

    // ── Inlay hints ───────────────────────────────────────────────────────────

    async fn inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> LspResult<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri);
        Ok(features::inlay_hints::provide(doc.as_deref()))
    }

    // ── Code actions ──────────────────────────────────────────────────────────

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> LspResult<Option<CodeActionResponse>> {
        let uri   = &params.text_document.uri;
        let diags = &params.context.diagnostics;
        let doc   = self.documents.get(uri);
        Ok(features::code_actions::provide(doc.as_deref(), diags))
    }

    // ── Folding ───────────────────────────────────────────────────────────────

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> LspResult<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri);
        Ok(features::folding::provide(doc.as_deref()))
    }
}
