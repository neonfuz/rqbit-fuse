use std::fmt;
use thiserror::Error;

/// Reason why data is unavailable
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataUnavailableReason {
    /// Torrent is paused and pieces haven't been downloaded
    Paused,
    /// Requested pieces haven't been downloaded yet
    NotDownloaded,
}

impl fmt::Display for DataUnavailableReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataUnavailableReason::Paused => write!(f, "torrent is paused"),
            DataUnavailableReason::NotDownloaded => write!(f, "pieces not downloaded"),
        }
    }
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

/// Unified error type for rqbit-fuse.
///
/// This enum consolidates all error types in the codebase:
/// - FUSE operation errors (previously FuseError)
/// - API errors (previously ApiError)
/// - Configuration errors (previously ConfigError)
///
/// Using a single error type eliminates duplicate error mappings and provides
/// consistent error handling throughout the codebase.
#[derive(Error, Debug, Clone)]
pub enum RqbitFuseError {
    // === Not Found Errors ===
    /// Entity not found (ENOENT)
    #[error("Not found")]
    NotFound,

    /// Torrent not found
    #[error("Torrent not found: {0}")]
    TorrentNotFound(u64),

    /// File not found in torrent
    #[error("File not found in torrent {torrent_id}: file_idx={file_idx}")]
    FileNotFound { torrent_id: u64, file_idx: usize },

    // === Permission/Auth Errors ===
    /// Permission denied (EACCES)
    #[error("Permission denied")]
    PermissionDenied,

    /// Authentication failed
    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    // === Timeout Errors ===
    /// Operation timed out (ETIMEDOUT)
    #[error("Operation timed out")]
    TimedOut,

    /// Connection timeout - rqbit server not responding
    #[error("Connection timeout - rqbit server not responding")]
    ConnectionTimeout,

    /// Read timeout - request took too long
    #[error("Read timeout - request took too long")]
    ReadTimeout,

    // === I/O Errors ===
    /// Input/output error (EIO)
    #[error("I/O error: {0}")]
    IoError(String),

    /// Failed to read file
    #[error("Failed to read config file: {0}")]
    ReadError(String),

    // === Network/API Errors ===
    /// HTTP request failed
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    /// rqbit server disconnected
    #[error("rqbit server disconnected")]
    ServerDisconnected,

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Service unavailable
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Circuit breaker open - too many failures
    #[error("Circuit breaker open - too many failures")]
    CircuitBreakerOpen,

    /// API returned error
    #[error("API returned error: {status} - {message}")]
    ApiError { status: u16, message: String },

    /// Failed to initialize HTTP client
    #[error("Failed to initialize HTTP client: {0}")]
    ClientInitializationError(String),

    /// Failed to clone HTTP request
    #[error("Failed to clone HTTP request: {0}")]
    RequestCloneError(String),

    // === Validation Errors ===
    /// Invalid argument (EINVAL)
    #[error("Invalid argument")]
    InvalidArgument,

    /// Invalid range request
    #[error("Invalid range request: {0}")]
    InvalidRange(String),

    /// Invalid config value
    #[error("Invalid config value: {0}")]
    InvalidValue(String),

    /// Validation error with multiple issues
    #[error("Validation error: {}", .0.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("; "))]
    ValidationError(Vec<ValidationIssue>),

    // === Resource Errors ===
    /// Resource temporarily unavailable (EAGAIN)
    #[error("Resource temporarily unavailable")]
    NotReady,

    /// Device or resource busy (EBUSY)
    #[error("Device or resource busy")]
    DeviceBusy,

    /// Channel is full, cannot send request
    #[error("Request channel is full")]
    ChannelFull,

    // === State Errors ===
    /// Worker has disconnected
    #[error("Async worker disconnected")]
    WorkerDisconnected,

    /// Retry limit exceeded
    #[error("Retry limit exceeded")]
    RetryLimitExceeded,

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Failed to parse config file
    #[error("Failed to parse config file: {0}")]
    ParseError(String),

    // === Directory Errors ===
    /// Is a directory (EISDIR)
    #[error("Is a directory")]
    IsDirectory,

    /// Not a directory (ENOTDIR)
    #[error("Not a directory")]
    NotDirectory,

    // === Filesystem Errors ===
    /// Read-only filesystem (EROFS)
    #[error("Read-only filesystem")]
    ReadOnlyFilesystem,

