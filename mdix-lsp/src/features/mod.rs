// mdix-lsp/src/features/mod.rs
pub mod code_actions;
pub mod code_lens;
pub mod call_hierarchy;
pub mod commands;
pub mod completions;
pub mod document_color;
pub mod document_highlights;
pub mod document_symbols;
pub mod folding;
pub mod formatting;
pub mod goto_definition;
pub mod hover;
pub mod hover_data;          // ← signature tables extracted from hover.rs
pub mod inlay_hints;
pub mod references;
pub mod rename;
pub mod semantic_tokens;
pub mod signature_help;
pub mod workspace_symbols;
