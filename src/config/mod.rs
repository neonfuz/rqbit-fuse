use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

// Macros for reducing config boilerplate
macro_rules! default_fn {
    ($name:ident, $ty:ty, $val:expr) => {
        fn $name() -> $ty {
            $val
        }
    };
}

macro_rules! default_impl {
    ($struct:ty, $($field:ident: $default_fn:ident),* $(,)?) => {
        impl Default for $struct {
            fn default() -> Self {
                Self {
                    $($field: $default_fn(),)*
                }
            }
        }
    };
}

macro_rules! env_var {
    // String type - no parsing needed
    ($env_name:expr, $field:expr) => {
        if let Ok(val) = std::env::var($env_name) {
            $field = val;
        }
    };
    // Type with parsing (numbers, bools, PathBuf)
    ($env_name:expr, $field:expr, $parse:expr) => {
        if let Ok(val) = std::env::var($env_name) {
            $field = $parse(&val).map_err(|_| {
                ConfigError::InvalidValue(concat!($env_name, " has invalid format").into())
            })?;
        }
    };
}

/// Main configuration container for torrent-fuse.
///
/// Combines all configuration sections (API, cache, mount, performance, monitoring, logging)
/// into a single struct that can be loaded from files, environment variables, or CLI arguments.
///
/// # Loading Configuration
///
/// Configuration is loaded in the following order (later sources override earlier):
/// 1. Default values
/// 2. Config file (TOML or JSON)
/// 3. Environment variables
/// 4. CLI arguments
///
/// # Fields
///
/// * `api` - API connection settings for rqbit
/// * `cache` - Cache TTL and capacity settings
/// * `mount` - FUSE mount point and options
/// * `performance` - Read timeouts and concurrency limits
/// * `monitoring` - Status polling and stall detection
/// * `logging` - Log verbosity and metrics settings
///
/// # Example
///
/// ```rust
/// use torrent_fuse::config::Config;
///
/// let config = Config::load().expect("Failed to load config");
/// config.validate().expect("Invalid configuration");
/// ```
///
/// ## Complete TOML Configuration Example
///
/// ```toml
/// # Basic configuration for torrent-fuse
/// # Copy this to ~/.config/torrent-fuse/config.toml or /etc/torrent-fuse/config.toml
///
/// [api]
/// url = "http://127.0.0.1:3030"
/// # Optional: HTTP Basic authentication
/// # username = "admin"
/// # password = "secret"
///
/// [cache]
/// # How long to cache file metadata (seconds)
/// metadata_ttl = 60
/// # How long to cache torrent list (seconds)
/// torrent_list_ttl = 30
/// # How long to cache downloaded pieces (seconds)
/// piece_ttl = 5
/// # Maximum number of cache entries
/// max_entries = 1000
///
/// [mount]
/// # Where to mount the FUSE filesystem
/// mount_point = "/mnt/torrents"
/// # Allow other users to access the mount
/// allow_other = false
/// # Automatically unmount on process exit
/// auto_unmount = true
/// # User ID for file ownership (default: current user's EUID)
/// # uid = 1000
/// # Group ID for file ownership (default: current user's EGID)
/// # gid = 1000
///
/// [performance]
/// # Timeout for read operations (seconds)
/// read_timeout = 30
/// # Maximum concurrent read operations
/// max_concurrent_reads = 10
/// # Read-ahead buffer size (bytes)
/// readahead_size = 33554432
/// # Enable piece verification checksums
/// piece_check_enabled = true
/// # Return EAGAIN when data is unavailable
/// return_eagain_for_unavailable = false
///
/// [monitoring]
/// # Interval between status polls (seconds)
/// status_poll_interval = 5
/// # Timeout before marking torrent as stalled (seconds)
/// stalled_timeout = 300
///
/// [logging]
/// # Log level: error, warn, info, debug, trace
/// level = "info"
/// # Log all FUSE operations
/// log_fuse_operations = true
/// # Log all API calls to rqbit
/// log_api_calls = true
/// # Enable metrics collection and logging
/// metrics_enabled = true
/// # Interval between metrics logs (seconds)
/// metrics_interval_secs = 60
/// ```
///
/// ## Complete JSON Configuration Example
///
/// ```json
/// {
///   "api": {
///     "url": "http://127.0.0.1:3030",
///     "username": "admin",
///     "password": "secret"
///   },
///   "cache": {
///     "metadata_ttl": 60,
///     "torrent_list_ttl": 30,
///     "piece_ttl": 5,
///     "max_entries": 1000
///   },
///   "mount": {
///     "mount_point": "/mnt/torrents",
///     "allow_other": false,
///     "auto_unmount": true,
///     "uid": 1000,
///     "gid": 1000
///   },
///   "performance": {
///     "read_timeout": 30,
///     "max_concurrent_reads": 10,
///     "readahead_size": 33554432,
///     "piece_check_enabled": true,
///     "return_eagain_for_unavailable": false
///   },
///   "monitoring": {
///     "status_poll_interval": 5,
///     "stalled_timeout": 300
///   },
///   "logging": {
///     "level": "info",
///     "log_fuse_operations": true,
///     "log_api_calls": true,
///     "metrics_enabled": true,
///     "metrics_interval_secs": 60
///   }
/// }
/// ```
///
/// ## Minimal Configuration
///
/// For most users, only the API URL and mount point are required:
///
/// ```toml
/// [api]
/// url = "http://127.0.0.1:3030"
///
/// [mount]
/// mount_point = "/tmp/torrents"
/// ```
///
/// ## Environment Variable Overrides
///
/// Any config value can be overridden with environment variables:
///
/// ```bash
/// # Set API URL
/// export TORRENT_FUSE_API_URL="http://localhost:8080"
///
/// # Set mount point
/// export TORRENT_FUSE_MOUNT_POINT="/my/torrents"
///
/// # Adjust cache settings
/// export TORRENT_FUSE_METADATA_TTL=120
/// export TORRENT_FUSE_MAX_ENTRIES=5000
///
/// # Enable debug logging
/// export TORRENT_FUSE_LOG_LEVEL=debug
///
/// # Authentication
/// export TORRENT_FUSE_AUTH_USERPASS="username:password"
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// API connection settings for rqbit daemon.
    #[serde(default)]
    pub api: ApiConfig,
    /// Cache TTL and capacity settings.
    #[serde(default)]
    pub cache: CacheConfig,
    /// FUSE mount point and options.
    #[serde(default)]
    pub mount: MountConfig,
    /// Read timeouts and concurrency limits.
    #[serde(default)]
    pub performance: PerformanceConfig,
    /// Status polling and stall detection settings.
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    /// Log verbosity and metrics settings.
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Configuration for the rqbit API connection.
///
/// # Fields
///
/// * `url` - Base URL of the rqbit HTTP API (default: `http://127.0.0.1:3030`)
/// * `username` - Optional username for HTTP Basic authentication
/// * `password` - Optional password for HTTP Basic authentication
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_API_URL` - Override the API URL
/// - `TORRENT_FUSE_AUTH_USERPASS` - Combined credentials as "username:password"
/// - `TORRENT_FUSE_AUTH_USERNAME` - Username for authentication
/// - `TORRENT_FUSE_AUTH_PASSWORD` - Password for authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Configuration for caching behavior.
///
/// Controls TTL (time-to-live) and capacity limits for various cached data types.
///
/// # Fields
///
/// * `metadata_ttl` - How long to cache file metadata in seconds (default: 60)
/// * `torrent_list_ttl` - How long to cache torrent list in seconds (default: 30)
/// * `piece_ttl` - How long to cache downloaded pieces in seconds (default: 5)
/// * `max_entries` - Maximum number of entries in the cache (default: 1000)
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_METADATA_TTL` - Metadata cache TTL in seconds
/// - `TORRENT_FUSE_TORRENT_LIST_TTL` - Torrent list cache TTL in seconds
/// - `TORRENT_FUSE_PIECE_TTL` - Piece cache TTL in seconds
/// - `TORRENT_FUSE_MAX_ENTRIES` - Maximum cache entries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    pub metadata_ttl: u64,
    pub torrent_list_ttl: u64,
    pub piece_ttl: u64,
    pub max_entries: usize,
}