    // === Data Errors ===
    /// Data unavailable for torrent
    #[error("Data unavailable for torrent {torrent_id}: {reason}")]
    DataUnavailable {
        torrent_id: u64,
        reason: DataUnavailableReason,
    },
}

impl RqbitFuseError {
    /// Convert the error to a libc error code suitable for FUSE replies.
    pub fn to_errno(&self) -> i32 {
        match self {
            // Not found errors
            RqbitFuseError::NotFound
            | RqbitFuseError::TorrentNotFound(_)
            | RqbitFuseError::FileNotFound { .. } => libc::ENOENT,

            // Permission errors
            RqbitFuseError::PermissionDenied | RqbitFuseError::AuthenticationError(_) => {
                libc::EACCES
            }

            // Timeout errors
            RqbitFuseError::TimedOut => libc::ETIMEDOUT,
            RqbitFuseError::ConnectionTimeout | RqbitFuseError::ReadTimeout => libc::EAGAIN,

            // I/O errors
            RqbitFuseError::IoError(_) | RqbitFuseError::ReadError(_) => libc::EIO,

            // Network errors
            RqbitFuseError::ServerDisconnected => libc::ENOTCONN,
            RqbitFuseError::NetworkError(_) => libc::ENETUNREACH,
            RqbitFuseError::ServiceUnavailable(_)
            | RqbitFuseError::CircuitBreakerOpen
            | RqbitFuseError::RetryLimitExceeded => libc::EAGAIN,

            // API errors with status codes
            RqbitFuseError::ApiError { status, .. } => match status {
                400 | 416 => libc::EINVAL,
                401 | 403 => libc::EACCES,
                404 => libc::ENOENT,
                408 | 423 | 429 | 503 | 504 => libc::EAGAIN,
                409 => libc::EEXIST,
                413 => libc::EFBIG,
                500 | 502 => libc::EIO,
                _ => libc::EIO,
            },

            RqbitFuseError::HttpError(_)
            | RqbitFuseError::ClientInitializationError(_)
            | RqbitFuseError::RequestCloneError(_) => libc::EIO,

            // Validation errors
            RqbitFuseError::InvalidArgument
            | RqbitFuseError::InvalidRange(_)
            | RqbitFuseError::InvalidValue(_)
            | RqbitFuseError::ValidationError(_) => libc::EINVAL,

            // Resource errors
            RqbitFuseError::NotReady => libc::EAGAIN,
            RqbitFuseError::DeviceBusy => libc::EBUSY,
            RqbitFuseError::ChannelFull => libc::EIO,

            // State errors
            RqbitFuseError::WorkerDisconnected => libc::EIO,
            RqbitFuseError::SerializationError(_) | RqbitFuseError::ParseError(_) => libc::EINVAL,

            // Directory errors
            RqbitFuseError::IsDirectory => libc::EISDIR,
            RqbitFuseError::NotDirectory => libc::ENOTDIR,

            // Filesystem errors
            RqbitFuseError::ReadOnlyFilesystem => libc::EROFS,

            // Data errors
            RqbitFuseError::DataUnavailable { .. } => libc::EIO,
        }
    }

    /// Check if this error is transient and retryable
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            RqbitFuseError::ConnectionTimeout
                | RqbitFuseError::ReadTimeout
                | RqbitFuseError::ServerDisconnected
                | RqbitFuseError::NetworkError(_)
                | RqbitFuseError::ServiceUnavailable(_)
                | RqbitFuseError::CircuitBreakerOpen
                | RqbitFuseError::RetryLimitExceeded
                | RqbitFuseError::NotReady
                | RqbitFuseError::ApiError {
                    status: 408 | 429 | 502 | 503 | 504,
                    ..
                }
        )
    }

    /// Check if this error indicates the server is unavailable
    pub fn is_server_unavailable(&self) -> bool {
        matches!(
            self,
            RqbitFuseError::ConnectionTimeout
                | RqbitFuseError::ServerDisconnected
                | RqbitFuseError::NetworkError(_)
                | RqbitFuseError::ServiceUnavailable(_)
                | RqbitFuseError::CircuitBreakerOpen
        )
    }
}

// === Conversion Implementations ===

