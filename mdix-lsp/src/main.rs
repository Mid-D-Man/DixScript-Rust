//! mdix-lsp — language server for DixScript (.mdix files).
//! Communicates with editors over stdio using the Language Server Protocol.

use tower_lsp::{LspService, Server};

mod analyzer;
mod capabilities;
mod converters;
mod document;
mod features;
mod server;

#[tokio::main]
async fn main() {
    // Tracing output goes to stderr so it never contaminates the LSP stdio channel.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let stdin  = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(server::Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
          }
