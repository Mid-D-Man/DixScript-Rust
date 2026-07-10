//! mdix-lsp — language server for DixScript (.mdix files).
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

use tower_lsp::{LspService, Server};
use tracing_subscriber::{ EnvFilter};

mod analyzer;
mod capabilities;
mod converters;
mod document;
mod features;
mod server;

#[tokio::main]
async fn main() {
    setup_logging();

    let stdin  = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(server::Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

fn setup_logging() {
    // Default to info level; user can override with RUST_LOG.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("mdix_lsp=info,warn"));

    // Check if user wants a log file (useful for debugging without corrupting stdio).
    let log_file = std::env::var("MDIX_LSP_LOG").ok().filter(|s| !s.is_empty());

    if let Some(path) = log_file {
        // Write to both stderr and the log file.
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(file) => {
                // Use a non-blocking writer for the file to avoid blocking tokio.
                let (non_blocking, _guard) = tracing_appender::non_blocking(file);
                // Store guard in a Box::leak so it lives for the process lifetime.
                Box::leak(Box::new(_guard));

                tracing_subscriber::fmt()
                    .with_env_filter(filter)
                    .with_writer(non_blocking)
                    .with_ansi(false)
                    .with_target(true)
                    .with_thread_ids(false)
                    .init();

                // Also print to stderr so LSP4IJ console shows something.
                eprintln!("[mdix-lsp] logging to file: {}", path);
            }
            Err(e) => {
                eprintln!("[mdix-lsp] could not open log file {}: {}", path, e);
                setup_stderr_logging(filter);
            }
        }
    } else {
        setup_stderr_logging(filter);
    }
}

fn setup_stderr_logging(filter: EnvFilter) {
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .init();
}
