//! `mdix-lsp` as a library.
//!
//! The published binary (`src/main.rs`) is a thin wrapper around
//! [`run`] with no extensions registered — behavior is unchanged from
//! before this module existed. Downstream dialects that are still valid
//! `.mdix` grammar underneath (see `extensions` module docs) depend on
//! this crate directly and call [`run_with_extensions`] from their own
//! `main.rs` instead.
//!
//! Everything internal (the tower_lsp `Backend`, the analysis pipeline,
//! the built-in completion/hover/etc. implementations) stays private —
//! the supported public surface is deliberately just [`Document`],
//! [`extensions`], and the two `run*` functions below, so it can stay
//! stable across mdix-lsp releases without pinning downstream crates to
//! mdix-lsp's internals.

mod analyzer;
mod capabilities;
mod converters;
mod document;
pub mod extensions;
mod features;
mod server;

pub use document::Document;
pub use extensions::{CompletionExtension, Extensions, HoverExtension};

/// Re-exported so downstream extension crates name `CompletionItem` /
/// `Position` / `Hover` / etc. from the exact same `tower_lsp` this crate
/// was built against, rather than adding their own possibly-mismatched
/// `tower-lsp` dependency (a version mismatch there makes `Position` in
/// your crate a different type than the `Position` in the
/// `CompletionExtension`/`HoverExtension` trait signatures, which won't
/// compile). Use `mdix_lsp::tower_lsp::lsp_types::...` in extension
/// implementations instead of depending on `tower-lsp` directly.
pub use tower_lsp;

use tower_lsp::{LspService, Server};

/// Runs the server with no extensions registered — identical to the
/// published `mdix-lsp` binary's own behavior.
pub async fn run() {
    run_with_extensions(Extensions::default()).await;
}

/// Runs the server with the given extensions registered. See the
/// `extensions` module for what these can and can't do.
pub async fn run_with_extensions(extensions: Extensions) {
    let stdin  = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) =
        LspService::new(move |client| server::Backend::new(client, extensions));
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// Sets up tracing to stderr (and optionally a file via `MDIX_LSP_LOG`),
/// honoring `RUST_LOG`. Exposed so downstream binaries built on this
/// library get the exact same logging behavior as `mdix-lsp` itself
/// without re-implementing it. See `src/main.rs` for env var docs.
pub fn setup_logging() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("mdix_lsp=info,warn"));

    let log_file = std::env::var("MDIX_LSP_LOG").ok().filter(|s| !s.is_empty());

    if let Some(path) = log_file {
        match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => {
                let (non_blocking, guard) = tracing_appender::non_blocking(file);
                Box::leak(Box::new(guard));

                tracing_subscriber::fmt()
                    .with_env_filter(filter)
                    .with_writer(non_blocking)
                    .with_ansi(false)
                    .with_target(true)
                    .with_thread_ids(false)
                    .init();

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

fn setup_stderr_logging(filter: tracing_subscriber::EnvFilter) {
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false)
        .init();
}
