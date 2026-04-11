// mdix-lsp/src/capabilities.rs

//! Declares which LSP capabilities this server supports.
//!
//! ## Semantic token legend
//! The index of each entry in TOKEN_TYPES is the u32 that the classifier in
//! semantic_tokens.rs emits.  The NAMES here are what editors use to apply
//! colors.  Standard names (keyword, string, number, …) are picked up
//! automatically by most editors with zero client config.  Custom names
//! (macro, decorator) need a client-side mapping but give us dedicated color
//! slots that the standard names don't.
//!
//! Legend → Ferrous dark color mapping (for reference):
//!   keyword       → #569CD6   (blue)
//!   string        → #CE9178   (orange-brown)
//!   number        → #B5CEA8   (light green)
//!   operator      → #D4D4D4   (white)
//!   variable      → #9CDCFE   (light blue — identifiers)
//!   function      → #DCDCAA   (yellow)
//!   type          → #4EC9B0   (teal — <type> annotations)
//!   enumMember    → #4FC1FF   (bright light-blue — enum fields/access)
//!   comment       → #6A9955   (green)
//!   namespace     → #9CDCFE   (light blue — import aliases)
//!   property      → #C586C0   (purple — table paths, group arrays, DLM)
//!   parameter     → #9CDCFE   (light blue — QuickFunc params)
//!   macro         → #C586C0   (purple — DLM module names: DCompressor etc.)
//!   decorator     → #CE9178   (orange — DLM subtypes: .gzip, .aes256)
//!   struct        → #4EC9B0   (teal — enum type NAMES in @ENUMS declaration)
//!   regexp        → #D16969   (red — r:() regex constructors)
//!   event         → #B5CEA8   (green — date / timestamp literals)

use tower_lsp::lsp_types::{
    CodeActionProviderCapability, ColorProviderCapability, CompletionOptions,
    HoverProviderCapability, OneOf, SaveOptions, SemanticTokenModifier, SemanticTokenType,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions,
    WorkDoneProgressOptions,
};

// ── Token type index constants ────────────────────────────────────────────────
// These must stay in sync with TOKEN_TYPES below — index N here = index N there.

pub const TT_KEYWORD:     u32 = 0;   // @section keywords, control flow
pub const TT_STRING:      u32 = 1;   // string literals (double / single / interpolated)
pub const TT_NUMBER:      u32 = 2;   // integers, floats, doubles, hex literals
pub const TT_OPERATOR:    u32 = 3;   // =, ->, ::, ~, arithmetic, comparison
pub const TT_VARIABLE:    u32 = 4;   // plain identifiers
pub const TT_FUNCTION:    u32 = 5;   // QuickFunc names (call sites and declarations)
pub const TT_TYPE:        u32 = 6;   // <int>, <string>, … type annotations
pub const TT_ENUM_MEMBER: u32 = 7;   // enum field values (PASSIVE, AGGRESSIVE, …)
pub const TT_COMMENT:     u32 = 8;   // // line comments
pub const TT_NAMESPACE:   u32 = 9;   // import alias names (Base, Utils, …)
pub const TT_PROPERTY:    u32 = 10;  // table path segments (server.host, tags::)
pub const TT_PARAMETER:   u32 = 11;  // QuickFunc parameter names
pub const TT_MACRO:       u32 = 12;  // DLM module names: DCompressor, DEncryptor, DAuditor
pub const TT_DECORATOR:   u32 = 13;  // DLM subtypes: .gzip, .aes256, .chacha20
pub const TT_STRUCT:      u32 = 14;  // enum TYPE names in @ENUMS declaration block
pub const TT_REGEXP:      u32 = 15;  // r:() regex constructor token
pub const TT_EVENT:       u32 = 16;  // date / timestamp literals

// ── Modifier bitmasks ─────────────────────────────────────────────────────────

pub const MOD_DECLARATION: u32 = 1 << 0;  // definition site (func name, enum name)
pub const MOD_READONLY:    u32 = 1 << 1;  // immutable / section keywords
pub const MOD_DEPRECATED:  u32 = 1 << 2;  // reserved for future use

/// Ordered list of semantic token types exposed to the client.
/// INDEX = the u32 value emitted by the classifier.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,       // 0  — section names, control flow
    SemanticTokenType::STRING,        // 1  — string literals
    SemanticTokenType::NUMBER,        // 2  — numeric literals, hex literals
    SemanticTokenType::OPERATOR,      // 3  — operators and punctuation-operators
    SemanticTokenType::VARIABLE,      // 4  — plain identifiers
    SemanticTokenType::FUNCTION,      // 5  — QuickFunc names
    SemanticTokenType::TYPE,          // 6  — <type> annotations
    SemanticTokenType::ENUM_MEMBER,   // 7  — enum field names
    SemanticTokenType::COMMENT,       // 8  — line comments
    SemanticTokenType::NAMESPACE,     // 9  — import alias names
    SemanticTokenType::PROPERTY,      // 10 — table path segments, group array names
    SemanticTokenType::PARAMETER,     // 11 — QuickFunc parameters
    SemanticTokenType::MACRO,         // 12 — DLM module names
    SemanticTokenType::DECORATOR,     // 13 — DLM subtype suffixes
    SemanticTokenType::STRUCT,        // 14 — enum type declaration names
    SemanticTokenType::REGEXP,        // 15 — regex constructors
    SemanticTokenType::EVENT,         // 16 — date / timestamp literals
];

pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION, // bit 0
    SemanticTokenModifier::READONLY,    // bit 1
    SemanticTokenModifier::DEPRECATED,  // bit 2
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

        hover_provider:      Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        color_provider:      Some(ColorProviderCapability::Simple(true)),
        inlay_hint_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),

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
