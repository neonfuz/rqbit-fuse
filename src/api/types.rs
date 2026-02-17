use serde::{Deserialize, Serialize};
use strum::Display;
use thiserror::Error;

/// Reason why data is unavailable
#[derive(Debug, Clone, PartialEq, Eq, Display)]
#[strum(serialize_all = "snake_case")]
pub enum DataUnavailableReason {
    /// Torrent is paused and pieces haven't been downloaded
    Paused,
    /// Requested pieces haven't been downloaded yet
    NotDownloaded,
}

/// Errors that can occur when interacting with the rqbit API
#[derive(Error, Debug, Clone)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("Failed to initialize HTTP client: {0}")]
    ClientInitializationError(String),

    #[error("Failed to clone HTTP request: {0}")]
    RequestCloneError(String),

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

    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    #[error("Data unavailable for torrent {torrent_id}: {reason}")]
    DataUnavailable {
        torrent_id: u64,
        reason: DataUnavailableReason,
    },
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
            ApiError::TorrentNotFound(_) | ApiError::FileNotFound { .. } => libc::ENOENT,
            ApiError::ApiError { status, .. } => match status {
                400 | 416 => libc::EINVAL,
                401 | 403 => libc::EACCES,
                404 => libc::ENOENT,
                408 | 423 | 429 | 503 | 504 => libc::EAGAIN,
                409 => libc::EEXIST,
                413 => libc::EFBIG,
                500 | 502 => libc::EIO,
                _ => libc::EIO,
            },
            ApiError::InvalidRange(_) | ApiError::SerializationError(_) => libc::EINVAL,
            ApiError::ConnectionTimeout | ApiError::ReadTimeout => libc::EAGAIN,
            ApiError::ServerDisconnected => libc::ENOTCONN,
            ApiError::NetworkError(_) => libc::ENETUNREACH,
            ApiError::ServiceUnavailable(_)
            | ApiError::CircuitBreakerOpen
            | ApiError::RetryLimitExceeded => libc::EAGAIN,
            ApiError::AuthenticationError(_) => libc::EACCES,
            ApiError::DataUnavailable { .. } => libc::EIO,
            ApiError::HttpError(_)
            | ApiError::ClientInitializationError(_)
            | ApiError::RequestCloneError(_) => libc::EIO,
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

/// Speed information from stats endpoint (used for both download and upload)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Speed {
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
    pub download_speed: Speed,
    #[serde(rename = "upload_speed")]
    pub upload_speed: Speed,
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

/// Response from adding a torrent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTorrentResponse {
    pub id: u64,
    #[serde(rename = "info_hash")]
    pub info_hash: String,
}

/// Result of listing torrents, including both successes and failures
///
/// This type allows callers to handle partial failures - some torrents may fail
/// to load while others succeed. Use `is_partial()` to check if any failures
/// occurred, and `has_successes()` to verify at least some torrents loaded.
#[derive(Debug, Clone)]
pub struct ListTorrentsResult {
    /// Successfully loaded torrents with full details
    pub torrents: Vec<TorrentInfo>,
    /// Failed torrent fetches: (id, name, error)
    pub errors: Vec<(u64, String, ApiError)>,
}

impl ListTorrentsResult {
    /// Returns true if there were any failures (partial result)
    pub fn is_partial(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns true if at least one torrent loaded successfully
    pub fn has_successes(&self) -> bool {
        !self.torrents.is_empty()
    }

    /// Returns true if there are no successfully loaded torrents
    pub fn is_empty(&self) -> bool {
        self.torrents.is_empty()
    }

    /// Returns the total number of torrents attempted (successes + failures)
    pub fn total_attempted(&self) -> usize {
        self.torrents.len() + self.errors.len()
    }
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

    /// Check if all pieces in a given byte range are available
    ///
    /// # Arguments
    /// * `offset` - Starting byte offset in the torrent
    /// * `size` - Number of bytes to check
    /// * `piece_length` - Size of each piece in bytes
    ///
    /// # Returns
    /// `true` if all pieces covering the byte range are downloaded, `false` otherwise
    pub fn has_piece_range(&self, offset: u64, size: u64, piece_length: u64) -> bool {
        if size == 0 {
            return true;
        }
        if piece_length == 0 {
            return false;
        }

        // Calculate the start and end piece indices
        let start_piece = (offset / piece_length) as usize;
        let end_byte = offset.saturating_add(size - 1);
        let end_piece = (end_byte / piece_length) as usize;

        // Check that all pieces in the range are available
        for piece_idx in start_piece..=end_piece {
            if !self.has_piece(piece_idx) {
                return false;
            }
        }

        true
    }
}

/// Status of a torrent for monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, Serialize)]
#[strum(serialize_all = "snake_case")]
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

/// Comprehensive torrent status information
#[derive(Debug, Clone, Serialize)]
pub struct TorrentStatus {
    pub torrent_id: u64,
    pub state: TorrentState,
    pub progress_pct: f64,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub downloaded_pieces: usize,
    pub total_pieces: usize,
    #[serde(skip)]
    pub last_updated: std::time::Instant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_piece_range_complete_bitfield() {
        // All pieces available (bitfield: 0b11111111 = all 8 pieces)
        let bitfield = PieceBitfield {
            bits: vec![0b11111111],
            num_pieces: 8,
        };

        // Check various ranges - all should return true
        assert!(bitfield.has_piece_range(0, 100, 100)); // First piece
        assert!(bitfield.has_piece_range(0, 800, 100)); // All pieces
        assert!(bitfield.has_piece_range(350, 100, 100)); // Middle of piece 3
        assert!(bitfield.has_piece_range(700, 100, 100)); // Last piece
        assert!(bitfield.has_piece_range(0, 1, 100)); // Single byte in first piece
        assert!(bitfield.has_piece_range(799, 1, 100)); // Last byte
    }

