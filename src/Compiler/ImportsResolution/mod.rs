// src/Compiler/ImportsResolution/mod.rs

//! # ImportsResolution - Cloud imports and file resolution
//!
//! This module handles:
//! - Cloud file downloads (HTTP/HTTPS, future: S3, Azure, GCP)
//! - Local file caching
//! - Hash verification for security
//! - Network error handling (firewalls, proxies, timeouts)
//!
//! ## Ported Components (v1.0.0):
//! - ✅ CloudStorageProvider trait (cloud_storage_provider.rs)
//! - ✅ CloudFileCache (cloud_file_cache.rs)
//! - ✅ CloudProviderFactory (cloud_provider_factory.rs)
//! - ✅ HashVerifier (hash_verifier.rs)
//! - ✅ HttpCloudProvider (http_cloud_provider.rs)
//!
//! ## TODO: Components to Port:
//! - ⏳ ImportsResolver - Main resolver orchestrating all import operations
//! - ⏳ LocalFileResolver - Resolves local file paths
//! - ⏳ CircularDependencyDetector - Detects circular import chains
//! - ⏳ ImportsCacheManager - Manages import resolution cache
//!
//! ## Usage Example:
//! ```rust,ignore
//! use dixscript::Compiler::ImportsResolution::*;
//! use dixscript::ErrorManager::ErrorManager;
//!
//! let error_manager = ErrorManager::get_shared_instance();
//! let cache = CloudFileCache::new(error_manager.clone());
//!
//! // Check cache first
//! if let Some(content) = cache.get_cached_content("https://example.com/file.mdix") {
//!     println!("Using cached content");
//! } else {
//!     // Download from cloud
//!     let provider = CloudProviderFactory::get_provider(
//!         "https://example.com/file.mdix",
//!         &error_manager
//!     )?;
//!
//!     let content = provider.download_file_async("https://example.com/file.mdix").await?;
//!
//!     // Cache for future use
//!     cache.cache_file("https://example.com/file.mdix", &content);
//! }
//! ```

pub mod cloud_storage_provider;
pub mod cloud_file_cache;
pub mod cloud_provider_factory;
pub mod hash_verifier;
pub mod http_cloud_provider;

// TODO: Port these modules
// pub mod imports_resolver;
// pub mod local_file_resolver;
// pub mod circular_dependency_detector;
// pub mod imports_cache_manager;

// Re-exports
pub use cloud_storage_provider::{CloudStorageProvider, CloudStorageError};
pub use cloud_file_cache::{CloudFileCache, CacheStatistics};
pub use cloud_provider_factory::CloudProviderFactory;
pub use hash_verifier::{HashVerifier, HashVerificationError};
pub use http_cloud_provider::HttpCloudProvider;

// TODO: Re-export these when ported
// pub use imports_resolver::ImportsResolver;
// pub use local_file_resolver::LocalFileResolver;
// pub use circular_dependency_detector::CircularDependencyDetector;
// pub use imports_cache_manager::ImportsCacheManager;