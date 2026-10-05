// ============================================================================
// NOTICE: Full documentation, design decisions, and fix history for this file
// live in docs/dixscript/utilities.md, section "Utilities/AsyncTrait/boxed_future.rs"
// ============================================================================
//! Replacement for the `async-trait` attribute macro: a type alias and a
//! recipe that reproduces the macro's expansion **exactly**, not a macro.
//!
//! ## Why there is no macro here
//! `#[async_trait]` is a procedural macro. Writing one means a separate
//! `proc-macro = true` crate and, realistically, `syn` + `quote` -- exactly the
//! kind of dependency this pass exists to remove. And it is needed in only
//! **two places**: the `CloudStorageProvider` trait declaration and its one
//! implementation, `HttpCloudProvider`. Writing out what the macro expands to,
//! by hand, is less code than a macro would be.
//!
//! ## Why it can't just be `async fn` in the trait
//! Native `async fn` in traits (stable since 1.75) is not dyn-compatible, and
//! this trait is used as `Arc<dyn CloudStorageProvider + Send + Sync>`
//! (`cloud_provider_factory.rs`). A method returning a boxed future is. That is
//! the entire job `async-trait` does.
//!
//! ## The recipe: copy the macro's signature, don't invent a simpler one
//! Trait declaration -- was `async fn f(&self, url: &str) -> R;`
//! ```ignore
//! fn f<'life0, 'life1, 'async_trait>(&'life0 self, url: &'life1 str)
//!     -> BoxFuture<'async_trait, R>
//! where
//!     'life0: 'async_trait,
//!     'life1: 'async_trait,
//!     Self: 'async_trait;
//! ```
//! Implementation -- was `async fn f(&self, url: &str) -> R { body }`
//! ```ignore
//! fn f<'life0, 'life1, 'async_trait>(&'life0 self, url: &'life1 str)
//!     -> BoxFuture<'async_trait, R>
//! where
//!     'life0: 'async_trait,
//!     'life1: 'async_trait,
//!     Self: 'async_trait,
//! {
//!     Box::pin(async move { body })
//! }
//! ```
//! - One lifetime per reference argument plus `'async_trait`, with `'a:
//!   'async_trait` bounds, is what `#[async_trait]` generates for a `&self`
//!   method. A first draft of this module used a simpler single-lifetime
//!   `fn f<'a>(&'a self, url: &'a str)`; that would have broken any outside
//!   crate that implemented the trait with `#[async_trait]` (their generated
//!   impl has three lifetime parameters, which does not match one). Matching
//!   the macro's own signature removes that break.
//! - `return Err(..)` inside `body` keeps working (`return` inside an `async`
//!   block returns from the block). The one caller --
//!   `block_on(provider.download_file_async(url))` -- is unchanged.
//! - If the compiler can't infer the error type of a `?` inside the block, end
//!   the block with `Ok::<_, CloudStorageError>(value)`.
//!
//! ## What is verified, and against what
//! The tests use the real `async-trait` crate as an oracle, in both directions:
//! (1) an implementor written the way a downstream crate would have -- with
//! `#[async_trait::async_trait] impl Trait for X` -- must compile against a
//! trait declared with the hand-written signature and work behind
//! `Arc<dyn Trait + Send + Sync>`; (2) a hand-written implementation must
//! satisfy a trait declared by the real macro. Compiling is the proof: the two
//! signatures either unify or rustc rejects the impl (E0195). They also cover a
//! `Pending` suspension, `return` and `?` inside the block, and borrowing a
//! non-`'static` local. They do NOT touch `CloudStorageProvider` itself; the
//! wired trait and impl compile as part of the crate.
//!
//! ## `Send`
//! `BoxFuture` is `Send`, as the macro's default output is. The future captures
//! `&self`, so the implementing type must be `Sync`; `HttpCloudProvider` (a
//! `reqwest::Client` plus an `ErrorManager`) is.
//!
//! ## Honest scope of the saving
//! This removes the `async-trait` crate from `[dependencies]` (it stays as a
//! dev-dependency, as the oracle). It does not shrink the build much: its
//! `proc-macro2`, `quote` and `syn` dependencies are also pulled in by
//! `serde_derive`.

use std::future::Future;
use std::pin::Pin;

/// A heap-allocated, `Send` future borrowing for `'a` -- what an
/// `#[async_trait]` method returns, with `'a` being the macro's `'async_trait`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    // -- a minimal executor, so the tests need no async runtime -------------
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

    /// `Pending` once, then `Ready` -- proves the boxed future is really polled
    /// across a suspension point, not just run to completion.
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

    // -- a trait shaped like CloudStorageProvider, declared by hand ----------
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

    // -- a hand-desugared implementation (what HttpCloudProvider now is) -----
    struct Backend {
        prefix: String,
    }

    impl Backend {
        fn parse(key: &str) -> Result<u32, TestError> {
            key.parse::<u32>().map_err(|_| TestError::NotANumber(key.to_string()))
        }
    }

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
                let n = Self::parse(key)?;
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

    // -- (1) an OUTSIDE-style implementor, written with the REAL macro -------
    // The oracle direction that matters for downstream crates: someone who
    // implemented the old `#[async_trait]`-declared trait with `#[async_trait]`
    // must still compile against the hand-written declaration above.
    struct Outsider;

    #[::async_trait::async_trait]
    impl Provider for Outsider {
        async fn fetch(&self, key: &str) -> Result<String, TestError> {
            if key.is_empty() {
                return Err(TestError::Empty);
            }
            YieldOnce(false).await;
            Ok(format!("outsider:{}", key))
        }

        async fn exists(&self, key: &str) -> Result<bool, TestError> {
            Ok(key == "yes")
        }
    }

    // -- (2) a trait declared by the REAL macro, satisfied by hand -----------
    #[::async_trait::async_trait]
    trait RealDeclared {
        async fn fetch(&self, key: &str) -> Result<String, TestError>;
    }

    // Its own type, so `backend.fetch(..)` stays unambiguous in the other tests.
    struct HandForReal {
        prefix: String,
    }

    impl RealDeclared for HandForReal {
        fn fetch<'life0, 'life1, 'async_trait>(
            &'life0 self,
            key: &'life1 str,
        ) -> BoxFuture<'async_trait, Result<String, TestError>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move { Ok(format!("real-declared:{}{}", self.prefix, key)) })
        }
    }

    fn assert_send<T: Send>(_: &T) {}

    #[test]
    fn hand_desugared_impl_works_behind_arc_dyn() {
        let provider: Arc<dyn Provider + Send + Sync> = Arc::new(Backend { prefix: "v=".into() });
        assert_eq!(block_on(provider.fetch("21")), Ok("v=42".to_string()));
        assert_eq!(block_on(provider.exists("21")), Ok(true));
    }

    #[test]
    fn outsider_written_with_the_real_macro_compiles_and_runs_against_the_hand_declared_trait() {
        let provider: Arc<dyn Provider + Send + Sync> = Arc::new(Outsider);
        assert_eq!(block_on(provider.fetch("k")), Ok("outsider:k".to_string()));
        assert_eq!(block_on(provider.fetch("")), Err(TestError::Empty));
        assert_eq!(block_on(provider.exists("yes")), Ok(true));
        assert_eq!(block_on(provider.exists("no")), Ok(false));
    }

    #[test]
    fn hand_written_impl_satisfies_a_trait_declared_by_the_real_macro() {
        let b = HandForReal { prefix: "p-".into() };
        assert_eq!(block_on(RealDeclared::fetch(&b, "x")), Ok("real-declared:p-x".to_string()));
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
