
//! Backend — implements the tower-lsp LanguageServer trait.
//!
//! Owns a DashMap of open documents. A tokio Mutex serialises pipeline
//! execution so that section parsers sharing the global ErrorManager
//! singleton do not contaminate each other across concurrent analyses.
//! (Phase 2 work will make section parsers fully isolated; for now one
//! pipeline runs at a time.)

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::analyzer::run_pipeline;
use crate::capabilities::server_capabilities;
use crate::converters::to_diagnostics;
use crate::document::Document;
use crate::features;

pub struct Backend {
    pub client:        Client,
    pub documents:     DashMap<Url, Document>,
    /// Serialises pipeline execution until section parsers are fully isolated.
    pub pipeline_lock: tokio::sync::Mutex<()>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Backend {
            client,
            documents:     DashMap::new(),
            pipeline_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Re-run the pipeline for `uri` and push fresh diagnostics to the client.
    async fn analyze_and_publish(&self, uri: Url, source: String, version: i32) {
        let _guard = self.pipeline_lock.lock().await;

        let mut doc = Document::new(uri.clone(), source, version);
        let errors  = run_pipeline(&mut doc);
        let diags   = to_diagnostics(&errors);

        self.documents.insert(uri.clone(), doc);
        self.client
            .publish_diagnostics(uri, diags, Some(version))
            .await;
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
        Ok(())
    }

    // ── Document sync ──────────────────────────────────────────────────────────

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.analyze_and_publish(doc.uri, doc.text, doc.version).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // FULL sync — we always receive the complete document text.
        if let Some(change) = params.content_changes.into_iter().last() {
            self.analyze_and_publish(
                params.text_document.uri,
                change.text,
                params.text_document.version,
            )
            .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        // Clear diagnostics when the file is closed.
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

        // Extract the trigger character from the LSP context when the editor
        // supplies it.  This is more reliable than inferring it from the
        // source text because the cursor may be on a different token
        // (e.g. inside a QuickFunc body when the user typed '<' for a type
        // annotation elsewhere on the line).
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