/// Configuration for FUSE mount options.
///
/// Controls where and how the filesystem is mounted.
///
/// # Fields
///
/// * `mount_point` - Directory to mount the FUSE filesystem (default: `/mnt/torrents`)
/// * `allow_other` - Allow other users to access the mounted filesystem (default: false)
/// * `auto_unmount` - Automatically unmount on process exit (default: true)
/// * `uid` - User ID for file ownership (default: current user's EUID)
/// * `gid` - Group ID for file ownership (default: current user's EGID)
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_MOUNT_POINT` - Mount point directory path
/// - `TORRENT_FUSE_ALLOW_OTHER` - Boolean to allow other users access
/// - `TORRENT_FUSE_AUTO_UNMOUNT` - Boolean to enable auto unmount
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MountConfig {
    pub mount_point: PathBuf,
    pub allow_other: bool,
    pub auto_unmount: bool,
    pub uid: u32,
    pub gid: u32,
}

/// Configuration for performance-related settings.
///
/// Controls read timeouts, concurrency limits, and prefetching behavior.
///
/// # Fields
///
/// * `read_timeout` - Timeout for read operations in seconds (default: 30)
/// * `max_concurrent_reads` - Maximum concurrent read operations (default: 10)
/// * `readahead_size` - Size of read-ahead buffer in bytes (default: 32 MiB)
/// * `piece_check_enabled` - Enable piece verification checksums (default: true)
/// * `return_eagain_for_unavailable` - Return EAGAIN when data is unavailable (default: false)
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_READ_TIMEOUT` - Read timeout in seconds
/// - `TORRENT_FUSE_MAX_CONCURRENT_READS` - Maximum concurrent reads
/// - `TORRENT_FUSE_READAHEAD_SIZE` - Read-ahead buffer size in bytes
/// - `TORRENT_FUSE_PIECE_CHECK_ENABLED` - Enable piece verification
/// - `TORRENT_FUSE_RETURN_EAGAIN` - Return EAGAIN for unavailable data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceConfig {
    pub read_timeout: u64,
    pub max_concurrent_reads: usize,
    pub readahead_size: u64,
    pub piece_check_enabled: bool,
    pub return_eagain_for_unavailable: bool,
}

