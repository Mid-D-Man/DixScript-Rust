//! Declares which LSP capabilities this server supports.
//! One place to enable or disable features across the server.

use tower_lsp::lsp_types::{
    CodeActionProviderCapability, ColorProviderCapability, CompletionOptions,
    HoverProviderCapability, OneOf, SaveOptions, SemanticTokenModifier, SemanticTokenType,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions,
    WorkDoneProgressOptions,
};

/// Semantic token types exposed to the client.
/// The index of each entry is used when encoding token data in semantic_tokens.rs.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,       // 0 — section names, control flow
    SemanticTokenType::STRING,        // 1
    SemanticTokenType::NUMBER,        // 2
    SemanticTokenType::OPERATOR,      // 3 — =, ->, ::, ~
    SemanticTokenType::VARIABLE,      // 4 — DATA identifiers
    SemanticTokenType::FUNCTION,      // 5 — QuickFunc names
    SemanticTokenType::TYPE,          // 6 — type annotations <int> etc.
    SemanticTokenType::ENUM_MEMBER,   // 7 — EnumName.VALUE
    SemanticTokenType::COMMENT,       // 8
    SemanticTokenType::NAMESPACE,     // 9 — imported alias names
    SemanticTokenType::PROPERTY,      // 10 — table path segments
    SemanticTokenType::PARAMETER,     // 11 — QuickFunc parameters
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION, // 0
    SemanticTokenModifier::READONLY,    // 1
    SemanticTokenModifier::DEPRECATED,  // 2
];

pub fn semantic_token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types:     TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                // FULL sync: always send the complete document text on every change.
                // Incremental would need a diff engine — not worth the complexity here.
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(true),
                })),
                ..Default::default()
            },
        )),

        completion_provider: Some(CompletionOptions {
            resolve_provider:   Some(false),
            trigger_characters: Some(vec![
                "@".to_string(),  // section completions
                ".".to_string(),  // enum access, method calls, table paths
                "<".to_string(),  // type annotation completions
                "~".to_string(),  // function prefix
            ]),
            ..Default::default()
        }),

        hover_provider:      Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),

        // HexColor tokens show an inline color swatch in supporting editors.
        color_provider: Some(ColorProviderCapability::Simple(true)),

        // Inlay hints show inferred types on untyped DATA variables.
        inlay_hint_provider: Some(OneOf::Left(true)),

        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),

        semantic_tokens_provider: Some(
            SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                legend: semantic_token_legend(),
                full:   Some(SemanticTokensFullOptions::Bool(true)),
                ..Default::default()
            }),
        ),

        ..Default::default()
    }
  }
