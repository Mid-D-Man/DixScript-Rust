// src/Compiler/ImportsResolution/cloud_file_cache.rs

use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use crate::ErrorManager::ErrorManager;

/// Local file system cache for cloud-downloaded imports
///
/// Cache location:
/// - Linux/Mac: ~/.mdix_cache/
/// - Windows: %LOCALAPPDATA%/mdix_cache/
///
/// Cache key: First 16 chars of SHA256(cloudUrl)
/// v1.0.0 - No external packages (uses std only)
pub struct CloudFileCache {
    cache_root_directory: PathBuf,
    error_manager: ErrorManager,
}

impl CloudFileCache {
    /// Create new CloudFileCache
    pub fn new(error_manager: ErrorManager) -> Self {
        let cache_root = Self::get_cache_root_directory();

        // Ensure cache directory exists
        if let Err(e) = fs::create_dir_all(&cache_root) {
            error_manager.log_Warning(&format!(
                "Failed to create cache directory: {}",
                e
            ));
        } else {
            error_manager.log_debug(&format!(
                "Cache directory: {}",
                cache_root.display()
            ));
        }

        CloudFileCache {
            cache_root_directory: cache_root,
            error_manager,
        }
    }

    /// Get platform-specific cache root directory
    fn get_cache_root_directory() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            // Windows: %LOCALAPPDATA%/mdix_cache/
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                PathBuf::from(local_app_data).join("mdix_cache")
            } else {
                PathBuf::from(".mdix_cache")
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Linux/Mac: ~/.mdix_cache/
            if let Ok(home) = std::env::var("HOME") {
                PathBuf::from(home).join(".mdix_cache")
            } else {
                PathBuf::from(".mdix_cache")
            }
        }
    }

    /// Get cache path for a cloud URL
    ///
    /// Format: <cache_root>/<sha256_first_16_chars>/<filename>
    pub fn get_cache_path(&self, cloud_url: &str) -> PathBuf {
        // Compute SHA256 of URL
        let url_hash = Self::compute_url_hash(cloud_url);

        // Use first 16 chars as directory name
        let cache_subdir = &url_hash[..16];

        // Extract filename from URL
        let filename = Self::extract_filename_from_url(cloud_url);

        self.cache_root_directory
            .join(cache_subdir)
            .join(filename)
    }

    /// Check if file is cached
    pub fn is_cached(&self, cloud_url: &str) -> bool {
        let cache_path = self.get_cache_path(cloud_url);
        cache_path.exists()
    }

    /// Get cached file content
    ///
    /// Returns None if not cached or read fails
    pub fn get_cached_content(&self, cloud_url: &str) -> Option<String> {
        let cache_path = self.get_cache_path(cloud_url);

        if !cache_path.exists() {
            return None;
        }

        match fs::read_to_string(&cache_path) {
            Ok(content) => {
                self.error_manager.log_debug(&format!(
                    "Cache HIT: {} -> {}",
                    cloud_url,
                    cache_path.display()
                ));
                Some(content)
            }
            Err(e) => {
                self.error_manager.log_Warning(&format!(
                    "Failed to read cached file for '{}': {}",
                    cloud_url, e
                ));
                None
            }
        }
    }

    /// Cache file content
    pub fn cache_file(&self, cloud_url: &str, content: &str) {
        let cache_path = self.get_cache_path(cloud_url);

        // Ensure directory exists
        if let Some(cache_dir) = cache_path.parent() {
            if let Err(e) = fs::create_dir_all(cache_dir) {
                self.error_manager.log_Warning(&format!(
                    "Failed to create cache directory: {}",
                    e
                ));
                return;
            }
        }

        // Write content
        match fs::write(&cache_path, content) {
            Ok(_) => {
                self.error_manager.log_debug(&format!(
                    "Cached file: {} -> {}",
                    cloud_url,
                    cache_path.display()
                ));
            }
            Err(e) => {
                self.error_manager.log_Warning(&format!(
                    "Failed to cache file for '{}': {}",
                    cloud_url, e
                ));
                // Don't fail - caching is optional optimization
            }
        }
    }

    /// Clear entire cache (useful for testing)
    pub fn clear_cache(&self) {
        if self.cache_root_directory.exists() {
            if let Err(e) = fs::remove_dir_all(&self.cache_root_directory) {
                self.error_manager.log_Warning(&format!(
                    "Failed to clear cache: {}",
                    e
                ));
            } else {
                // Recreate directory
                let _ = fs::create_dir_all(&self.cache_root_directory);
                self.error_manager.log_debug("Cache cleared");
            }
        }
    }

    /// Get cache statistics (for debugging)
    pub fn get_statistics(&self) -> CacheStatistics {
        if !self.cache_root_directory.exists() {
            return CacheStatistics {
                cached_file_count: 0,
                total_size_bytes: 0,
                cache_directory: self.cache_root_directory.display().to_string(),
            };
        }

        let mut cached_file_count = 0;
        let mut total_size_bytes = 0;

        // Walk directory tree
        if let Ok(entries) = fs::read_dir(&self.cache_root_directory) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        cached_file_count += 1;
                        if let Ok(metadata) = entry.metadata() {
                            total_size_bytes += metadata.len();
                        }
                    } else if file_type.is_dir() {
                        // Recurse into subdirectories
                        if let Ok(sub_entries) = fs::read_dir(entry.path()) {
                            for sub_entry in sub_entries.flatten() {
                                if sub_entry.path().extension().and_then(|s| s.to_str()) == Some("mdix") {
                                    cached_file_count += 1;
                                    if let Ok(metadata) = sub_entry.metadata() {
                                        total_size_bytes += metadata.len();
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

    /// Compute SHA256 hash of URL for cache key
    fn compute_url_hash(url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let result = hasher.finalize();

        // Convert to lowercase hex string
        result.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    }

    /// Extract filename from URL
    fn extract_filename_from_url(url: &str) -> String {
        // Try to parse URL and extract filename
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(segments) = parsed.path_segments() {
                if let Some(last) = segments.last() {
                    if !last.is_empty() {
                        return last.to_string();
                    }
                }
            }
        }

        // Fallback: use hash of URL as filename
        let hash = Self::compute_url_hash(url);
        format!("import_{}.mdix", &hash[..8])
    }
}

/// Cache statistics for debugging
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