/// Configuration for monitoring and status polling.
///
/// Controls how often the filesystem polls for torrent status updates.
///
/// # Fields
///
/// * `status_poll_interval` - Interval between status polls in seconds (default: 5)
/// * `stalled_timeout` - Timeout in seconds before marking a torrent as stalled (default: 300)
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_STATUS_POLL_INTERVAL` - Status poll interval in seconds
/// - `TORRENT_FUSE_STALLED_TIMEOUT` - Stall timeout in seconds
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitoringConfig {
    pub status_poll_interval: u64,
    pub stalled_timeout: u64,
}

/// Configuration for logging and metrics output.
///
/// Controls log verbosity, operation logging, and metrics collection.
///
/// # Fields
///
/// * `level` - Log level: error, warn, info, debug, or trace (default: "info")
/// * `log_fuse_operations` - Log all FUSE operations (default: true)
/// * `log_api_calls` - Log all API calls to rqbit (default: true)
/// * `metrics_enabled` - Enable metrics collection and logging (default: true)
/// * `metrics_interval_secs` - Interval between metrics logs in seconds (default: 60)
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_LOG_LEVEL` - Log level (error|warn|info|debug|trace)
/// - `TORRENT_FUSE_LOG_FUSE_OPS` - Boolean to enable FUSE operation logging
/// - `TORRENT_FUSE_LOG_API_CALLS` - Boolean to enable API call logging
/// - `TORRENT_FUSE_METRICS_ENABLED` - Boolean to enable metrics
/// - `TORRENT_FUSE_METRICS_INTERVAL` - Metrics log interval in seconds
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub log_fuse_operations: bool,
    pub log_api_calls: bool,
    pub metrics_enabled: bool,
    pub metrics_interval_secs: u64,
}

default_fn!(default_api_url, String, "http://127.0.0.1:3030".to_string());
default_fn!(default_metadata_ttl, u64, 60);
default_fn!(default_torrent_list_ttl, u64, 30);
default_fn!(default_piece_ttl, u64, 5);
default_fn!(default_max_entries, usize, 1000);
default_fn!(default_mount_point, PathBuf, PathBuf::from("/mnt/torrents"));
default_fn!(default_allow_other, bool, false);
default_fn!(default_auto_unmount, bool, true);
default_fn!(default_uid, u32, unsafe { libc::geteuid() });
default_fn!(default_gid, u32, unsafe { libc::getegid() });
default_fn!(default_read_timeout, u64, 30);
default_fn!(default_max_concurrent_reads, usize, 10);
default_fn!(default_readahead_size, u64, 33554432);
default_fn!(default_piece_check_enabled, bool, true);
default_fn!(default_return_eagain_for_unavailable, bool, false);
default_fn!(default_status_poll_interval, u64, 5);
default_fn!(default_stalled_timeout, u64, 300);
default_fn!(default_log_level, String, "info".to_string());
default_fn!(default_log_fuse_operations, bool, true);
default_fn!(default_log_api_calls, bool, true);
default_fn!(default_metrics_enabled, bool, true);
default_fn!(default_metrics_interval_secs, u64, 60);
default_fn!(default_none, Option<String>, None);

