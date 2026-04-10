// mdix-lsp/src/server.rs

use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};  // LanguageServer must be in scope here

use crate::analyzer::run_pipeline;
use crate::capabilities::server_capabilities;
use crate::converters::to_diagnostics;
use crate::document::Document;
use crate::features;

// ─── Backend ──────────────────────────────────────────────────────────────────

pub struct Backend {
    pub client:             Client,
    pub documents:          DashMap<Url, Document>,
    /// Serialises the pipeline so two rapid saves don't interleave their
    /// DashMap writes.  Held only for the duration of run_pipeline + insert.
    pub pipeline_lock:      tokio::sync::Mutex<()>,
    /// Set to true on `shutdown`; prevents new analyses from starting.
    pub shutdown_requested: AtomicBool,
    /// Tracked task handles so we can abort them on shutdown.
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

    /// Re-runs the pipeline for one document and publishes fresh diagnostics.
    ///
    /// `Client` and `DashMap` are both cheaply cloneable (Arc-backed), so we
    /// clone them before spawning instead of passing a raw `self` pointer.
    fn spawn_analysis(&self, uri: Url, source: String, version: i32) {
        if self.shutdown_requested.load(Ordering::Relaxed) {
            return;
        }

        // Clone the Arc-backed handles — this is O(1) and safe to send across
        // thread boundaries.
        let client    = self.client.clone();
        let documents = self.documents.clone();

        let handle = tokio::spawn(async move {
            let mut doc    = Document::new(uri.clone(), source, version);
            let errors     = run_pipeline(&mut doc);
            let diags      = to_diagnostics(&errors);
            documents.insert(uri.clone(), doc);
            client.publish_diagnostics(uri, diags, Some(version)).await;
        });

        if let Ok(mut tasks) = self.analysis_tasks.lock() {
            tasks.retain(|h| !h.is_finished());
            tasks.push(handle);
        }
    }
}

// ─── LanguageServer impl ──────────────────────────────────────────────────────

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
        self.shutdown_requested.store(true, Ordering::SeqCst);
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
        let trigger = params
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
