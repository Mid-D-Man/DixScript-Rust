// mdix-lsp/src/capabilities.rs
use tower_lsp::lsp_types::*;

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
// index 14 = STRUCT reserved for legend stability
pub const TT_REGEXP:      u32 = 15;
pub const TT_EVENT:       u32 = 16;

pub const MOD_DECLARATION: u32 = 1 << 0;
pub const MOD_READONLY:    u32 = 1 << 1;

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
    SemanticTokenType::STRUCT,   // index 14 — legend stability
    SemanticTokenType::REGEXP,
    SemanticTokenType::EVENT,
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::DEPRECATED,
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
                change:     Some(TextDocumentSyncKind::FULL),
                save:       Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(true),
                })),
                ..Default::default()
            },
        )),

        // ── Language intelligence ─────────────────────────────────────────────
        completion_provider: Some(CompletionOptions {
            resolve_provider:   Some(false),
            trigger_characters: Some(vec![
                "@".to_string(), ".".to_string(),
                "<".to_string(), "~".to_string(),
            ]),
            ..Default::default()
        }),

        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters:   Some(vec!["(".to_string(), ",".to_string()]),
            retrigger_characters: Some(vec![",".to_string()]),
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: None,
            },
        }),

        hover_provider:      Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),

        // ── Symbol navigation ─────────────────────────────────────────────────
        references_provider: Some(OneOf::Left(true)),

        document_highlight_provider: Some(OneOf::Left(true)),

        document_symbol_provider: Some(OneOf::Left(true)),

        // ── Editing assistance ────────────────────────────────────────────────
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: None,
            },
        })),

        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),

        // ── Visual enrichment ─────────────────────────────────────────────────
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

        inlay_hint_provider:    Some(OneOf::Left(true)),
        color_provider:         Some(ColorProviderCapability::Simple(true)),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),

        ..Default::default()
    }
}
