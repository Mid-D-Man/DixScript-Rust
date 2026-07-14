use tower_lsp::lsp_types::*;
use crate::features::code_lens::ALL_COMMANDS;

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
pub const TT_METHOD:      u32 = 17;

pub const MOD_DECLARATION: u32 = 1 << 0;
pub const MOD_READONLY:    u32 = 1 << 1;
pub const MOD_DEPRECATED:  u32 = 1 << 2;
pub const MOD_STATIC:      u32 = 1 << 3;

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,     // 0
    SemanticTokenType::STRING,      // 1
    SemanticTokenType::NUMBER,      // 2
    SemanticTokenType::OPERATOR,    // 3
    SemanticTokenType::VARIABLE,    // 4
    SemanticTokenType::FUNCTION,    // 5
    SemanticTokenType::TYPE,        // 6
    SemanticTokenType::ENUM_MEMBER, // 7
    SemanticTokenType::COMMENT,     // 8
    SemanticTokenType::NAMESPACE,   // 9
    SemanticTokenType::PROPERTY,    // 10
    SemanticTokenType::PARAMETER,   // 11
    SemanticTokenType::MACRO,       // 12
    SemanticTokenType::DECORATOR,   // 13
    SemanticTokenType::STRUCT,      // 14 — reserved
    SemanticTokenType::REGEXP,      // 15
    SemanticTokenType::EVENT,       // 16
    SemanticTokenType::METHOD,      // 17
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::DEPRECATED,
    SemanticTokenModifier::STATIC,
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

        completion_provider: Some(CompletionOptions {
            resolve_provider:   Some(false),
            trigger_characters: Some(vec![
                "@".to_string(), ".".to_string(), "<".to_string(),
                "~".to_string(), "{".to_string(), "(".to_string(), "[".to_string(),
            ]),
            ..Default::default()
        }),

        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters:   Some(vec!["(".to_string(), ",".to_string()]),
            retrigger_characters: Some(vec![",".to_string()]),
            work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
        }),

        hover_provider:      Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),

        references_provider:         Some(OneOf::Left(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        document_symbol_provider:    Some(OneOf::Left(true)),

        // ── NEW: workspace-wide symbol search (Cmd+T) ─────────────────────────
        workspace_symbol_provider: Some(OneOf::Left(true)),

        // ── NEW: call hierarchy for QuickFuncs ────────────────────────────────
        call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),

        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
        })),

        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),

        document_formatting_provider: Some(OneOf::Left(true)),

        document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
            first_trigger_character: "\n".to_string(),
            more_trigger_character:  None,
        }),

        code_lens_provider: Some(CodeLensOptions {
            resolve_provider: Some(false),
        }),

        execute_command_provider: Some(ExecuteCommandOptions {
            commands: ALL_COMMANDS.iter().map(|s| s.to_string()).collect(),
            work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
        }),

        semantic_tokens_provider: Some(
            SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                legend: semantic_token_legend(),
                full:   Some(SemanticTokensFullOptions::Bool(true)),
                work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
                ..Default::default()
            }),
        ),

        inlay_hint_provider:    Some(OneOf::Left(true)),
        color_provider:         Some(ColorProviderCapability::Simple(true)),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),

        ..Default::default()
    }
                                       }
