// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/AsyncTrait"
// ============================================================================
//! Replacement for the `async-trait` attribute macro: a [`BoxFuture`] alias
//! and a recipe, not a macro.
//!
//! ## Why there is no macro here
//! `#[async_trait]` is a procedural macro. Writing one means a separate
//! `proc-macro = true` crate and, realistically, `syn` + `quote` -- exactly
//! the kind of dependency this pass exists to remove. It is needed in only
//! **two places**: the `CloudStorageProvider` trait declaration and its one
//! implementation (`HttpCloudProvider`). Writing out what the macro expands
//! to, by hand, is less code than the macro would be.
//!
//! ## Why it can't just be `async fn` in the trait
//! Native `async fn` in traits (stable since 1.75) is not dyn-compatible,
//! and this trait is used as `Arc<dyn CloudStorageProvider + Send + Sync>`
//! (`cloud_provider_factory.rs`). A method that returns a boxed future is
//! dyn-compatible. That is the entire job `async-trait` does.
//!
//! ## The recipe -- the macro's EXACT expansion
//! `CloudStorageProvider` is `pub` and re-exported from
//! `Compiler::ImportsResolution`, so it is an extension point: code outside
//! this repo implements it with `#[async_trait]`, and the signature that macro
//! generates is part of what they compile against. The recipe therefore
//! reproduces that signature exactly -- three lifetimes and three `where`
//! bounds -- instead of a simpler one-lifetime form (which fails to compile
//! against an `#[async_trait]` impl with E0195, "lifetime parameters or
//! bounds on method do not match the trait declaration").
//!
//! Trait declaration -- was `async fn f(&self, url: &str) -> R;`
//! ```text
//! fn f<'life0, 'life1, 'async_trait>(
//!     &'life0 self,
//!     url: &'life1 str,
//! ) -> BoxFuture<'async_trait, R>
//! where
//!     'life0: 'async_trait,
//!     'life1: 'async_trait,
//!     Self: 'async_trait;
//! ```
//! Implementation -- was `async fn f(&self, url: &str) -> R { body }`
//! ```text
//! fn f<'life0, 'life1, 'async_trait>(
//!     &'life0 self,
//!     url: &'life1 str,
//! ) -> BoxFuture<'async_trait, R>
//! where
//!     'life0: 'async_trait,
//!     'life1: 'async_trait,
//!     Self: 'async_trait,
//! {
//!     Box::pin(async move { body })
//! }
//! ```
//! - Delete `#[async_trait::async_trait]` from both.
//! - The body is not edited. `return Err(..)` inside an `async` block returns
//!   from the block, which is what it returned from before, and `self` is
//!   captured by the `async move` block.
//! - The caller -- `block_on(provider.download_file_async(url))` in
//!   `imports_resolver.rs` -- does not change; `Pin<Box<dyn Future>>` is
//!   itself a `Future`.
//! - If the compiler cannot infer the error type of a `?` inside the block,
//!   end the block with `Ok::<_, CloudStorageError>(value)`.
//!
//! ## `Send`
//! [`BoxFuture`] is `Send`, as the macro's default output is. The future
//! captures `&self`, so the implementing type must be `Sync`;
//! `HttpCloudProvider` (a `reqwest::Client` plus an `ErrorManager`) is. The
//! trait itself has no `Send`/`Sync` supertrait, exactly like the original.
//!
//! ## Verification
//! The tests below use the real `async-trait` as an oracle in BOTH
//! directions: an `#[async_trait]` impl compiles against the hand-written
//! trait, and a hand-written impl compiles against an `#[async_trait]` trait.
//! Re-introducing the one-lifetime signature makes the first of those fail
//! with E0195 (checked by hand, not kept as a test).
//!
//! ## Honest scope of the saving
//! This removes the `async-trait` crate. It does not shrink the build much
//! beyond that: `async-trait` depends on `proc-macro2`, `quote` and `syn`,
//! and `serde_derive` (through `serde`'s `derive` feature) pulls in the same
//! three, so they stay in the dependency graph regardless.

use std::future::Future;
use std::pin::Pin;

