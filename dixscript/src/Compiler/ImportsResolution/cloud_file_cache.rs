//! Cache for cloud-downloaded imports.
//!
//! Two backends, picked at compile time by target:
//!
//! - **Native**: local filesystem, ~/.mdix_cache (Linux/macOS) or
//!   %LOCALAPPDATA%/mdix_cache (Windows). Persists across processes.
//! - **wasm32**: browser `localStorage`. There's no real filesystem on
//!   wasm32-unknown-unknown, but `localStorage` is a genuine persistent
//!   store the host already has, and its `getItem`/`setItem` are
//!   *synchronous* Web APIs — no promises, no async rewrite needed to fit
//!   DixScript's synchronous resolution pipeline. Persists across page
//!   reloads within the same origin, same as the native cache persists
//!   across process runs.
//!
//! Both backends key on SHA-256(url with any query string stripped), so
//! `https://x/y.mdix?v=1` and `https://x/y.mdix?v=2` hit the same entry —
//! matches how `ImportsResolver` already treats cache lookups.
//!
//! `cache_file`/`prefetch` are the same operation under two names:
//! `cache_file` is what the resolver calls after a real download;
//! `prefetch` is the same call, meant to be exposed to a host (e.g. a
//! `mdix-wasm` binding) that wants to seed the cache *before* compiling —
//! JS does a normal `async fetch()`, then hands the result in here, and the
//! synchronous compiler finds it already cached when it hits that
//! `@IMPORTS` URL. No network access happens inside wasm at all.

use sha2::{Digest, Sha256};

/// Strips any `?query` suffix and lowercases nothing else — same
/// normalization `ImportsResolver::strip_query_parameters` applies before
/// it ever calls into this cache, duplicated here so the cache key is
/// correct even for callers that don't (e.g. a future JS `prefetchImport`
/// binding calling in with a raw, unstripped URL).
#[inline]
fn normalize_cache_key(cloud_url: &str) -> &str {
    match cloud_url.find('?') {
        Some(idx) => &cloud_url[..idx],
        None      => cloud_url,
    }
}

