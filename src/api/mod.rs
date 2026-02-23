pub mod client;
pub mod streaming;
pub mod types;

pub use client::create_api_client;
pub use streaming::{PersistentStreamManager, StreamManagerStats};
pub use types::{ListTorrentsResult, TorrentInfo, TorrentSummary};

// Re-export RqbitFuseError for backward compatibility
pub use crate::error::RqbitFuseError as ApiError;
