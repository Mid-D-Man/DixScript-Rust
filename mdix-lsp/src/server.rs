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

const ANALYSIS_TIMEOUT_SECS: u64 = 15;

pub struct Backend {
    pub client:             Client,
    pub documents:          Arc<DashMap<Url, Document>>,
    pub pipeline_lock:      tokio::sync::Mutex<()>,
    pub shutdown_requested: AtomicBool,
    pub analysis_tasks:     StdMutex<Vec<tokio::task::JoinHandle<()>>>,
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
                        "Analysis timed out after {}s for {} — stale diagnostics preserved",
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

    // ── Folding ranges ────────────────────────────────────────────────────────
    // Folds: @SECTION(...) blocks, { } brace blocks, multi-line table/group entries
    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> LspResult<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;
        let doc = self.documents.get(uri);
        Ok(features::folding::provide(doc.as_deref()))
    }
}
