use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur when interacting with the rqbit API
#[derive(Error, Debug, Clone)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("API returned error: {status} - {message}")]
    ApiError { status: u16, message: String },

    #[error("Torrent not found: {0}")]
    TorrentNotFound(u64),

    #[error("File not found in torrent {torrent_id}: file_idx={file_idx}")]
    FileNotFound { torrent_id: u64, file_idx: usize },

    #[error("Invalid range request: {0}")]
    InvalidRange(String),

    #[error("Retry limit exceeded")]
    RetryLimitExceeded,

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Connection timeout - rqbit server not responding")]
    ConnectionTimeout,

    #[error("Read timeout - request took too long")]
    ReadTimeout,

    #[error("rqbit server disconnected")]
    ServerDisconnected,

    #[error("Circuit breaker open - too many failures")]
    CircuitBreakerOpen,

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl From<reqwest::Error> for ApiError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            if err.to_string().contains("connect") {
                ApiError::ConnectionTimeout
            } else {
                ApiError::ReadTimeout
            }
        } else if err.is_connect() {
            ApiError::ServerDisconnected
        } else if err.is_request() {
            ApiError::NetworkError(err.to_string())
        } else {
            ApiError::HttpError(err.to_string())
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::SerializationError(err.to_string())
    }
}

impl ApiError {
    /// Map API errors to FUSE error codes
    pub fn to_fuse_error(&self) -> libc::c_int {
        match self {
            // Not found errors
            ApiError::TorrentNotFound(_) | ApiError::FileNotFound { .. } => libc::ENOENT,

            // API HTTP status errors
            ApiError::ApiError { status, .. } => match status {
                400 => libc::EINVAL, // Bad request
                401 => libc::EACCES, // Unauthorized
                403 => libc::EACCES, // Forbidden
                404 => libc::ENOENT, // Not found
                408 => libc::EAGAIN, // Request timeout
                409 => libc::EEXIST, // Conflict
                413 => libc::EFBIG,  // Payload too large
                416 => libc::EINVAL, // Range not satisfiable
                423 => libc::EAGAIN, // Locked
                429 => libc::EAGAIN, // Too many requests
                500 => libc::EIO,    // Internal server error
                502 => libc::EIO,    // Bad gateway
                503 => libc::EAGAIN, // Service unavailable
                504 => libc::EAGAIN, // Gateway timeout
                _ => libc::EIO,
            },

            // Invalid input errors
            ApiError::InvalidRange(_) => libc::EINVAL,
            ApiError::SerializationError(_) => libc::EIO,

            // Timeout errors - return EAGAIN to suggest retry
            ApiError::ConnectionTimeout | ApiError::ReadTimeout => libc::EAGAIN,

            // Server/Network errors
            ApiError::ServerDisconnected => libc::ENOTCONN,
            ApiError::NetworkError(_) => libc::ENETUNREACH,
            ApiError::ServiceUnavailable(_) => libc::EAGAIN,

            // Circuit breaker - service temporarily unavailable
            ApiError::CircuitBreakerOpen => libc::EAGAIN,

            // Retry limit exceeded
            ApiError::RetryLimitExceeded => libc::EAGAIN,

            // Generic HTTP errors
            ApiError::HttpError(_) => libc::EIO,
        }
    }

    /// Check if this error is transient and retryable
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ApiError::ConnectionTimeout
                | ApiError::ReadTimeout
                | ApiError::ServerDisconnected
                | ApiError::NetworkError(_)
                | ApiError::ServiceUnavailable(_)
                | ApiError::CircuitBreakerOpen
                | ApiError::RetryLimitExceeded
                | ApiError::ApiError {
                    status: 408 | 429 | 502 | 503 | 504,
                    ..
                }
        )
    }

    /// Check if this error indicates the server is unavailable
    pub fn is_server_unavailable(&self) -> bool {
        matches!(
            self,
            ApiError::ConnectionTimeout
                | ApiError::ServerDisconnected
                | ApiError::NetworkError(_)
                | ApiError::ServiceUnavailable(_)
                | ApiError::CircuitBreakerOpen
        )
    }
}

