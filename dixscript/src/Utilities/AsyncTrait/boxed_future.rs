// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/AsyncTrait/boxed_future.rs"
// ============================================================================
//! Replacement for the `async-trait` attribute macro: a type alias and a
//! recipe, not a macro.
//!
//! ## Why there is no macro here
//! `#[async_trait]` is a procedural macro. Writing one means a separate
//! `proc-macro = true` crate and, realistically, `syn` + `quote` -- exactly
//! the kind of dependency this pass exists to remove. And it's only needed
//! in **three places**: the `CloudStorageProvider` trait declaration, its one
//! implementation (`HttpCloudProvider`), and no other implementor exists.
//! Writing out what the macro expands to, by hand, in three places is less
//! code than the macro would be.
//!
//! ## Why it can't just be `async fn` in the trait
//! Native `async fn` in traits (stable since 1.75) is not dyn-compatible,
//! and this trait is used as `Arc<dyn CloudStorageProvider + Send + Sync>`
//! (`cloud_provider_factory.rs`). A method that returns a boxed future is
//! dyn-compatible. That is the entire job `async-trait` does.
//!
//! ## The recipe
//! Trait declaration -- was `async fn f(&self, url: &str) -> R;`
//! ```ignore
//! fn f<'a>(&'a self, url: &'a str) -> BoxFuture<'a, R>;
//! ```
//! Implementation -- was `async fn f(&self, url: &str) -> R { body }`
//! ```ignore
//! fn f<'a>(&'a self, url: &'a str) -> BoxFuture<'a, R> {
//!     Box::pin(async move { body })
//! }
//! ```
//! - Delete `#[async_trait::async_trait]` from both.
//! - `return Err(..)` inside `body` keeps working: `return` inside an `async`
//!   block returns from the block, which is what it returned from before.
//! - The caller -- `block_on(provider.download_file_async(url))` in
//!   `imports_resolver.rs` -- does not change; `Pin<Box<dyn Future>>` is
//!   itself a `Future`.
//! - Both borrows share one lifetime `'a`. The real macro invents a separate
//!   lifetime per argument; sharing is equivalent for callers, because a
//!   longer borrow shortens to `'a` automatically.
//! - If the compiler can't infer the error type of a `?` inside the block,
//!   end the block with `Ok::<_, CloudStorageError>(value)`. The bodies today
//!   use `return Err(CloudStorageError::..)` with explicit variants, which
//!   pins the type, but that's the thing to reach for if an edit breaks it.
//!
//! ## `Send`
//! `BoxFuture` is `Send`, as the macro's default output is. The future
//! captures `&self`, so the implementing type must be `Sync`;
//! `HttpCloudProvider` (a `reqwest::Client` plus an `ErrorManager`) is.
//!
//! ## This one changes public API -- decide before wiring it in
//! `CloudStorageProvider` is `pub use`d from `Compiler::ImportsResolution`, so
//! it is an extension point, not an internal detail. Anyone outside this repo
//! who implemented it did so with `#[async_trait]`, whose generated method
//! signature (three lifetime parameters and `where` bounds) does not match the
//! hand-desugared one (one lifetime parameter). Their `impl` would stop
//! compiling with "lifetime parameters or bounds on method do not match the
//! trait declaration". Nothing in this workspace implements it outside
//! `HttpCloudProvider`, but that says nothing about other crates. If that
//! matters, this replacement should wait for a version bump -- or not happen.
//!
//! ## Honest scope of the saving
//! This removes the `async-trait` crate. It does not shrink the build much
//! beyond that: `async-trait` depends on `proc-macro2`, `quote` and `syn`,
//! and `serde_derive` (through `serde`'s `derive` feature) pulls in the same
//! three, so they stay in the dependency graph regardless.
//!
//! ## Not verified against the real trait
//! The tests below hand-desugar a trait with the same shape as
//! `CloudStorageProvider` (two methods, `&self` plus a `&str`, used behind
//! `Arc<dyn Trait + Send + Sync>`, a `return Err` and a `?`) and drive it
//! with a tiny `block_on`. They do NOT touch `CloudStorageProvider` itself,
//! which isn't converted until the wiring pass.

use std::future::Future;
use std::pin::Pin;

/// A heap-allocated, `Send` future borrowing for `'a` -- what an
/// `#[async_trait]` method returns.
pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    // ── a minimal executor, so the tests need no async runtime ───────────
    struct NoopWake;
    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = Box::pin(fut);
        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    /// Returns `Pending` once, then `Ready` -- proves the boxed future really
    /// is polled across a suspension point, not just run to completion.
    struct YieldOnce(bool);
    impl Future for YieldOnce {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    // ── a trait shaped like CloudStorageProvider, desugared by hand ──────
    #[derive(Debug, PartialEq)]
    enum TestError {
        Empty,
        NotANumber(String),
    }

    // Deliberately NO `Send + Sync` supertrait, exactly like the real trait:
    // those bounds appear only on the `Arc<dyn ..>` below.
    trait Provider {
        fn fetch<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<String, TestError>>;
        fn exists<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<bool, TestError>>;
    }

    struct Backend {
        prefix: String,
    }

    impl Backend {
        fn parse(key: &str) -> Result<u32, TestError> {
            key.parse::<u32>().map_err(|_| TestError::NotANumber(key.to_string()))
        }
    }

    impl Provider for Backend {
        fn fetch<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<String, TestError>> {
            Box::pin(async move {
                if key.is_empty() {
                    return Err(TestError::Empty);
                }
                YieldOnce(false).await;
                let n = Self::parse(key)?;
                Ok(format!("{}{}", self.prefix, n * 2))
            })
        }

        fn exists<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<bool, TestError>> {
            Box::pin(async move { Ok(!key.is_empty() && self.prefix.len() > 0) })
        }
    }

    fn assert_send<T: Send>(_: &T) {}

    #[test]
    fn works_behind_arc_dyn_with_send_and_sync_added_only_at_the_use_site() {
        let provider: Arc<dyn Provider + Send + Sync> = Arc::new(Backend { prefix: "v=".into() });
        assert_eq!(block_on(provider.fetch("21")), Ok("v=42".to_string()));
        assert_eq!(block_on(provider.exists("21")), Ok(true));
    }

    #[test]
    fn return_and_question_mark_inside_the_block_behave_as_before() {
        let provider = Backend { prefix: String::new() };
        assert_eq!(block_on(provider.fetch("")), Err(TestError::Empty));
        assert_eq!(
            block_on(provider.fetch("abc")),
            Err(TestError::NotANumber("abc".to_string()))
        );
    }

    #[test]
    fn borrows_of_locals_are_accepted_without_static() {
        let provider = Backend { prefix: "n=".into() };
        let key = String::from("5"); // not 'static
        assert_eq!(block_on(provider.fetch(&key)), Ok("n=10".to_string()));
    }

    #[test]
    fn the_returned_future_is_send() {
        let provider = Backend { prefix: "x".into() };
        let fut = provider.fetch("1");
        assert_send(&fut);
        assert_eq!(block_on(fut), Ok("x2".to_string()));
    }
}
