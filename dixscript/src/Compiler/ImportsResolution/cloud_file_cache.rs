
//! Local filesystem cache for cloud-downloaded imports.
//!
//! Cache root: ~/.mdix_cache (Linux/macOS) or %LOCALAPPDATA%/mdix_cache (Windows).
//! The subdirectory key is the first 16 hex characters of SHA-256(url).

use std::fs;
use std::path::{PathBuf};
use sha2::{Digest, Sha256};
use crate::ErrorManager::ErrorManager;

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
        let url_hash = Self::compute_url_hash(cloud_url);
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

    fn compute_url_hash(url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect()
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
        let hash = Self::compute_url_hash(url);
        format!("import_{}.mdix", &hash[..8])
    }
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
