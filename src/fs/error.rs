use std::fmt;

/// Error types for FUSE operations.
/// These errors map directly to FUSE error codes and provide
/// a type-safe alternative to string-based error detection.
#[derive(Debug, Clone)]
pub enum FuseError {
    /// Entity not found (ENOENT)
    NotFound,
    /// Permission denied (EACCES)
    PermissionDenied,
    /// Operation timed out (ETIMEDOUT)
    TimedOut,
    /// Input/output error (EIO)
    IoError(String),
    /// Resource temporarily unavailable (EAGAIN)
    NotReady,
    /// Channel is full, cannot send request
    ChannelFull,
    /// Worker has disconnected
    WorkerDisconnected,
    /// Invalid argument (EINVAL)
    InvalidArgument,
    /// Is a directory (EISDIR)
    IsDirectory,
    /// Not a directory (ENOTDIR)
    NotDirectory,
    /// Device or resource busy (EBUSY)
    DeviceBusy,
    /// Read-only filesystem (EROFS)
    ReadOnlyFilesystem,
}

impl FuseError {
    /// Convert the error to a libc error code suitable for FUSE replies.
    pub fn to_errno(&self) -> i32 {
        match self {
            FuseError::NotFound => libc::ENOENT,
            FuseError::PermissionDenied => libc::EACCES,
            FuseError::TimedOut => libc::ETIMEDOUT,
            FuseError::IoError(_) => libc::EIO,
            FuseError::NotReady => libc::EAGAIN,
            FuseError::ChannelFull => libc::EIO,
            FuseError::WorkerDisconnected => libc::EIO,
            FuseError::InvalidArgument => libc::EINVAL,
            FuseError::IsDirectory => libc::EISDIR,
            FuseError::NotDirectory => libc::ENOTDIR,
            FuseError::DeviceBusy => libc::EBUSY,
            FuseError::ReadOnlyFilesystem => libc::EROFS,
        }
    }
}

impl fmt::Display for FuseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FuseError::NotFound => write!(f, "Not found"),
            FuseError::PermissionDenied => write!(f, "Permission denied"),
            FuseError::TimedOut => write!(f, "Operation timed out"),
            FuseError::IoError(msg) => write!(f, "I/O error: {}", msg),
            FuseError::NotReady => write!(f, "Resource temporarily unavailable"),
            FuseError::ChannelFull => write!(f, "Request channel is full"),
            FuseError::WorkerDisconnected => write!(f, "Async worker disconnected"),
            FuseError::InvalidArgument => write!(f, "Invalid argument"),
            FuseError::IsDirectory => write!(f, "Is a directory"),
            FuseError::NotDirectory => write!(f, "Not a directory"),
            FuseError::DeviceBusy => write!(f, "Device or resource busy"),
            FuseError::ReadOnlyFilesystem => write!(f, "Read-only filesystem"),
        }
    }
}

impl std::error::Error for FuseError {}

/// Trait for converting errors to FUSE error codes.
/// Implemented for anyhow::Error and ApiError to provide
/// consistent error mapping across the codebase.
pub trait ToFuseError {
    /// Convert the error to a FUSE error code.
    fn to_fuse_error(&self) -> i32;
}

impl ToFuseError for anyhow::Error {
    fn to_fuse_error(&self) -> i32 {
        // Check for specific error types through downcasting
        if let Some(api_err) = self.downcast_ref::<crate::api::types::ApiError>() {
            return api_err.to_fuse_error();
        }

        if let Some(fuse_err) = self.downcast_ref::<FuseError>() {
            return fuse_err.to_errno();
        }

        // String matching (temporary, should use typed errors exclusively)
        let err_str = self.to_string().to_lowercase();
        if err_str.contains("not found") {
            libc::ENOENT
        } else if err_str.contains("permission") || err_str.contains("access") {
            libc::EACCES
        } else if err_str.contains("timeout") {
            libc::ETIMEDOUT
        } else if err_str.contains("range") {
            libc::EINVAL
        } else {
            // Covers: channel full, disconnected, and other errors
            libc::EIO
        }
    }
}

impl ToFuseError for crate::api::types::ApiError {
    fn to_fuse_error(&self) -> i32 {
        use crate::api::types::ApiError;
        match self {
            ApiError::TorrentNotFound(_) => libc::ENOENT,
            ApiError::FileNotFound { .. } => libc::ENOENT,
            ApiError::InvalidRange(_) => libc::EINVAL,
            ApiError::ReadTimeout => libc::ETIMEDOUT,
            ApiError::ConnectionTimeout => libc::ETIMEDOUT,
            _ => libc::EIO,
        }
    }
}

/// Convert a std::io::Error to a FUSE error code.
impl From<std::io::Error> for FuseError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => FuseError::NotFound,
            std::io::ErrorKind::PermissionDenied => FuseError::PermissionDenied,
            std::io::ErrorKind::TimedOut => FuseError::TimedOut,
            std::io::ErrorKind::InvalidInput => FuseError::InvalidArgument,
            _ => FuseError::IoError(err.to_string()),
        }
    }
}

/// Result type alias for FUSE operations.
pub type FuseResult<T> = Result<T, FuseError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuse_error_to_errno() {
        assert_eq!(FuseError::NotFound.to_errno(), libc::ENOENT);
        assert_eq!(FuseError::PermissionDenied.to_errno(), libc::EACCES);
        assert_eq!(FuseError::TimedOut.to_errno(), libc::ETIMEDOUT);
        assert_eq!(FuseError::IoError("test".to_string()).to_errno(), libc::EIO);
        assert_eq!(FuseError::NotReady.to_errno(), libc::EAGAIN);
    }

    #[test]
    fn test_fuse_error_display() {
        assert_eq!(format!("{}", FuseError::NotFound), "Not found");
        assert_eq!(
            format!("{}", FuseError::PermissionDenied),
            "Permission denied"
        );
        assert_eq!(
            format!("{}", FuseError::IoError("test".to_string())),
            "I/O error: test"
        );
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let fuse_err: FuseError = io_err.into();
        assert!(matches!(fuse_err, FuseError::NotFound));

        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let fuse_err: FuseError = io_err.into();
        assert!(matches!(fuse_err, FuseError::PermissionDenied));
    }
}
