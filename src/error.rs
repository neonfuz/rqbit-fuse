use thiserror::Error;

/// Represents a single validation error in the configuration.
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

/// Unified error type for rqbit-fuse with 11 essential variants.
#[derive(Error, Debug, Clone)]
pub enum RqbitFuseError {
    /// Entity not found (ENOENT)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Permission denied (EACCES)
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Operation timed out (ETIMEDOUT)
    #[error("Operation timed out: {0}")]
    TimedOut(String),

    /// Network error - covers server disconnected, circuit breaker, etc.
    #[error("Network error: {0}")]
    NetworkError(String),

    /// API returned error with HTTP status code
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(String),

    /// Invalid argument (EINVAL)
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Validation error with multiple issues
    #[error("Validation error: {}", .0.iter().map(|i| i.to_string()).collect::<Vec<_>>().join("; "))]
    ValidationError(Vec<ValidationIssue>),

    /// Resource temporarily unavailable (EAGAIN)
    #[error("Resource temporarily unavailable: {0}")]
    NotReady(String),

    /// Parse/serialization error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Is a directory (EISDIR)
    #[error("Is a directory")]
    IsDirectory,

    /// Not a directory (ENOTDIR)
    #[error("Not a directory")]
    NotDirectory,
}

impl RqbitFuseError {
    /// Convert the error to a libc error code suitable for FUSE replies.
    pub fn to_errno(&self) -> i32 {
        match self {
            RqbitFuseError::NotFound(_) => libc::ENOENT,
            RqbitFuseError::PermissionDenied(_) => libc::EACCES,
            RqbitFuseError::TimedOut(_) => libc::ETIMEDOUT,
            RqbitFuseError::NetworkError(_) => libc::ENETUNREACH,
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
            RqbitFuseError::IoError(_) => libc::EIO,
            RqbitFuseError::InvalidArgument(_) => libc::EINVAL,
            RqbitFuseError::ValidationError(_) => libc::EINVAL,
            RqbitFuseError::NotReady(_) => libc::EAGAIN,
            RqbitFuseError::ParseError(_) => libc::EINVAL,
            RqbitFuseError::IsDirectory => libc::EISDIR,
            RqbitFuseError::NotDirectory => libc::ENOTDIR,
        }
    }

    /// Check if this error is transient and retryable
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            RqbitFuseError::TimedOut(_)
                | RqbitFuseError::NetworkError(_)
                | RqbitFuseError::NotReady(_)
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
            RqbitFuseError::TimedOut(_) | RqbitFuseError::NetworkError(_)
        )
    }
}

// === Conversion Implementations ===

impl From<std::io::Error> for RqbitFuseError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => RqbitFuseError::NotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => {
                RqbitFuseError::PermissionDenied(err.to_string())
            }
            std::io::ErrorKind::TimedOut => RqbitFuseError::TimedOut(err.to_string()),
            std::io::ErrorKind::InvalidInput => RqbitFuseError::InvalidArgument(err.to_string()),
            _ => RqbitFuseError::IoError(err.to_string()),
        }
    }
}

impl From<reqwest::Error> for RqbitFuseError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            RqbitFuseError::TimedOut(err.to_string())
        } else if err.is_connect() {
            RqbitFuseError::NetworkError(format!("Server disconnected: {}", err))
        } else if err.is_request() {
            RqbitFuseError::NetworkError(err.to_string())
        } else {
            RqbitFuseError::IoError(format!("HTTP error: {}", err))
        }
    }
}

impl From<serde_json::Error> for RqbitFuseError {
    fn from(err: serde_json::Error) -> Self {
        RqbitFuseError::ParseError(err.to_string())
    }
}

impl From<toml::de::Error> for RqbitFuseError {
    fn from(err: toml::de::Error) -> Self {
        RqbitFuseError::ParseError(err.to_string())
    }
}

// === Legacy Error Type Conversions (for backward compatibility during migration) ===

/// Trait for converting errors to FUSE error codes.
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
        assert_eq!(
            RqbitFuseError::NotFound("test".to_string()).to_errno(),
            libc::ENOENT
        );

        // Permission errors
        assert_eq!(
            RqbitFuseError::PermissionDenied("test".to_string()).to_errno(),
            libc::EACCES
        );

        // Timeout errors
        assert_eq!(
            RqbitFuseError::TimedOut("test".to_string()).to_errno(),
            libc::ETIMEDOUT
        );

        // Network errors
        assert_eq!(
            RqbitFuseError::NetworkError("test".to_string()).to_errno(),
            libc::ENETUNREACH
        );

        // I/O errors
        assert_eq!(
            RqbitFuseError::IoError("test".to_string()).to_errno(),
            libc::EIO
        );

        // Validation errors
        assert_eq!(
            RqbitFuseError::InvalidArgument("test".to_string()).to_errno(),
            libc::EINVAL
        );

        // Directory errors
        assert_eq!(RqbitFuseError::IsDirectory.to_errno(), libc::EISDIR);
        assert_eq!(RqbitFuseError::NotDirectory.to_errno(), libc::ENOTDIR);

        // Resource errors
        assert_eq!(
            RqbitFuseError::NotReady("test".to_string()).to_errno(),
            libc::EAGAIN
        );
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
        assert!(RqbitFuseError::TimedOut("test".to_string()).is_transient());
        assert!(RqbitFuseError::NetworkError("test".to_string()).is_transient());
        assert!(RqbitFuseError::NotReady("test".to_string()).is_transient());
        assert!(RqbitFuseError::ApiError {
            status: 429,
            message: "test".to_string()
        }
        .is_transient());

        // Non-transient errors
        assert!(!RqbitFuseError::NotFound("test".to_string()).is_transient());
        assert!(!RqbitFuseError::PermissionDenied("test".to_string()).is_transient());
        assert!(!RqbitFuseError::InvalidArgument("test".to_string()).is_transient());
        assert!(!RqbitFuseError::ApiError {
            status: 400,
            message: "test".to_string()
        }
        .is_transient());
    }

    #[test]
    fn test_is_server_unavailable() {
        assert!(RqbitFuseError::TimedOut("test".to_string()).is_server_unavailable());
        assert!(RqbitFuseError::NetworkError("test".to_string()).is_server_unavailable());

        // Not server unavailable
        assert!(!RqbitFuseError::NotFound("test".to_string()).is_server_unavailable());
        assert!(!RqbitFuseError::NotReady("test".to_string()).is_server_unavailable());
    }

    #[test]
    fn test_display_formatting() {
        assert_eq!(
            format!("{}", RqbitFuseError::NotFound("test".to_string())),
            "Not found: test"
        );
    }
}
