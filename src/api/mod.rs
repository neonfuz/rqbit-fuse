//! HTTP client for rqbit API and torrent streaming.

use base64::Engine;

pub mod client;
pub mod streaming;
pub mod types;

pub use client::create_api_client;
pub use streaming::{PersistentStreamManager, StreamManagerStats};
pub use types::{ListTorrentsResult, TorrentInfo, TorrentSummary};

// Re-export RqbitFuseError for backward compatibility
pub use crate::error::RqbitFuseError as ApiError;

/// Create Authorization header for HTTP Basic Auth
pub fn create_auth_header(auth_credentials: Option<&(String, String)>) -> Option<String> {
    auth_credentials.map(|(username, password)| {
        let credentials = format!("{}:{}", username, password);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {}", encoded)
    })
}
