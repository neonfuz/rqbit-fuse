//! Configuration management for CLI, environment variables, and config files.

use crate::error::{RqbitFuseError, ValidationIssue};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration for rqbit-fuse.
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
    pub logging: LoggingConfig,
}

/// Configuration for the rqbit API connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Configuration for caching behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
    pub metadata_ttl: u64,
    pub max_entries: usize,
}

/// Configuration for FUSE mount options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MountConfig {
    pub mount_point: PathBuf,
}

/// Configuration for performance-related settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PerformanceConfig {
    pub read_timeout: u64,
    pub max_concurrent_reads: usize,
    pub readahead_size: u64,
}

/// Configuration for logging output.
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
            if val.is_empty() || !val.chars().all(|c| c.is_ascii_digit()) {
                return Err(RqbitFuseError::InvalidArgument(
                    "TORRENT_FUSE_READ_TIMEOUT has invalid format".into(),
                ));
            }
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
#[derive(Debug, Clone, Default)]
pub struct CliArgs {
    pub api_url: Option<String>,
    pub mount_point: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    pub username: Option<String>,
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

    fn parse_config_content(content: &str, ext: &str) -> Config {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();
        let mut path = temp_file.path().to_path_buf();
        path.set_extension(ext);
        std::fs::rename(temp_file.path(), &path).unwrap();
        Config::from_file(&path).unwrap()
    }

    #[test]
    fn test_toml_config_parsing() {
        let c = parse_config_content(
            r#"[api]
url = "http://localhost:8080"

[cache]
metadata_ttl = 120
max_entries = 500

[mount]
mount_point = "/tmp/torrents"

[performance]
read_timeout = 60
max_concurrent_reads = 20"#,
            "toml",
        );
        assert_eq!(c.api.url, "http://localhost:8080");
        assert_eq!(c.cache.metadata_ttl, 120);
        assert_eq!(c.cache.max_entries, 500);
        assert_eq!(c.mount.mount_point, PathBuf::from("/tmp/torrents"));
        assert_eq!(c.performance.read_timeout, 60);
        assert_eq!(c.performance.max_concurrent_reads, 20);
    }

    #[test]
    fn test_json_config_parsing() {
        let c = parse_config_content(
            r#"{"api": {"url": "http://localhost:9090"}, "cache": {"metadata_ttl": 90, "max_entries": 500}}"#,
            "json",
        );
        assert_eq!(c.api.url, "http://localhost:9090");
        assert_eq!(c.cache.metadata_ttl, 90);
        assert_eq!(c.cache.max_entries, 500);
    }

    #[rstest::rstest]
    #[case("json", "http://localhost:9091")]
    #[case("JSON", "http://localhost:9091")]
    #[case("toml", "http://localhost:8082")]
    #[case("TOML", "http://localhost:8082")]
    #[case("Toml", "http://localhost:8083")]
    fn test_file_extension_case_handling(#[case] ext: &str, #[case] expected_url: &str) {
        let content = if ext.eq_ignore_ascii_case("json") {
            format!(r#"{{"api": {{"url": "{}"}}}}"#, expected_url)
        } else {
            format!("[api]\nurl = \"{}\"", expected_url)
        };

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();

        let mut path = temp_file.path().to_path_buf();
        path.set_extension(ext);
        std::fs::rename(temp_file.path(), &path).unwrap();

        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.api.url, expected_url);
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

    #[rstest::rstest]
    #[case("error", true)]
    #[case("warn", true)]
    #[case("info", true)]
    #[case("debug", true)]
    #[case("trace", true)]
    #[case("invalid", false)]
    #[case("ERROR", false)]
    fn test_validate_log_level(#[case] level: &str, #[case] should_pass: bool) {
        let mut config = Config::default();
        config.logging.level = level.to_string();
        let result = config.validate();
        if should_pass {
            assert!(result.is_ok(), "Level {} should be valid", level);
        } else {
            assert!(result.is_err(), "Level {} should be invalid", level);
        }
    }
}
