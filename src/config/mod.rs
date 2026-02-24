use crate::error::{RqbitFuseError, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration container for rqbit-fuse.
///
/// Combines all configuration sections (API, cache, mount, performance, logging)
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
/// * `logging` - Log verbosity and metrics settings
///
/// # Example
///
/// ```rust
/// use rqbit_fuse::config::Config;
///
/// let config = Config::load().expect("Failed to load config");
/// config.validate().expect("Invalid configuration");
/// ```
///
/// ## Complete TOML Configuration Example
///
/// ```toml
/// # Basic configuration for rqbit-fuse
/// # Copy this to ~/.config/rqbit-fuse/config.toml or /etc/rqbit-fuse/config.toml
///
/// [api]
/// url = "http://127.0.0.1:3030"
/// # Optional: HTTP Basic authentication
/// # username = "admin"
/// # password = "secret"
///
/// [cache]
/// # How long to cache data (seconds)
/// metadata_ttl = 60
/// # Maximum number of cache entries
/// max_entries = 1000
///
/// [mount]
/// # Where to mount the FUSE filesystem
/// mount_point = "/mnt/torrents"
///
/// [performance]
/// # Timeout for read operations (seconds)
/// read_timeout = 30
/// # Maximum concurrent read operations
/// max_concurrent_reads = 10
/// # Read-ahead buffer size (bytes)
/// readahead_size = 33554432
///
/// [logging]
/// # Log level: error, warn, info, debug, trace
/// level = "info"
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
///     "max_entries": 1000
///   },
///   "mount": {
///     "mount_point": "/mnt/torrents"
///   },
///   "performance": {
///     "read_timeout": 30,
///     "max_concurrent_reads": 10,
///     "readahead_size": 33554432
///   },
///   "logging": {
///     "level": "info"
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
/// # Set read timeout
/// export TORRENT_FUSE_READ_TIMEOUT=30
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
/// Controls TTL (time-to-live) and capacity limits for cached data.
///
/// # Fields
///
/// * `metadata_ttl` - How long to cache data in seconds (default: 60)
/// * `max_entries` - Maximum number of entries in the cache (default: 1000)
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_METADATA_TTL` - Cache TTL in seconds
/// - `TORRENT_FUSE_MAX_ENTRIES` - Maximum cache entries
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    pub metadata_ttl: u64,
    pub max_entries: usize,
}

/// Configuration for FUSE mount options.
///
/// Controls where the filesystem is mounted.
///
/// # Fields
///
/// * `mount_point` - Directory to mount the FUSE filesystem (default: `/mnt/torrents`)
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_MOUNT_POINT` - Mount point directory path
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MountConfig {
    pub mount_point: PathBuf,
}

/// Configuration for performance-related settings.
///
/// Controls read timeouts, concurrency limits, and read-ahead behavior.
///
/// # Fields
///
/// * `read_timeout` - Timeout for read operations in seconds (default: 30)
/// * `max_concurrent_reads` - Maximum concurrent read operations (default: 10)
/// * `readahead_size` - Size of read-ahead buffer in bytes (default: 32 MiB)
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_READ_TIMEOUT` - Read timeout in seconds
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceConfig {
    pub read_timeout: u64,
    pub max_concurrent_reads: usize,
    pub readahead_size: u64,
}

/// Configuration for logging output.
///
/// Controls log verbosity.
///
/// # Fields
///
/// * `level` - Log level: error, warn, info, debug, or trace (default: "info")
///
/// # Environment Variables
///
/// - `TORRENT_FUSE_LOG_LEVEL` - Log level (error|warn|info|debug|trace)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:3030".to_string(),
            username: None,
            password: None,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            metadata_ttl: 60,
            max_entries: 1000,
        }
    }
}

impl Default for MountConfig {
    fn default() -> Self {
        Self {
            mount_point: PathBuf::from("/mnt/torrents"),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            read_timeout: 30,
            max_concurrent_reads: 10,
            readahead_size: 33554432,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
        }
    }
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_file(path: &PathBuf) -> Result<Self, RqbitFuseError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| RqbitFuseError::IoError(e.to_string()))?;

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        match ext.as_deref() {
            Some("json") => serde_json::from_str(&content)
                .map_err(|e| RqbitFuseError::ParseError(e.to_string())),
            _ => toml::from_str(&content).map_err(|e| RqbitFuseError::ParseError(e.to_string())),
        }
    }

    pub fn from_default_locations() -> Result<Self, RqbitFuseError> {
        let config_dirs = [
            dirs::config_dir().map(|d| d.join("rqbit-fuse/config.toml")),
            Some(PathBuf::from("/etc/rqbit-fuse/config.toml")),
            Some(PathBuf::from("./rqbit-fuse.toml")),
        ];

        for path in config_dirs.iter().flatten() {
            if path.exists() {
                tracing::info!("Loading config from: {}", path.display());
                return Self::from_file(path);
            }
        }

        Ok(Self::default())
    }

    pub fn merge_from_env(mut self) -> Result<Self, RqbitFuseError> {
        if let Ok(val) = std::env::var("TORRENT_FUSE_API_URL") {
            self.api.url = val;
        }
        if let Ok(val) = std::env::var("TORRENT_FUSE_MOUNT_POINT") {
            self.mount.mount_point = PathBuf::from(val);
        }
        if let Ok(val) = std::env::var("TORRENT_FUSE_METADATA_TTL") {
            self.cache.metadata_ttl = val.parse().map_err(|_| {
                RqbitFuseError::InvalidArgument(
                    "TORRENT_FUSE_METADATA_TTL has invalid format".into(),
                )
            })?;
        }
        if let Ok(val) = std::env::var("TORRENT_FUSE_MAX_ENTRIES") {
            self.cache.max_entries = val.parse().map_err(|_| {
                RqbitFuseError::InvalidArgument(
                    "TORRENT_FUSE_MAX_ENTRIES has invalid format".into(),
                )
            })?;
        }
        if let Ok(val) = std::env::var("TORRENT_FUSE_READ_TIMEOUT") {
            self.performance.read_timeout = val.parse().map_err(|_| {
                RqbitFuseError::InvalidArgument(
                    "TORRENT_FUSE_READ_TIMEOUT has invalid format".into(),
                )
            })?;
        }

        if let Ok(val) = std::env::var("TORRENT_FUSE_LOG_LEVEL") {
            self.logging.level = val;
        }

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

    pub fn load() -> Result<Self, RqbitFuseError> {
        Self::from_default_locations()?.merge_from_env()
    }

    pub fn load_with_cli(cli: &CliArgs) -> Result<Self, RqbitFuseError> {
        Ok(Self::from_default_locations()?
            .merge_from_env()?
            .merge_from_cli(cli))
    }

    pub fn validate(&self) -> Result<(), RqbitFuseError> {
        let mut issues = Vec::new();

        if self.api.url.is_empty() {
            issues.push(ValidationIssue {
                field: "api.url".to_string(),
                message: "URL cannot be empty".to_string(),
            });
        } else if let Err(e) = reqwest::Url::parse(&self.api.url) {
            issues.push(ValidationIssue {
                field: "api.url".to_string(),
                message: format!("Invalid URL format: {}", e),
            });
        }

        if !self.mount.mount_point.is_absolute() {
            issues.push(ValidationIssue {
                field: "mount.mount_point".to_string(),
                message: "Mount point must be an absolute path".to_string(),
            });
        }

        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            issues.push(ValidationIssue {
                field: "logging.level".to_string(),
                message: format!(
                    "Invalid log level '{}'. Valid levels: {}",
                    self.logging.level,
                    valid_levels.join(", ")
                ),
            });
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(RqbitFuseError::ValidationError(issues))
        }
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
                "max_entries": 500
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
        assert_eq!(config.cache.max_entries, 500);
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
        assert!(matches!(err, RqbitFuseError::ValidationError(_)));
    }

    #[test]
    fn test_validate_invalid_url() {
        let mut config = Config::default();
        config.api.url = "not-a-url".to_string();
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, RqbitFuseError::ValidationError(_)));
    }

    #[test]
    fn test_validate_url_without_scheme() {
        // After simplification, any parseable URL is accepted
        // "localhost:3030" is treated as a valid URL with "localhost" as scheme
        let mut config = Config::default();
        config.api.url = "localhost:3030".to_string();
        let result = config.validate();
        assert!(
            result.is_ok(),
            "URL without explicit scheme should be valid after simplification"
        );
    }

    #[test]
    fn test_validate_url_with_non_http_scheme() {
        // After simplification, any valid URL scheme is accepted
        let mut config = Config::default();
        config.api.url = "ftp://localhost:3030".to_string();
        let result = config.validate();
        assert!(
            result.is_ok(),
            "URL with non-http scheme should be valid after simplification"
        );
    }

    #[test]
    fn test_validate_relative_mount_point() {
        let mut config = Config::default();
        config.mount.mount_point = PathBuf::from("relative/path");
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
}
