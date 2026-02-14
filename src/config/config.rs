use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub rqbit_url: String,
    pub mount_point: String,
    pub cache_size_mb: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rqbit_url: "http://localhost:3030".to_string(),
            mount_point: "/mnt/torrents".to_string(),
            cache_size_mb: 100,
        }
    }
}
