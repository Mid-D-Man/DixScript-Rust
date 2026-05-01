// mdix-lsp/src/capabilities.rs
//! Declares which LSP capabilities this server supports.

use tower_lsp::lsp_types::{
    CodeActionProviderCapability, ColorProviderCapability, CompletionOptions,
    FoldingRangeProviderCapability, HoverProviderCapability, OneOf, SaveOptions,
    SemanticTokenModifier, SemanticTokenType, SemanticTokensFullOptions,
    SemanticTokensLegend, SemanticTokensOptions, SemanticTokensServerCapabilities,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, WorkDoneProgressOptions,
};

// ── Token type index constants ────────────────────────────────────────────────

pub const TT_KEYWORD:     u32 = 0;
pub const TT_STRING:      u32 = 1;
pub const TT_NUMBER:      u32 = 2;
pub const TT_OPERATOR:    u32 = 3;
pub const TT_VARIABLE:    u32 = 4;
pub const TT_FUNCTION:    u32 = 5;
pub const TT_TYPE:        u32 = 6;
pub const TT_ENUM_MEMBER: u32 = 7;
pub const TT_COMMENT:     u32 = 8;
pub const TT_NAMESPACE:   u32 = 9;
pub const TT_PROPERTY:    u32 = 10;
pub const TT_PARAMETER:   u32 = 11;
pub const TT_MACRO:       u32 = 12;
pub const TT_DECORATOR:   u32 = 13;
// Index 14 reserved for STRUCT — kept in TOKEN_TYPES for legend index stability.
// Constant intentionally omitted (no current use site).
pub const TT_REGEXP:      u32 = 15;
pub const TT_EVENT:       u32 = 16;

// ── Modifier bitmasks ─────────────────────────────────────────────────────────

pub const MOD_DECLARATION: u32 = 1 << 0;
pub const MOD_READONLY:    u32 = 1 << 1;
// Bit 2 (DEPRECATED) reserved in legend but has no use site — constant omitted.

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::TYPE,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::COMMENT,
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::MACRO,
    SemanticTokenType::DECORATOR,
    SemanticTokenType::STRUCT,   // index 14 — kept for legend stability
    SemanticTokenType::REGEXP,   // index 15 = TT_REGEXP
    SemanticTokenType::EVENT,    // index 16 = TT_EVENT
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION, // bit 0 = MOD_DECLARATION
    SemanticTokenModifier::READONLY,    // bit 1 = MOD_READONLY
    SemanticTokenModifier::DEPRECATED,  // bit 2 — reserved in legend, no current use
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
                "@".to_string(),
                ".".to_string(),
                "<".to_string(),
                "~".to_string(),
            ]),
            ..Default::default()
        }),

        hover_provider:       Some(HoverProviderCapability::Simple(true)),
        definition_provider:  Some(OneOf::Left(true)),
        color_provider:       Some(ColorProviderCapability::Simple(true)),
        inlay_hint_provider:  Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),

        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),

        semantic_tokens_provider: Some(
            SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                legend: semantic_token_legend(),
                full:   Some(SemanticTokensFullOptions::Bool(true)),
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
                ..Default::default()
            }),
        ),

        ..Default::default()
    }
}
