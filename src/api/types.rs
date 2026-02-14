use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur when interacting with the rqbit API
#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

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
    SerializationError(#[from] serde_json::Error),
}

impl ApiError {
    /// Map API errors to FUSE error codes
    pub fn to_fuse_error(&self) -> libc::c_int {
        match self {
            ApiError::TorrentNotFound(_) | ApiError::FileNotFound { .. } => libc::ENOENT,
            ApiError::ApiError { status, .. } => match status {
                404 => libc::ENOENT,
                400 => libc::EINVAL,
                403 => libc::EACCES,
                416 => libc::EINVAL,
                _ => libc::EIO,
            },
            ApiError::InvalidRange(_) => libc::EINVAL,
            _ => libc::EIO,
        }
    }
}

/// Response from listing all torrents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentListResponse {
    pub torrents: Vec<TorrentInfo>,
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
    pub file_count: usize,
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

/// Response from getting torrent statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentStats {
    #[serde(rename = "file_count")]
    pub file_count: usize,
    pub files: Vec<FileStats>,
    pub finished: bool,
    #[serde(rename = "progress_bytes")]
    pub progress_bytes: u64,
    #[serde(rename = "progress_pct")]
    pub progress_pct: f64,
    #[serde(rename = "total_bytes")]
    pub total_bytes: u64,
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
        let state = if stats.finished {
            TorrentState::Seeding
        } else {
            TorrentState::Downloading
        };

        let (downloaded_pieces, total_pieces) = if let Some(bf) = bitfield {
            (bf.downloaded_count(), bf.num_pieces)
        } else {
            (0, 0)
        };

        Self {
            torrent_id,
            state,
            progress_pct: stats.progress_pct,
            progress_bytes: stats.progress_bytes,
            total_bytes: stats.total_bytes,
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