/// A heap-allocated, `Send` future borrowing for `'a` -- what an
/// `#[async_trait]` method returns.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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

    #[derive(Debug, PartialEq)]
    enum TestError {
        Empty,
        NotANumber(String),
    }

    fn parse(key: &str) -> Result<u32, TestError> {
        key.parse::<u32>().map_err(|_| TestError::NotANumber(key.to_string()))
    }

    // ── the hand-desugared trait, shaped like CloudStorageProvider ───────
    // Deliberately NO `Send + Sync` supertrait, exactly like the real trait:
    // those bounds appear only on the `Arc<dyn ..>` in the tests.
    trait Provider {
        fn fetch<'life0, 'life1, 'async_trait>(
            &'life0 self,
            key: &'life1 str,
        ) -> BoxFuture<'async_trait, Result<String, TestError>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait;

        fn exists<'life0, 'life1, 'async_trait>(
            &'life0 self,
            key: &'life1 str,
        ) -> BoxFuture<'async_trait, Result<bool, TestError>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait;
    }

    struct Backend {
        prefix: String,
    }

    // A hand-desugared impl; the bodies are written exactly as they would be
    // inside an `async fn`.
    impl Provider for Backend {
        fn fetch<'life0, 'life1, 'async_trait>(
            &'life0 self,
            key: &'life1 str,
        ) -> BoxFuture<'async_trait, Result<String, TestError>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                if key.is_empty() {
                    return Err(TestError::Empty);
                }
                YieldOnce(false).await;
                let n = parse(key)?;
                Ok(format!("{}{}", self.prefix, n * 2))
            })
        }

        fn exists<'life0, 'life1, 'async_trait>(
            &'life0 self,
            key: &'life1 str,
        ) -> BoxFuture<'async_trait, Result<bool, TestError>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move { Ok(!key.is_empty() && !self.prefix.is_empty()) })
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

    // ── oracle direction 1: a REAL #[async_trait] impl of OUR trait ──────
    // This is what an external implementor of `CloudStorageProvider` does.
    struct RealImpl;

    #[::async_trait::async_trait]
    impl Provider for RealImpl {
        async fn fetch(&self, key: &str) -> Result<String, TestError> {
            if key.is_empty() {
                return Err(TestError::Empty);
            }
            YieldOnce(false).await;
            Ok(format!("real:{}", parse(key)?))
        }

        async fn exists(&self, key: &str) -> Result<bool, TestError> {
            Ok(!key.is_empty())
        }
    }

    #[test]
    fn a_real_async_trait_impl_compiles_against_the_hand_written_trait() {
        let provider: Arc<dyn Provider + Send + Sync> = Arc::new(RealImpl);
        assert_eq!(block_on(provider.fetch("7")), Ok("real:7".to_string()));
        assert_eq!(block_on(provider.fetch("")), Err(TestError::Empty));
        assert_eq!(block_on(provider.exists("a")), Ok(true));
    }

    // ── oracle direction 2: OUR hand-written impl of a REAL trait ────────
    #[::async_trait::async_trait]
    trait RealProvider {
        async fn fetch(&self, key: &str) -> Result<String, TestError>;
        async fn exists(&self, key: &str) -> Result<bool, TestError>;
    }

    struct HandBackend {
        prefix: String,
    }

    impl RealProvider for HandBackend {
        fn fetch<'life0, 'life1, 'async_trait>(
            &'life0 self,
            key: &'life1 str,
        ) -> BoxFuture<'async_trait, Result<String, TestError>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                if key.is_empty() {
                    return Err(TestError::Empty);
                }
                Ok(format!("{}{}", self.prefix, parse(key)? + 1))
            })
        }

        fn exists<'life0, 'life1, 'async_trait>(
            &'life0 self,
            key: &'life1 str,
        ) -> BoxFuture<'async_trait, Result<bool, TestError>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move { Ok(!key.is_empty()) })
        }
    }

    #[test]
    fn a_hand_written_impl_compiles_against_a_real_async_trait_trait() {
        let provider: Arc<dyn RealProvider + Send + Sync> = Arc::new(HandBackend { prefix: "h=".into() });
        assert_eq!(block_on(provider.fetch("9")), Ok("h=10".to_string()));
        assert_eq!(block_on(provider.exists("")), Ok(false));
    }
}