default_impl!(ApiConfig, url: default_api_url, username: default_none, password: default_none);
default_impl!(CacheConfig, metadata_ttl: default_metadata_ttl, torrent_list_ttl: default_torrent_list_ttl, piece_ttl: default_piece_ttl, max_entries: default_max_entries);
default_impl!(MountConfig, mount_point: default_mount_point, allow_other: default_allow_other, auto_unmount: default_auto_unmount, uid: default_uid, gid: default_gid);
default_impl!(PerformanceConfig, read_timeout: default_read_timeout, max_concurrent_reads: default_max_concurrent_reads, readahead_size: default_readahead_size, piece_check_enabled: default_piece_check_enabled, return_eagain_for_unavailable: default_return_eagain_for_unavailable);
default_impl!(MonitoringConfig, status_poll_interval: default_status_poll_interval, stalled_timeout: default_stalled_timeout);
default_impl!(LoggingConfig, level: default_log_level, log_fuse_operations: default_log_fuse_operations, log_api_calls: default_log_api_calls, metrics_enabled: default_metrics_enabled, metrics_interval_secs: default_metrics_interval_secs);

/// Errors that can occur during configuration loading or validation.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    ParseError(String),
    #[error("Invalid config value: {0}")]
    InvalidValue(String),
    #[error("Validation error: {}", .0.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("; "))]
    ValidationError(Vec<ValidationIssue>),
}