fn compute_url_hash(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

#[derive(Debug, Clone)]
pub struct CacheStatistics {
    pub cached_file_count: usize,
    pub total_size_bytes: u64,
    pub cache_directory: String,
}

impl std::fmt::Display for CacheStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let size_mb = self.total_size_bytes as f64 / (1024.0 * 1024.0);
        write!(
            f,
            "Cache: {} files, {:.2} MB in {}",
            self.cached_file_count, size_mb, self.cache_directory
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Native backend — local filesystem
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
mod native_cache {
    use std::fs;
    use std::path::PathBuf;
    use crate::ErrorManager::ErrorManager;
    use super::{normalize_cache_key, compute_url_hash, CacheStatistics};

    pub struct CloudFileCache {
        cache_root_directory: PathBuf,
        error_manager: ErrorManager,
    }

    impl CloudFileCache {
        pub fn new(error_manager: ErrorManager) -> Self {
            let cache_root = Self::get_cache_root_directory();
            if let Err(e) = fs::create_dir_all(&cache_root) {
                error_manager.log_warning(&format!("Failed to create cache directory: {}", e));
            }
            CloudFileCache { cache_root_directory: cache_root, error_manager }
        }

        fn get_cache_root_directory() -> PathBuf {
            #[cfg(target_os = "windows")]
            {
                if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                    PathBuf::from(local_app_data).join("mdix_cache")
                } else {
                    PathBuf::from(".mdix_cache")
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                if let Ok(home) = std::env::var("HOME") {
                    PathBuf::from(home).join(".mdix_cache")
                } else {
                    PathBuf::from(".mdix_cache")
                }
            }
        }

        pub fn get_cache_path(&self, cloud_url: &str) -> PathBuf {
            let url_hash = compute_url_hash(normalize_cache_key(cloud_url));
            // First 16 hex chars of the hash are enough for a unique-enough subdirectory.
            let cache_subdir = &url_hash[..16];
            let filename = Self::extract_filename_from_url(cloud_url);
            self.cache_root_directory.join(cache_subdir).join(filename)
        }

        pub fn is_cached(&self, cloud_url: &str) -> bool {
            self.get_cache_path(cloud_url).exists()
        }

        pub fn get_cached_content(&self, cloud_url: &str) -> Option<String> {
            let cache_path = self.get_cache_path(cloud_url);
            if !cache_path.exists() {
                return None;
            }
            match fs::read_to_string(&cache_path) {
                Ok(content) => {
                    self.error_manager.log_debug(&format!(
                        "Cache hit: {} -> {}",
                        cloud_url,
                        cache_path.display()
                    ));
                    Some(content)
                }
                Err(e) => {
                    self.error_manager.log_warning(&format!(
                        "Failed to read cached file for '{}': {}",
                        cloud_url, e
                    ));
                    None
                }
            }
        }

        pub fn cache_file(&self, cloud_url: &str, content: &str) {
            let cache_path = self.get_cache_path(cloud_url);
            if let Some(cache_dir) = cache_path.parent() {
                if let Err(e) = fs::create_dir_all(cache_dir) {
                    self.error_manager
                        .log_warning(&format!("Failed to create cache directory: {}", e));
                    return;
                }
            }
            match fs::write(&cache_path, content) {
                Ok(_) => {
                    self.error_manager.log_debug(&format!(
                        "Cached: {} -> {}",
                        cloud_url,
                        cache_path.display()
                    ));
                }
                Err(e) => {
                    // Caching is an optimisation; a write failure is non-fatal.
                    self.error_manager.log_warning(&format!(
                        "Failed to cache file for '{}': {}",
                        cloud_url, e
                    ));
                }
            }
        }

        /// Same operation as `cache_file` under the name a host uses when
        /// seeding the cache ahead of a compile rather than after a real
        /// download. Native rarely needs this (the disk cache already
        /// persists across runs) but it's here for API parity with wasm.
        pub fn prefetch(&self, cloud_url: &str, content: &str) {
            self.cache_file(cloud_url, content);
        }

        pub fn clear_cache(&self) {
            if self.cache_root_directory.exists() {
                if let Err(e) = fs::remove_dir_all(&self.cache_root_directory) {
                    self.error_manager.log_warning(&format!("Failed to clear cache: {}", e));
                } else {
                    let _ = fs::create_dir_all(&self.cache_root_directory);
                }
            }
        }

        pub fn get_statistics(&self) -> CacheStatistics {
            if !self.cache_root_directory.exists() {
                return CacheStatistics {
                    cached_file_count: 0,
                    total_size_bytes: 0,
                    cache_directory: self.cache_root_directory.display().to_string(),
                };
            }

            let mut cached_file_count = 0usize;
            let mut total_size_bytes = 0u64;

            if let Ok(entries) = fs::read_dir(&self.cache_root_directory) {
                for entry in entries.flatten() {
                    if let Ok(ft) = entry.file_type() {
                        if ft.is_file() {
                            cached_file_count += 1;
                            if let Ok(meta) = entry.metadata() {
                                total_size_bytes += meta.len();
                            }
                        } else if ft.is_dir() {
                            if let Ok(sub) = fs::read_dir(entry.path()) {
                                for sub_entry in sub.flatten() {
                                    if sub_entry
                                        .path()
                                        .extension()
                                        .and_then(|s| s.to_str())
                                        == Some("mdix")
                                    {
                                        cached_file_count += 1;
                                        if let Ok(meta) = sub_entry.metadata() {
                                            total_size_bytes += meta.len();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            CacheStatistics {
                cached_file_count,
                total_size_bytes,
                cache_directory: self.cache_root_directory.display().to_string(),
            }
        }

        fn extract_filename_from_url(url: &str) -> String {
            if let Ok(parsed) = url::Url::parse(url) {
                if let Some(mut segments) = parsed.path_segments() {
                    if let Some(last) = segments.next_back() {
                        if !last.is_empty() {
                            return last.to_string();
                        }
                    }
                }
            }
            let hash = compute_url_hash(normalize_cache_key(url));
            format!("import_{}.mdix", &hash[..8])
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// wasm32 backend — browser localStorage
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod wasm_cache {
    use crate::ErrorManager::ErrorManager;
    use super::{normalize_cache_key, compute_url_hash, CacheStatistics};

    /// Every key this cache owns in localStorage is prefixed so
    /// `clear_cache`/`get_statistics` can enumerate just its own entries
    /// without touching anything else the host page has stored there.
    const KEY_PREFIX: &str = "mdix_cache:";

    pub struct CloudFileCache {
        error_manager: ErrorManager,
    }

    impl CloudFileCache {
        pub fn new(error_manager: ErrorManager) -> Self {
            if Self::storage().is_none() {
                // Not fatal — same "caching is best-effort" stance as the
                // native backend's directory-creation failure. Happens in
                // some private-browsing modes, or if this isn't actually
                // running in a browser (e.g. a non-browser wasm host with
                // no `window`).
                error_manager.log_warning(
                    "localStorage unavailable — cloud-import caching disabled for this session"
                );
            }
            CloudFileCache { error_manager }
        }

        fn storage() -> Option<web_sys::Storage> {
            web_sys::window()?.local_storage().ok()?
        }

        fn key_for(cloud_url: &str) -> String {
            format!("{}{}", KEY_PREFIX, compute_url_hash(normalize_cache_key(cloud_url)))
        }

        pub fn is_cached(&self, cloud_url: &str) -> bool {
            match Self::storage() {
                Some(s) => matches!(s.get_item(&Self::key_for(cloud_url)), Ok(Some(_))),
                None    => false,
            }
        }

        pub fn get_cached_content(&self, cloud_url: &str) -> Option<String> {
            let storage = Self::storage()?;
            match storage.get_item(&Self::key_for(cloud_url)) {
                Ok(Some(content)) => {
                    self.error_manager.log_debug(&format!(
                        "Cache hit (localStorage): {}",
                        cloud_url
                    ));
                    Some(content)
                }
                Ok(None) => None,
                Err(_) => {
                    // A JsValue error here means localStorage itself threw
                    // (rare — e.g. disabled mid-session). Treat like a miss.
                    None
                }
            }
        }

        pub fn cache_file(&self, cloud_url: &str, content: &str) {
            let Some(storage) = Self::storage() else {
                self.error_manager.log_warning("localStorage unavailable — caching skipped");
                return;
            };
            match storage.set_item(&Self::key_for(cloud_url), content) {
                Ok(_) => {
                    self.error_manager.log_debug(&format!(
                        "Cached (localStorage): {}",
                        cloud_url
                    ));
                }
                Err(_) => {
                    // localStorage quota is small (~5-10MB total, browser-
                    // dependent) and set_item throws on overflow. Caching
                    // is an optimisation; a write failure is non-fatal,
                    // same stance as the native disk-cache write failure.
                    self.error_manager.log_warning(&format!(
                        "Failed to cache '{}' in localStorage (quota exceeded?)",
                        cloud_url
                    ));
                }
            }
        }

        /// The actual point of the wasm backend: a host (e.g. a
        /// `mdix-wasm` JS binding) fetches the URL itself with a normal
        /// async `fetch()`, then calls this *before* `loadStr()` runs. The
        /// synchronous resolver's `is_cached`/`get_cached_content` calls
        /// then transparently find it — no network access ever happens
        /// inside wasm.
        pub fn prefetch(&self, cloud_url: &str, content: &str) {
            self.cache_file(cloud_url, content);
        }

        pub fn clear_cache(&self) {
            let Some(storage) = Self::storage() else { return; };
            let len = storage.length().unwrap_or(0);
            // Collect first — removing while iterating storage's own live
            // index is unspecified per the Web Storage spec.
            let mut keys_to_remove = Vec::new();
            for i in 0..len {
                if let Ok(Some(key)) = storage.key(i) {
                    if key.starts_with(KEY_PREFIX) {
                        keys_to_remove.push(key);
                    }
                }
            }
            for key in keys_to_remove {
                let _ = storage.remove_item(&key);
            }
        }

        pub fn get_statistics(&self) -> CacheStatistics {
            let Some(storage) = Self::storage() else {
                return CacheStatistics {
                    cached_file_count: 0,
                    total_size_bytes: 0,
                    cache_directory: "localStorage (unavailable)".to_string(),
                };
            };
            let len = storage.length().unwrap_or(0);
            let mut cached_file_count = 0usize;
            let mut total_size_bytes = 0u64;
            for i in 0..len {
                if let Ok(Some(key)) = storage.key(i) {
                    if key.starts_with(KEY_PREFIX) {
                        cached_file_count += 1;
                        if let Ok(Some(val)) = storage.get_item(&key) {
                            total_size_bytes += val.len() as u64;
                        }
                    }
                }
            }
            CacheStatistics {
                cached_file_count,
                total_size_bytes,
                cache_directory: "localStorage".to_string(),
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native_cache::CloudFileCache;
#[cfg(target_arch = "wasm32")]
pub use wasm_cache::CloudFileCache;
