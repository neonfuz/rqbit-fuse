use crate::error::RqbitFuseError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration for rqbit-fuse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // API settings
    #[serde(default = "default_api_url")]
    pub api_url: String,
    #[serde(default)]
    pub api_username: Option<String>,
    #[serde(default)]
    pub api_password: Option<String>,

    // Cache settings
    #[serde(default = "default_metadata_ttl")]
    pub metadata_ttl: u64,
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,

    // Mount settings
    #[serde(default = "default_mount_point")]
    pub mount_point: PathBuf,

    // Performance settings
    #[serde(default = "default_read_timeout")]
    pub read_timeout: u64,
    #[serde(default = "default_max_concurrent_reads")]
    pub max_concurrent_reads: usize,
    #[serde(default = "default_readahead_size")]
    pub readahead_size: u64,

    // Logging settings
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

// Default value functions for serde
fn default_api_url() -> String {
    "http://127.0.0.1:3030".to_string()
}

fn default_metadata_ttl() -> u64 {
    60
}

fn default_max_entries() -> usize {
    1000
}

fn default_mount_point() -> PathBuf {
    PathBuf::from("/mnt/torrents")
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

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_url: default_api_url(),
            api_username: None,
            api_password: None,
            metadata_ttl: default_metadata_ttl(),
            max_entries: default_max_entries(),
            mount_point: default_mount_point(),
            read_timeout: default_read_timeout(),
            max_concurrent_reads: default_max_concurrent_reads(),
            readahead_size: default_readahead_size(),
            log_level: default_log_level(),
        }
    }
}