/// Represents a single validation error in the configuration.
///
/// Contains the field name that failed validation and a description of the issue.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationIssue {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_file(path: &PathBuf) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        match ext.as_deref() {
            Some("json") => {
                serde_json::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))
            }
            _ => toml::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string())),
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
        env_var!("TORRENT_FUSE_API_URL", self.api.url);
        env_var!(
            "TORRENT_FUSE_MOUNT_POINT",
            self.mount.mount_point,
            |v| Ok::<_, std::convert::Infallible>(PathBuf::from(v))
        );
        env_var!(
            "TORRENT_FUSE_METADATA_TTL",
            self.cache.metadata_ttl,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_TORRENT_LIST_TTL",
            self.cache.torrent_list_ttl,
            str::parse
        );
        env_var!("TORRENT_FUSE_PIECE_TTL", self.cache.piece_ttl, str::parse);
        env_var!(
            "TORRENT_FUSE_MAX_ENTRIES",
            self.cache.max_entries,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_READ_TIMEOUT",
            self.performance.read_timeout,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_MAX_CONCURRENT_READS",
            self.performance.max_concurrent_reads,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_READAHEAD_SIZE",
            self.performance.readahead_size,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_ALLOW_OTHER",
            self.mount.allow_other,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_AUTO_UNMOUNT",
            self.mount.auto_unmount,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_STATUS_POLL_INTERVAL",
            self.monitoring.status_poll_interval,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_STALLED_TIMEOUT",
            self.monitoring.stalled_timeout,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_PIECE_CHECK_ENABLED",
            self.performance.piece_check_enabled,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_RETURN_EAGAIN",
            self.performance.return_eagain_for_unavailable,
            str::parse
        );
        env_var!("TORRENT_FUSE_LOG_LEVEL", self.logging.level);
        env_var!(
            "TORRENT_FUSE_LOG_FUSE_OPS",
            self.logging.log_fuse_operations,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_LOG_API_CALLS",
            self.logging.log_api_calls,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_METRICS_ENABLED",
            self.logging.metrics_enabled,
            str::parse
        );
        env_var!(
            "TORRENT_FUSE_METRICS_INTERVAL",
            self.logging.metrics_interval_secs,
            str::parse
        );

        // Auth credentials - support both individual fields and combined format
        if let Ok(auth_str) = std::env::var("TORRENT_FUSE_AUTH_USERPASS") {
            // Combined format: "username:password"
            if let Some((username, password)) = auth_str.split_once(':') {
                self.api.username = Some(username.to_string());
                self.api.password = Some(password.to_string());
            }
        } else {
            // Individual fields
            if let Ok(val) = std::env::var("TORRENT_FUSE_AUTH_USERNAME") {
                self.api.username = Some(val);
            }
            if let Ok(val) = std::env::var("TORRENT_FUSE_AUTH_PASSWORD") {
                self.api.password = Some(val);
            }
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

        if let Some(ref username) = cli.username {
            self.api.username = Some(username.clone());
        }

        if let Some(ref password) = cli.password {
            self.api.password = Some(password.clone());
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

    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut issues = Vec::new();

        if let Err(e) = self.validate_api_config() {
            issues.push(e);
        }
        if let Err(e) = self.validate_cache_config() {
            issues.push(e);
        }
        if let Err(e) = self.validate_mount_config() {
            issues.push(e);
        }
        if let Err(e) = self.validate_performance_config() {
            issues.push(e);
        }
        if let Err(e) = self.validate_monitoring_config() {
            issues.push(e);
        }
        if let Err(e) = self.validate_logging_config() {
            issues.push(e);
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::ValidationError(issues))
        }
    }

    fn validate_api_config(&self) -> Result<(), ValidationIssue> {
        if self.api.url.is_empty() {
            return Err(ValidationIssue {
                field: "api.url".to_string(),
                message: "URL cannot be empty".to_string(),
            });
        }

        if let Err(e) = reqwest::Url::parse(&self.api.url) {
            return Err(ValidationIssue {
                field: "api.url".to_string(),
                message: format!("Invalid URL format: {}", e),
            });
        }

        Ok(())
    }

    fn validate_cache_config(&self) -> Result<(), ValidationIssue> {
        if self.cache.metadata_ttl == 0 {
            return Err(ValidationIssue {
                field: "cache.metadata_ttl".to_string(),
                message: "TTL must be greater than 0".to_string(),
            });
        }

        if self.cache.metadata_ttl > 86400 {
            return Err(ValidationIssue {
                field: "cache.metadata_ttl".to_string(),
                message: "TTL exceeds maximum of 86400 seconds (24 hours)".to_string(),
            });
        }

        if self.cache.torrent_list_ttl == 0 {
            return Err(ValidationIssue {
                field: "cache.torrent_list_ttl".to_string(),
                message: "TTL must be greater than 0".to_string(),
            });
        }

        if self.cache.piece_ttl == 0 {
            return Err(ValidationIssue {
                field: "cache.piece_ttl".to_string(),
                message: "TTL must be greater than 0".to_string(),
            });
        }

        if self.cache.max_entries == 0 {
            return Err(ValidationIssue {
                field: "cache.max_entries".to_string(),
                message: "max_entries must be greater than 0".to_string(),
            });
        }

        if self.cache.max_entries > 1000000 {
            return Err(ValidationIssue {
                field: "cache.max_entries".to_string(),
                message: "max_entries exceeds maximum of 1000000".to_string(),
            });
        }

        Ok(())
    }

    fn validate_mount_config(&self) -> Result<(), ValidationIssue> {
        if !self.mount.mount_point.is_absolute() {
            return Err(ValidationIssue {
                field: "mount.mount_point".to_string(),
                message: "Mount point must be an absolute path".to_string(),
            });
        }

        if self.mount.mount_point.exists() && !self.mount.mount_point.is_dir() {
            return Err(ValidationIssue {
                field: "mount.mount_point".to_string(),
                message: "Mount point exists but is not a directory".to_string(),
            });
        }

        if u64::from(self.mount.uid) > u64::from(u32::MAX) {
            return Err(ValidationIssue {
                field: "mount.uid".to_string(),
                message: "UID exceeds maximum value".to_string(),
            });
        }

        if u64::from(self.mount.gid) > u64::from(u32::MAX) {
            return Err(ValidationIssue {
                field: "mount.gid".to_string(),
                message: "GID exceeds maximum value".to_string(),
            });
        }

        Ok(())
    }

    fn validate_performance_config(&self) -> Result<(), ValidationIssue> {
        if self.performance.read_timeout == 0 {
            return Err(ValidationIssue {
                field: "performance.read_timeout".to_string(),
                message: "Read timeout must be greater than 0".to_string(),
            });
        }

        if self.performance.read_timeout > 3600 {
            return Err(ValidationIssue {
                field: "performance.read_timeout".to_string(),
                message: "Read timeout exceeds maximum of 3600 seconds (1 hour)".to_string(),
            });
        }

        if self.performance.max_concurrent_reads == 0 {
            return Err(ValidationIssue {
                field: "performance.max_concurrent_reads".to_string(),
                message: "max_concurrent_reads must be greater than 0".to_string(),
            });
        }

        if self.performance.max_concurrent_reads > 1000 {
            return Err(ValidationIssue {
                field: "performance.max_concurrent_reads".to_string(),
                message: "max_concurrent_reads exceeds maximum of 1000".to_string(),
            });
        }

        if self.performance.readahead_size == 0 {
            return Err(ValidationIssue {
                field: "performance.readahead_size".to_string(),
                message: "readahead_size must be greater than 0".to_string(),
            });
        }

        if self.performance.readahead_size > 1073741824 {
            return Err(ValidationIssue {
                field: "performance.readahead_size".to_string(),
                message: "readahead_size exceeds maximum of 1GB".to_string(),
            });
        }

        Ok(())
    }

    fn validate_monitoring_config(&self) -> Result<(), ValidationIssue> {
        if self.monitoring.status_poll_interval == 0 {
            return Err(ValidationIssue {
                field: "monitoring.status_poll_interval".to_string(),
                message: "status_poll_interval must be greater than 0".to_string(),
            });
        }

        if self.monitoring.status_poll_interval > 3600 {
            return Err(ValidationIssue {
                field: "monitoring.status_poll_interval".to_string(),
                message: "status_poll_interval exceeds maximum of 3600 seconds (1 hour)"
                    .to_string(),
            });
        }

        if self.monitoring.stalled_timeout == 0 {
            return Err(ValidationIssue {
                field: "monitoring.stalled_timeout".to_string(),
                message: "stalled_timeout must be greater than 0".to_string(),
            });
        }

        if self.monitoring.stalled_timeout > 86400 {
            return Err(ValidationIssue {
                field: "monitoring.stalled_timeout".to_string(),
                message: "stalled_timeout exceeds maximum of 86400 seconds (24 hours)".to_string(),
            });
        }

        if self.monitoring.status_poll_interval > self.monitoring.stalled_timeout {
            return Err(ValidationIssue {
                field: "monitoring.status_poll_interval".to_string(),
                message: "status_poll_interval must be less than or equal to stalled_timeout"
                    .to_string(),
            });
        }

        Ok(())
    }

    fn validate_logging_config(&self) -> Result<(), ValidationIssue> {
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(ValidationIssue {
                field: "logging.level".to_string(),
                message: format!(
                    "Invalid log level '{}'. Valid levels: {}",
                    self.logging.level,
                    valid_levels.join(", ")
                ),
            });
        }

        if self.logging.metrics_interval_secs == 0 && self.logging.metrics_enabled {
            return Err(ValidationIssue {
                field: "logging.metrics_interval_secs".to_string(),
                message:
                    "metrics_interval_secs must be greater than 0 when metrics_enabled is true"
                        .to_string(),
            });
        }

        if self.logging.metrics_interval_secs > 86400 {
            return Err(ValidationIssue {
                field: "logging.metrics_interval_secs".to_string(),
                message: "metrics_interval_secs exceeds maximum of 86400 seconds (24 hours)"
                    .to_string(),
            });
        }

        Ok(())
    }
}

