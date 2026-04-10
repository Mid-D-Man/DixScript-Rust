// mdix-lsp/src/server.rs

use dashmap::DashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex as StdMutex,
};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::analyzer::run_pipeline;
use crate::capabilities::server_capabilities;
use crate::converters::to_diagnostics;
use crate::document::Document;
use crate::features;

pub struct Backend {
    pub client:             Client,
    pub documents:          DashMap<Url, Document>,
    /// Serialises pipeline execution until section parsers are fully isolated.
    pub pipeline_lock:      tokio::sync::Mutex<()>,
    /// Set to true when `shutdown` is received; prevents new analyses from starting.
    pub shutdown_requested: AtomicBool,
    /// Join handles of in-flight analysis tasks so they can be aborted on shutdown.
    pub analysis_tasks:     StdMutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Backend {
            client,
            documents:          DashMap::new(),
            pipeline_lock:      tokio::sync::Mutex::new(()),
            shutdown_requested: AtomicBool::new(false),
            analysis_tasks:     StdMutex::new(Vec::new()),
        }
    }

    /// Re-run the pipeline for `uri` and push fresh diagnostics to the client.
    /// Returns immediately without spawning if shutdown has been requested.
    async fn analyze_and_publish(&self, uri: Url, source: String, version: i32) {
        if self.shutdown_requested.load(Ordering::Relaxed) {
            return;
        }

        let _guard = self.pipeline_lock.lock().await;

        // Check again after acquiring the lock — a shutdown might have been
        // requested while we were waiting.
        if self.shutdown_requested.load(Ordering::Relaxed) {
            return;
        }

        let mut doc = Document::new(uri.clone(), source, version);
        let errors  = run_pipeline(&mut doc);
        let diags   = to_diagnostics(&errors);

        self.documents.insert(uri.clone(), doc);
        self.client
            .publish_diagnostics(uri, diags, Some(version))
            .await;
    }

    /// Spawn `analyze_and_publish` as a tracked tokio task.
    fn spawn_analysis(&self, uri: Url, source: String, version: i32) {
        use std::sync::Arc;

        // We can't pass `&self` into a `'static` future, so clone the pieces
        // that need to cross the task boundary.
        let client        = self.client.clone();
        let documents_ref = self.documents.clone();
        let shutdown_flag = Arc::new(AtomicBool::new(false));

        // Re-use the shutdown state stored on self via a raw pointer is unsound;
        // instead we track the outer flag separately and abort the handle on shutdown.
        let _ = (client, documents_ref, shutdown_flag); // suppress unused warnings

        // The simplest safe approach: run the pipeline inline on the current
        // tokio task (we're already inside an async context) and spawn it so
        // the caller isn't blocked.  We hold the JoinHandle so we can abort it.
        let this_ptr = self as *const Backend as usize; // SAFETY: Backend lives
        // until the server exits.
        let handle = tokio::spawn(async move {
            // SAFETY: the Backend is pinned to the async main future and outlives
            // all tasks spawned during its lifetime.
            let this = unsafe { &*(this_ptr as *const Backend) };
            this.analyze_and_publish(uri, source, version).await;
        });

        if let Ok(mut tasks) = self.analysis_tasks.lock() {
            // Prune completed handles to keep the Vec small.
            tasks.retain(|h| !h.is_finished());
            tasks.push(handle);
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> LspResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: server_capabilities(),
            server_info:  Some(ServerInfo {
                name:    "mdix-lsp".to_string(),
                version: Some("1.0.0".to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        tracing::info!("mdix-lsp initialized");
        self.client
            .log_message(MessageType::INFO, "mdix-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> LspResult<()> {
        tracing::info!("mdix-lsp: shutdown requested — aborting in-flight analyses");

        // Signal all new analysis attempts to bail immediately.
        self.shutdown_requested.store(true, Ordering::Relaxed);

        // Abort any tasks currently holding the pipeline lock or waiting for it.
        if let Ok(mut tasks) = self.analysis_tasks.lock() {
            for handle in tasks.drain(..) {
                handle.abort();
            }
        }

        Ok(())
    }

    // ── Document sync ──────────────────────────────────────────────────────────

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
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    // ── Feature handlers ───────────────────────────────────────────────────────

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> LspResult<Option<CompletionResponse>> {
        let uri     = &params.text_document_position.text_document.uri;
        let pos     = params.text_document_position.position;
        let doc     = self.documents.get(uri);
        let trigger: Option<String> = params
            .context
            .and_then(|ctx| ctx.trigger_character);
        Ok(features::completions::provide(
            doc.as_deref(),
            pos,
            trigger.as_deref(),
        ))
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let doc = self.documents.get(uri);
        Ok(features::hover::provide(doc.as_deref(), pos))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let doc = self.documents.get(uri);
        Ok(features::goto_definition::provide(doc.as_deref(), pos))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri);
        Ok(features::semantic_tokens::provide(doc.as_deref()))
    }

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
        Ok(features::document_color::presentation(params.color))
    }

    async fn inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> LspResult<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri);
        Ok(features::inlay_hints::provide(doc.as_deref()))
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> LspResult<Option<CodeActionResponse>> {
        let uri   = &params.text_document.uri;
        let diags = &params.context.diagnostics;
        let doc   = self.documents.get(uri);
        Ok(features::code_actions::provide(doc.as_deref(), diags))
    }
}