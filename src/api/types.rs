use serde::{Deserialize, Serialize};

// DataUnavailableReason and ApiError have been moved to crate::error::RqbitFuseError
// Re-export for backward compatibility: pub use crate::error::RqbitFuseError as ApiError;

/// Torrent summary from list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentSummary {
    pub id: u64,
    #[serde(rename = "info_hash")]
    pub info_hash: String,
    pub name: String,
    #[serde(rename = "output_folder")]
    pub output_folder: String,
}

/// Response from listing torrents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentListResponse {
    pub torrents: Vec<TorrentSummary>,
}

/// Full torrent information.
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

/// File information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub length: u64,
    pub components: Vec<String>,
}

/// Speed information from stats endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Speed {
    pub mbps: f64,
    #[serde(rename = "human_readable")]
    pub human_readable: String,
}

/// Torrent download state snapshot.
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

/// Live stats for active torrents.
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

/// Response from torrent statistics endpoint.
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

/// Response from adding a torrent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTorrentResponse {
    pub id: u64,
    #[serde(rename = "info_hash")]
    pub info_hash: String,
}

/// Result of listing torrents (handles partial failures).
#[derive(Debug, Clone)]
pub struct ListTorrentsResult {
    /// Successfully loaded torrents with full details
    pub torrents: Vec<TorrentInfo>,
    /// Failed torrent fetches: (id, name, error)
    pub errors: Vec<(u64, String, crate::error::RqbitFuseError)>,
}

impl ListTorrentsResult {
    pub fn is_partial(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn has_successes(&self) -> bool {
        !self.torrents.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.torrents.is_empty()
    }

    pub fn total_attempted(&self) -> usize {
        self.torrents.len() + self.errors.len()
    }
}

/// Request to add torrent from magnet link.
#[derive(Debug, Clone, Serialize)]
pub struct AddMagnetRequest {
    #[serde(rename = "magnet_link")]
    pub magnet_link: String,
}

/// Request to add torrent from URL.
#[derive(Debug, Clone, Serialize)]
pub struct AddTorrentUrlRequest {
    #[serde(rename = "torrent_link")]
    pub torrent_link: String,
}

/// Piece availability bitfield.
#[derive(Debug, Clone)]
pub struct PieceBitfield {
    pub bits: Vec<u8>,
    pub num_pieces: usize,
}

impl PieceBitfield {
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

    pub fn downloaded_count(&self) -> usize {
        (0..self.num_pieces).filter(|&i| self.has_piece(i)).count()
    }

    pub fn is_complete(&self) -> bool {
        self.downloaded_count() == self.num_pieces
    }

    /// Check if all pieces in a byte range are available.
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

/// Torrent state for monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TorrentState {
    Downloading,
    Seeding,
    Paused,
    Stalled,
    Error,
    Unknown,
}

/// Torrent status information.
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

    pub fn is_complete(&self) -> bool {
        self.state == TorrentState::Seeding || self.progress_pct >= 100.0
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
