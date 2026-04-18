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

/// How long a single pipeline run may take before it is abandoned.
/// Prevents a hung analysis from blocking the shutdown sequence.
const ANALYSIS_TIMEOUT_SECS: u64 = 15;

pub struct Backend {
    pub client:             Client,
    pub documents:          Arc<DashMap<Url, Document>>,
    /// Never actually locked as a gate — retained for future serialization use.
    pub pipeline_lock:      tokio::sync::Mutex<()>,
    pub shutdown_requested: AtomicBool,
    pub analysis_tasks:     StdMutex<Vec<tokio::task::JoinHandle<()>>>,
    /// Tracks the latest analysis version scheduled per URI.
    /// Results from superseded analyses (stale after rapid edits) are discarded.
    pub pending_versions:   Arc<DashMap<Url, i32>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Backend {
            client,
            documents:          Arc::new(DashMap::new()),
            pipeline_lock:      tokio::sync::Mutex::new(()),
            shutdown_requested: AtomicBool::new(false),
            analysis_tasks:     StdMutex::new(Vec::new()),
            pending_versions:   Arc::new(DashMap::new()),
        }
    }

    fn spawn_analysis(&self, uri: Url, source: String, version: i32) {
        if self.shutdown_requested.load(Ordering::Relaxed) {
            return;
        }

        // Mark this as the latest version for this URI.
        self.pending_versions.insert(uri.clone(), version);

        let client    = self.client.clone();
        let documents = Arc::clone(&self.documents);
        let versions  = Arc::clone(&self.pending_versions);

        let handle = tokio::spawn(async move {
            // spawn_blocking moves CPU-bound work off the tokio executor so
            // hover / completion / semantic-token requests are never blocked
            // while analysis is in progress.
            //
            // The outer timeout ensures a hung spawn_blocking call (e.g. a
            // deadlock inside VersionManager after a poisoned lock) does not
            // keep the server alive past the LSP client's shutdown deadline.
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
                        "Analysis timed out after {}s for {} — stale diagnostics preserved",
                        ANALYSIS_TIMEOUT_SECS, uri
                    );
                    // Do NOT publish empty diagnostics — leave the last good
                    // set in place so the editor still shows something useful.
                }
                Ok(Err(panic_err)) => {
                    // The catch_unwind in run_pipeline should have caught this,
                    // but if spawn_blocking itself panicked we handle it here.
                    tracing::error!("Analysis task panicked: {:?}", panic_err);
                }
                Ok(Ok((uri, doc, errors, ver))) => {
                    // Discard results if a newer analysis was scheduled while
                    // this one ran (e.g. user typed faster than analysis speed).
                    let latest = versions.get(&uri).map(|v| *v).unwrap_or(ver);
                    if ver >= latest {
                        let diags = to_diagnostics(&errors);
                        documents.insert(uri.clone(), doc);
                        client.publish_diagnostics(uri, diags, Some(ver)).await;
                    } else {
                        tracing::debug!(
                            "Discarding stale analysis v{} for {} (latest is v{})",
                            ver, uri, latest
                        );
                    }
                }
            }
        });

        if let Ok(mut tasks) = self.analysis_tasks.lock() {
            // Prune finished handles to avoid unbounded growth.
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
        tracing::info!("mdix-lsp: shutdown — aborting in-flight analyses");
        self.shutdown_requested.store(true, Ordering::SeqCst);
        // Abort the outer async wrappers.  The spawn_blocking threads inside
        // are not directly abortable, but the ANALYSIS_TIMEOUT_SECS guard
        // ensures they self-terminate before the LSP client's deadline.
        if let Ok(mut tasks) = self.analysis_tasks.lock() {
            for handle in tasks.drain(..) {
                handle.abort();
            }
        }
        Ok(())
    }

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
        Ok(features::document_color::presentation(params.color, params.range))
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