/// Summary of a torrent from the list endpoint
/// This is a simplified view without file details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSummary {
    pub id: u64,
    #[serde(rename = "info_hash")]
    pub info_hash: String,
    pub name: String,
    #[serde(rename = "output_folder")]
    pub output_folder: String,
}

/// Response from listing all torrents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentListResponse {
    pub torrents: Vec<TorrentSummary>,
}

/// Torrent information from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInfo {
    pub id: u64,
    #[serde(rename = "info_hash")]
    pub info_hash: String,
    pub name: String,
    #[serde(rename = "output_folder")]
    pub output_folder: String,
    #[serde(rename = "file_count")]
    pub file_count: Option<usize>,
    pub files: Vec<FileInfo>,
    #[serde(rename = "piece_length")]
    pub piece_length: Option<u64>,
}

/// File information from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub length: u64,
    pub components: Vec<String>,
}

/// Download speed information from stats endpoint
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadSpeed {
    pub mbps: f64,
    #[serde(rename = "human_readable")]
    pub human_readable: String,
}

/// Upload speed information from stats endpoint
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UploadSpeed {
    pub mbps: f64,
    #[serde(rename = "human_readable")]
    pub human_readable: String,
}

/// Snapshot of torrent download state from stats endpoint
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TorrentSnapshot {
    #[serde(rename = "downloaded_and_checked_bytes")]
    pub downloaded_and_checked_bytes: u64,
    #[serde(rename = "downloaded_and_checked_pieces")]
    pub downloaded_and_checked_pieces: Option<u64>,
    #[serde(rename = "fetched_bytes")]
    pub fetched_bytes: Option<u64>,
    #[serde(rename = "uploaded_bytes")]
    pub uploaded_bytes: Option<u64>,
    #[serde(rename = "remaining_bytes")]
    pub remaining_bytes: Option<u64>,
    #[serde(rename = "total_piece_download_ms")]
    pub total_piece_download_ms: Option<u64>,
    #[serde(rename = "peer_stats")]
    pub peer_stats: Option<serde_json::Value>,
}

/// Live stats for an active torrent (present when state is "live")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveStats {
    pub snapshot: TorrentSnapshot,
    #[serde(rename = "average_piece_download_time")]
    pub average_piece_download_time: Option<serde_json::Value>,
    #[serde(rename = "download_speed")]
    pub download_speed: DownloadSpeed,
    #[serde(rename = "upload_speed")]
    pub upload_speed: UploadSpeed,
    #[serde(rename = "time_remaining")]
    pub time_remaining: Option<serde_json::Value>,
}

/// Response from getting torrent statistics (v1 endpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentStats {
    pub state: String,
    #[serde(rename = "file_progress")]
    pub file_progress: Vec<u64>,
    pub error: Option<String>,
    #[serde(rename = "progress_bytes")]
    pub progress_bytes: u64,
    #[serde(rename = "uploaded_bytes")]
    pub uploaded_bytes: u64,
    #[serde(rename = "total_bytes")]
    pub total_bytes: u64,
    pub finished: bool,
    pub live: Option<LiveStats>,
}

/// File statistics from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStats {
    pub length: u64,
    pub included: bool,
}

/// Response from adding a torrent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTorrentResponse {
    pub id: u64,
    #[serde(rename = "info_hash")]
    pub info_hash: String,
}

/// Request to add torrent from magnet link
#[derive(Debug, Clone, Serialize)]
pub struct AddMagnetRequest {
    #[serde(rename = "magnet_link")]
    pub magnet_link: String,
}

/// Request to add torrent from URL
#[derive(Debug, Clone, Serialize)]
pub struct AddTorrentUrlRequest {
    #[serde(rename = "torrent_link")]
    pub torrent_link: String,
}

