//! Extension points for dialects built on top of `.mdix`.
//!
//! `.mdix` itself (the `dixscript` crate — tokenizer, parser, section list)
//! is not extensible from here and this module doesn't try to make it so:
//! `SectionId` is a closed enum and adding real new *syntax* means forking
//! `dixscript`. What this module covers is the much more common case: a
//! dialect (e.g. `.msx`) that's still 100% valid `.mdix` grammar, just with
//! certain identifiers — `scene`, `animation`, whatever your dialect adds —
//! that mean something extra to your tooling even though they're ordinary,
//! unremarkable identifiers as far as the tokenizer/parser are concerned.
//! mdix-lsp's own resolution already runs fine on files like that (it just
//! doesn't have any special vocabulary for those names); this gives you a
//! place to bolt extra completions and hover text on top, in a separate
//! downstream crate, without touching mdix-lsp's published source per
//! dialect.
//!
//! ## Usage
//!
//! ```ignore
//! // in your own `msx-lsp` crate, depending on `mdix-lsp` as a library:
//! use mdix_lsp::extensions::{CompletionExtension, Extensions};
//! use mdix_lsp::Document;
//! use tower_lsp::lsp_types::{CompletionItem, Position};
//!
//! struct MsxCompletions;
//! impl CompletionExtension for MsxCompletions {
//!     fn extra_completions(&self, doc: &Document, pos: Position, trigger: Option<&str>) -> Vec<CompletionItem> {
//!         // inspect doc.source / doc.tokens yourself and return whatever's relevant
//!         vec![]
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     mdix_lsp::setup_logging();
//!     mdix_lsp::run_with_extensions(
//!         Extensions::new().with_completion_extension(MsxCompletions),
//!     ).await;
//! }
//! ```

use tower_lsp::lsp_types::{CompletionItem, Hover, Position};

use crate::document::Document;

/// Adds extra completion items alongside whatever mdix-lsp's own core
/// resolution already produced for this position. Always additive — core
/// completions are never removed or replaced, extras are appended after
/// them. Called on every completion request when at least one extension is
/// registered, regardless of whether the core list came back empty or not,
/// so it's fine to only respond to positions/triggers you actually care
/// about and return an empty `Vec` otherwise.
pub trait CompletionExtension: Send + Sync {
    fn extra_completions(
        &self,
        doc:     &Document,
        pos:     Position,
        trigger: Option<&str>,
    ) -> Vec<CompletionItem>;
}

/// Supplies hover text for positions mdix-lsp's own resolution doesn't
/// recognize. Only consulted when the core hover provider returns `None`
/// for this position — registered extensions are tried in registration
/// order and the first `Some` wins. (Core hover is never overridden by an
/// extension: if mdix-lsp already has something to say about a position,
/// e.g. because your dialect's identifier also happens to be a real
/// QuickFunc name, that answer is used as-is.)
pub trait HoverExtension: Send + Sync {
    fn extra_hover(&self, doc: &Document, pos: Position) -> Option<Hover>;
}

/// Bundle of extensions to register when starting the server. Empty by
/// default — the published `mdix-lsp` binary runs with
/// `Extensions::default()`, i.e. exactly the behavior it had before this
/// module existed, nothing added or changed for existing users.
#[derive(Default)]
pub struct Extensions {
    pub completions: Vec<Box<dyn CompletionExtension>>,
    pub hover:        Vec<Box<dyn HoverExtension>>,
}

impl Extensions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_completion_extension(mut self, ext: impl CompletionExtension + 'static) -> Self {
        self.completions.push(Box::new(ext));
        self
    }

    pub fn with_hover_extension(mut self, ext: impl HoverExtension + 'static) -> Self {
        self.hover.push(Box::new(ext));
        self
    }
  }
