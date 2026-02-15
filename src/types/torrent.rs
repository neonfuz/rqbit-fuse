use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Torrent {
    pub id: u64,
    pub name: String,
    pub info_hash: String,
    pub total_size: u64,
    pub piece_length: u64,
    pub num_pieces: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFile {
    pub path: Vec<String>,
    pub length: u64,
    pub offset: u64,
}
