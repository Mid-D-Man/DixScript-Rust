// mdix-lsp/src/features/mod.rs
//! Feature provider modules.
//! Each module exposes one `provide()` function called from server.rs.

pub mod code_actions;
pub mod completions;
pub mod document_color;
pub mod folding;
pub mod goto_definition;
pub mod hover;
pub mod inlay_hints;
pub mod semantic_tokens;