/// Piece availability bitfield
#[derive(Debug, Clone)]
pub struct PieceBitfield {
    pub bits: Vec<u8>,
    pub num_pieces: usize,
}

impl PieceBitfield {
    /// Check if a piece is downloaded
    pub fn has_piece(&self, piece_idx: usize) -> bool {
        if piece_idx >= self.num_pieces {
            return false;
        }
        let byte_idx = piece_idx / 8;
        let bit_idx = piece_idx % 8; // LSB first
        if byte_idx < self.bits.len() {
            (self.bits[byte_idx] >> bit_idx) & 1 == 1
        } else {
            false
        }
    }

    /// Count downloaded pieces
    pub fn downloaded_count(&self) -> usize {
        (0..self.num_pieces).filter(|&i| self.has_piece(i)).count()
    }

    /// Check if all pieces are downloaded
    pub fn is_complete(&self) -> bool {
        self.downloaded_count() == self.num_pieces
    }
}

/// Status of a torrent for monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TorrentState {
    /// Torrent is downloading
    Downloading,
    /// Torrent is seeding (complete)
    Seeding,
    /// Torrent is paused
    Paused,
    /// Torrent appears stalled (no progress)
    Stalled,
    /// Torrent has encountered an error
    Error,
    /// Unknown state
    Unknown,
}

impl std::fmt::Display for TorrentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TorrentState::Downloading => write!(f, "downloading"),
            TorrentState::Seeding => write!(f, "seeding"),
            TorrentState::Paused => write!(f, "paused"),
            TorrentState::Stalled => write!(f, "stalled"),
            TorrentState::Error => write!(f, "error"),
            TorrentState::Unknown => write!(f, "unknown"),
        }
    }
}

/// Comprehensive torrent status information
#[derive(Debug, Clone)]
pub struct TorrentStatus {
    pub torrent_id: u64,
    pub state: TorrentState,
    pub progress_pct: f64,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub downloaded_pieces: usize,
    pub total_pieces: usize,
    pub last_updated: std::time::Instant,
}

impl TorrentStatus {
    /// Create a new status from stats and bitfield
    pub fn new(torrent_id: u64, stats: &TorrentStats, bitfield: Option<&PieceBitfield>) -> Self {
        let progress_bytes = stats.progress_bytes;
        let total_bytes = stats.total_bytes;

        // Calculate progress percentage
        let progress_pct = if total_bytes > 0 {
            (progress_bytes as f64 / total_bytes as f64) * 100.0
        } else {
            0.0
        };

        // Determine state from API response and progress
        let state = if stats.error.is_some() {
            TorrentState::Error
        } else if stats.finished || (progress_bytes >= total_bytes && total_bytes > 0) {
            TorrentState::Seeding
        } else if stats.state == "paused" {
            TorrentState::Paused
        } else if stats.state == "live" {
            TorrentState::Downloading
        } else {
            TorrentState::Unknown
        };

        let (downloaded_pieces, total_pieces) = if let Some(bf) = bitfield {
            (bf.downloaded_count(), bf.num_pieces)
        } else {
            (0, 0)
        };

        Self {
            torrent_id,
            state,
            progress_pct,
            progress_bytes,
            total_bytes,
            downloaded_pieces,
            total_pieces,
            last_updated: std::time::Instant::now(),
        }
    }

    /// Check if the torrent is complete
    pub fn is_complete(&self) -> bool {
        self.state == TorrentState::Seeding || self.progress_pct >= 100.0
    }

    /// Get status as a JSON string for xattr
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"torrent_id":{},"state":"{}","progress_pct":{:.2},"progress_bytes":{},"total_bytes":{},"downloaded_pieces":{},"total_pieces":{}}}"#,
            self.torrent_id,
            self.state,
            self.progress_pct,
            self.progress_bytes,
            self.total_bytes,
            self.downloaded_pieces,
            self.total_pieces
        )
    }
}
