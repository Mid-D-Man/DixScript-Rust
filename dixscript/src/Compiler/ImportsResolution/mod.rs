//! File and cloud import resolution pipeline.

pub mod hash_verifier;
pub mod imports_resolver;
pub mod cloud_storage_provider;
pub mod cloud_file_cache;

// ── Cloud-only modules — compiled only when the `cloud-import` feature is on
// AND the target isn't wasm32. Without the feature gate, cloud_provider_factory
// pulls in http_cloud_provider which pulls in reqwest — breaking any build with
// default-features = false. Without the target gate, this would try to compile
// against reqwest/tokio on wasm32, where those two are absent from the
// dependency graph on purpose (see Cargo.toml — tokio's rt-multi-thread/fs
// features hard-fail to compile on wasm32-unknown-unknown). imports_resolver.rs
// already has its own wasm32 fallback branch that never calls into these.
#[cfg(all(feature = "cloud-import", not(target_arch = "wasm32")))]
pub mod cloud_provider_factory;
#[cfg(all(feature = "cloud-import", not(target_arch = "wasm32")))]
pub mod http_cloud_provider;

pub use hash_verifier::{HashVerifier, HashVerificationError};
pub use imports_resolver::{ImportsResolver, ImportResolutionStats};
pub use cloud_storage_provider::{CloudStorageProvider, CloudStorageError};
pub use cloud_file_cache::{CloudFileCache, CacheStatistics};

#[cfg(all(feature = "cloud-import", not(target_arch = "wasm32")))]
pub use cloud_provider_factory::CloudProviderFactory;
#[cfg(all(feature = "cloud-import", not(target_arch = "wasm32")))]
pub use http_cloud_provider::HttpCloudProvider;