impl From<std::io::Error> for RqbitFuseError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => RqbitFuseError::NotFound,
            std::io::ErrorKind::PermissionDenied => RqbitFuseError::PermissionDenied,
            std::io::ErrorKind::TimedOut => RqbitFuseError::TimedOut,
            std::io::ErrorKind::InvalidInput => RqbitFuseError::InvalidArgument,
            _ => RqbitFuseError::IoError(err.to_string()),
        }
    }
}

impl From<reqwest::Error> for RqbitFuseError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            if err.to_string().contains("connect") {
                RqbitFuseError::ConnectionTimeout
            } else {
                RqbitFuseError::ReadTimeout
            }
        } else if err.is_connect() {
            RqbitFuseError::ServerDisconnected
        } else if err.is_request() {
            RqbitFuseError::NetworkError(err.to_string())
        } else {
            RqbitFuseError::HttpError(err.to_string())
        }
    }
}

impl From<serde_json::Error> for RqbitFuseError {
    fn from(err: serde_json::Error) -> Self {
        RqbitFuseError::SerializationError(err.to_string())
    }
}

impl From<toml::de::Error> for RqbitFuseError {
    fn from(err: toml::de::Error) -> Self {
        RqbitFuseError::ParseError(err.to_string())
    }
}

// === Legacy Error Type Conversions (for backward compatibility during migration) ===

/// Trait for converting errors to FUSE error codes.
/// Implemented for anyhow::Error to provide consistent error mapping.
pub trait ToFuseError {
    /// Convert the error to a FUSE error code.
    fn to_fuse_error(&self) -> i32;
}

impl ToFuseError for anyhow::Error {
    fn to_fuse_error(&self) -> i32 {
        // Check for specific error types through downcasting
        if let Some(rqbit_err) = self.downcast_ref::<RqbitFuseError>() {
            return rqbit_err.to_errno();
        }

        // Check for std::io::Error
        if let Some(io_err) = self.downcast_ref::<std::io::Error>() {
            return match io_err.kind() {
                std::io::ErrorKind::NotFound => libc::ENOENT,
                std::io::ErrorKind::PermissionDenied => libc::EACCES,
                std::io::ErrorKind::TimedOut => libc::ETIMEDOUT,
                std::io::ErrorKind::InvalidInput => libc::EINVAL,
                _ => libc::EIO,
            };
        }

        // Default to EIO for unknown errors
        libc::EIO
    }
}

