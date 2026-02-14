use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub mount: MountConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(default = "default_api_url")]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_metadata_ttl")]
    pub metadata_ttl: u64,
    #[serde(default = "default_torrent_list_ttl")]
    pub torrent_list_ttl: u64,
    #[serde(default = "default_piece_ttl")]
    pub piece_ttl: u64,
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    #[serde(default = "default_mount_point")]
    pub mount_point: PathBuf,
    #[serde(default = "default_allow_other")]
    pub allow_other: bool,
    #[serde(default = "default_auto_unmount")]
    pub auto_unmount: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default = "default_read_timeout")]
    pub read_timeout: u64,
    #[serde(default = "default_max_concurrent_reads")]
    pub max_concurrent_reads: usize,
    #[serde(default = "default_readahead_size")]
    pub readahead_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    #[serde(default = "default_status_poll_interval")]
    pub status_poll_interval: u64,
    #[serde(default = "default_stalled_timeout")]
    pub stalled_timeout: u64,
}

fn default_api_url() -> String {
    "http://127.0.0.1:3030".to_string()
}

fn default_metadata_ttl() -> u64 {
    60
}

fn default_torrent_list_ttl() -> u64 {
    30
}

fn default_piece_ttl() -> u64 {
    5
}

fn default_max_entries() -> usize {
    1000
}

fn default_mount_point() -> PathBuf {
    PathBuf::from("/mnt/torrents")
}

fn default_allow_other() -> bool {
    false
}

fn default_auto_unmount() -> bool {
    true
}

fn default_read_timeout() -> u64 {
    30
}

fn default_max_concurrent_reads() -> usize {
    10
}

fn default_readahead_size() -> u64 {
    33554432
}

fn default_status_poll_interval() -> u64 {
    5
}

fn default_stalled_timeout() -> u64 {
    300
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            url: default_api_url(),
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            metadata_ttl: default_metadata_ttl(),
            torrent_list_ttl: default_torrent_list_ttl(),
            piece_ttl: default_piece_ttl(),
            max_entries: default_max_entries(),
        }
    }
}

impl Default for MountConfig {
    fn default() -> Self {
        Self {
            mount_point: default_mount_point(),
            allow_other: default_allow_other(),
            auto_unmount: default_auto_unmount(),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            read_timeout: default_read_timeout(),
            max_concurrent_reads: default_max_concurrent_reads(),
            readahead_size: default_readahead_size(),
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            status_poll_interval: default_status_poll_interval(),
            stalled_timeout: default_stalled_timeout(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    ParseError(String),
    #[error("Invalid config value: {0}")]
    InvalidValue(String),
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_file(path: &PathBuf) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            serde_json::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))
        } else {
            toml::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))
        }
    }

    pub fn from_default_locations() -> Result<Self, ConfigError> {
        let config_dirs = [
            dirs::config_dir().map(|d| d.join("torrent-fuse/config.toml")),
            Some(PathBuf::from("/etc/torrent-fuse/config.toml")),
            Some(PathBuf::from("./torrent-fuse.toml")),
        ];

        for path in config_dirs.iter().flatten() {
            if path.exists() {
                tracing::info!("Loading config from: {}", path.display());
                return Self::from_file(path);
            }
        }

        Ok(Self::default())
    }

    pub fn merge_from_env(mut self) -> Result<Self, ConfigError> {
        if let Ok(url) = std::env::var("TORRENT_FUSE_API_URL") {
            self.api.url = url;
        }

        if let Ok(mount_point) = std::env::var("TORRENT_FUSE_MOUNT_POINT") {
            self.mount.mount_point = PathBuf::from(mount_point);
        }

        if let Ok(ttl) = std::env::var("TORRENT_FUSE_METADATA_TTL") {
            self.cache.metadata_ttl = ttl.parse().map_err(|_| {
                ConfigError::InvalidValue("TORRENT_FUSE_METADATA_TTL must be a number".into())
            })?;
        }

        if let Ok(ttl) = std::env::var("TORRENT_FUSE_TORRENT_LIST_TTL") {
            self.cache.torrent_list_ttl = ttl.parse().map_err(|_| {
                ConfigError::InvalidValue("TORRENT_FUSE_TORRENT_LIST_TTL must be a number".into())
            })?;
        }

        if let Ok(ttl) = std::env::var("TORRENT_FUSE_PIECE_TTL") {
            self.cache.piece_ttl = ttl.parse().map_err(|_| {
                ConfigError::InvalidValue("TORRENT_FUSE_PIECE_TTL must be a number".into())
            })?;
        }

        if let Ok(entries) = std::env::var("TORRENT_FUSE_MAX_ENTRIES") {
            self.cache.max_entries = entries.parse().map_err(|_| {
                ConfigError::InvalidValue("TORRENT_FUSE_MAX_ENTRIES must be a number".into())
            })?;
        }

        if let Ok(timeout) = std::env::var("TORRENT_FUSE_READ_TIMEOUT") {
            self.performance.read_timeout = timeout.parse().map_err(|_| {
                ConfigError::InvalidValue("TORRENT_FUSE_READ_TIMEOUT must be a number".into())
            })?;
        }

        if let Ok(concurrent) = std::env::var("TORRENT_FUSE_MAX_CONCURRENT_READS") {
            self.performance.max_concurrent_reads = concurrent.parse().map_err(|_| {
                ConfigError::InvalidValue(
                    "TORRENT_FUSE_MAX_CONCURRENT_READS must be a number".into(),
                )
            })?;
        }

        if let Ok(size) = std::env::var("TORRENT_FUSE_READAHEAD_SIZE") {
            self.performance.readahead_size = size.parse().map_err(|_| {
                ConfigError::InvalidValue("TORRENT_FUSE_READAHEAD_SIZE must be a number".into())
            })?;
        }

        if let Ok(val) = std::env::var("TORRENT_FUSE_ALLOW_OTHER") {
            self.mount.allow_other = val.parse::<bool>().map_err(|_| {
                ConfigError::InvalidValue("TORRENT_FUSE_ALLOW_OTHER must be true or false".into())
            })?;
        }

        if let Ok(val) = std::env::var("TORRENT_FUSE_AUTO_UNMOUNT") {
            self.mount.auto_unmount = val.parse::<bool>().map_err(|_| {
                ConfigError::InvalidValue("TORRENT_FUSE_AUTO_UNMOUNT must be true or false".into())
            })?;
        }

        if let Ok(interval) = std::env::var("TORRENT_FUSE_STATUS_POLL_INTERVAL") {
            self.monitoring.status_poll_interval = interval.parse().map_err(|_| {
                ConfigError::InvalidValue(
                    "TORRENT_FUSE_STATUS_POLL_INTERVAL must be a number".into(),
                )
            })?;
        }

        if let Ok(timeout) = std::env::var("TORRENT_FUSE_STALLED_TIMEOUT") {
            self.monitoring.stalled_timeout = timeout.parse().map_err(|_| {
                ConfigError::InvalidValue("TORRENT_FUSE_STALLED_TIMEOUT must be a number".into())
            })?;
        }

        Ok(self)
    }

    pub fn merge_from_cli(mut self, cli: &CliArgs) -> Self {
        if let Some(ref url) = cli.api_url {
            self.api.url = url.clone();
        }

        if let Some(ref mount_point) = cli.mount_point {
            self.mount.mount_point = mount_point.clone();
        }

        self
    }

    pub fn load() -> Result<Self, ConfigError> {
        Self::from_default_locations()?.merge_from_env()
    }

    pub fn load_with_cli(cli: &CliArgs) -> Result<Self, ConfigError> {
        Ok(Self::from_default_locations()?
            .merge_from_env()?
            .merge_from_cli(cli))
    }
}

