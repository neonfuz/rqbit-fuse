use thiserror::Error;

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

    /// Validation error with messages
    #[error("Validation error: {}", .0.join("; "))]
    ValidationError(Vec<String>),

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

macro_rules! impl_from_error {
    ($err_type:ty, $arm:pat => $body:expr) => {
        impl From<$err_type> for RqbitFuseError {
            fn from(err: $err_type) -> Self {
                match err {
                    $arm => $body,
                }
            }
        }
    };
}

impl_from_error!(std::io::Error, e => match e.kind() {
    std::io::ErrorKind::NotFound => RqbitFuseError::NotFound(e.to_string()),
    std::io::ErrorKind::PermissionDenied => RqbitFuseError::PermissionDenied(e.to_string()),
    std::io::ErrorKind::TimedOut => RqbitFuseError::TimedOut(e.to_string()),
    std::io::ErrorKind::InvalidInput => RqbitFuseError::InvalidArgument(e.to_string()),
    _ => RqbitFuseError::IoError(e.to_string()),
});

impl_from_error!(reqwest::Error, e => if e.is_timeout() {
    RqbitFuseError::TimedOut(e.to_string())
} else if e.is_connect() {
    RqbitFuseError::NetworkError(format!("Server disconnected: {}", e))
} else if e.is_request() {
    RqbitFuseError::NetworkError(e.to_string())
} else {
    RqbitFuseError::IoError(format!("HTTP error: {}", e))
});

impl_from_error!(serde_json::Error, e => RqbitFuseError::ParseError(e.to_string()));
impl_from_error!(toml::de::Error, e => RqbitFuseError::ParseError(e.to_string()));

/// Convert an anyhow error to a FUSE error code.
pub fn anyhow_to_errno(err: &anyhow::Error) -> i32 {
    if let Some(rqbit_err) = err.downcast_ref::<RqbitFuseError>() {
        rqbit_err.to_errno()
    } else if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
        match io_err.kind() {
            std::io::ErrorKind::NotFound => libc::ENOENT,
            std::io::ErrorKind::PermissionDenied => libc::EACCES,
            std::io::ErrorKind::TimedOut => libc::ETIMEDOUT,
            std::io::ErrorKind::InvalidInput => libc::EINVAL,
            _ => libc::EIO,
        }
    } else {
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
