use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;

use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use dixscript::Compiler::AST::DixScript;
use dixscript::Compiler::AST::data_types::DebugMode;
use dixscript::Compiler::Core::SemanticAnalysisResult;
use dixscript::Compiler::Core::ValueResolution::ValueResolver;
use dixscript::Runtime::DixLoader;

use crate::analyzer::run_pipeline;
use crate::capabilities::server_capabilities;
use crate::converters::to_diagnostics;
use crate::document::Document;
use crate::extensions::Extensions;
use crate::features;
use crate::features::code_lens::{
    CMD_COMPILE, CMD_CREATE_RESOLVED, CMD_GET_SETTINGS_VALUES, CMD_GET_THEME_COLORS,
    CMD_MINIFY, CMD_SHOW_AST, CMD_TO_JSON, CMD_TO_TOML,
};
use crate::features::commands::{
    run_compile, run_convert_to_json, run_convert_to_toml, run_create_resolved,
    run_get_settings_values, run_get_theme_colors, run_minify, run_show_ast,run_test_regex, CommandResult,
};

const ANALYSIS_TIMEOUT_SECS: u64 = 10;

pub struct Backend {
    pub client:             Client,
    pub documents:          Arc<DashMap<Url, Document>>,
    pub shutdown_requested: AtomicBool,
    pub analysis_tasks:     StdMutex<Vec<tokio::task::JoinHandle<()>>>,
    pub pending_versions:   Arc<DashMap<Url, i32>>,
    pub extensions:         Extensions,
}

