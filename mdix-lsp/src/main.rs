//! mdix-lsp — language server for DixScript (.mdix files).
//!
//! This binary is a thin wrapper over the `mdix_lsp` library with no
//! extensions registered. Dialects built on top of `.mdix` (still valid
//! `.mdix` grammar, extra meaning layered on top — see `mdix_lsp::extensions`)
//! depend on `mdix-lsp` as a library from their own binary instead of
//! forking this one.
//!
//! ## Logging
//!
//! All tracing goes to stderr (never stdout — stdout is the LSP stdio channel).
//!
//! Set RUST_LOG for log levels:
//!   RUST_LOG=mdix_lsp=debug  mdix-lsp
//!
//! Set MDIX_LSP_LOG to also write to a file:
//!   MDIX_LSP_LOG=/tmp/mdix-lsp.log  mdix-lsp

#[tokio::main]
async fn main() {
    mdix_lsp::setup_logging();
    mdix_lsp::run().await;
}