#[derive(Debug, Clone, Default)]
pub struct CliArgs {
    pub api_url: Option<String>,
    pub mount_point: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.api.url, "http://127.0.0.1:3030");
        assert_eq!(config.cache.metadata_ttl, 60);
        assert_eq!(config.cache.max_entries, 1000);
        assert_eq!(config.mount.mount_point, PathBuf::from("/mnt/torrents"));
        assert_eq!(config.performance.read_timeout, 30);
    }

    #[test]
    fn test_toml_config_parsing() {
        let toml_content = r#"
[api]
url = "http://localhost:8080"

[cache]
metadata_ttl = 120
max_entries = 500

[mount]
mount_point = "/tmp/torrents"
allow_other = true

[performance]
read_timeout = 60
max_concurrent_reads = 20
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let config = Config::from_file(&temp_file.path().to_path_buf()).unwrap();

        assert_eq!(config.api.url, "http://localhost:8080");
        assert_eq!(config.cache.metadata_ttl, 120);
        assert_eq!(config.cache.max_entries, 500);
        assert_eq!(config.mount.mount_point, PathBuf::from("/tmp/torrents"));
        assert!(config.mount.allow_other);
        assert_eq!(config.performance.read_timeout, 60);
        assert_eq!(config.performance.max_concurrent_reads, 20);
    }

    #[test]
    fn test_json_config_parsing() {
        let json_content = r#"{
            "api": {
                "url": "http://localhost:9090"
            },
            "cache": {
                "metadata_ttl": 90,
                "piece_ttl": 10
            }
        }"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json_content.as_bytes()).unwrap();

        let mut json_path = temp_file.path().to_path_buf();
        json_path.set_extension("json");
        std::fs::rename(temp_file.path(), &json_path).unwrap();

        let config = Config::from_file(&json_path).unwrap();

        assert_eq!(config.api.url, "http://localhost:9090");
        assert_eq!(config.cache.metadata_ttl, 90);
        assert_eq!(config.cache.piece_ttl, 10);
    }

    #[test]
    fn test_merge_from_cli() {
        let config = Config::default();
        let cli = CliArgs {
            api_url: Some("http://custom:8080".to_string()),
            mount_point: Some(PathBuf::from("/custom/mount")),
            config_file: None,
        };

        let merged = config.merge_from_cli(&cli);

        assert_eq!(merged.api.url, "http://custom:8080");
        assert_eq!(merged.mount.mount_point, PathBuf::from("/custom/mount"));
    }
}
