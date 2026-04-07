
//! Trait and error types for pluggable cloud storage backends.

use std::fmt;

/// Abstraction over HTTP, S3, Azure, and GCP storage backends.
///
/// The `async_trait` attribute desugars async methods into boxed futures,
/// making the trait dyn-compatible for use behind `Arc<dyn CloudStorageProvider>`.
#[async_trait::async_trait]
pub trait CloudStorageProvider {
    async fn download_file_async(&self, cloud_url: &str) -> Result<String, CloudStorageError>;
    async fn file_exists_async(&self, cloud_url: &str) -> Result<bool, CloudStorageError>;
}

#[derive(Debug, Clone)]
pub enum CloudStorageError {
    NetworkError {
        url: String,
        message: String,
        is_likely_firewall: bool,
        is_likely_proxy: bool,
    },
    HttpError {
        url: String,
        status_code: u16,
        message: String,
    },
    Timeout {
        url: String,
        duration_secs: u64,
    },
    FileTooLarge {
        url: String,
        size_bytes: u64,
        max_size_bytes: u64,
    },
    InvalidUrl {
        url: String,
        message: String,
    },
    ReceivedHtml {
        url: String,
        message: String,
    },
    TooManyRedirects {
        url: String,
        redirect_count: usize,
    },
    SslError {
        url: String,
        message: String,
    },
    Other {
        url: String,
        message: String,
    },
}

impl CloudStorageError {
    pub fn is_likely_firewall(&self) -> bool {
        match self {
            Self::NetworkError { is_likely_firewall, .. } => *is_likely_firewall,
            Self::Timeout { .. } => true,
            _ => false,
        }
    }

    pub fn is_likely_proxy(&self) -> bool {
        match self {
            Self::NetworkError { is_likely_proxy, .. } => *is_likely_proxy,
            Self::HttpError { status_code, .. } => *status_code == 407,
            _ => false,
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::NetworkError { url, message, is_likely_firewall, is_likely_proxy } => {
                let mut msg = format!("Network error downloading '{}': {}", url, message);
                if *is_likely_firewall {
                    msg.push_str(
                        "\nThis may be blocked by a firewall. Check your network settings.",
                    );
                }
                if *is_likely_proxy {
                    msg.push_str(
                        "\nYou may need to configure proxy settings. \
                         Set HTTP_PROXY or HTTPS_PROXY environment variables.",
                    );
                }
                msg
            }

            Self::HttpError { url, status_code, message } => match status_code {
                404 => format!("File not found at '{}': {}", url, message),
                403 => format!(
                    "Access denied to '{}': {}. Check that the file is publicly accessible.",
                    url, message
                ),
                407 => format!(
                    "Proxy authentication required for '{}': {}. Configure proxy credentials.",
                    url, message
                ),
                500..=599 => format!(
                    "Server error at '{}': {} (HTTP {})",
                    url, message, status_code
                ),
                _ => format!("HTTP error {} for '{}': {}", status_code, url, message),
            },

            Self::Timeout { url, duration_secs } => format!(
                "Download timeout after {}s for '{}'. \
                 Check your network connection or increase the timeout.",
                duration_secs, url
            ),

            Self::FileTooLarge { url, size_bytes, max_size_bytes } => format!(
                "File at '{}' is too large: {} bytes (max: {} bytes)",
                url, size_bytes, max_size_bytes
            ),

            Self::InvalidUrl { url, message } => {
                format!("Invalid URL '{}': {}", url, message)
            }

            Self::ReceivedHtml { url, message } => format!(
                "Received HTML instead of file for '{}'. {}. \
                 For Dropbox, ensure the URL has the 'dl=1' parameter.",
                url, message
            ),

            Self::TooManyRedirects { url, redirect_count } => {
                format!("Too many redirects ({}) for '{}'.", redirect_count, url)
            }

            Self::SslError { url, message } => format!(
                "SSL/TLS error for '{}': {}. \
                 Check certificate validity or use HTTP for local development.",
                url, message
            ),

            Self::Other { url, message } => {
                format!("Error downloading '{}': {}", url, message)
            }
        }
    }

    pub fn url(&self) -> &str {
        match self {
            Self::NetworkError { url, .. }
            | Self::HttpError { url, .. }
            | Self::Timeout { url, .. }
            | Self::FileTooLarge { url, .. }
            | Self::InvalidUrl { url, .. }
            | Self::ReceivedHtml { url, .. }
            | Self::TooManyRedirects { url, .. }
            | Self::SslError { url, .. }
            | Self::Other { url, .. } => url,
        }
    }
}

impl fmt::Display for CloudStorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message())
    }
}

impl std::error::Error for CloudStorageError {}