/// Command-line arguments that override configuration values.
///
/// These values take precedence over config files and environment variables.
///
/// # Fields
///
/// * `api_url` - Override the rqbit API URL
/// * `mount_point` - Override the FUSE mount point
/// * `config_file` - Path to a config file to load
/// * `username` - Username for HTTP Basic authentication
/// * `password` - Password for HTTP Basic authentication
#[derive(Debug, Clone, Default)]
pub struct CliArgs {
    /// Override the rqbit API URL (e.g., "http://localhost:3030")
    pub api_url: Option<String>,
    /// Override the FUSE mount point (must be absolute path)
    pub mount_point: Option<PathBuf>,
    /// Path to a config file to load (TOML or JSON)
    pub config_file: Option<PathBuf>,
    /// Username for HTTP Basic authentication
    pub username: Option<String>,
    /// Password for HTTP Basic authentication
    pub password: Option<String>,
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
    fn test_json_uppercase_extension() {
        let json_content = r#"{
            "api": {
                "url": "http://localhost:9091"
            }
        }"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json_content.as_bytes()).unwrap();

        let mut json_path = temp_file.path().to_path_buf();
        json_path.set_extension("JSON");
        std::fs::rename(temp_file.path(), &json_path).unwrap();

        let config = Config::from_file(&json_path).unwrap();
        assert_eq!(config.api.url, "http://localhost:9091");
    }

    #[test]
    fn test_toml_uppercase_extension() {
        let toml_content = r#"
[api]
url = "http://localhost:8082"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let mut toml_path = temp_file.path().to_path_buf();
        toml_path.set_extension("TOML");
        std::fs::rename(temp_file.path(), &toml_path).unwrap();

        let config = Config::from_file(&toml_path).unwrap();
        assert_eq!(config.api.url, "http://localhost:8082");
    }

    #[test]
    fn test_toml_mixed_case_extension() {
        let toml_content = r#"
[api]
url = "http://localhost:8083"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();

        let mut toml_path = temp_file.path().to_path_buf();
        toml_path.set_extension("Toml");
        std::fs::rename(temp_file.path(), &toml_path).unwrap();

        let config = Config::from_file(&toml_path).unwrap();
        assert_eq!(config.api.url, "http://localhost:8083");
    }

    #[test]
    fn test_merge_from_cli() {
        let config = Config::default();
        let cli = CliArgs {
            api_url: Some("http://custom:8080".to_string()),
            mount_point: Some(PathBuf::from("/custom/mount")),
            config_file: None,
            username: None,
            password: None,
        };

        let merged = config.merge_from_cli(&cli);

        assert_eq!(merged.api.url, "http://custom:8080");
        assert_eq!(merged.mount.mount_point, PathBuf::from("/custom/mount"));
    }

    #[test]
    fn test_merge_auth_from_cli() {
        let config = Config::default();
        let cli = CliArgs {
            api_url: None,
            mount_point: None,
            config_file: None,
            username: Some("testuser".to_string()),
            password: Some("testpass".to_string()),
        };

        let merged = config.merge_from_cli(&cli);

        assert_eq!(merged.api.username, Some("testuser".to_string()));
        assert_eq!(merged.api.password, Some("testpass".to_string()));
    }

    #[test]
    fn test_validate_default_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_url() {
        let mut config = Config::default();
        config.api.url = "".to_string();
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::ValidationError(_)));
    }

    #[test]
    fn test_validate_invalid_url() {
        let mut config = Config::default();
        config.api.url = "not-a-url".to_string();
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::ValidationError(_)));
    }

    #[test]
    fn test_validate_zero_metadata_ttl() {
        let mut config = Config::default();
        config.cache.metadata_ttl = 0;
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_exceeds_max_ttl() {
        let mut config = Config::default();
        config.cache.metadata_ttl = 100000;
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_zero_max_entries() {
        let mut config = Config::default();
        config.cache.max_entries = 0;
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_relative_mount_point() {
        let mut config = Config::default();
        config.mount.mount_point = PathBuf::from("relative/path");
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_zero_read_timeout() {
        let mut config = Config::default();
        config.performance.read_timeout = 0;
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_log_level() {
        let mut config = Config::default();
        config.logging.level = "invalid".to_string();
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_valid_log_levels() {
        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        for level in valid_levels {
            let mut config = Config::default();
            config.logging.level = level.to_string();
            assert!(config.validate().is_ok(), "Level {} should be valid", level);
        }
    }

    #[test]
    fn test_validate_poll_greater_than_stalled() {
        let mut config = Config::default();
        config.monitoring.status_poll_interval = 100;
        config.monitoring.stalled_timeout = 50;
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_metrics_disabled_no_interval_required() {
        let mut config = Config::default();
        config.logging.metrics_enabled = false;
        config.logging.metrics_interval_secs = 0;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_max_concurrent_reads() {
        let mut config = Config::default();
        config.performance.max_concurrent_reads = 0;
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_zero_readahead_size() {
        let mut config = Config::default();
        config.performance.readahead_size = 0;
        let result = config.validate();
        assert!(result.is_err());
    }
}