macro_rules! merge_env_var {
    ($self:ident, $field:ident, $var:expr) => {
        if let Ok(val) = std::env::var($var) {
            $self.$field = val;
        }
    };
    ($self:ident, $field:ident, $var:expr, |$v:ident| $parser:expr) => {
        if let Ok(val) = std::env::var($var) {
            $self.$field = (|$v: &str| $parser)(&val).map_err(|_| {
                RqbitFuseError::InvalidArgument(format!("{} has invalid format", $var))
            })?;
        }
    };
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn merge_from_env(mut self) -> Result<Self, RqbitFuseError> {
        merge_env_var!(self, api_url, "TORRENT_FUSE_API_URL");
        merge_env_var!(
            self,
            mount_point,
            "TORRENT_FUSE_MOUNT_POINT",
            |v| Ok::<_, ()>(PathBuf::from(v))
        );
        merge_env_var!(self, metadata_ttl, "TORRENT_FUSE_METADATA_TTL", |v| v
            .parse::<u64>());
        merge_env_var!(self, max_entries, "TORRENT_FUSE_MAX_ENTRIES", |v| v
            .parse::<usize>());
        if let Ok(val) = std::env::var("TORRENT_FUSE_READ_TIMEOUT") {
            if !val.chars().all(|c| c.is_ascii_digit()) {
                return Err(RqbitFuseError::InvalidArgument(
                    "TORRENT_FUSE_READ_TIMEOUT has invalid format".into(),
                ));
            }
            self.read_timeout = val.parse().map_err(|_| {
                RqbitFuseError::InvalidArgument(
                    "TORRENT_FUSE_READ_TIMEOUT has invalid format".into(),
                )
            })?;
        }
        merge_env_var!(self, log_level, "TORRENT_FUSE_LOG_LEVEL");

        // Auth credentials - support both individual fields and combined format
        if let Ok(auth_str) = std::env::var("TORRENT_FUSE_AUTH_USERPASS") {
            if let Some((username, password)) = auth_str.split_once(':') {
                self.api_username = Some(username.to_string());
                self.api_password = Some(password.to_string());
            }
        } else {
            merge_env_var!(self, api_username, "TORRENT_FUSE_AUTH_USERNAME", |v| Ok::<
                _,
                (),
            >(
                Some(v.to_string())
            ));
            merge_env_var!(self, api_password, "TORRENT_FUSE_AUTH_PASSWORD", |v| Ok::<
                _,
                (),
            >(
                Some(v.to_string())
            ));
        }

        Ok(self)
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
        [
            dirs::config_dir().map(|d| d.join("rqbit-fuse/config.toml")),
            Some(PathBuf::from("/etc/rqbit-fuse/config.toml")),
            Some(PathBuf::from("./rqbit-fuse.toml")),
        ]
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .map(|p| Self::from_file(&p))
        .transpose()
        .map(|opt| opt.unwrap_or_default())
    }

    pub fn merge_from_cli(mut self, cli: &CliArgs) -> Self {
        if let Some(ref url) = cli.api_url {
            self.api_url = url.clone();
        }

        if let Some(ref mount_point) = cli.mount_point {
            self.mount_point = mount_point.clone();
        }

        if let Some(ref username) = cli.username {
            self.api_username = Some(username.clone());
        }

        if let Some(ref password) = cli.password {
            self.api_password = Some(password.clone());
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
        if self.api_url.is_empty() {
            return Err(RqbitFuseError::ValidationError(vec![
                "api_url: URL cannot be empty".to_string(),
            ]));
        }

        if let Err(e) = reqwest::Url::parse(&self.api_url) {
            return Err(RqbitFuseError::ValidationError(vec![format!(
                "api_url: Invalid URL format: {}",
                e
            )]));
        }

        if !self.mount_point.is_absolute() {
            return Err(RqbitFuseError::ValidationError(vec![
                "mount_point: Mount point must be an absolute path".to_string(),
            ]));
        }

        let valid_levels = ["error", "warn", "info", "debug", "trace"];
        if !valid_levels.contains(&self.log_level.as_str()) {
            return Err(RqbitFuseError::ValidationError(vec![format!(
                "log_level: Invalid log level '{}'. Valid levels: {}",
                self.log_level,
                valid_levels.join(", ")
            )]));
        }

        Ok(())
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
        assert_eq!(config.api_url, "http://127.0.0.1:3030");
        assert_eq!(config.metadata_ttl, 60);
        assert_eq!(config.max_entries, 1000);
        assert_eq!(config.mount_point, PathBuf::from("/mnt/torrents"));
        assert_eq!(config.read_timeout, 30);
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
            r#"api_url = "http://localhost:8080"
metadata_ttl = 120
max_entries = 500
mount_point = "/tmp/torrents"
read_timeout = 60
max_concurrent_reads = 20"#,
            "toml",
        );
        assert_eq!(c.api_url, "http://localhost:8080");
        assert_eq!(c.metadata_ttl, 120);
        assert_eq!(c.max_entries, 500);
        assert_eq!(c.mount_point, PathBuf::from("/tmp/torrents"));
        assert_eq!(c.read_timeout, 60);
        assert_eq!(c.max_concurrent_reads, 20);
    }

    #[test]
    fn test_json_config_parsing() {
        let c = parse_config_content(
            r#"{"api_url": "http://localhost:9090", "metadata_ttl": 90, "max_entries": 500}"#,
            "json",
        );
        assert_eq!(c.api_url, "http://localhost:9090");
        assert_eq!(c.metadata_ttl, 90);
        assert_eq!(c.max_entries, 500);
    }

    #[rstest::rstest]
    #[case("json", "http://localhost:9091")]
    #[case("JSON", "http://localhost:9091")]
    #[case("toml", "http://localhost:8082")]
    #[case("TOML", "http://localhost:8082")]
    #[case("Toml", "http://localhost:8083")]
    fn test_file_extension_case_handling(#[case] ext: &str, #[case] expected_url: &str) {
        let content = if ext.eq_ignore_ascii_case("json") {
            format!(r#"{{"api_url": "{}"}}"#, expected_url)
        } else {
            format!("api_url = \"{}\"", expected_url)
        };

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(content.as_bytes()).unwrap();

        let mut path = temp_file.path().to_path_buf();
        path.set_extension(ext);
        std::fs::rename(temp_file.path(), &path).unwrap();

        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.api_url, expected_url);
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

        assert_eq!(merged.api_url, "http://custom:8080");
        assert_eq!(merged.mount_point, PathBuf::from("/custom/mount"));
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

        assert_eq!(merged.api_username, Some("testuser".to_string()));
        assert_eq!(merged.api_password, Some("testpass".to_string()));
    }

    #[test]
    fn test_validate_default_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_url() {
        let mut config = Config::default();
        config.api_url = "".to_string();
        let result = config.validate();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, RqbitFuseError::ValidationError(_)));
    }

    #[test]
    fn test_validate_invalid_url() {
        let mut config = Config::default();
        config.api_url = "not-a-url".to_string();
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
        config.api_url = "localhost:3030".to_string();
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
        config.api_url = "ftp://localhost:3030".to_string();
        let result = config.validate();
        assert!(
            result.is_ok(),
            "URL with non-http scheme should be valid after simplification"
        );
    }

    #[test]
    fn test_validate_relative_mount_point() {
        let mut config = Config::default();
        config.mount_point = PathBuf::from("relative/path");
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
        config.log_level = level.to_string();
        let result = config.validate();
        if should_pass {
            assert!(result.is_ok(), "Level {} should be valid", level);
        } else {
            assert!(result.is_err(), "Level {} should be invalid", level);
        }
    }
}