impl Backend {
    pub fn new(client: Client, extensions: Extensions) -> Self {
        Backend {
            client,
            documents:          Arc::new(DashMap::new()),
            shutdown_requested: AtomicBool::new(false),
            analysis_tasks:     StdMutex::new(Vec::new()),
            pending_versions:   Arc::new(DashMap::new()),
            extensions,
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

    /// Waits, with a hard budget, for `documents[uri]` to catch up to at
    /// least `want_version` before a request reads it.
    ///
    /// `spawn_analysis` re-tokenizes/re-parses/re-analyzes in a background
    /// task and only swaps the new `Document` into `documents` once that
    /// finishes — there is no synchronous update in `did_change` itself.
    /// Position-based requests (completion, hover, signature help, ...)
    /// previously read `documents.get(uri)` the instant they arrived, with
    /// no guarantee the in-flight analysis for the very edit that triggered
    /// the request had landed yet. On a fast edit burst — e.g. backspace
    /// right after a `.` and immediately retyping — the request commonly
    /// wins that race and gets served a `Document` that is one or more
    /// edits stale. `Document` bundles source/tokens/AST together and they
    /// only get swapped in as one atomic unit, so it isn't just inferred
    /// types that lag during that window, it's the receiver text itself —
    /// which reliably produces an empty completion list right when the
    /// popup is expected.
    ///
    /// This never blocks indefinitely: if analysis is slow, erroring, or
    /// stuck, the budget still expires and the caller proceeds with
    /// whatever is cached, same as before this existed — it can only help,
    /// never hang a request.
    async fn wait_for_fresh_document(&self, uri: &Url, want_version: i32) {
        const BUDGET: std::time::Duration = std::time::Duration::from_millis(400);
        const POLL:   std::time::Duration = std::time::Duration::from_millis(15);
        let deadline = tokio::time::Instant::now() + BUDGET;

        loop {
            let fresh = self.documents.get(uri)
                .map(|d| d.version >= want_version)
                .unwrap_or(false);
            if fresh { return; }
            if tokio::time::Instant::now() >= deadline { return; }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Convenience wrapper: waits for `uri` to reach whatever version was
    /// last reported via `did_change`, if any is on record.
    async fn wait_for_latest(&self, uri: &Url) {
        if let Some(want) = self.pending_versions.get(uri).map(|v| *v) {
            self.wait_for_fresh_document(uri, want).await;
        }
    }

    async fn show_message(&self, success: bool, msg: &str) {
        let kind = if success { MessageType::INFO } else { MessageType::ERROR };
        self.client.show_message(kind, msg).await;
    }
}

// ── Value-resolution helper used by conversion commands ──────────────────────

fn resolve_ast_owned(
    ast:             Option<DixScript>,
    semantic_result: Option<SemanticAnalysisResult>,
) -> Option<DixScript> {
    let ast = ast?;

    let has_local_fns = ast.quick_functions.as_ref()
        .map(|q| !q.functions.is_empty()).unwrap_or(false);
    let has_imported_fns = semantic_result.as_ref()
        .and_then(|sr| sr.symbol_table.as_ref())
        .map(|st| st.namespaces.values().any(|ns| !ns.functions.is_empty()))
        .unwrap_or(false);
    // These two were missing entirely until now. Without them, a file with
    // ONLY an imported enum reference and no @QUICKFUNCS anywhere (nothing
    // else to trip has_local_fns/has_imported_fns) hit the early return
    // below and got handed back completely unresolved: ValueResolver's
    // Phase 1 enum merge never ran, so `ast.enums` never picked up the
    // synthesized "Namespace.EnumName" declaration for the imported enum,
    // and every DixConverter call downstream (convert-to-json/toml, minify)
    // fell through `ast_value_to_dix_value`'s `.unwrap_or(0)` for every
    // single enum reference. Adding a QuickFunc call anywhere "fixed" it
    // only as a side effect of flipping has_local_fns true, which happened
    // to defeat the early return for an unrelated reason. This mirrors the
    // exact fix `loader.rs`'s Stage 7 gate already has for
    // `compile_to_resolved_ast` -- that copy was fixed; this LSP-local
    // duplicate of the same gate was not.
    let has_local_enums = ast.enums.as_ref()
        .map(|e| !e.enums.is_empty()).unwrap_or(false);
    let has_imported_enums = semantic_result.as_ref()
        .and_then(|sr| sr.symbol_table.as_ref())
        .map(|st| st.namespaces.values().any(|ns| !ns.enums.is_empty()))
        .unwrap_or(false);

    if (!has_local_fns && !has_imported_fns && !has_local_enums && !has_imported_enums)
        || ast.data.is_none()
    {
        return Some(ast);
    }

    let st = match semantic_result.as_ref().and_then(|sr| sr.symbol_table.as_ref()) {
        Some(st) => st,
        None     => return Some(ast),
    };

    let ast_clone = ast.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let resolver = ValueResolver::new(ast_clone, st, DebugMode::Off);
        resolver.resolve()
    }));

    match result {
        Ok(resolution) if resolution.is_success => {
            tracing::debug!(
                "Value resolution: {} call(s) resolved",
                resolution.function_calls_resolved
            );
            resolution.resolved_ast.or(Some(ast))
        }
        Ok(resolution) => {
            tracing::warn!(
                "Value resolution failed ({} error(s)), using enhanced AST",
                resolution.errors.len()
            );
            Some(ast)
        }
        Err(payload) => {
            let msg = payload.downcast_ref::<String>().cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            tracing::error!("Value resolution panicked: {}", msg);
            Some(ast)
        }
    }
}

// ── LanguageServer implementation ─────────────────────────────────────────────

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> LspResult<InitializeResult> {
        tracing::info!("mdix-lsp initialize");
        Ok(InitializeResult {
            capabilities: server_capabilities(),
            server_info: Some(ServerInfo {
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

    // ── Text document features ────────────────────────────────────────────────

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> LspResult<Option<CompletionResponse>> {
        let uri     = &params.text_document_position.text_document.uri;
        let pos     = params.text_document_position.position;
        let trigger = params.context.and_then(|c| c.trigger_character);
        self.wait_for_latest(uri).await;

        let doc_guard = self.documents.get(uri);
        let doc       = doc_guard.as_deref();

        let mut items: Vec<CompletionItem> =
            match features::completions::provide(doc, pos, trigger.as_deref()) {
                Some(CompletionResponse::Array(items)) => items,
                Some(CompletionResponse::List(list))   => list.items,
                None                                    => Vec::new(),
            };

        if let Some(doc) = doc {
            for ext in &self.extensions.completions {
                items.extend(ext.extra_completions(doc, pos, trigger.as_deref()));
            }
        }

        Ok(if items.is_empty() { None } else { Some(CompletionResponse::Array(items)) })
    }

    async fn signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> LspResult<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        self.wait_for_latest(uri).await;
        Ok(features::signature_help::provide(
            self.documents.get(uri).as_deref(), pos, params.context,
        ))
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        self.wait_for_latest(uri).await;

        let doc_guard = self.documents.get(uri);
        let doc       = doc_guard.as_deref();

        if let Some(hover) = features::hover::provide(doc, pos) {
            return Ok(Some(hover));
        }

        if let Some(doc) = doc {
            for ext in &self.extensions.hover {
                if let Some(hover) = ext.extra_hover(doc, pos) {
                    return Ok(Some(hover));
                }
            }
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(features::goto_definition::provide(self.documents.get(uri).as_deref(), pos))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(features::document_highlights::provide(self.documents.get(uri).as_deref(), pos))
    }

    async fn references(
        &self,
        params: ReferenceParams,
    ) -> LspResult<Option<Vec<Location>>> {
        let uri  = &params.text_document_position.text_document.uri;
        let pos  = params.text_document_position.position;
        let incl = params.context.include_declaration;
        Ok(features::references::provide(
            self.documents.get(uri).as_deref(), uri, pos, incl,
        ))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
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

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        Ok(features::document_symbols::provide(self.documents.get(uri).as_deref()))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> LspResult<Option<Vec<SymbolInformation>>> {
        Ok(features::workspace_symbols::provide(&self.documents, &params.query))
    }

    // ── Call hierarchy ────────────────────────────────────────────────────────
    //
    // tower-lsp trait names:
    //   prepare_call_hierarchy  → textDocument/prepareCallHierarchy
    //   incoming_calls          → callHierarchy/incomingCalls
    //   outgoing_calls          → callHierarchy/outgoingCalls

    async fn prepare_call_hierarchy(          // ← was call_hierarchy_prepare
                                              &self,
                                              params: CallHierarchyPrepareParams,
    ) -> LspResult<Option<Vec<CallHierarchyItem>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        Ok(features::call_hierarchy::prepare(
            self.documents.get(uri).as_deref(), pos,
        ))
    }

    async fn incoming_calls(                  // ← was call_hierarchy_incoming_calls
                                              &self,
                                              params: CallHierarchyIncomingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyIncomingCall>>> {
        let uri = &params.item.uri;
        Ok(features::call_hierarchy::incoming_calls(
            self.documents.get(uri).as_deref(), &params,
        ))
    }

    async fn outgoing_calls(                  // ← was call_hierarchy_outgoing_calls
                                              &self,
                                              params: CallHierarchyOutgoingCallsParams,
    ) -> LspResult<Option<Vec<CallHierarchyOutgoingCall>>> {
        let uri = &params.item.uri;
        Ok(features::call_hierarchy::outgoing_calls(
            self.documents.get(uri).as_deref(), &params,
        ))
    }

    // ── Token / semantic features ─────────────────────────────────────────────

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let uri = &params.text_document.uri;
        Ok(features::semantic_tokens::provide(self.documents.get(uri).as_deref()))
    }

    async fn document_color(
        &self,
        params: DocumentColorParams,
    ) -> LspResult<Vec<ColorInformation>> {
        let uri = &params.text_document.uri;
        Ok(features::document_color::provide(self.documents.get(uri).as_deref()))
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
        Ok(features::inlay_hints::provide(self.documents.get(uri).as_deref()))
    }

    // ── Code actions / lens ───────────────────────────────────────────────────

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> LspResult<Option<CodeActionResponse>> {
        let uri   = &params.text_document.uri;
        let range = params.range;
        let diags = &params.context.diagnostics;
        Ok(features::code_actions::provide(self.documents.get(uri).as_deref(), range, diags))
    }

    async fn code_lens(
        &self,
        params: CodeLensParams,
    ) -> LspResult<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;
        // Was reading `self.documents.get(uri)` immediately, racing the
        // background `spawn_analysis` task the same way completion/hover/
        // signature_help used to (see `wait_for_fresh_document`'s doc
        // comment) — on a fresh edit, that's a real chance of getting served
        // a `Document` whose `tokens` predate the edit, silently dropping
        // whatever lens depended on the new token (e.g. a just-typed date/
        // timestamp literal's 📅 Edit lens). Same fix as those three.
        self.wait_for_latest(uri).await;
        Ok(features::code_lens::provide(self.documents.get(uri).as_deref()))
    }

    // ── Folding / formatting ──────────────────────────────────────────────────

    async fn folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> LspResult<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;
        Ok(features::folding::provide(self.documents.get(uri).as_deref()))
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        let uri  = &params.text_document.uri;
        let opts = &params.options;
        Ok(features::formatting::provide(self.documents.get(uri).as_deref(), opts))
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        if params.ch != "\n" { return Ok(None); }

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

        let indent = prev_line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect::<String>();
        let inner_indent = if params.options.insert_spaces {
            " ".repeat(params.options.tab_size as usize)
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
                range:    Range::new(
                    Position::new(pos.line, 0),
                    Position::new(pos.line, cur_line_text.len() as u32),
                ),
                new_text: format!("{}{}{}", indent, inner_indent, cur_line_text.trim_start()),
            });
        } else {
            edits.push(TextEdit {
                range:    Range::new(
                    Position::new(pos.line, 0),
                    Position::new(pos.line, cur_line_text.len() as u32),
                ),
                new_text: format!("{}{}\n{}{}", indent, inner_indent, indent, close),
            });
        }
        let _ = open;
        if edits.is_empty() { Ok(None) } else { Ok(Some(edits)) }
    }

    // ── Execute command ───────────────────────────────────────────────────────

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> LspResult<Option<serde_json::Value>> {
        let command = params.command.as_str();

        let uri_str = params.arguments.first()
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let uri = uri_str.as_deref().and_then(|s| Url::parse(s).ok());
        let source_path: Option<std::path::PathBuf> =
            uri.as_ref().and_then(|u| u.to_file_path().ok());

        match command {

            CMD_TO_JSON => {
                let (ast_opt, semantic_opt) = {
                    let doc = uri.as_ref().and_then(|u| self.documents.get(u));
                    (
                        doc.as_ref().and_then(|d| d.ast.clone()),
                        doc.as_ref().and_then(|d| d.semantic_result.clone()),
                    )
                };
                let path_clone = source_path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    match resolve_ast_owned(ast_opt, semantic_opt) {
                        Some(ast) => run_convert_to_json(&ast, path_clone.as_deref()),
                        None      => CommandResult::err("File has not been parsed yet."),
                    }
                })
                    .await
                    .unwrap_or_else(|_| CommandResult::err("JSON conversion task panicked."));
                self.show_message(result.success, &result.message).await;
            }

            CMD_TO_TOML => {
                let (ast_opt, semantic_opt) = {
                    let doc = uri.as_ref().and_then(|u| self.documents.get(u));
                    (
                        doc.as_ref().and_then(|d| d.ast.clone()),
                        doc.as_ref().and_then(|d| d.semantic_result.clone()),
                    )
                };
                let path_clone = source_path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    match resolve_ast_owned(ast_opt, semantic_opt) {
                        Some(ast) => run_convert_to_toml(&ast, path_clone.as_deref()),
                        None      => CommandResult::err("File has not been parsed yet."),
                    }
                })
                    .await
                    .unwrap_or_else(|_| CommandResult::err("TOML conversion task panicked."));
                self.show_message(result.success, &result.message).await;
            }

            CMD_MINIFY => {
                let (ast_opt, semantic_opt) = {
                    let doc = uri.as_ref().and_then(|u| self.documents.get(u));
                    (
                        doc.as_ref().and_then(|d| d.ast.clone()),
                        doc.as_ref().and_then(|d| d.semantic_result.clone()),
                    )
                };
                let path_clone = source_path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    match resolve_ast_owned(ast_opt, semantic_opt) {
                        Some(ast) => run_minify(&ast, path_clone.as_deref()),
                        None      => CommandResult::err("File has not been parsed yet."),
                    }
                })
                    .await
                    .unwrap_or_else(|_| CommandResult::err("Minify task panicked."));
                self.show_message(result.success, &result.message).await;
            }

            CMD_CREATE_RESOLVED => {
                let path_clone = source_path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        match path_clone {
                            Some(ref path) if path.exists() => {
                                let loader = DixLoader::new();
                                match loader.compile_to_resolved_ast(path.to_str().unwrap_or("")) {
                                    Ok(resolved_ast) => run_create_resolved(&resolved_ast, Some(path)),
                                    Err(e) => CommandResult::err(format!("⊞ Resolution failed: {}", e)),
                                }
                            }
                            Some(ref path) => {
                                CommandResult::err(format!("⊞ File not found: {}", path.display()))
                            }
                            None => CommandResult::err("⊞ Save the file before resolving."),
                        }
                    }))
                        .unwrap_or_else(|payload| {
                            let msg = payload.downcast_ref::<String>().cloned()
                                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                                .unwrap_or_else(|| "unknown panic in resolver".to_string());
                            tracing::error!("CMD_CREATE_RESOLVED inner panic: {}", msg);
                            CommandResult::err(format!("⊞ Resolve panicked: {}", msg))
                        })
                })
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!("CMD_CREATE_RESOLVED spawn_blocking failed: {:?}", e);
                        CommandResult::err("⊞ Resolve task panicked unexpectedly.")
                    });
                self.show_message(result.success, &result.message).await;
            }

            CMD_COMPILE => {
                let ast_clone = {
                    let doc = uri.as_ref().and_then(|u| self.documents.get(u));
                    doc.as_ref().and_then(|d| d.ast.clone())
                };
                let path_clone = source_path.clone();
                let result = tokio::task::spawn_blocking(move || {
                    run_compile(path_clone.as_deref(), ast_clone.as_ref())
                })
                    .await
                    .unwrap_or_else(|_| CommandResult::err("Compile task panicked."));
                self.show_message(result.success, &result.message).await;
            }

            CMD_SHOW_AST => {
                let ast_clone = {
                    let doc = uri.as_ref().and_then(|u| self.documents.get(u));
                    doc.as_ref().and_then(|d| d.ast.clone())
                };
                let result = match ast_clone {
                    Some(ast) => run_show_ast(&ast),
                    None      => CommandResult::err("AST not available — wait for analysis."),
                };
                self.show_message(result.success, &result.message).await;
            }

            // Returns its JSON payload directly instead of falling through to
            // the unconditional `Ok(None)` below — see the doc comment on
            // `run_get_theme_colors` in commands.rs for why this one command
            // doesn't go through CommandResult/show_message like the rest.
            CMD_GET_THEME_COLORS => {
                let ast_clone = {
                    let doc = uri.as_ref().and_then(|u| self.documents.get(u));
                    doc.as_ref().and_then(|d| d.ast.clone())
                };
                let payload = tokio::task::spawn_blocking(move || {
                    match ast_clone {
                        Some(ast) => run_get_theme_colors(&ast),
                        None => serde_json::json!({
                            "success": false,
                            "message": "AST not available — wait for analysis.",
                            "dark": null, "light": null, "warnings": []
                        }),
                    }
                })
                    .await
                    .unwrap_or_else(|_| serde_json::json!({
                        "success": false,
                        "message": "Theme colors task panicked.",
                        "dark": null, "light": null, "warnings": []
                    }));
                return Ok(Some(payload));
            }

            // Same shape as CMD_GET_THEME_COLORS above.
            CMD_GET_SETTINGS_VALUES => {
                let ast_clone = {
                    let doc = uri.as_ref().and_then(|u| self.documents.get(u));
                    doc.as_ref().and_then(|d| d.ast.clone())
                };
                let payload = tokio::task::spawn_blocking(move || {
                    match ast_clone {
                        Some(ast) => run_get_settings_values(&ast),
                        None => serde_json::json!({
                            "success": false,
                            "message": "AST not available — wait for analysis.",
                            "settings": [], "warnings": []
                        }),
                    }
                })
                    .await
                    .unwrap_or_else(|_| serde_json::json!({
                        "success": false,
                        "message": "Settings values task panicked.",
                        "settings": [], "warnings": []
                    }));
                return Ok(Some(payload));
            }
CMD_TEST_REGEX => {
        let pattern = params.arguments.get(0).and_then(|v| v.as_str()).unwrap_or("");
        let test_text = params.arguments.get(1).and_then(|v| v.as_str()).unwrap_or("");
 
        let result = run_test_regex(pattern, test_text);
        return Ok(Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)));
                                                          }
            other => {
                tracing::warn!("Unknown command: {}", other);
                self.client.show_message(
                    MessageType::WARNING,
                    &format!("Unknown command: {}", other),
                ).await;
            }
        }

        Ok(None)
    }
            }