    #[test]
    fn test_has_piece_range_partial_bitfield() {
        // Partial availability (bitfield: 0b10101010 = pieces 1,3,5,7 available)
        // Note: bit 0 is LSB, so 0b10101010 means bits 1,3,5,7 are set
        let bitfield = PieceBitfield {
            bits: vec![0b10101010],
            num_pieces: 8,
        };

        // Check ranges within available pieces (odd-indexed: 1, 3, 5, 7)
        assert!(bitfield.has_piece_range(100, 100, 100)); // Piece 1 available
        assert!(bitfield.has_piece_range(300, 100, 100)); // Piece 3 available
        assert!(bitfield.has_piece_range(500, 100, 100)); // Piece 5 available
        assert!(bitfield.has_piece_range(700, 100, 100)); // Piece 7 available

        // Check ranges that include unavailable pieces (even-indexed: 0, 2, 4, 6)
        assert!(!bitfield.has_piece_range(0, 100, 100)); // Piece 0 unavailable
        assert!(!bitfield.has_piece_range(0, 200, 100)); // Spans piece 0 and 1 (0 unavailable)
        assert!(!bitfield.has_piece_range(200, 100, 100)); // Piece 2 unavailable
        assert!(!bitfield.has_piece_range(0, 800, 100)); // All pieces - some unavailable
        assert!(!bitfield.has_piece_range(50, 200, 100)); // Spans pieces 0-1
    }

    #[test]
    fn test_has_piece_range_edge_cases() {
        let bitfield = PieceBitfield {
            bits: vec![0b11110000], // Pieces 4-7 available
            num_pieces: 8,
        };

        // Empty range should return true
        assert!(bitfield.has_piece_range(0, 0, 100));
        assert!(bitfield.has_piece_range(500, 0, 100));

        // Zero piece length should return false (except empty range)
        assert!(!bitfield.has_piece_range(0, 100, 0));
        assert!(bitfield.has_piece_range(0, 0, 0)); // Empty range is still ok

        // Range beyond file size (checking available pieces)
        assert!(bitfield.has_piece_range(400, 100, 100)); // Piece 4
        assert!(!bitfield.has_piece_range(0, 100, 100)); // Piece 0 unavailable
    }

    #[test]
    fn test_has_piece_range_spans_multiple_pieces() {
        // Create bitfield with 16 pieces, first 8 available
        let mut bits = vec![0u8; 2];
        bits[0] = 0b11111111; // Pieces 0-7 available
        bits[1] = 0b00000000; // Pieces 8-15 unavailable

        let bitfield = PieceBitfield {
            bits,
            num_pieces: 16,
        };

        // Range spanning multiple pieces within available range
        assert!(bitfield.has_piece_range(0, 800, 100)); // Pieces 0-7
        assert!(bitfield.has_piece_range(100, 500, 100)); // Pieces 1-5
        assert!(bitfield.has_piece_range(350, 250, 100)); // Pieces 3-5

        // Range that crosses into unavailable pieces
        assert!(!bitfield.has_piece_range(700, 200, 100)); // Spans pieces 7-8
        assert!(!bitfield.has_piece_range(750, 100, 100)); // Piece 7 available but goes into 8
    }

    #[test]
    fn test_has_piece_range_large_piece_length() {
        // Test with larger piece sizes (more realistic)
        let bitfield = PieceBitfield {
            bits: vec![0b00001111], // Pieces 0-3 available
            num_pieces: 8,
        };

        // 1MB piece size
        let piece_length = 1024 * 1024;

        // Check ranges within first 4 pieces
        assert!(bitfield.has_piece_range(0, piece_length, piece_length));
        assert!(bitfield.has_piece_range(0, 4 * piece_length, piece_length));
        assert!(bitfield.has_piece_range(piece_length / 2, piece_length, piece_length));

        // Range crossing into unavailable piece
        assert!(!bitfield.has_piece_range(3 * piece_length, 2 * piece_length, piece_length));
    }
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
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