/// Result type alias for operations that can fail with RqbitFuseError.
pub type RqbitFuseResult<T> = Result<T, RqbitFuseError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_to_errno() {
        // Not found errors
        assert_eq!(RqbitFuseError::NotFound.to_errno(), libc::ENOENT);
        assert_eq!(RqbitFuseError::TorrentNotFound(1).to_errno(), libc::ENOENT);
        assert_eq!(
            RqbitFuseError::FileNotFound {
                torrent_id: 1,
                file_idx: 0
            }
            .to_errno(),
            libc::ENOENT
        );

        // Permission errors
        assert_eq!(RqbitFuseError::PermissionDenied.to_errno(), libc::EACCES);
        assert_eq!(
            RqbitFuseError::AuthenticationError("test".to_string()).to_errno(),
            libc::EACCES
        );

        // Timeout errors
        assert_eq!(RqbitFuseError::TimedOut.to_errno(), libc::ETIMEDOUT);
        assert_eq!(RqbitFuseError::ConnectionTimeout.to_errno(), libc::EAGAIN);
        assert_eq!(RqbitFuseError::ReadTimeout.to_errno(), libc::EAGAIN);

        // I/O errors
        assert_eq!(
            RqbitFuseError::IoError("test".to_string()).to_errno(),
            libc::EIO
        );

        // Validation errors
        assert_eq!(RqbitFuseError::InvalidArgument.to_errno(), libc::EINVAL);
        assert_eq!(
            RqbitFuseError::InvalidRange("test".to_string()).to_errno(),
            libc::EINVAL
        );

        // Directory errors
        assert_eq!(RqbitFuseError::IsDirectory.to_errno(), libc::EISDIR);
        assert_eq!(RqbitFuseError::NotDirectory.to_errno(), libc::ENOTDIR);

        // Resource errors
        assert_eq!(RqbitFuseError::NotReady.to_errno(), libc::EAGAIN);
        assert_eq!(RqbitFuseError::DeviceBusy.to_errno(), libc::EBUSY);
    }

    #[test]
    fn test_api_error_to_errno() {
        // API error status code mappings
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 400,
                message: "test".to_string()
            }
            .to_errno(),
            libc::EINVAL
        );
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 404,
                message: "test".to_string()
            }
            .to_errno(),
            libc::ENOENT
        );
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 429,
                message: "test".to_string()
            }
            .to_errno(),
            libc::EAGAIN
        );
        assert_eq!(
            RqbitFuseError::ApiError {
                status: 500,
                message: "test".to_string()
            }
            .to_errno(),
            libc::EIO
        );
    }

    #[test]
    fn test_is_transient() {
        assert!(RqbitFuseError::ConnectionTimeout.is_transient());
        assert!(RqbitFuseError::ReadTimeout.is_transient());
        assert!(RqbitFuseError::ServerDisconnected.is_transient());
        assert!(RqbitFuseError::NetworkError("test".to_string()).is_transient());
        assert!(RqbitFuseError::CircuitBreakerOpen.is_transient());
        assert!(RqbitFuseError::RetryLimitExceeded.is_transient());
        assert!(RqbitFuseError::NotReady.is_transient());
        assert!(RqbitFuseError::ApiError {
            status: 429,
            message: "test".to_string()
        }
        .is_transient());

        // Non-transient errors
        assert!(!RqbitFuseError::NotFound.is_transient());
        assert!(!RqbitFuseError::PermissionDenied.is_transient());
        assert!(!RqbitFuseError::TorrentNotFound(1).is_transient());
        assert!(!RqbitFuseError::InvalidRange("test".to_string()).is_transient());
    }

    #[test]
    fn test_is_server_unavailable() {
        assert!(RqbitFuseError::ConnectionTimeout.is_server_unavailable());
        assert!(RqbitFuseError::ServerDisconnected.is_server_unavailable());
        assert!(RqbitFuseError::NetworkError("test".to_string()).is_server_unavailable());
        assert!(RqbitFuseError::CircuitBreakerOpen.is_server_unavailable());

        // Not server unavailable
        assert!(!RqbitFuseError::NotFound.is_server_unavailable());
        assert!(!RqbitFuseError::ReadTimeout.is_server_unavailable());
        assert!(!RqbitFuseError::RetryLimitExceeded.is_server_unavailable());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let rqbit_err: RqbitFuseError = io_err.into();
        assert!(matches!(rqbit_err, RqbitFuseError::NotFound));

        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let rqbit_err: RqbitFuseError = io_err.into();
        assert!(matches!(rqbit_err, RqbitFuseError::PermissionDenied));

        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
        let rqbit_err: RqbitFuseError = io_err.into();
        assert!(matches!(rqbit_err, RqbitFuseError::TimedOut));
    }

    #[test]
    fn test_display_formatting() {
        assert_eq!(format!("{}", RqbitFuseError::NotFound), "Not found");
        assert_eq!(
            format!("{}", RqbitFuseError::PermissionDenied),
            "Permission denied"
        );
        assert_eq!(
            format!("{}", RqbitFuseError::IoError("test".to_string())),
            "I/O error: test"
        );
        assert_eq!(
            format!("{}", RqbitFuseError::TorrentNotFound(42)),
            "Torrent not found: 42"
        );
        assert_eq!(
            format!(
                "{}",
                RqbitFuseError::FileNotFound {
                    torrent_id: 1,
                    file_idx: 2
                }
            ),
            "File not found in torrent 1: file_idx=2"
        );
    }

    #[test]
    fn test_validation_error_display() {
        let issues = vec![
            ValidationIssue {
                field: "api.url".to_string(),
                message: "URL cannot be empty".to_string(),
            },
            ValidationIssue {
                field: "cache.max_entries".to_string(),
                message: "must be greater than 0".to_string(),
            },
        ];
        let err = RqbitFuseError::ValidationError(issues);
        let display = format!("{}", err);
        assert!(display.contains("api.url: URL cannot be empty"));
        assert!(display.contains("cache.max_entries: must be greater than 0"));
    }

    #[test]
    fn test_anyhow_to_fuse_error() {
        let err = anyhow::Error::new(RqbitFuseError::NotFound);
        assert_eq!(err.to_fuse_error(), libc::ENOENT);

        let err = anyhow::Error::new(RqbitFuseError::PermissionDenied);
        assert_eq!(err.to_fuse_error(), libc::EACCES);

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err = anyhow::Error::new(io_err);
        assert_eq!(err.to_fuse_error(), libc::ENOENT);
    }
}
