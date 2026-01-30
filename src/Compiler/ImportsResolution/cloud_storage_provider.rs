// src/Compiler/ImportsResolution/cloud_storage_provider.rs

use std::fmt;

/// Cloud storage provider trait
/// Implementations: HttpCloudProvider, S3CloudProvider (future), etc.
#[async_trait::async_trait]
pub trait CloudStorageProvider {
    /// Download file content from cloud URL
    ///
    /// # Errors
    /// - Network errors (timeout, connection refused, DNS failure)
    /// - HTTP errors (404, 403, 500, etc.)
    /// - Firewall/proxy blocking
    /// - Content too large
    async fn download_file_async(&self, cloud_url: &str) -> Result<String, CloudStorageError>;

    /// Check if file exists at cloud URL (HEAD request)
    ///
    /// # Errors
    /// - Network errors
    /// - HTTP errors
    async fn file_exists_async(&self, cloud_url: &str) -> Result<bool, CloudStorageError>;
}

/// Cloud storage error types
#[derive(Debug, Clone)]
pub enum CloudStorageError {
    /// Network connection failed (firewall, no internet, DNS failure)
    NetworkError {
        url: String,
        message: String,
        is_likely_firewall: bool,
        is_likely_proxy: bool,
    },

    /// HTTP error (4xx, 5xx status codes)
    HttpError {
        url: String,
        status_code: u16,
        message: String,
    },

    /// Request timeout (slow network or server)
    Timeout {
        url: String,
        duration_secs: u64,
    },

    /// File too large
    FileTooLarge {
        url: String,
        size_bytes: u64,
        max_size_bytes: u64,
    },

    /// Invalid URL format
    InvalidUrl {
        url: String,
        message: String,
    },

    /// Received HTML instead of file (Dropbox preview page, etc.)
    ReceivedHtml {
        url: String,
        message: String,
    },

    /// Too many redirects
    TooManyRedirects {
        url: String,
        redirect_count: usize,
    },

    /// SSL/TLS error (certificate validation failed)
    SslError {
        url: String,
        message: String,
    },

    /// Generic error
    Other {
        url: String,
        message: String,
    },
}

impl CloudStorageError {
    /// Check if error is likely due to firewall blocking
    pub fn is_likely_firewall(&self) -> bool {
        match self {
            Self::NetworkError { is_likely_firewall, .. } => *is_likely_firewall,
            Self::Timeout { .. } => true, // Timeouts often indicate firewall
            _ => false,
        }
    }

    /// Check if error is likely due to proxy configuration
    pub fn is_likely_proxy(&self) -> bool {
        match self {
            Self::NetworkError { is_likely_proxy, .. } => *is_likely_proxy,
            Self::HttpError { status_code, .. } => *status_code == 407, // Proxy auth required
            _ => false,
        }
    }

    /// Get user-friendly error message with troubleshooting hints
    pub fn user_message(&self) -> String {
        match self {
            Self::NetworkError { url, message, is_likely_firewall, is_likely_proxy } => {
                let mut msg = format!("Network error downloading '{}': {}", url, message);

                if *is_likely_firewall {
                    msg.push_str("\n💡 This may be blocked by a firewall. Check your network settings.");
                }

                if *is_likely_proxy {
                    msg.push_str("\n💡 You may need to configure proxy settings. Set HTTP_PROXY or HTTPS_PROXY environment variables.");
                }

                msg
            }

            Self::HttpError { url, status_code, message } => {
                match status_code {
                    404 => format!("File not found at '{}': {}", url, message),
                    403 => format!("Access denied to '{}': {}\n💡 Check that the file is publicly accessible.", url, message),
                    407 => format!("Proxy authentication required for '{}': {}\n💡 Configure proxy credentials.", url, message),
                    500..=599 => format!("Server error at '{}': {} (HTTP {})", url, message, status_code),
                    _ => format!("HTTP error {} for '{}': {}", status_code, url, message),
                }
            }

            Self::Timeout { url, duration_secs } => {
                format!("Download timeout after {}s for '{}'.\n💡 Check your network connection or try increasing timeout.", duration_secs, url)
            }

            Self::FileTooLarge { url, size_bytes, max_size_bytes } => {
                format!(
                    "File at '{}' is too large: {} bytes (max: {} bytes)",
                    url, size_bytes, max_size_bytes
                )
            }

            Self::InvalidUrl { url, message } => {
                format!("Invalid URL '{}': {}", url, message)
            }

            Self::ReceivedHtml { url, message } => {
                format!("Received HTML instead of file for '{}'.\n{}\n💡 For Dropbox, ensure URL has 'dl=1' parameter.", url, message)
            }

            Self::TooManyRedirects { url, redirect_count } => {
                format!("Too many redirects ({}) for '{}'.", redirect_count, url)
            }

            Self::SslError { url, message } => {
                format!("SSL/TLS error for '{}': {}\n💡 Check certificate validity or use HTTP for local development.", url, message)
            }

            Self::Other { url, message } => {
                format!("Error downloading '{}': {}", url, message)
            }
        }
    }

    /// Get the URL associated with this error
    pub fn url(&self) -> &str {
        match self {
            Self::NetworkError { url, .. } |
            Self::HttpError { url, .. } |
            Self::Timeout { url, .. } |
            Self::FileTooLarge { url, .. } |
            Self::InvalidUrl { url, .. } |
            Self::ReceivedHtml { url, .. } |
            Self::TooManyRedirects { url, .. } |
            Self::SslError { url, .. } |
            Self::Other { url, .. } => url,
        }
    }
}

impl fmt::Display for CloudStorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message())
    }
}

impl std::error::Error for CloudStorageError {}